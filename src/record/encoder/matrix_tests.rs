//! Unix fake-encoder outcome matrix (05-03, SAFE-04): one row per outcome
//! kind plus the stderr, shell-safety, idle-hold and drop-backstop checks.
//! Executed on macOS only; Linux and Windows are not run here.

use std::path::Path;
use std::time::{Duration, Instant};

use super::report::Outcome;
use super::test_util::{cfg_for, fake_ffmpeg, pid_gone, start_retry, wait_for_file, Scratch};
use crate::record::values::Format;

const FRAME: &[u8; 10] = b"JPEGDATA01";
const BOTH: &[&str] = &["libx264", "libvpx-vp9"];
const OK_BODY: &str = r#"for a; do out="$a"; done; cat > "$out""#;

fn cfg(script: &Path, queue: usize, timeout: Duration) -> super::EncoderConfig {
    cfg_for(fake_ffmpeg(script, BOTH), Format::WebM, 10, queue, timeout)
}

fn short() -> Duration {
    Duration::from_millis(500)
}

// a: pre-created empty output, exit 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_empty_precreated_output() {
    let s = Scratch::new();
    let script = s.script("fake-enc-a", "cat >/dev/null; exit 0");
    let out = s.0.join("rec.webm");
    std::fs::write(&out, b"").unwrap();
    let enc = start_retry(&cfg(&script, 8, Duration::from_secs(15)), &out).unwrap();
    let pid = enc.pid().unwrap();
    for i in 0..3u64 {
        enc.send_frame(FRAME.to_vec(), i * 100);
    }
    let r = enc.finish().await;
    assert_eq!(r.outcome, Outcome::EmptyOutput, "{}", r.verdict());
    assert_eq!(r.bytes, 0);
    assert!(r.verdict().contains("missing or empty"));
    assert!(pid_gone(pid));
}

// b: output never created.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_missing_output() {
    let s = Scratch::new();
    let script = s.script("fake-enc-b", "cat >/dev/null; exit 0");
    let out = s.0.join("rec.webm");
    let enc = start_retry(&cfg(&script, 8, Duration::from_secs(15)), &out).unwrap();
    let pid = enc.pid().unwrap();
    for i in 0..3u64 {
        enc.send_frame(FRAME.to_vec(), i * 100);
    }
    let r = enc.finish().await;
    assert_eq!(r.outcome, Outcome::EmptyOutput, "{}", r.verdict());
    assert!(pid_gone(pid));
}

// c: no frames sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_no_frames() {
    let s = Scratch::new();
    let script = s.script("fake-enc-c", OK_BODY);
    let out = s.0.join("rec.webm");
    let enc = start_retry(&cfg(&script, 8, Duration::from_secs(15)), &out).unwrap();
    let pid = enc.pid().unwrap();
    let r = enc.finish().await;
    assert_eq!(r.outcome, Outcome::NoFrames, "{}", r.verdict());
    assert_eq!(r.frames_written, 0);
    assert!(
        r.verdict().starts_with("recording failed: no frames"),
        "{}",
        r.verdict()
    );
    assert!(pid_gone(pid));
}

// d: encoder exits 0 without reading; the write error beats NoFrames.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d_exits_without_reading() {
    let s = Scratch::new();
    let script = s.script("fake-enc-d", "exit 0");
    let out = s.0.join("rec.webm");
    let enc = start_retry(&cfg(&script, 8, Duration::from_secs(15)), &out).unwrap();
    let pid = enc.pid().unwrap();
    // Wait until the child has exited (zombie or gone: its stdin is closed)
    // instead of a fixed sleep, which raced with sh start-up under load.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let o = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        let stat = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if stat.is_empty() || stat.starts_with('Z') {
            break;
        }
        assert!(Instant::now() < deadline, "child never exited: {stat:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    for i in 0..5u64 {
        enc.send_frame(FRAME.to_vec(), i * 100);
    }
    let r = enc.finish().await;
    assert!(
        matches!(r.outcome, Outcome::StoppedReading { .. }),
        "{:?} {}",
        r.outcome,
        r.verdict()
    );
    assert!(r.verdict().contains("stopped reading"), "{}", r.verdict());
    assert!(pid_gone(pid));
}

// e: ignores stdin EOF and keeps running.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e_ignores_eof_is_reaped() {
    let s = Scratch::new();
    let script = s.script("fake-enc-e", "cat >/dev/null; exec sleep 30");
    let out = s.0.join("rec.webm");
    let enc = start_retry(&cfg(&script, 8, short()), &out).unwrap();
    let pid = enc.pid().unwrap();
    for i in 0..3u64 {
        enc.send_frame(FRAME.to_vec(), i * 100);
    }
    let t = Instant::now();
    let r = enc.finish().await;
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    assert!(
        matches!(r.outcome, Outcome::Unfinalized { reaped: true, .. }),
        "{:?}",
        r.outcome
    );
    assert!(pid_gone(pid));
}

// f: never reads stdin; large frames block the writer (pipe is 64 KiB).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f_never_reads_large_frames() {
    let s = Scratch::new();
    let script = s.script("fake-enc-f", "exec sleep 30");
    let out = s.0.join("rec.webm");
    let enc = start_retry(&cfg(&script, 8, short()), &out).unwrap();
    let pid = enc.pid().unwrap();
    for i in 0..4u64 {
        enc.send_frame(vec![7u8; 256 * 1024], i * 100);
    }
    let t = Instant::now();
    let r = enc.finish().await;
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    assert!(
        matches!(r.outcome, Outcome::Unfinalized { reaped: true, .. }),
        "{:?}",
        r.outcome
    );
    assert!(pid_gone(pid));
}

// g: slow consumer; send_frame never blocks, every frame is counted once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_slow_consumer_drops() {
    let s = Scratch::new();
    let script = s.script("fake-enc-g", "exec sleep 30");
    let out = s.0.join("rec.webm");
    let enc = start_retry(&cfg(&script, 2, short()), &out).unwrap();
    let pid = enc.pid().unwrap();
    let t = Instant::now();
    for i in 0..100u64 {
        enc.send_frame(vec![7u8; 256 * 1024], i * 100);
    }
    assert!(t.elapsed() < Duration::from_secs(2), "send blocked");
    let live = enc.frames_dropped();
    let r = enc.finish().await;
    assert_eq!(r.frames_queued + r.frames_dropped, 100, "{r:?}");
    assert!(r.frames_dropped >= 90, "dropped {}", r.frames_dropped);
    assert_eq!(live, r.frames_dropped);
    assert!(
        matches!(r.outcome, Outcome::Unfinalized { reaped: true, .. }),
        "{:?}",
        r.outcome
    );
    assert!(pid_gone(pid));
}

// h: 100 KB of stderr must not deadlock; the kept tail is bounded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_stderr_flood_bounded_tail() {
    let s = Scratch::new();
    let script = s.script(
        "fake-enc-h",
        r#"for a; do out="$a"; done; head -c 100000 /dev/zero | tr '\0' e >&2; echo LAST-LINE >&2; cat > "$out""#,
    );
    let out = s.0.join("rec.webm");
    let enc = start_retry(&cfg(&script, 8, Duration::from_secs(15)), &out).unwrap();
    let pid = enc.pid().unwrap();
    for i in 0..3u64 {
        enc.send_frame(FRAME.to_vec(), i * 100);
    }
    let r = enc.finish().await;
    assert_eq!(r.outcome, Outcome::Finalized, "{}", r.verdict());
    assert!(r.stderr_tail.len() <= 4096, "{}", r.stderr_tail.len());
    assert!(r.stderr_tail.trim_end().ends_with("LAST-LINE"));
    assert!(pid_gone(pid));
}

// i: hostile output path; argv spawn means no shell ever interprets it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn i_hostile_output_path() {
    let s = Scratch::new();
    let script = s.script("fake-enc-i", OK_BODY);
    let dir = s.0.join("a b");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("rec;$(touch pwned)'\"\\|&.webm");
    let enc = start_retry(&cfg(&script, 8, Duration::from_secs(15)), &out).unwrap();
    let pid = enc.pid().unwrap();
    for i in 0..3u64 {
        enc.send_frame(FRAME.to_vec(), i * 100);
    }
    let r = enc.finish().await;
    assert_eq!(r.outcome, Outcome::Finalized, "{}", r.verdict());
    assert!(out.is_file(), "output not at the exact path");
    assert!(!s.0.join("pwned").exists() && !dir.join("pwned").exists());
    let cwd = std::env::current_dir().unwrap();
    assert!(!cwd.join("pwned").exists(), "a shell ran in the cwd");
    assert!(pid_gone(pid));
}

// j: the held frame is written while idle, not only at the stop (D5-1).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn j_idle_hold_flushes_before_finish() {
    let s = Scratch::new();
    let script = s.script("fake-enc-j", OK_BODY);
    let out = s.0.join("rec.webm");
    let enc = start_retry(&cfg(&script, 8, Duration::from_secs(15)), &out).unwrap();
    let pid = enc.pid().unwrap();
    // The fake's shell must be up (it creates the output) before the idle
    // clock starts; fresh-script exec can take seconds under load.
    wait_for_file(&out).await;
    enc.send_frame(FRAME.to_vec(), 0);
    tokio::time::sleep(Duration::from_millis(2500)).await;
    // Bounded poll: still strictly before finish, but tolerant of a loaded
    // scheduler delaying the fake's read of the pipe.
    let poll_end = Instant::now() + Duration::from_secs(10);
    let mut before = 0;
    while before < 150 && Instant::now() < poll_end {
        before = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(before >= 150, "file only {before} bytes before finish");
    let r = enc.finish().await;
    assert_eq!(r.outcome, Outcome::Finalized, "{}", r.verdict());
    assert!(
        (24..=400).contains(&r.frames_written),
        "{}",
        r.frames_written
    );
    assert!(pid_gone(pid));
}

// k: dropping without finish; kill_on_drop stops it (tokio reaps dropped
// children best-effort, so a zombie is accepted; Phase 9 owns orphan reaping).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn k_drop_without_finish_stops_child() {
    let s = Scratch::new();
    let script = s.script("fake-enc-k", "exec sleep 30");
    let out = s.0.join("rec.webm");
    let enc = start_retry(&cfg(&script, 8, short()), &out).unwrap();
    let pid = enc.pid().unwrap();
    drop(enc);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let o = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        let stat = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if stat.is_empty() || stat.starts_with('Z') {
            break;
        }
        assert!(Instant::now() < deadline, "child still running: {stat:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
