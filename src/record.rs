//! Session recording (opt-in screencast to a video file).
//!
//! Module map:
//! - `values`: defaults/limits consts, Format, FrameSize, value parsers,
//!   BLADE_RECORD on/off/dir parsing.
//! - `config`: record.json loading, RecordOverrides/RecordConfig, precedence
//!   resolution per-call > env > config file > default.
//! - `names`: caller name sanitization, UTC stamp, short id, file names.
//! - `dir`: private canonical recordings dir, create_new file creation; never rotated.
//! - `ffmpeg`: ffmpeg lookup (BLADE_FFMPEG > PATH > known dirs), bounded
//!   -version/-encoders probe cached once per process, pure output parsers.
//!
//! Fork-only module (SYNC-01): upstream files are touched only by the hook
//! lines listed in .planning/HOOKS.md.

pub mod config;
pub mod dir;
pub mod ffmpeg;
pub mod names;
pub mod values;

pub use config::{
    load_file_config, resolve, resolve_from_env, FileConfig, RecordConfig, RecordOverrides,
};
pub use dir::{create_recording_file, prepare_dir};
pub use ffmpeg::{parse_encoders, parse_version, EncoderSet, FfmpegVersion};
pub use names::{file_name, sanitize_stem};
pub use values::{Format, FrameSize};
