//! Integration tests for the `state` recording ops (`record-start` / `record-stop`)
//! over the real binary's MCP stdio server (07-03).
//!
//! The hermetic tests run in the normal suite and need no Chrome (the server
//! launches Chrome lazily, so `tools/list` and a stdin close touch no browser).
//! The live tests are `#[ignore]`: they need Chrome + ffmpeg and local fixtures,
//! and run through `tools/record_sidecar/scenarios/hub_live.sh`.
//!
//! Harness inputs of the live tests (not product options):
//!   REC_HUB_FIX      fixture base URL, e.g. http://127.0.0.1:8765 (unset: live tests skip)
//!   REC_HUB_FFMPEG   ffmpeg path, passed to the child as BLADE_FFMPEG
//!   REC_HUB_FFPROBE  ffprobe path (default: `ffprobe` on PATH)

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_bladebro")
}

fn tmp_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bbhub-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch home");
    dir
}

/// Basename of the scratch home: unique per test, and stable under the
/// /var -> /private/var symlink that makes full paths differ in `ps`.
fn home_tag(home: &Path) -> String {
    home.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(unix)]
fn pgrep(args: &[&str]) -> Vec<String> {
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
fn procs_with_home(home: &Path) -> Vec<String> {
    pgrep(&["-fl", &home_tag(home)])
}

#[cfg(unix)]
fn ffmpeg_children(pid: u32) -> Vec<String> {
    pgrep(&["-P", &pid.to_string(), "-fl", "ffmpeg"])
}

#[cfg(not(unix))]
fn ffmpeg_children(_pid: u32) -> Vec<String> {
    Vec::new()
}

struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    home: PathBuf,
    next_id: u64,
}

impl Mcp {
    fn spawn(tag: &str, envs: &[(&str, &str)]) -> Mcp {
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

    fn send(&mut self, v: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{v}").expect("write request");
        stdin.flush().expect("flush request");
    }

    /// One request, one response with the same id (60 s default cap for tool calls).
    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Value {
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
    fn call(&mut self, tool: &str, args: Value) -> (bool, String) {
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

    fn nav(&mut self, url: &str) -> (bool, String) {
        self.call("act", json!({"action":"navigate","url":url}))
    }

    fn state(&mut self, args: Value) -> (bool, String) {
        self.call("state", args)
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn recordings(&self) -> Vec<PathBuf> {
        let dir = self.home.join("recordings");
        match std::fs::read_dir(&dir) {
            Ok(rd) => {
                let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
                v.sort();
                v
            }
            Err(_) => Vec::new(),
        }
    }

    fn close_stdin(&mut self) {
        self.stdin = None;
    }

    fn wait_exit(&mut self, within: Duration) -> bool {
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

// ---------------------------------------------------------------- hermetic

#[test]
fn mcp_tools_list_has_record_ops() {
    let mut mcp = Mcp::spawn("list", &[]);
    let resp = mcp.request("tools/list", json!({}), Duration::from_secs(20));
    let tools = resp["result"]["tools"].as_array().expect("tools array");
    let mut names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        ["act", "run", "see", "state", "vision"],
        "five tools, forever"
    );

    let state = tools
        .iter()
        .find(|t| t["name"] == "state")
        .expect("state tool");
    let ops: Vec<&str> = state["inputSchema"]["properties"]["op"]["enum"]
        .as_array()
        .expect("op enum")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(ops.iter().filter(|o| **o == "record-start").count(), 1);
    assert_eq!(ops.iter().filter(|o| **o == "record-stop").count(), 1);

    // No property was added for recording: `name` is the pre-existing one.
    let props: Vec<&str> = state["inputSchema"]["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .map(|k| k.as_str())
        .collect();
    for forbidden in ["record", "recording", "fps", "quality", "format", "output"] {
        assert!(
            !props.contains(&forbidden),
            "no new state property: {forbidden}"
        );
    }

    #[cfg(unix)]
    assert!(
        procs_with_home(&mcp.home).is_empty(),
        "tools/list must not start Chrome: {:?}",
        procs_with_home(&mcp.home)
    );
    mcp.close_stdin();
    assert!(
        mcp.wait_exit(Duration::from_secs(10)),
        "server must exit after stdin closes"
    );
}

#[test]
fn cli_record_start_without_name_is_usage() {
    let home = tmp_home("cli-usage");
    let out = Command::new(bin())
        .args(["state", "record-start"])
        .env("BLADE_HOME", &home)
        .env("BLADE_NO_UPDATE_CHECK", "1")
        .env("BLADE_HEADLESS", "1")
        .output()
        .expect("bladebro must run");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "must exit non-zero: {text}");
    assert!(text.contains("needs a name"), "usage text expected: {text}");
    #[cfg(unix)]
    assert!(
        procs_with_home(&home).is_empty(),
        "a parse error must start no daemon or Chrome: {:?}",
        procs_with_home(&home)
    );
    let _ = std::fs::remove_dir_all(&home);
}

// -------------------------------------------------------------------- live

fn fix() -> Option<String> {
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

fn ffmpeg() -> String {
    std::env::var("REC_HUB_FFMPEG").unwrap_or_else(|_| "ffmpeg".into())
}

fn ffprobe() -> String {
    std::env::var("REC_HUB_FFPROBE").unwrap_or_else(|_| "ffprobe".into())
}

/// (codec, width, height) of the first video stream; None if ffprobe cannot read it.
fn probe(path: &Path) -> Option<(String, u32, u32)> {
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
fn decodes_clean(path: &Path) -> bool {
    Command::new(ffmpeg())
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()
        .map(|o| o.status.success() && o.stderr.is_empty())
        .unwrap_or(false)
}

fn live_env<'a>(extra: &[(&'a str, &'a str)], ff: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut v = vec![("BLADE_FFMPEG", ff)];
    v.extend_from_slice(extra);
    v
}

fn field_after(text: &str, key: &str) -> f64 {
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

fn saved_path(text: &str) -> PathBuf {
    let rest = text
        .split("recording saved: ")
        .nth(1)
        .unwrap_or_else(|| panic!("no 'recording saved: ' in {text}"));
    PathBuf::from(rest.split(" (bytes ").next().expect("path"))
}

const LIVE: &str = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)";

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn start_browse_stop() {
    let _ = LIVE;
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("sbs", &live_env(&[("BLADE_RECORD", "1")], &ff));
    let (e, t) = mcp.nav(&format!("{fix}/anim.html"));
    assert!(!e, "nav anim: {t}");
    let t0 = Instant::now();
    let (e, start) = mcp.state(json!({"op":"record-start","name":"t1"}));
    eprintln!("RESULT record-start: {start}");
    assert!(
        !e && start.contains("recording started: /"),
        "start: {start}"
    );
    let (e, t) = mcp.nav(&format!("{fix}/tick.html"));
    assert!(!e, "nav tick: {t}");
    std::thread::sleep(Duration::from_secs(6));
    let (e, stop) = mcp.state(json!({"op":"record-stop"}));
    let wall = t0.elapsed().as_secs_f64();
    eprintln!("RESULT record-stop: {stop}");
    assert!(!e, "stop must not be an error: {stop}");
    assert!(stop.contains("recording saved: "), "{stop}");

    let path = saved_path(&stop);
    assert!(path.is_absolute(), "absolute path: {path:?}");
    assert!(path.exists(), "file exists: {path:?}");
    assert!(
        path.starts_with(mcp.home.join("recordings"))
            || path.starts_with(
                std::fs::canonicalize(&mcp.home)
                    .unwrap_or_default()
                    .join("recordings")
            ),
        "under <home>/recordings: {path:?}"
    );
    let size = std::fs::metadata(&path).expect("stat").len() as f64;
    assert_eq!(
        field_after(&stop, "bytes "),
        size,
        "bytes equal file size: {stop}"
    );
    let dur = field_after(&stop, "duration ");
    assert!(
        (dur - wall).abs() <= wall * 0.30,
        "duration {dur} s within 30% of wall {wall:.1} s: {stop}"
    );
    assert!(field_after(&stop, "frames ") > 0.0, "{stop}");
    assert!(stop.contains("dropped "), "{stop}");
    assert!(stop.contains("segments [0.0 s tab "), "{stop}");
    let (codec, w, h) = probe(&path).expect("ffprobe reads the file");
    eprintln!("RESULT ffprobe: {codec} {w}x{h} wall={wall:.2}s duration={dur}s");
    assert_eq!(codec, "vp9");
    assert!(decodes_clean(&path), "full decode must be clean");
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn double_start_names_file() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("dbl", &live_env(&[("BLADE_RECORD", "1")], &ff));
    assert!(!mcp.nav(&format!("{fix}/anim.html")).0);
    let (e, a) = mcp.state(json!({"op":"record-start","name":"a"}));
    assert!(!e, "{a}");
    let first = a
        .split("recording started: ")
        .nth(1)
        .and_then(|r| r.split(" (tab").next())
        .expect("first path")
        .to_string();
    let (e, b) = mcp.state(json!({"op":"record-start","name":"b"}));
    eprintln!("RESULT double-start: {b}");
    assert!(e, "second start must be isError: {b}");
    assert!(b.contains(&first), "names the first file {first}: {b}");
    assert!(b.contains("record-stop"), "{b}");
    let files = mcp.recordings();
    assert_eq!(files.len(), 1, "exactly one file: {files:?}");
    // A stop before any frame is an honest "no frames" error (by design), so let
    // the first frames arrive before the clean stop.
    std::thread::sleep(Duration::from_secs(2));
    let (e, s) = mcp.state(json!({"op":"record-stop"}));
    assert!(!e, "{s}");
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn off_mode_refuses_and_creates_nothing() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("off", &live_env(&[], &ff));
    let pid = mcp.pid();
    let url = format!("{fix}/a.html");
    let (e, t) = mcp.nav(&url);
    assert!(!e, "{t}");
    assert!(ffmpeg_children(pid).is_empty(), "no ffmpeg after nav");
    let (e, t) = mcp.call("see", json!({"mode":"content"}));
    assert!(!e, "{t}");
    let (e, t) = mcp.state(json!({"op":"tabs"}));
    assert!(!e, "{t}");
    assert!(ffmpeg_children(pid).is_empty(), "no ffmpeg after tabs");
    let (e, t) = mcp.state(json!({"op":"record-start","name":"x"}));
    eprintln!("RESULT off record-start: {t}");
    assert!(
        e && t.contains("BLADE_RECORD") && t.contains("bladebro stop"),
        "{t}"
    );
    let (_, t) = mcp.state(json!({"op":"record-stop"}));
    eprintln!("RESULT off record-stop: {t}");
    assert!(t.contains("no active recording"), "{t}");
    assert!(ffmpeg_children(pid).is_empty(), "no ffmpeg after refusals");
    assert!(
        !mcp.home.join("recordings").exists(),
        "off mode must not create the recordings dir"
    );
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn missing_ffmpeg() {
    let Some(fix) = fix() else { return };
    let mut mcp = Mcp::spawn(
        "noff",
        &[
            ("BLADE_RECORD", "1"),
            ("BLADE_FFMPEG", "/nonexistent/ffmpeg"),
        ],
    );
    assert!(!mcp.nav(&format!("{fix}/a.html")).0);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"x"}));
    eprintln!("RESULT missing ffmpeg: {t}");
    assert!(e && t.contains("BLADE_FFMPEG"), "{t}");
    assert!(
        mcp.recordings().is_empty(),
        "no file: {:?}",
        mcp.recordings()
    );
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn pipe_refusal() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn(
        "pipe",
        &live_env(&[("BLADE_RECORD", "1"), ("BLADE_TRANSPORT", "pipe")], &ff),
    );
    let pid = mcp.pid();
    assert!(!mcp.nav(&format!("{fix}/a.html")).0);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"x"}));
    eprintln!("RESULT pipe refusal: {t}");
    assert!(e && t.to_lowercase().contains("pipe"), "{t}");
    assert!(ffmpeg_children(pid).is_empty(), "no ffmpeg child");
    assert!(
        mcp.recordings().is_empty(),
        "no file: {:?}",
        mcp.recordings()
    );
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn stdin_eof_finalizes() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("eof", &live_env(&[("BLADE_RECORD", "1")], &ff));
    assert!(!mcp.nav(&format!("{fix}/anim.html")).0);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"eof"}));
    assert!(!e, "{t}");
    std::thread::sleep(Duration::from_secs(4));
    mcp.close_stdin();
    assert!(
        mcp.wait_exit(Duration::from_secs(40)),
        "server must exit within 40 s of stdin EOF"
    );
    let files = mcp.recordings();
    assert_eq!(files.len(), 1, "exactly one file: {files:?}");
    let (codec, w, h) = probe(&files[0]).expect("ffprobe reads the EOF-finalized file");
    eprintln!(
        "RESULT eof finalize: {} {codec} {w}x{h}",
        files[0].display()
    );
    assert!(decodes_clean(&files[0]), "full decode must be clean");
}
