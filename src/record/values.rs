//! Recording setting types and the value parsers shared by env, record.json
//! and per-call input. Every parser returns a bare reason string; the caller
//! in `config` prefixes the source (`BLADE_RECORD_FPS:`, `record.json`,
//! `per-call`) so an error always names where the bad value came from.
//!
//! Errors are product surface: an agent reads them mid-task, so each one
//! states the valid range and the default.

use std::fmt;
use std::path::{Path, PathBuf};

// Env var names: the option registry (launch_mode::registry) and the parsers
// share these, so a renamed variable cannot drift from the docs check (DOC-02).
/// Recording switch / directory.
pub const ENV_RECORD: &str = "BLADE_RECORD";
/// Recording container.
pub const ENV_RECORD_FORMAT: &str = "BLADE_RECORD_FORMAT";
/// Recording frame rate.
pub const ENV_RECORD_FPS: &str = "BLADE_RECORD_FPS";
/// Recording frame size.
pub const ENV_RECORD_SIZE: &str = "BLADE_RECORD_SIZE";

/// 10 fps: vp9 4.55 cpu-s/min at 10 fps 1280x720 with 0 live drops
/// (tools/record_sidecar/RESULTS.md "Recommended defaults for Phase 3").
pub const DEFAULT_FPS: u32 = 10;
/// Lower bound: below 5 fps the screencast looks like a slideshow.
pub const MIN_FPS: u32 = 5;
/// Upper bound: Chrome delivers ~60-100 fps, and the recorder drops frames to
/// the cap; 15 fps costs 5.40 cpu-s/min (RESULTS.md).
pub const MAX_FPS: u32 = 15;
/// JPEG quality 70: the value used in every measured run (RESULTS.md). q60 to
/// q80 was not compared, so this is the measured default, not a proven optimum.
pub const DEFAULT_QUALITY: u8 = 70;
/// Frame queue depth: 16 with drop-at-queue, ack always first (RESULTS.md).
/// Deliberately not env-tunable (D3-5).
pub const QUEUE_LEN: usize = 16;
/// Default output size 1280x720: 1080p costs 6.18 cpu-s/min and 1,124,821 B
/// per minute vs 870,778 B at 720p (RESULTS.md).
pub const DEFAULT_WIDTH: u32 = 1280;
/// See [`DEFAULT_WIDTH`].
pub const DEFAULT_HEIGHT: u32 = 720;
/// Smallest accepted frame width (D3-6).
pub const MIN_WIDTH: u32 = 160;
/// Smallest accepted frame height (D3-6).
pub const MIN_HEIGHT: u32 = 120;
/// Largest accepted frame width (D3-6).
pub const MAX_WIDTH: u32 = 3840;
/// Largest accepted frame height (D3-6).
pub const MAX_HEIGHT: u32 = 2160;
/// Config file name, resolved under the blade dir (CTRL-03).
pub const CONFIG_FILE: &str = "record.json";
/// Default output folder, under the blade dir.
pub const RECORDINGS_SUBDIR: &str = "recordings";

/// Output container and codec. VP8 is deliberately absent: RESULTS.md measured
/// 25.74 cpu-s/min and 55 live drops, so it is not offered as a fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// WebM with VP9. The default.
    #[default]
    WebM,
    /// MP4 with H.264 (x264). Opt-in only.
    Mp4,
}

impl Format {
    /// Parse `webm` or `mp4`, case-insensitive, surrounding whitespace ignored.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "webm" => Ok(Self::WebM),
            "mp4" => Ok(Self::Mp4),
            _ => Err(format!(
                "format {:?} is not supported; use webm or mp4 (default webm)",
                s.trim()
            )),
        }
    }

    /// File extension without the dot.
    pub fn ext(self) -> &'static str {
        match self {
            Self::WebM => "webm",
            Self::Mp4 => "mp4",
        }
    }

    /// ffmpeg video encoder name for this container.
    pub fn encoder(self) -> &'static str {
        match self {
            Self::WebM => "libvpx-vp9",
            Self::Mp4 => "libx264",
        }
    }
}

/// Output frame size. Both sides are even (encoders need it) and inside the
/// `MIN_*`..=`MAX_*` bounds. Never clamped: an out-of-range size is an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSize {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Default for FrameSize {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        }
    }
}

impl fmt::Display for FrameSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl FrameSize {
    /// Parse `WIDTHxHEIGHT` (`x` or `X`). Odd sides are rejected, not rounded:
    /// the error suggests the nearest smaller even size instead.
    pub fn parse(s: &str) -> Result<Self, String> {
        let t = s.trim();
        let bad = || {
            format!(
                "size {:?} is not WIDTHxHEIGHT (e.g. 1280x720; valid range {}x{} to {}x{})",
                t, MIN_WIDTH, MIN_HEIGHT, MAX_WIDTH, MAX_HEIGHT
            )
        };
        let parts: Vec<&str> = t.split(['x', 'X']).collect();
        if parts.len() != 2 {
            return Err(bad());
        }
        let width: u32 = parts[0].trim().parse().map_err(|_| bad())?;
        let height: u32 = parts[1].trim().parse().map_err(|_| bad())?;

        if width < MIN_WIDTH || height < MIN_HEIGHT {
            return Err(format!(
                "size {width}x{height} is below the minimum {MIN_WIDTH}x{MIN_HEIGHT}"
            ));
        }
        if width > MAX_WIDTH || height > MAX_HEIGHT {
            return Err(format!(
                "size {width}x{height} exceeds the {MAX_WIDTH}x{MAX_HEIGHT} maximum"
            ));
        }
        if width % 2 != 0 || height % 2 != 0 {
            return Err(format!(
                "size {width}x{height}: width and height must be even (try {}x{})",
                width & !1,
                height & !1
            ));
        }
        Ok(Self { width, height })
    }
}

/// Parse a frames-per-second value. Whole numbers only, 5..=15. Fractions,
/// negatives and words are rejected, never clamped.
pub fn parse_fps(s: &str) -> Result<u32, String> {
    let t = s.trim();
    let n: i64 = t.parse().map_err(|_| {
        format!(
            "fps {t:?} is not a whole number; valid range {MIN_FPS}-{MAX_FPS} (default {DEFAULT_FPS})"
        )
    })?;
    check_fps(n)
}

/// Range-check an fps already read as an integer (record.json carries JSON
/// numbers, not strings). i64 keeps negatives and huge values in the range
/// check instead of a parse failure.
pub fn check_fps(n: i64) -> Result<u32, String> {
    if !(MIN_FPS as i64..=MAX_FPS as i64).contains(&n) {
        return Err(format!(
            "fps {n} out of range {MIN_FPS}-{MAX_FPS} (default {DEFAULT_FPS})"
        ));
    }
    Ok(n as u32)
}

/// Validate a JPEG quality read from record.json. The range is checked before
/// the cast so an out-of-range i64 can never wrap into a valid u8.
pub fn parse_quality(q: i64) -> Result<u8, String> {
    if !(1..=100).contains(&q) {
        return Err(format!(
            "quality {q} out of range 1-100 (default {DEFAULT_QUALITY})"
        ));
    }
    Ok(q as u8)
}

/// Parsed `BLADE_RECORD` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordEnv {
    /// Unset or blank: the env layer says nothing, so config/default decide.
    Unset,
    /// A recognised on/off/dir value.
    Set(Toggle),
}

/// What `BLADE_RECORD` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Toggle {
    /// `0|off|false|no`: recording disabled.
    Off,
    /// `1|on|true|yes` (no dir), or an absolute dir (the dir is `Some`).
    On(Option<PathBuf>),
}

/// Parse `BLADE_RECORD` (D3-4). Booleans are case-insensitive. Anything else
/// must be an absolute directory: the daemon's cwd differs from the CLI's, so a
/// relative path would resolve to two different places.
pub fn parse_record_env(raw: Option<&str>) -> Result<RecordEnv, String> {
    let t = match raw.map(str::trim) {
        None | Some("") => return Ok(RecordEnv::Unset),
        Some(t) => t,
    };
    match parse_switch(t) {
        Some(false) => Ok(RecordEnv::Set(Toggle::Off)),
        Some(true) => Ok(RecordEnv::Set(Toggle::On(None))),
        None => {
            if Path::new(t).is_absolute() {
                Ok(RecordEnv::Set(Toggle::On(Some(PathBuf::from(t)))))
            } else {
                Err(format!(
                    "{t:?} must be an absolute directory path, or on/off; relative paths are \
                     ambiguous because the daemon's working directory differs from the CLI's"
                ))
            }
        }
    }
}

/// Shared on/off spellings for BLADE_RECORD and BLADE_HEADLESS (one parser so
/// the two switches can never diverge). Case-insensitive, trimmed.
pub fn parse_switch(t: &str) -> Option<bool> {
    match t.trim().to_ascii_lowercase().as_str() {
        "1" | "on" | "true" | "yes" => Some(true),
        "0" | "off" | "false" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_switch_table() {
        for t in ["1", "on", "ON", " true ", "yes"] {
            assert_eq!(parse_switch(t), Some(true), "{t:?}");
        }
        for t in ["0", "off", "False", "no"] {
            assert_eq!(parse_switch(t), Some(false), "{t:?}");
        }
        for t in ["", "2", "maybe", "y"] {
            assert_eq!(parse_switch(t), None, "{t:?}");
        }
    }

    /// Absolute-path literal that is absolute on the current OS. `/x` is not
    /// absolute on Windows, so tests build `C:\x` there.
    fn abs(p: &str) -> String {
        if cfg!(windows) {
            format!("C:{}", p.replace('/', "\\"))
        } else {
            p.to_string()
        }
    }

    fn run<T: PartialEq + std::fmt::Debug>(
        name: &str,
        cases: &[(&str, Result<T, &str>)],
        f: impl Fn(&str) -> Result<T, String>,
    ) {
        for (input, want) in cases {
            let got = f(input);
            match (&got, want) {
                (Ok(g), Ok(w)) => assert_eq!(g, w, "{name}({input:?})"),
                (Err(e), Err(needle)) => assert!(
                    e.contains(needle),
                    "{name}({input:?}): error {e:?} lacks {needle:?}"
                ),
                _ => panic!("{name}({input:?}): got {got:?}, want {want:?}"),
            }
        }
    }

    #[test]
    fn format_parse_table() {
        run(
            "Format::parse",
            &[
                ("webm", Ok(Format::WebM)),
                ("WebM", Ok(Format::WebM)),
                (" mp4 ", Ok(Format::Mp4)),
                ("vp8", Err("webm or mp4")),
                ("avi", Err("webm or mp4")),
                ("", Err("webm or mp4")),
                ("mkv", Err("webm or mp4")),
            ],
            Format::parse,
        );
        assert_eq!(Format::default(), Format::WebM);
        assert_eq!(Format::WebM.ext(), "webm");
        assert_eq!(Format::Mp4.ext(), "mp4");
        assert_eq!(Format::WebM.encoder(), "libvpx-vp9");
        assert_eq!(Format::Mp4.encoder(), "libx264");
    }

    #[test]
    fn fps_parse_table() {
        run(
            "parse_fps",
            &[
                ("5", Ok(5)),
                ("10", Ok(10)),
                ("15", Ok(15)),
                ("0", Err("5-15")),
                ("4", Err("5-15")),
                ("16", Err("5-15")),
                ("-1", Err("5-15")),
                ("abc", Err("5-15")),
                ("", Err("5-15")),
                ("10.5", Err("5-15")),
            ],
            parse_fps,
        );
    }

    #[test]
    fn frame_size_parse_table() {
        run(
            "FrameSize::parse",
            &[
                (
                    "1280x720",
                    Ok(FrameSize {
                        width: 1280,
                        height: 720,
                    }),
                ),
                (
                    "1920X1080",
                    Ok(FrameSize {
                        width: 1920,
                        height: 1080,
                    }),
                ),
                (
                    " 640x480 ",
                    Ok(FrameSize {
                        width: 640,
                        height: 480,
                    }),
                ),
                ("1281x720", Err("even")),
                ("1281x720", Err("1280x720")),
                ("0x0", Err("minimum")),
                ("abcx720", Err("WIDTHxHEIGHT")),
                ("1280", Err("WIDTHxHEIGHT")),
                ("1280x", Err("WIDTHxHEIGHT")),
                ("x720", Err("WIDTHxHEIGHT")),
                ("-2x4", Err("WIDTHxHEIGHT")),
                ("1280*720", Err("WIDTHxHEIGHT")),
                ("99999999999x2", Err("WIDTHxHEIGHT")),
                ("3842x2160", Err("3840x2160")),
                ("4000x3000", Err("3840x2160")),
                ("158x120", Err("minimum")),
            ],
            FrameSize::parse,
        );
        assert_eq!(
            FrameSize::default(),
            FrameSize {
                width: 1280,
                height: 720
            }
        );
        assert_eq!(FrameSize::default().to_string(), "1280x720");
    }

    #[test]
    fn quality_parse_table() {
        assert_eq!(parse_quality(1), Ok(1));
        assert_eq!(parse_quality(70), Ok(70));
        assert_eq!(parse_quality(100), Ok(100));
        assert!(parse_quality(0).is_err());
        assert!(parse_quality(101).is_err());
        // An i64 that would wrap into 1..=100 as a u8 must still be rejected.
        assert!(parse_quality(256 + 70).is_err());
    }

    #[test]
    fn record_env_table() {
        let dir = PathBuf::from(abs("/abs/dir"));
        let set_dir = RecordEnv::Set(Toggle::On(Some(dir)));
        assert_eq!(parse_record_env(Some(&abs("/abs/dir"))), Ok(set_dir));
        for off in ["0", "OFF", "false", "no"] {
            assert_eq!(
                parse_record_env(Some(off)),
                Ok(RecordEnv::Set(Toggle::Off)),
                "{off}"
            );
        }
        for on in ["1", "on", "TRUE", "yes"] {
            assert_eq!(
                parse_record_env(Some(on)),
                Ok(RecordEnv::Set(Toggle::On(None))),
                "{on}"
            );
        }
        assert_eq!(parse_record_env(None), Ok(RecordEnv::Unset));
        assert_eq!(parse_record_env(Some("  ")), Ok(RecordEnv::Unset));
        for rel in ["rel/dir", "./x"] {
            let err = parse_record_env(Some(rel)).unwrap_err();
            assert!(err.contains("absolute"), "{rel}: {err}");
        }
    }
}
