//! Unix lifecycle tests against fake `fake-enc-*` shell encoders (no real
//! ffmpeg, no browser). Executed on macOS only; Linux/Windows not run here.

use std::path::Path;
use std::time::{Duration, Instant};

use super::report::{EncoderError, Outcome};
use super::test_util::{cfg_for, fake_ffmpeg, pid_gone, start_retry, wait_for_file, Scratch};
use crate::record::values::Format;

const FRAME: &[u8; 10] = b"JPEGDATA01";
const BOTH: &[&str] = &["libx264", "libvpx-vp9"];

fn webm_cfg(script: &Path, queue: usize, timeout: Duration) -> super::EncoderConfig {
    cfg_for(fake_ffmpeg(script, BOTH), Format::WebM, 10, queue, timeout)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn success_finalizes_and_reaps() {
    let s = Scratch::new();
    let script = s.script(
        "fake-enc-ok",
        r#"for a; do out="$a"; done; printf '%s\n' "$@" > "$out.args"; cat > "$out""#,
    );
    let out = s.0.join("rec.webm");
    let enc = start_retry(&webm_cfg(&script, 64, Duration::from_secs(15)), &out).unwrap();
    let pid = enc.pid().unwrap();
    for i in 0..=10u64 {
        assert!(enc.send_frame(FRAME.to_vec(), i * 100));
    }
    let r = enc.finish().await;
    assert_eq!(r.outcome, Outcome::Finalized, "{}", r.verdict());
    assert!(r.ok());
    assert_eq!(r.frames_queued, 11);
    assert_eq!(r.frames_dropped, 0);
    assert!(
        (11..=13).contains(&r.frames_written),
        "{}",
        r.frames_written
    );
    assert_eq!(r.bytes, r.frames_written * 10);
    assert!(pid_gone(pid));
    let args = std::fs::read_to_string(s.0.join("rec.webm.args")).unwrap();
    assert_eq!(args.lines().next(), Some("-hide_banner"));
    assert_eq!(args.lines().last(), Some(out.to_str().unwrap()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_zero_exit_reports_stderr() {
    let s = Scratch::new();
    let script = s.script(
        "fake-enc-fail",
        r#"cat >/dev/null; echo "Unknown encoder 'libvpx-vp9'" >&2; exit 1"#,
    );
    let out = s.0.join("rec.webm");
    let enc = start_retry(&webm_cfg(&script, 8, Duration::from_secs(15)), &out).unwrap();
    let pid = enc.pid().unwrap();
    enc.send_frame(FRAME.to_vec(), 0);
    let r = enc.finish().await;
    assert_eq!(
        r.outcome,
        Outcome::ExitFailed {
            status: "exit code 1".into()
        }
    );
    assert!(r.stderr_tail.contains("Unknown encoder"));
    let v = r.verdict();
    assert!(
        v.contains("Unknown encoder") && v.contains("exit code 1"),
        "{v}"
    );
    assert!(pid_gone(pid));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hang_is_killed_and_reaped() {
    let s = Scratch::new();
    let script = s.script("fake-enc-hang", "exec sleep 30");
    let out = s.0.join("rec.webm");
    let enc = start_retry(&webm_cfg(&script, 8, Duration::from_millis(500)), &out).unwrap();
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
    assert!(r.verdict().starts_with("recording unfinalized"));
    assert!(r.finalize_ms < 15_000, "{}", r.finalize_ms);
    assert!(pid_gone(pid));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stuck_reader_is_aborted_then_killed() {
    // Never reads stdin: big frames fill the pipe, the writer blocks in
    // write_all; finish must abort it, kill the child and reap.
    let s = Scratch::new();
    let script = s.script("fake-enc-stuck", "exec sleep 30");
    let out = s.0.join("rec.webm");
    let enc = start_retry(&webm_cfg(&script, 8, Duration::from_millis(500)), &out).unwrap();
    let pid = enc.pid().unwrap();
    for i in 0..4u64 {
        enc.send_frame(vec![7u8; 1 << 20], i * 100);
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn early_exit_reports_stopped_reading_or_failure() {
    let s = Scratch::new();
    let script = s.script("fake-enc-early", "exit 0");
    let out = s.0.join("rec.webm");
    let enc = start_retry(&webm_cfg(&script, 8, Duration::from_secs(15)), &out).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    for i in 0..30u64 {
        enc.send_frame(vec![1u8; 1 << 16], i * 100);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let r = enc.finish().await;
    assert!(!r.ok(), "{}", r.verdict());
    assert!(
        matches!(
            r.outcome,
            Outcome::StoppedReading { .. } | Outcome::NoFrames
        ),
        "{:?}",
        r.outcome
    );
    assert!(r.frames_dropped > 0 || r.frames_queued > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_queue_drops_without_blocking() {
    let s = Scratch::new();
    let script = s.script("fake-enc-slow", "exec sleep 30");
    let out = s.0.join("rec.webm");
    let enc = start_retry(&webm_cfg(&script, 1, Duration::from_millis(300)), &out).unwrap();
    let t = Instant::now();
    for i in 0..200u64 {
        enc.send_frame(vec![9u8; 1 << 20], i);
    }
    assert!(t.elapsed() < Duration::from_secs(2));
    assert!(enc.frames_dropped() > 0);
    let pid = enc.pid().unwrap();
    let r = enc.finish().await;
    assert!(r.frames_dropped > 0);
    assert!(pid_gone(pid));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_binary_is_a_spawn_error() {
    let s = Scratch::new();
    let bin = s.0.join("no-such-ffmpeg");
    let cfg = webm_cfg(&bin, 4, Duration::from_secs(1));
    match start_retry(&cfg, &s.0.join("a.webm")) {
        Err(e @ EncoderError::Spawn { .. }) => {
            let t = e.to_string();
            assert!(
                t.contains("cannot run ffmpeg at") && t.contains("no-such-ffmpeg"),
                "{t}"
            );
        }
        Err(e) => panic!("wrong error {e:?}"),
        Ok(_) => panic!("spawned a missing binary"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_encoder_never_spawns() {
    let s = Scratch::new();
    let script = s.script("fake-enc-marker", r#"touch "$0.ran"; cat >/dev/null"#);
    let cfg = cfg_for(
        fake_ffmpeg(&script, &["libvpx-vp9"]),
        Format::Mp4,
        10,
        4,
        Duration::from_secs(1),
    );
    match start_retry(&cfg, &s.0.join("a.mp4")) {
        Err(EncoderError::EncoderMissing { alt, .. }) => assert_eq!(alt, Some(Format::WebM)),
        Err(e) => panic!("wrong error {e:?}"),
        Ok(_) => panic!("started"),
    }
    assert!(!s.0.join("fake-enc-marker.ran").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_output_path_never_spawns() {
    let s = Scratch::new();
    let script = s.script("fake-enc-marker2", r#"touch "$0.ran"; cat >/dev/null"#);
    match start_retry(
        &webm_cfg(&script, 4, Duration::from_secs(1)),
        Path::new("relative.webm"),
    ) {
        Err(EncoderError::BadOutputPath { .. }) => {}
        Err(e) => panic!("wrong error {e:?}"),
        Ok(_) => panic!("started"),
    }
    assert!(!s.0.join("fake-enc-marker2.ran").exists());
}

// T5: the writer is aborted at the stop deadline (pending repeats of huge
// holds). ffmpeg still sees EOF and exits 0, so the report must not claim a
// clean, complete recording.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn writer_abort_is_reported_as_truncated() {
    let s = Scratch::new();
    // Starts reading late, so the writer is blocked on a full pipe at the
    // stop deadline, then drains and exits 0 after EOF.
    let script = s.script(
        "fake-enc-late",
        r#"for a; do out="$a"; done; touch "$0.up"; sleep 1; cat > "$out""#,
    );
    let out = s.0.join("rec.webm");
    let enc = start_retry(&webm_cfg(&script, 8, Duration::from_millis(500)), &out).unwrap();
    let pid = enc.pid().unwrap();
    wait_for_file(&s.0.join("fake-enc-late.up")).await;
    // Frames 600 s apart: each hold is clamped to 600 s = 6000 writes of 64 KiB.
    for i in 0..3u64 {
        assert!(enc.send_frame(vec![5u8; 64 * 1024], i * 600_000));
    }
    let r = enc.finish().await;
    assert!(!r.ok(), "{}", r.verdict());
    assert_eq!(r.frames_queued, 3);
    assert!(r.frames_unwritten >= 1, "{r:?}");
    assert!(
        matches!(r.outcome, Outcome::Truncated { unwritten } if unwritten == r.frames_unwritten),
        "{:?} {}",
        r.outcome,
        r.verdict()
    );
    let v = r.verdict();
    assert!(v.contains("queued frames were not written"), "{v}");
    assert!(!v.contains("0 dropped"), "{v}");
    assert!(pid_gone(pid));
}

/// The last frame is held to `mark_end`, not to a later `finish` (06-04 gap 3).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mark_end_stops_the_final_hold_before_finish() {
    let s = Scratch::new();
    let script = s.script("fake-enc-mark", r#"for a; do out="$a"; done; cat > "$out""#);
    let run = |mark: bool, name: &'static str| {
        let (script, dir) = (script.clone(), s.0.clone());
        async move {
            let out = dir.join(name);
            let enc = start_retry(&webm_cfg(&script, 8, Duration::from_secs(15)), &out).unwrap();
            assert!(enc.send_frame(FRAME.to_vec(), 0));
            if mark {
                enc.mark_end();
            }
            // Stands in for the source's stop grace between stop and finish.
            tokio::time::sleep(Duration::from_millis(700)).await;
            enc.finish().await.frames_written
        }
    };
    let marked = run(true, "m.webm").await;
    let unmarked = run(false, "u.webm").await;
    assert_eq!(marked, 1, "hold ends at the mark");
    assert!(
        unmarked >= 6,
        "without a mark the hold runs to finish: {unmarked}"
    );
}
