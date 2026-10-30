//! Bounded ffmpeg probe (`-version`, `-encoders`) and the once-per-process
//! discovery cache.
//!
//! D4-1: std, not tokio. The `OnceLock` initializer is sync and later callers
//! (the encoder, the recording hub) must be able to call it from any context.
//! A child plus a stdout-drain thread plus a 10 ms `try_wait` poll is bounded
//! and needs no runtime. Callers inside the daemon runtime should wrap the
//! first call in `spawn_blocking`.
//!
//! D4-5: argv only (no shell), stdin and stderr null, 5 s per call, output
//! capped at 1 MiB then drained to a sink so the child never blocks on a full
//! pipe; timeout kills the child.

use super::lookup::{self, EnvFn, Found, Lookup, TargetOs};
use super::parse::{parse_encoders, parse_version, EncoderSet, FfmpegVersion};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Per-call bound; same as doctor's Chrome `--version` check.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// Captured stdout cap; the rest is drained and discarded.
pub const MAX_PROBE_OUTPUT: usize = 1 << 20;
/// How long to wait for the drain thread after the child exited (a grandchild
/// may still hold the pipe).
const DRAIN_GRACE: Duration = Duration::from_secs(1);

/// What a successful probe learned.
#[derive(Debug, Clone)]
pub struct ProbeInfo {
    pub version: FfmpegVersion,
    pub encoders: EncoderSet,
}

/// Lookup result plus, when a binary was found, its probe.
#[derive(Debug, Clone)]
pub struct Discovery {
    pub lookup: Lookup,
    /// `Some` iff `lookup` is `Found`.
    pub probe: Option<Result<ProbeInfo, String>>,
}

fn fmt_timeout(t: Duration) -> String {
    if t < Duration::from_secs(1) {
        format!("{}ms", t.as_millis())
    } else {
        format!("{}s", t.as_secs())
    }
}

/// Run `path -hide_banner <flag>` and return (capped) stdout.
fn run_capture(path: &Path, flag: &str, timeout: Duration) -> Result<String, String> {
    let mut cmd = Command::new(path);
    cmd.args(["-hide_banner", flag])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flash when doctor/daemon has none.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", path.display()))?;
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("cannot read output of {}", path.display()));
    };
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut stdout)
            .take(MAX_PROBE_OUTPUT as u64)
            .read_to_end(&mut buf);
        // Keep reading so the child never blocks on a full pipe.
        let _ = std::io::copy(&mut stdout, &mut std::io::sink());
        let _ = tx.send(buf);
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("cannot wait for {}: {e}", path.display()));
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("timed out after {}", fmt_timeout(timeout)));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let buf = rx
        .recv_timeout(DRAIN_GRACE)
        .map_err(|_| "output pipe stayed open after exit".to_string())?;
    if !status.success() {
        return Err(format!("exited with {status}"));
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Probe `path`: version first (must look like ffmpeg), then encoders.
pub fn probe(path: &Path, timeout: Duration) -> Result<ProbeInfo, String> {
    let vtext = run_capture(path, "-version", timeout)?;
    let Some(version) = parse_version(&vtext) else {
        let first = vtext.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        return Err(format!(
            "does not look like ffmpeg (first line: {})",
            crate::platform::truncate_utf8(first.trim(), 80)
        ));
    };
    let etext = run_capture(path, "-encoders", timeout)?;
    Ok(ProbeInfo {
        version,
        encoders: parse_encoders(&etext),
    })
}

/// Look up and, when found, probe.
pub fn discover(env: EnvFn, known: &[PathBuf], os: TargetOs, timeout: Duration) -> Discovery {
    let lookup = lookup::find_ffmpeg(env, known, os);
    let probe = match &lookup {
        Lookup::Found(Found { path, .. }) => Some(self::probe(path, timeout)),
        _ => None,
    };
    Discovery { lookup, probe }
}

static DISCOVERY: OnceLock<Discovery> = OnceLock::new();

/// Discovery against the real env, run once per process. A changed
/// BLADE_FFMPEG needs a restart (daemon: `bladebro stop`).
pub fn discover_cached() -> &'static Discovery {
    DISCOVERY.get_or_init(|| {
        let os = TargetOs::current();
        let env = lookup::process_env;
        let known = lookup::known_locations(os, &crate::platform::home_dir(), &env);
        discover(&env, &known, os, PROBE_TIMEOUT)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::ffmpeg::lookup::Source;

    #[test]
    fn missing_binary_is_cannot_run() {
        let e = probe(
            Path::new("/nonexistent-bladebro-dir/ffmpeg"),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(e.contains("cannot run"), "{e}");
    }

    #[test]
    fn real_ffmpeg_probes_when_installed() {
        let p = crate::platform::home_dir().join(".local/ffmpeg/bin/ffmpeg");
        if !p.is_file() {
            eprintln!("skip: no {p:?}");
            return;
        }
        let info = probe(&p, PROBE_TIMEOUT).unwrap();
        assert_ne!(info.version, FfmpegVersion::Unknown);
        assert!(info.encoders.listed);
    }

    #[test]
    fn discover_cached_is_cached() {
        let a = discover_cached();
        let b = discover_cached();
        assert!(std::ptr::eq(a, b));
    }

    #[cfg(unix)]
    mod unix {
        use super::*;
        use std::collections::HashMap;
        use std::ffi::OsString;
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicU32, Ordering};

        static NEXT: AtomicU32 = AtomicU32::new(0);

        const VERSION: &str = "ffmpeg version n9.0.2 Copyright (c) 2000-2026 the FFmpeg developers";
        const ENCODERS: &str = "Encoders:\n V..... = Video\n ------\n V..... libx264 x\n V..... libvpx-vp9 y\n A....D aac z\n";

        struct Scratch(PathBuf);

        impl Scratch {
            fn new() -> Self {
                let p = std::env::temp_dir().join(format!(
                    "bladebro-ffmpeg-probe-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::SeqCst)
                ));
                std::fs::create_dir_all(&p).unwrap();
                Self(p)
            }

            fn script(&self, name: &str, body: &str) -> PathBuf {
                let p = self.0.join(name);
                std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
                p
            }
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        /// Retry on ETXTBSY: a concurrent test's fork can briefly hold the
        /// just-written script open for writing (Linux).
        fn spawn_retry(p: &Path, t: Duration) -> Result<ProbeInfo, String> {
            let mut last = Err(String::new());
            for _ in 0..5 {
                last = probe(p, t);
                match &last {
                    Err(e) if e.contains("Text file busy") => {
                        std::thread::sleep(Duration::from_millis(50))
                    }
                    _ => break,
                }
            }
            last
        }

        fn happy_script(r: &Scratch) -> PathBuf {
            r.script(
                "ffmpeg",
                &format!(
                    "[ \"$1\" = -hide_banner ] || exit 3\ncase \"$2\" in\n-version) cat <<'EOF'\n{VERSION}\nEOF\n;;\n-encoders) cat <<'EOF'\n{ENCODERS}EOF\n;;\nesac"
                ),
            )
        }

        #[test]
        fn probe_parses_version_and_encoders() {
            let r = Scratch::new();
            let p = happy_script(&r);
            let info = spawn_retry(&p, PROBE_TIMEOUT).unwrap();
            assert_eq!(info.version.display(), "n9.0.2");
            assert!(info.encoders.has("libvpx-vp9") && info.encoders.has("libx264"));
            assert!(!info.encoders.has("aac"));
        }

        #[test]
        fn timeout_kills_the_child() {
            let r = Scratch::new();
            let p = r.script("ffmpeg", "exec sleep 30");
            let t0 = Instant::now();
            let e = spawn_retry(&p, Duration::from_millis(300)).unwrap_err();
            assert!(e.contains("timed out"), "{e}");
            assert!(t0.elapsed() < Duration::from_secs(3), "{:?}", t0.elapsed());
        }

        #[test]
        fn nonzero_exit_is_reported() {
            let r = Scratch::new();
            let p = r.script("ffmpeg", "exit 7");
            let e = spawn_retry(&p, PROBE_TIMEOUT).unwrap_err();
            assert!(e.contains("exited") && e.contains('7'), "{e}");
        }

        #[test]
        fn non_ffmpeg_output_is_rejected() {
            let r = Scratch::new();
            let p = r.script("ffmpeg", "echo hello");
            let e = spawn_retry(&p, PROBE_TIMEOUT).unwrap_err();
            assert!(
                e.contains("does not look like ffmpeg") && e.contains("hello"),
                "{e}"
            );
        }

        #[test]
        fn huge_output_does_not_deadlock_and_is_capped() {
            let r = Scratch::new();
            let p = r.script(
                "ffmpeg",
                &format!("echo '{VERSION}'\nhead -c 3000000 /dev/zero | tr '\\0' 'x'"),
            );
            let t0 = Instant::now();
            let out = loop {
                match run_capture(&p, "-version", Duration::from_secs(10)) {
                    Err(e) if e.contains("Text file busy") => {
                        std::thread::sleep(Duration::from_millis(50))
                    }
                    other => break other,
                }
            };
            let out = out.unwrap();
            assert!(out.len() <= MAX_PROBE_OUTPUT, "{}", out.len());
            assert!(out.starts_with("ffmpeg version"));
            assert!(t0.elapsed() < Duration::from_secs(10));
        }

        #[test]
        fn grandchild_holding_the_pipe_is_bounded() {
            let r = Scratch::new();
            let p = r.script("ffmpeg", "sleep 3 &\necho 'ffmpeg version 7.1'");
            let t0 = Instant::now();
            let _ = spawn_retry(&p, PROBE_TIMEOUT);
            assert!(t0.elapsed() < Duration::from_secs(4), "{:?}", t0.elapsed());
        }

        #[test]
        fn discover_probes_only_a_found_binary() {
            let r = Scratch::new();
            let p = happy_script(&r);
            let mk = |pairs: Vec<(&'static str, String)>| {
                let m: HashMap<&'static str, OsString> = pairs
                    .into_iter()
                    .map(|(k, v)| (k, OsString::from(v)))
                    .collect();
                move |k: &str| m.get(k).cloned()
            };
            let env = mk(vec![("BLADE_FFMPEG", p.to_string_lossy().into_owned())]);
            let mut d = discover(&env, &[], TargetOs::Linux, PROBE_TIMEOUT);
            for _ in 0..5 {
                if !matches!(&d.probe, Some(Err(e)) if e.contains("Text file busy")) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
                d = discover(&env, &[], TargetOs::Linux, PROBE_TIMEOUT);
            }
            assert!(
                matches!(&d.lookup, Lookup::Found(f) if f.source == Source::Override && f.path == p)
            );
            assert!(matches!(d.probe, Some(Ok(_))), "{:?}", d.probe);

            let env = mk(vec![(
                "BLADE_FFMPEG",
                r.0.join("missing").to_string_lossy().into_owned(),
            )]);
            let d = discover(&env, &[], TargetOs::Linux, PROBE_TIMEOUT);
            assert!(matches!(d.lookup, Lookup::OverrideUnusable { .. }));
            assert!(d.probe.is_none());

            let env = mk(vec![]);
            let d = discover(&env, &[], TargetOs::Linux, PROBE_TIMEOUT);
            assert!(matches!(d.lookup, Lookup::NotFound));
            assert!(d.probe.is_none());
        }
    }
}
