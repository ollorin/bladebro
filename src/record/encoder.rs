//! ffmpeg encoder for session recording: argv spawn (never a shell), JPEG
//! frames on stdin, stdin-EOF finalize with a bounded wait, and success only
//! on exit 0 plus a non-empty file (SAFE-04).
//!
//! Module map:
//! - `report`: error/outcome types, `classify`, honest verdict strings.
//! - `args`: pure argv builder and output-path checks.
//! - `pace`: frame timestamps to per-frame write counts (D5-1).
//! - `resolve` (05-02): ffmpeg discovery for the encoder, async-safe.
//! - `process` (05-02): spawn, writer task, stderr tail, bounded finish.
//!
//! Fork-only module (SYNC-01): no upstream file is touched by it.

mod args;
mod pace;
mod report;

pub use args::{build_args, check_out_path, clamp_fps, even_size, EncodeSpec};
pub use pace::{repeat_counts, Pacer, MAX_GAP_MS};
pub use report::{
    classify, tail_for_verdict, EncoderError, EncoderReport, ExitInfo, Outcome, VERDICT_TAIL_CHARS,
};
