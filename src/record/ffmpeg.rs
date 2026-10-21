//! ffmpeg discovery for session recording: find a usable binary, learn its
//! version and video encoders once per process.
//!
//! Module map:
//! - `parse`: pure `-version` / `-encoders` output parsers and fixtures.
//! - `lookup`: BLADE_FFMPEG > PATH > known-dirs search with injected env/home.
//!
//! Lookup order: BLADE_FFMPEG, then PATH, then known per-OS locations.
//! Fork-only module (SYNC-01): discovery is recording policy, so it lives
//! here and not in `platform.rs`.

mod lookup;
mod parse;

pub use lookup::{
    find_ffmpeg, find_ffmpeg_from_process, known_locations, EnvFn, Found, Lookup, Source, TargetOs,
};

pub use parse::{
    parse_encoders, parse_version, EncoderSet, FfmpegVersion, MIN_RECOMMENDED_MAJOR, VP9_ENCODER,
    X264_ENCODER,
};
