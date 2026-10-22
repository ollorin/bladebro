//! WRITTEN, NOT COMPILED OR RUN HERE (macOS-only environment). First compile
//! and run is the windows-latest CI job; until then LIFE-05 / SC4 is
//! "written, not executed".
//!
//! Fakes are `.cmd` batch files that ignore their arguments (batch splits
//! `%*` on `,` and `=`, which the -vf argument contains) and bake the output
//! path into the script text instead.
//!
//! There is deliberately no hang/kill row on Windows: a batch file can only
//! hang via a child exe (`timeout.exe`, `ping.exe`), and killing cmd.exe would
//! orphan that child and leak a process.
//!
//! Remaining uncertainty (uncertain, not run): Rust (>= 1.77.2) escapes
//! batch-file arguments and errors only on ones it cannot represent (NUL,
//! newlines), so the vf argument `(ow-iw)/2` is probably quoted, not refused;
//! this has never been compiled or executed. If CI shows a Spawn error, the
//! fix is a non-batch fake (e.g. a compiled test helper), not a weaker
//! assertion.

use std::time::Duration;

use super::report::{EncoderError, Outcome};
use super::test_util::{cfg_for, fake_ffmpeg, Scratch};
use super::Encoder;
use crate::record::values::Format;

const FRAME: &[u8; 10] = b"JPEGDATA01";
const BOTH: &[&str] = &["libx264", "libvpx-vp9"];

fn cfg(script: &std::path::Path) -> super::EncoderConfig {
    cfg_for(
        fake_ffmpeg(script, BOTH),
        Format::WebM,
        10,
        8,
        Duration::from_secs(10),
    )
}

async fn run(script: &std::path::Path, out: &std::path::Path) -> super::EncoderReport {
    let enc = Encoder::start(&cfg(script), out).unwrap();
    for i in 0..3u64 {
        enc.send_frame(FRAME.to_vec(), i * 100);
    }
    enc.finish().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w1_drains_stdin_and_writes_output() {
    let s = Scratch::new();
    let out = s.0.join("rec.webm");
    let body = format!("more > \"{}\"\nexit /b 0", out.display());
    let script = s.cmd("fake-enc-w1", &body);
    let r = run(&script, &out).await;
    assert_eq!(r.outcome, Outcome::Finalized, "{}", r.verdict());
    assert!(r.bytes > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w2_nonzero_exit_with_stderr() {
    let s = Scratch::new();
    let out = s.0.join("rec.webm");
    let script = s.cmd(
        "fake-enc-w2",
        "more > nul\necho fake failure 1>&2\nexit /b 3",
    );
    let r = run(&script, &out).await;
    assert_eq!(
        r.outcome,
        Outcome::ExitFailed {
            status: "exit code 3".into()
        },
        "{}",
        r.verdict()
    );
    assert!(r.stderr_tail.contains("fake failure"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w3_exit_zero_with_empty_output() {
    let s = Scratch::new();
    let out = s.0.join("rec.webm");
    std::fs::write(&out, b"").unwrap();
    let script = s.cmd("fake-enc-w3", "more > nul\nexit /b 0");
    let r = run(&script, &out).await;
    assert_eq!(r.outcome, Outcome::EmptyOutput, "{}", r.verdict());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_missing_binary_is_a_spawn_error() {
    let s = Scratch::new();
    let out = s.0.join("rec.webm");
    // Not a batch path: Rust does not check that a .cmd/.bat exists (cmd.exe
    // runs it and fails later), while CreateProcessW rejects a missing .exe.
    let script = s.0.join("no-such-ffmpeg.exe");
    let e = Encoder::start(&cfg(&script), &out).err().unwrap();
    assert!(matches!(e, EncoderError::Spawn { .. }), "{e}");
}
