//! ffmpeg encoder for session recording: argv spawn (never a shell), JPEG
//! frames on stdin, stdin-EOF finalize with a bounded wait, and success only
//! on exit 0 plus a non-empty file (SAFE-04).
//!
//! Module map:
//! - `report`: error/outcome types, `classify`, honest verdict strings.
//! - `args`: pure argv builder and output-path checks.
//! - `pace`: frame timestamps to per-frame write counts (D5-1).
//! - `resolve` (05-02): ffmpeg discovery for the encoder, async-safe.
//! - `process` (05-02): spawn, stderr tail, bounded finish, kill-and-reap last resort.
//! - `writer` (05-02): paced stdin writer task; dropping its stdin is the EOF stop.
//! - `test_util`, `lifecycle_tests`, `matrix_tests` (tests only): fake ffmpeg scripts, unix lifecycle tests and outcome matrix.
//! - `windows_tests` (tests only, cfg(windows)): .cmd fakes, written but never compiled here.
//! - `real_tests` (tests only): real ffmpeg end-to-end, skipped when absent.
//!
//! Fork-only module (SYNC-01): no upstream file is touched by it.

mod args;
#[cfg(all(test, unix))]
mod lifecycle_tests;
#[cfg(all(test, unix))]
mod matrix_tests;
mod pace;
mod process;
#[cfg(test)]
mod real_tests;
mod report;
mod resolve;
#[cfg(test)]
pub(crate) mod test_util;
#[cfg(all(test, windows))]
mod windows_tests;
mod writer;

pub use args::{build_args, check_out_path, clamp_fps, even_size, EncodeSpec};
pub use pace::{repeat_counts, Pacer, MAX_GAP_MS};
pub use process::{push_tail, Encoder, EncoderConfig, FINISH_TIMEOUT, STDERR_TAIL_BYTES};
pub use report::{
    classify, tail_for_verdict, EncoderError, EncoderReport, ExitInfo, Outcome, VERDICT_TAIL_CHARS,
};
pub use resolve::{resolve_ffmpeg, resolve_from, ResolvedFfmpeg};
pub use writer::HOLD_FLUSH_INTERVAL;
