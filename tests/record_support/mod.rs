//! Shared harness of the recording integration tests (MCP stdio client, scratch
//! homes, ffprobe helpers). Included with `mod record_support;` by
//! tests/record_hub.rs and tests/record_follow.rs.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_bladebro")
}

pub fn tmp_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bbhub-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch home");
    dir
}

/// Basename of the scratch home: unique per test, and stable under the
/// /var -> /private/var symlink that makes full paths differ in `ps`.
pub fn home_tag(home: &Path) -> String {
    home.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(unix)]
pub fn pgrep(args: &[&str]) -> Vec<String> {
    let out = Command::new("pgrep")
        .args(args)
        .output()
        .expect("pgrep must exist on unix");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect()
}

/// Processes whose command line carries this scratch home (Chrome, daemon, ffmpeg).
#[cfg(unix)]
pub fn procs_with_home(home: &Path) -> Vec<String> {
    pgrep(&["-fl", &home_tag(home)])
}

#[cfg(unix)]
pub fn ffmpeg_children(pid: u32) -> Vec<String> {
    pgrep(&["-P", &pid.to_string(), "-fl", "ffmpeg"])
}

#[cfg(not(unix))]
pub fn ffmpeg_children(_pid: u32) -> Vec<String> {
    Vec::new()
}

pub struct Mcp {
    pub child: Child,
    pub stdin: Option<ChildStdin>,
    pub rx: Receiver<String>,
    pub home: PathBuf,
    pub next_id: u64,
}

impl Mcp {
    pub fn spawn(tag: &str, envs: &[(&str, &str)]) -> Mcp {
        let home = tmp_home(tag);
        let mut cmd = Command::new(bin());
        cmd.arg("mcp")
            .env("BLADE_HOME", &home)
            .env("BLADE_NO_UPDATE_CHECK", "1")
            .env("BLADE_NO_WARMING", "1")
            .env("BLADE_HEADLESS", "1")
            .env_remove("BLADE_RECORD")
            .env_remove("BLADE_TRANSPORT")
            .env_remove("BLADE_FFMPEG")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("bladebro mcp must spawn");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut mcp = Mcp {
            child,
            stdin,
            rx,
            home,
            next_id: 1,
        };
        mcp.request(
            "initialize",
            json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"record_hub","version":"0"}}),
            Duration::from_secs(20),
        );
        mcp
    }

    pub fn send(&mut self, v: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{v}").expect("write request");
        stdin.flush().expect("flush request");
    }

    /// One request, one response with the same id (60 s default cap for tool calls).
    pub fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(line) => {
                    let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                    if v.get("id") == Some(&json!(id)) {
                        return v;
                    }
                }
                Err(RecvTimeoutError::Timeout) => panic!("no response to {method} in {timeout:?}"),
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("server closed stdout during {method}")
                }
            }
        }
    }

    /// `tools/call` -> (isError, joined text content).
    pub fn call(&mut self, tool: &str, args: Value) -> (bool, String) {
        let resp = self.request(
            "tools/call",
            json!({"name":tool,"arguments":args}),
            Duration::from_secs(60),
        );
        let result = resp.get("result").cloned().unwrap_or(Value::Null);
        let is_err = result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || resp.get("error").is_some();
        let mut text = result
            .get("content")
            .and_then(|c| c.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if text.is_empty() {
            if let Some(e) = resp.get("error") {
                text = e.to_string();
            }
        }
        (is_err, text)
    }

    pub fn nav(&mut self, url: &str) -> (bool, String) {
        self.call("act", json!({"action":"navigate","url":url}))
    }

    pub fn state(&mut self, args: Value) -> (bool, String) {
        self.call("state", args)
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Videos only (`.webm` / `.mp4`); sidecars are listed by `sidecars()`.
    pub fn recordings(&self) -> Vec<PathBuf> {
        self.listed(|p| p.extension().is_some_and(|x| x == "webm" || x == "mp4"))
    }

    /// Sidecar files next to the videos (`.png` poster, `.json` timeline).
    pub fn sidecars(&self) -> Vec<PathBuf> {
        self.listed(|p| p.extension().is_some_and(|x| x == "png" || x == "json"))
    }

    fn listed(&self, keep: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
        let dir = self.home.join("recordings");
        match std::fs::read_dir(&dir) {
            Ok(rd) => {
                let mut v: Vec<PathBuf> = rd
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| keep(p))
                    .collect();
                v.sort();
                v
            }
            Err(_) => Vec::new(),
        }
    }

    pub fn close_stdin(&mut self) {
        self.stdin = None;
    }

    pub fn wait_exit(&mut self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        self.stdin = None;
        if !self.wait_exit(Duration::from_secs(3)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        // Anything still carrying our scratch home is ours (Chrome the server
        // failed to reap): kill by the unique home basename only.
        #[cfg(unix)]
        {
            let tag = home_tag(&self.home);
            for _ in 0..20 {
                if procs_with_home(&self.home).is_empty() {
                    break;
                }
                let _ = Command::new("pkill").args(["-f", &tag]).status();
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

// -------------------------------------------------------------------- live

pub fn fix() -> Option<String> {
    match std::env::var("REC_HUB_FIX")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        Some(v) => Some(v.trim_end_matches('/').to_string()),
        None => {
            eprintln!("REC_HUB_FIX unset: live test skipped (see hub_live.sh)");
            None
        }
    }
}

pub fn ffmpeg() -> String {
    std::env::var("REC_HUB_FFMPEG").unwrap_or_else(|_| "ffmpeg".into())
}

pub fn ffprobe() -> String {
    std::env::var("REC_HUB_FFPROBE").unwrap_or_else(|_| "ffprobe".into())
}

/// (codec, width, height) of the first video stream; None if ffprobe cannot read it.
pub fn probe(path: &Path) -> Option<(String, u32, u32)> {
    let out = Command::new(ffprobe())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,width,height",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let mut it = s.split(',');
    Some((
        it.next()?.to_string(),
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    ))
}

/// Full decode to null: exit 0 and no stderr output.
pub fn decodes_clean(path: &Path) -> bool {
    Command::new(ffmpeg())
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()
        .map(|o| o.status.success() && o.stderr.is_empty())
        .unwrap_or(false)
}

pub fn live_env<'a>(extra: &[(&'a str, &'a str)], ff: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut v = vec![("BLADE_FFMPEG", ff)];
    v.extend_from_slice(extra);
    v
}

pub fn field_after(text: &str, key: &str) -> f64 {
    let rest = text
        .split(key)
        .nth(1)
        .unwrap_or_else(|| panic!("no {key:?} in {text}"));
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    num.parse()
        .unwrap_or_else(|_| panic!("bad number after {key:?} in {text}"))
}

pub fn saved_path(text: &str) -> PathBuf {
    let rest = text
        .split("recording saved: ")
        .nth(1)
        .unwrap_or_else(|| panic!("no 'recording saved: ' in {text}"));
    PathBuf::from(rest.split(" (bytes ").next().expect("path"))
}
