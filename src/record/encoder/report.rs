//! Encoder error and outcome types, outcome classification and the honest
//! verdict strings (SAFE-04). Pure: no process, no IO.
//!
//! The verdict text is product surface: an LLM reads it mid-task, so each
//! failure names its cause and the recovery path, and nothing says "finalized"
//! unless exit 0 + frames + a non-empty file were all proven.

use std::fmt;
use std::path::PathBuf;

use crate::record::values::Format;

/// How much of the ffmpeg stderr tail a verdict carries.
pub const VERDICT_TAIL_CHARS: usize = 300;

/// Why recording could not start (no process produced a recording).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncoderError {
    /// No ffmpeg anywhere; `hint` is the per-OS install hint.
    FfmpegNotFound { hint: &'static str },
    /// The override env var points at something unusable.
    OverrideUnusable { value: String, reason: String },
    /// ffmpeg was found but did not run properly.
    FfmpegUnusable { path: String, reason: String },
    /// ffmpeg lacks the encoder the format needs; `alt` is a format it can do.
    EncoderMissing {
        path: String,
        format: Format,
        alt: Option<Format>,
    },
    /// Output path rejected before any process exists.
    BadOutputPath { path: String, reason: String },
    /// ffmpeg could not be spawned.
    Spawn { path: String, error: String },
}

fn fmt_name(f: Format) -> &'static str {
    match f {
        Format::WebM => "WebM",
        Format::Mp4 => "MP4",
    }
}

impl fmt::Display for EncoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FfmpegNotFound { hint } => write!(
                f,
                "recording unavailable: ffmpeg not found (BLADE_FFMPEG unset; not on PATH or in known locations). Install it ({hint}) or set BLADE_FFMPEG=/path/to/ffmpeg, then run bladebro stop so a running daemon looks again. Browsing is unaffected."
            ),
            Self::OverrideUnusable { value, reason } => write!(
                f,
                "recording unavailable: BLADE_FFMPEG={value} is unusable ({reason}); PATH was not searched. Fix or unset BLADE_FFMPEG. Browsing is unaffected."
            ),
            Self::FfmpegUnusable { path, reason } => write!(
                f,
                "recording unavailable: ffmpeg at {path} is unusable: {reason}. Point BLADE_FFMPEG at a working ffmpeg (bladebro doctor shows what was found). Browsing is unaffected."
            ),
            Self::EncoderMissing { path, format, alt } => {
                let enc = format.encoder();
                write!(
                    f,
                    "recording unavailable: {path} has no {enc} encoder, which {} recording needs. Install an ffmpeg built with {enc} or point BLADE_FFMPEG at one",
                    fmt_name(*format)
                )?;
                if let Some(a) = alt {
                    write!(
                        f,
                        "; or record {ext} instead (BLADE_RECORD_FORMAT={ext} or per-call format)",
                        ext = a.ext()
                    )?;
                }
                write!(f, ". Browsing is unaffected.")
            }
            Self::BadOutputPath { path, reason } => {
                write!(
                    f,
                    "recording not started: output path {path:?} rejected: {reason}"
                )
            }
            Self::Spawn { path, error } => write!(
                f,
                "recording not started: cannot run ffmpeg at {path}: {error}. Check it is a working ffmpeg (bladebro doctor). Browsing is unaffected."
            ),
        }
    }
}

impl std::error::Error for EncoderError {}

/// How the ffmpeg process ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitInfo {
    Code(i32),
    Signal(String),
    WaitFailed(String),
    /// We killed it after the bounded wait; `reaped` = wait after kill succeeded.
    Killed {
        reaped: bool,
    },
}

/// The single classification of a finished recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Finalized,
    NoFrames,
    ExitFailed { status: String },
    StoppedReading { error: String },
    EmptyOutput,
    Unfinalized { waited_ms: u64, reaped: bool },
}

/// Everything known about a finished encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncoderReport {
    pub outcome: Outcome,
    pub path: PathBuf,
    pub bytes: u64,
    pub frames_queued: u64,
    pub frames_dropped: u64,
    pub frames_written: u64,
    pub gaps_clamped: u64,
    pub pid: Option<u32>,
    pub finalize_ms: u64,
    pub stderr_tail: String,
}

impl EncoderReport {
    /// True only for a proven, finalized recording.
    pub fn ok(&self) -> bool {
        self.outcome == Outcome::Finalized
    }

    /// The honest one-line verdict for this report.
    pub fn verdict(&self) -> String {
        let path = self.path.display();
        let tail = tail_for_verdict(&self.stderr_tail);
        let written = self.frames_written;
        match &self.outcome {
            Outcome::Finalized => format!(
                "recording finalized: {path} ({} bytes; {written} frames written, {} dropped at the queue)",
                self.bytes, self.frames_dropped
            ),
            Outcome::NoFrames => format!(
                "recording failed: no frames reached the encoder, so there is no video; {path} left in place"
            ),
            Outcome::ExitFailed { status } => format!(
                "recording failed: ffmpeg exited with {status} after {written} frames; {path} is not a valid recording. ffmpeg said: {tail}"
            ),
            Outcome::StoppedReading { error } => format!(
                "recording failed: ffmpeg stopped reading frames early ({error}) after {written} frames; {path} may be incomplete. ffmpeg said: {tail}"
            ),
            Outcome::EmptyOutput => format!(
                "recording failed: ffmpeg exited 0 but {path} is missing or empty"
            ),
            Outcome::Unfinalized { waited_ms, reaped } => {
                let mut s = format!(
                    "recording unfinalized: ffmpeg did not finish within {}s after its input closed and was killed; {path} may be truncated or unplayable (kept). ffmpeg said: {tail}",
                    waited_ms / 1000
                );
                if !reaped {
                    let pid = self
                        .pid
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "?".to_string());
                    s.push_str(&format!(" (process {pid} could not be reaped)"));
                }
                s
            }
        }
    }
}

/// Map the facts of a finished encode to exactly one outcome (D5-8,
/// first match wins).
pub fn classify(
    exit: &ExitInfo,
    frames_written: u64,
    write_error: Option<&str>,
    bytes: Option<u64>,
    waited_ms: u64,
) -> Outcome {
    if let ExitInfo::Killed { reaped } = exit {
        return Outcome::Unfinalized {
            waited_ms,
            reaped: *reaped,
        };
    }
    // ffmpeg exits 234 with zero frames; "no frames" is the honest cause.
    if frames_written == 0 && write_error.is_none() {
        return Outcome::NoFrames;
    }
    if *exit != ExitInfo::Code(0) {
        let status = match exit {
            ExitInfo::Code(c) => format!("exit code {c}"),
            ExitInfo::Signal(s) => s.clone(),
            ExitInfo::WaitFailed(e) => format!("unknown (wait failed: {e})"),
            ExitInfo::Killed { .. } => String::new(),
        };
        return Outcome::ExitFailed { status };
    }
    if let Some(e) = write_error {
        return Outcome::StoppedReading {
            error: e.to_string(),
        };
    }
    match bytes {
        None | Some(0) => Outcome::EmptyOutput,
        Some(_) => Outcome::Finalized,
    }
}

/// Last 3 non-empty lines joined " | ", capped to the last
/// `VERDICT_TAIL_CHARS` chars ("..." prefix when cut); "nothing" if empty.
pub fn tail_for_verdict(tail: &str) -> String {
    let lines: Vec<&str> = tail
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return "nothing".to_string();
    }
    let start = lines.len().saturating_sub(3);
    let joined = lines[start..].join(" | ");
    let n = joined.chars().count();
    if n <= VERDICT_TAIL_CHARS {
        return joined;
    }
    let kept: String = joined.chars().skip(n - VERDICT_TAIL_CHARS).collect();
    format!("...{kept}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(outcome: Outcome, tail: &str) -> EncoderReport {
        EncoderReport {
            outcome,
            path: PathBuf::from("/r/a.webm"),
            bytes: 1234,
            frames_queued: 50,
            frames_dropped: 3,
            frames_written: 47,
            gaps_clamped: 0,
            pid: Some(4242),
            finalize_ms: 10,
            stderr_tail: tail.to_string(),
        }
    }

    #[test]
    fn error_verdict_table() {
        type Row = (EncoderError, Vec<&'static str>, Vec<&'static str>);
        let rows: Vec<Row> = vec![
            (
                EncoderError::FfmpegNotFound {
                    hint: "brew install ffmpeg",
                },
                vec!["ffmpeg not found", "brew install ffmpeg", "bladebro stop"],
                vec![],
            ),
            (
                EncoderError::OverrideUnusable {
                    value: "/x/ff".into(),
                    reason: "not executable".into(),
                },
                vec![
                    "BLADE_FFMPEG=/x/ff",
                    "not executable",
                    "PATH was not searched",
                ],
                vec![],
            ),
            (
                EncoderError::FfmpegUnusable {
                    path: "/y/ffmpeg".into(),
                    reason: "timed out".into(),
                },
                vec!["/y/ffmpeg", "timed out", "bladebro doctor"],
                vec![],
            ),
            (
                EncoderError::EncoderMissing {
                    path: "/y/ffmpeg".into(),
                    format: Format::WebM,
                    alt: Some(Format::Mp4),
                },
                vec!["libvpx-vp9", "WebM", "BLADE_RECORD_FORMAT=mp4"],
                vec![],
            ),
            (
                EncoderError::EncoderMissing {
                    path: "/y/ffmpeg".into(),
                    format: Format::Mp4,
                    alt: None,
                },
                vec!["libx264", "MP4"],
                vec!["BLADE_RECORD_FORMAT"],
            ),
            (
                EncoderError::BadOutputPath {
                    path: "-x.webm".into(),
                    reason: "must be absolute".into(),
                },
                vec!["not started", "-x.webm", "must be absolute"],
                vec!["Browsing is unaffected"],
            ),
            (
                EncoderError::Spawn {
                    path: "/y/ffmpeg".into(),
                    error: "permission denied".into(),
                },
                vec!["not started", "/y/ffmpeg", "permission denied"],
                vec![],
            ),
        ];
        for (e, must, not) in rows {
            let v = e.to_string();
            for m in must {
                assert!(v.contains(m), "{v:?} lacks {m:?}");
            }
            for n in not {
                assert!(!v.contains(n), "{v:?} has {n:?}");
            }
            let unaffected = !matches!(e, EncoderError::BadOutputPath { .. });
            assert_eq!(v.ends_with("Browsing is unaffected."), unaffected, "{v}");
        }
    }

    #[test]
    fn outcome_verdict_table() {
        let failures = [
            Outcome::NoFrames,
            Outcome::ExitFailed {
                status: "exit code 1".into(),
            },
            Outcome::StoppedReading {
                error: "Broken pipe".into(),
            },
            Outcome::EmptyOutput,
            Outcome::Unfinalized {
                waited_ms: 10_000,
                reaped: true,
            },
        ];
        for o in failures {
            let r = report(o, "boom");
            let v = r.verdict();
            assert!(!v.starts_with("recording finalized"), "{v}");
            assert!(!r.ok(), "{v}");
        }
        let r = report(Outcome::Finalized, "");
        let v = r.verdict();
        assert!(r.ok());
        assert!(v.starts_with("recording finalized"));
        assert!(v.contains("1234 bytes") && v.contains("47 frames") && v.contains("3 dropped"));
    }

    #[test]
    fn failure_verdicts_carry_tail() {
        let cases = [
            Outcome::ExitFailed {
                status: "exit code 1".into(),
            },
            Outcome::StoppedReading {
                error: "Broken pipe".into(),
            },
            Outcome::Unfinalized {
                waited_ms: 10_000,
                reaped: true,
            },
        ];
        for o in cases {
            let with = report(o.clone(), "bad filter").verdict();
            assert!(with.contains("ffmpeg said: bad filter"), "{with}");
            let empty = report(o, "").verdict();
            assert!(empty.contains("ffmpeg said: nothing"), "{empty}");
        }
        let v = report(
            Outcome::ExitFailed {
                status: "exit code 1".into(),
            },
            "x",
        )
        .verdict();
        assert!(v.contains("exit code 1") && v.contains("47 frames"));
        let un = report(
            Outcome::Unfinalized {
                waited_ms: 10_000,
                reaped: false,
            },
            "",
        )
        .verdict();
        assert!(un.contains("could not be reaped") && un.contains("4242") && un.contains("10s"));
        let ok = report(
            Outcome::Unfinalized {
                waited_ms: 10_000,
                reaped: true,
            },
            "",
        )
        .verdict();
        assert!(!ok.contains("could not be reaped"));
    }

    #[test]
    fn tail_table() {
        assert_eq!(tail_for_verdict(""), "nothing");
        assert_eq!(tail_for_verdict("\n \r\n"), "nothing");
        assert_eq!(tail_for_verdict("a\nb\nc\nd\ne"), "c | d | e");
        assert_eq!(tail_for_verdict("a\n\n  \nb\r\nc\r\n"), "a | b | c");
        let long = "x".repeat(1000);
        let t = tail_for_verdict(&long);
        assert!(t.len() <= 303 && t.starts_with("..."), "{}", t.len());
        let multi = "ошибка ".repeat(100);
        let t = tail_for_verdict(&multi);
        assert!(t.starts_with("..."));
        assert!(t.chars().count() <= 303);
    }

    #[test]
    fn classify_table() {
        use ExitInfo::*;
        let ff = |s: &str| Outcome::ExitFailed { status: s.into() };
        let sr = || Outcome::StoppedReading {
            error: "Broken pipe".into(),
        };
        let un = |r| Outcome::Unfinalized {
            waited_ms: 7,
            reaped: r,
        };
        let bp = Some("Broken pipe");
        type Row<'a> = (ExitInfo, u64, Option<&'a str>, Option<u64>, Outcome);
        let rows: Vec<Row> = vec![
            (Killed { reaped: true }, 10, None, Some(5), un(true)),
            (Killed { reaped: false }, 0, None, None, un(false)),
            (Code(234), 0, None, Some(0), Outcome::NoFrames),
            (Code(0), 0, None, None, Outcome::NoFrames),
            (Code(1), 5, None, Some(9), ff("exit code 1")),
            (
                Signal("signal: 9 (SIGKILL)".into()),
                5,
                None,
                None,
                ff("signal: 9 (SIGKILL)"),
            ),
            (
                WaitFailed("x".into()),
                5,
                None,
                None,
                ff("unknown (wait failed: x)"),
            ),
            (Code(0), 0, bp, None, sr()),
            (Code(0), 5, bp, Some(100), sr()),
            (Code(1), 5, bp, None, ff("exit code 1")),
            (Code(0), 5, None, None, Outcome::EmptyOutput),
            (Code(0), 5, None, Some(0), Outcome::EmptyOutput),
            (Code(0), 5, None, Some(1), Outcome::Finalized),
        ];
        for (exit, w, we, b, want) in rows {
            assert_eq!(
                classify(&exit, w, we, b, 7),
                want,
                "{exit:?} {w} {we:?} {b:?}"
            );
        }
    }
}
