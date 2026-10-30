//! Poster frame for a finished recording (OUT-06, D10-1): the last frame of the
//! video as a PNG, made by one short ffmpeg run on the finished file. No CDP
//! call; the recorder's method set is unchanged.
//!
//! Why the finished file and not the last screencast JPEG: the file is what a
//! reviewer will open, so the poster proves it decodes and shows what it shows.
//!
//! Process rules: argv only (never a shell), stdin null, own process group on
//! unix, CREATE_NO_WINDOW on Windows, `kill_on_drop`, one total time budget.
//! Success = exit 0 AND the output starts with the PNG magic. Any failure
//! removes the poster file; the caller reports it and the stop never fails.
//!
//! Module map:
//! - `poster_args`: pure argv builder (tail seek first, plain retry second).
//! - `is_png`: 8-byte magic check.
//! - `make_poster` / `make_poster_with`: the bounded run and its cleanup.
//!
//! Fork-only module (SYNC-01).

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use super::encoder::ResolvedFfmpeg;
use super::marker::clean_text;
use super::sidecar::create_sidecar;

/// Total budget for the poster (both attempts). Typical runs take 0.1-0.3 s.
pub const POSTER_TIMEOUT: Duration = Duration::from_secs(3);
/// File suffix next to the video: `<video stem>.poster.png`.
pub const POSTER_SUFFIX: &str = ".poster.png";

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
/// Longest stderr line quoted in a reason.
const REASON_LINE_CHARS: usize = 80;

/// argv for one poster run. `tail_seek` decodes only the last second
/// (`-sseof -1`); the plain form decodes from the start and keeps the last
/// frame. `-update 1` keeps overwriting the one PNG.
pub fn poster_args(video: &Path, out: &Path, tail_seek: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "-nostdin".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
    ];
    if tail_seek {
        args.push("-sseof".into());
        args.push("-1".into());
    }
    args.push("-i".into());
    args.push(video.into());
    for a in [
        "-map", "0:v:0", "-an", "-update", "1", "-f", "image2", "-c:v", "png", "-y",
    ] {
        args.push(a.into());
    }
    args.push(out.into());
    args
}

/// True when `b` starts with the 8-byte PNG signature.
pub fn is_png(b: &[u8]) -> bool {
    b.starts_with(&PNG_MAGIC)
}

/// Why one attempt failed. A timeout is never retried (the budget is spent).
enum Miss {
    Timeout,
    Failed(String),
}

/// Budget as text for a reason: "3 s", or "300 ms" below one second.
fn budget_text(d: Duration) -> String {
    if d.subsec_millis() == 0 {
        format!("{} s", d.as_secs())
    } else {
        format!("{} ms", d.as_millis())
    }
}

/// Make `<video stem>.poster.png` next to `video` within `POSTER_TIMEOUT`.
pub async fn make_poster(ff: &ResolvedFfmpeg, video: &Path) -> Result<PathBuf, String> {
    make_poster_with(ff, video, POSTER_TIMEOUT).await
}

/// `make_poster` with an explicit budget (tests use a short one).
pub async fn make_poster_with(
    ff: &ResolvedFfmpeg,
    video: &Path,
    budget: Duration,
) -> Result<PathBuf, String> {
    if !ff.encoders.has("png") {
        return Err("ffmpeg lists no png encoder".to_string());
    }
    let (file, out) = create_sidecar(video, POSTER_SUFFIX)?;
    drop(file);
    let deadline = Instant::now() + budget;
    match run_with_retry(ff, video, &out, deadline, budget).await {
        Ok(()) => Ok(out),
        Err(reason) => {
            let _ = std::fs::remove_file(&out);
            Err(reason)
        }
    }
}

async fn run_with_retry(
    ff: &ResolvedFfmpeg,
    video: &Path,
    out: &Path,
    deadline: Instant,
    budget: Duration,
) -> Result<(), String> {
    let mut last = attempt(ff, video, out, true, deadline).await;
    if let Err(Miss::Failed(_)) = last {
        if Instant::now() < deadline {
            last = attempt(ff, video, out, false, deadline).await;
        }
    }
    last.map_err(|m| match m {
        Miss::Timeout => format!("timed out after {}", budget_text(budget)),
        Miss::Failed(reason) => reason,
    })
}

async fn attempt(
    ff: &ResolvedFfmpeg,
    video: &Path,
    out: &Path,
    tail_seek: bool,
    deadline: Instant,
) -> Result<(), Miss> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(Miss::Timeout);
    }
    let mut cmd = tokio::process::Command::new(&ff.path);
    cmd.args(poster_args(video, out, tail_seek))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Same reasons as the encoder: a terminal Ctrl-C must not reach ffmpeg
    // directly, and no console window may flash on Windows.
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let child = cmd
        .spawn()
        .map_err(|e| Miss::Failed(format!("cannot start ffmpeg: {e}")))?;
    // Dropping the wait future on timeout drops the child, and kill_on_drop
    // kills and reaps it.
    let run = match tokio::time::timeout(left, child.wait_with_output()).await {
        Ok(r) => r.map_err(|e| Miss::Failed(format!("ffmpeg wait failed: {e}")))?,
        Err(_) => return Err(Miss::Timeout),
    };
    if !run.status.success() {
        let head = match run.status.code() {
            Some(c) => format!("ffmpeg exited {c}"),
            None => "ffmpeg was killed by a signal".to_string(),
        };
        let line = String::from_utf8_lossy(&run.stderr)
            .lines()
            .rev()
            .map(|l| clean_text(l, REASON_LINE_CHARS))
            .find(|l| !l.is_empty());
        return Err(Miss::Failed(match line {
            Some(l) => format!("{head}: {l}"),
            None => head,
        }));
    }
    check_png(out).map_err(Miss::Failed)
}

/// The file must start with the PNG signature (exit 0 alone is not enough).
fn check_png(out: &Path) -> Result<(), String> {
    let mut head = [0u8; 8];
    let got = std::fs::File::open(out).and_then(|mut f| f.read_exact(&mut head));
    match got {
        Ok(()) if is_png(&head) => Ok(()),
        _ => Err("output is not a PNG".to_string()),
    }
}
