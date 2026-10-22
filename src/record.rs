//! Session recording (opt-in screencast to a video file).
//!
//! Module map:
//! - `values`: defaults/limits consts, Format, FrameSize, value parsers,
//!   BLADE_RECORD on/off/dir parsing.
//! - `config`: record.json loading, RecordOverrides/RecordConfig, the `resolve`
//!   delegate and the impure `resolve_from_env` (reads the project file).
//! - `layered`: resolve_layered, precedence per-call > env > project file >
//!   record.json > default.
//! - `names`: caller name sanitization, UTC stamp, short id, file names.
//! - `dir`: private canonical recordings dir, create_new file creation; never rotated.
//! - `ffmpeg`: ffmpeg lookup (BLADE_FFMPEG > PATH > known dirs), bounded
//!   -version/-encoders probe cached once per process, pure output parsers,
//!   doctor report (DoctorLine).
//! - `encoder`: ffmpeg encoder: argv builder, timestamp pacing, process lifecycle, honest verdicts (SAFE-04, LIFE-05).
//! - `source`: screencast frame source: own CDP connection, ack-first, fps gate, bounded hand-off, counters (CAPT-01/02/05, SAFE-01).
//!
//! Fork-only module (SYNC-01): upstream files are touched only by the hook
//! lines listed in .planning/HOOKS.md.

pub mod config;
pub mod dir;
pub mod encoder;
pub mod ffmpeg;
pub mod layered;
pub mod names;
pub mod source;
pub mod values;

pub use config::{
    load_file_config, resolve, resolve_from_env, FileConfig, RecordConfig, RecordOverrides,
};
pub use dir::{create_recording_file, prepare_dir};
pub use encoder::{
    build_args, classify, repeat_counts, resolve_ffmpeg, resolve_from, EncodeSpec, Encoder,
    EncoderConfig, EncoderError, EncoderReport, ExitInfo, Outcome, Pacer, ResolvedFfmpeg,
};
pub use ffmpeg::{
    discover, discover_cached, doctor_check, doctor_line, find_ffmpeg, known_locations,
    parse_encoders, parse_version, probe, Discovery, DoctorLine, EncoderSet, FfmpegVersion, Found,
    Level, Lookup, ProbeInfo, Source, TargetOs, PROBE_TIMEOUT,
};
pub use layered::{resolve_layered, ProjectRecord};
pub use names::{file_name, sanitize_stem};
pub use values::{Format, FrameSize};
