//! Pure ffmpeg argv builder and output-path checks. No process, no shell:
//! the result is an argv `Vec<OsString>` handed to `Command::args`.
//!
//! Why these flags:
//! - D5-1 (timestamps): input `-f image2pipe -framerate <fps>` stamps the
//!   i-th JPEG at i/fps; `-fps_mode passthrough` keeps every frame at that
//!   stamp (no dup, no drop); there is deliberately NO output `-r`, which
//!   under a cfg mode would let ffmpeg dup/drop on its own. The `Pacer`
//!   decides how many times each JPEG is written.
//! - D5-3: `-fps_mode` exists from ffmpeg 5.1 (Ubuntu 22.04 ships 4.4.2), so
//!   older releases get the legacy `-vsync passthrough`.
//! - D5-4: the path must not start with '-', must be absolute and must carry
//!   the format's extension; the muxer is explicit (`-f webm|mp4`). `-y` is
//!   required: the file is pre-created, and without it ffmpeg would prompt
//!   and read the answer from stdin, which is the frame pipe.
//! - D5-5: `-loglevel error` keeps stderr to the lines worth a verdict tail.

use std::ffi::OsString;
use std::path::Path;

use crate::record::config::RecordConfig;
use crate::record::ffmpeg::FfmpegVersion;
use crate::record::values::{Format, FrameSize, MAX_FPS, MIN_FPS};

use super::report::EncoderError;

/// VP9 realtime flags. crf 40 (not 36): the sidecar numbers in RESULTS.md
/// (870,778 B/min, 4.55 cpu-s/min) were measured at crf 40 (D5-2).
const VP9_FLAGS: &[&str] = &[
    "-deadline",
    "realtime",
    "-cpu-used",
    "8",
    "-row-mt",
    "1",
    "-crf",
    "40",
    "-b:v",
    "0",
];

/// x264 flags; `+faststart` moves the moov atom so the file plays at once.
const X264_FLAGS: &[&str] = &[
    "-preset",
    "veryfast",
    "-crf",
    "28",
    "-movflags",
    "+faststart",
];

/// What to encode: container/codec, frame rate and output size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeSpec {
    pub format: Format,
    pub fps: u32,
    pub size: FrameSize,
}

impl EncodeSpec {
    /// Take format, fps and size from a resolved recording config.
    pub fn from_config(c: &RecordConfig) -> Self {
        Self {
            format: c.format,
            fps: c.fps,
            size: c.size,
        }
    }
}

/// Encoders need even sides; round down, never below 2.
pub fn even_size(s: FrameSize) -> (u32, u32) {
    (((s.width) & !1).max(2), ((s.height) & !1).max(2))
}

/// Clamp fps into the supported range.
pub fn clamp_fps(fps: u32) -> u32 {
    fps.clamp(MIN_FPS, MAX_FPS)
}

fn bad(out: &Path, reason: String) -> EncoderError {
    EncoderError::BadOutputPath {
        path: out.display().to_string(),
        reason,
    }
}

/// Reject output paths ffmpeg could misread (option, relative/protocol) or
/// whose extension disagrees with the format. Runs before any process exists.
pub fn check_out_path(out: &Path, format: Format) -> Result<(), EncoderError> {
    if out.to_string_lossy().starts_with('-') {
        return Err(bad(
            out,
            "starts with '-' (ffmpeg would read it as an option)".to_string(),
        ));
    }
    if !out.is_absolute() {
        return Err(bad(out, "must be absolute".to_string()));
    }
    let ext = out.extension().and_then(|e| e.to_str());
    if !ext.is_some_and(|e| e.eq_ignore_ascii_case(format.ext())) {
        let name = match format {
            Format::WebM => "WebM",
            Format::Mp4 => "MP4",
        };
        return Err(bad(
            out,
            format!(
                "extension must be .{} for {name} (got {})",
                format.ext(),
                ext.unwrap_or("none")
            ),
        ));
    }
    Ok(())
}

/// Build the full ffmpeg argv (without the program name). The output path is
/// always the last element and is pushed as an `OsStr`, never lossily.
pub fn build_args(
    spec: &EncodeSpec,
    version: &FfmpegVersion,
    out: &Path,
) -> Result<Vec<OsString>, EncoderError> {
    check_out_path(out, spec.format)?;
    let (w, h) = even_size(spec.size);
    let fps = clamp_fps(spec.fps);
    // in_range=pc:out_range=tv: Chrome's screencast JPEGs are full range. Written as full-range VP9
    // (inter frames) Chrome plays only the first frame and ends at once, so the progress bar hits 100%
    // in about a second of a 7 s file; limited range plays in real time. Reproduced on Chrome 156.
    let vf = format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease:force_divisible_by=2:in_range=pc:out_range=tv,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,format=yuv420p"
    );
    let legacy_vsync = matches!(version, FfmpegVersion::Release { major, .. } if *major <= 5);
    let (codec_flags, muxer) = match spec.format {
        Format::WebM => (VP9_FLAGS, "webm"),
        Format::Mp4 => (X264_FLAGS, "mp4"),
    };

    let mut a: Vec<OsString> = Vec::with_capacity(40);
    let mut push = |s: &str| a.push(OsString::from(s));
    for s in ["-hide_banner", "-nostats", "-loglevel", "error", "-y"] {
        push(s);
    }
    for s in ["-f", "image2pipe", "-framerate"] {
        push(s);
    }
    push(&fps.to_string());
    for s in ["-c:v", "mjpeg", "-i", "pipe:0", "-vf"] {
        push(s);
    }
    push(&vf);
    if legacy_vsync {
        push("-vsync");
    } else {
        push("-fps_mode");
    }
    push("passthrough");
    push("-an");
    push("-c:v");
    push(spec.format.encoder());
    for s in codec_flags {
        push(s);
    }
    push("-f");
    push(muxer);
    a.push(out.as_os_str().to_owned());
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::ffmpeg::{VP9_ENCODER, X264_ENCODER};
    use std::path::PathBuf;

    fn abs(name: &str) -> PathBuf {
        std::env::temp_dir().join("x").join(name)
    }

    fn spec(format: Format) -> EncodeSpec {
        EncodeSpec {
            format,
            fps: 10,
            size: FrameSize {
                width: 1280,
                height: 720,
            },
        }
    }

    fn rel(n: u32) -> FfmpegVersion {
        FfmpegVersion::Release {
            display: format!("{n}.0"),
            major: n,
        }
    }

    fn v9() -> FfmpegVersion {
        FfmpegVersion::Release {
            display: "n9.0.2".into(),
            major: 9,
        }
    }

    fn strs(a: &[OsString]) -> Vec<String> {
        a.iter().map(|s| s.to_string_lossy().into_owned()).collect()
    }

    const HEAD: &str = "-hide_banner -nostats -loglevel error -y -f image2pipe -framerate 10 -c:v mjpeg -i pipe:0 -vf scale=1280:720:force_original_aspect_ratio=decrease:force_divisible_by=2:in_range=pc:out_range=tv,pad=1280:720:(ow-iw)/2:(oh-ih)/2,format=yuv420p -fps_mode passthrough -an";

    fn expected(tail: &str, out: &Path) -> Vec<OsString> {
        let mut v: Vec<OsString> = format!("{HEAD} {tail}")
            .split(' ')
            .map(OsString::from)
            .collect();
        v.push(out.as_os_str().to_owned());
        v
    }

    #[test]
    fn exact_webm_argv() {
        let out = abs("rec.webm");
        let got = build_args(&spec(Format::WebM), &v9(), &out).unwrap();
        let want = expected(
            "-c:v libvpx-vp9 -deadline realtime -cpu-used 8 -row-mt 1 -crf 40 -b:v 0 -f webm",
            &out,
        );
        assert_eq!(got, want);
    }

    #[test]
    fn exact_mp4_argv() {
        let out = abs("rec.mp4");
        let got = build_args(&spec(Format::Mp4), &v9(), &out).unwrap();
        let want = expected(
            "-c:v libx264 -preset veryfast -crf 28 -movflags +faststart -f mp4",
            &out,
        );
        assert_eq!(got, want);
    }

    #[test]
    fn odd_sizes_made_even() {
        let out = abs("rec.webm");
        for ((w, h), (ew, eh)) in [((1001, 641), (1000, 640)), ((161, 121), (160, 120))] {
            let mut s = spec(Format::WebM);
            s.size = FrameSize {
                width: w,
                height: h,
            };
            let a = strs(&build_args(&s, &v9(), &out).unwrap());
            let vf = a.iter().find(|x| x.starts_with("scale=")).unwrap();
            assert!(vf.contains(&format!("scale={ew}:{eh}:")), "{vf}");
            assert!(vf.contains(&format!("pad={ew}:{eh}:")), "{vf}");
        }
        assert_eq!(
            even_size(FrameSize {
                width: 1,
                height: 1
            }),
            (2, 2)
        );
    }

    #[test]
    fn fps_clamped() {
        let out = abs("rec.webm");
        for (fps, want) in [(30, "15"), (1, "5"), (0, "5"), (12, "12")] {
            let mut s = spec(Format::WebM);
            s.fps = fps;
            let a = strs(&build_args(&s, &v9(), &out).unwrap());
            let i = a.iter().position(|x| x == "-framerate").unwrap();
            assert_eq!(a[i + 1], want, "fps {fps}");
        }
    }

    #[test]
    fn version_gate() {
        let out = abs("rec.webm");
        let git = FfmpegVersion::Git {
            display: "N-1-g1".into(),
        };
        let cases = [
            (rel(4), true),
            (rel(5), true),
            (rel(6), false),
            (rel(9), false),
            (git, false),
            (FfmpegVersion::Unknown, false),
        ];
        for (v, legacy) in cases {
            let a = strs(&build_args(&spec(Format::WebM), &v, &out).unwrap());
            let (flag, other) = if legacy {
                ("-vsync", "-fps_mode")
            } else {
                ("-fps_mode", "-vsync")
            };
            let i = a.iter().position(|x| x == flag).unwrap();
            assert_eq!(a[i + 1], "passthrough", "{v:?}");
            assert!(!a.iter().any(|x| x == other), "{v:?}");
        }
    }

    #[test]
    fn codec_names_from_format() {
        for (f, name) in [(Format::WebM, VP9_ENCODER), (Format::Mp4, X264_ENCODER)] {
            let out = abs(&format!("rec.{}", f.ext()));
            let a = strs(&build_args(&spec(f), &v9(), &out).unwrap());
            let idx: Vec<usize> = a
                .iter()
                .enumerate()
                .filter(|(_, x)| *x == "-c:v")
                .map(|(i, _)| i)
                .collect();
            assert_eq!(idx.len(), 2);
            assert_eq!(a[idx[1] + 1], f.encoder());
            assert_eq!(a[idx[1] + 1], name);
        }
    }

    #[test]
    fn weird_path_is_one_element() {
        let plain = std::env::temp_dir().join("a b dir").join("plain.webm");
        let weird = std::env::temp_dir()
            .join("a b dir")
            .join("a b;$(touch pwned)'\"|&*.webm");
        let p = build_args(&spec(Format::WebM), &v9(), &plain).unwrap();
        let w = build_args(&spec(Format::WebM), &v9(), &weird).unwrap();
        assert_eq!(p.len(), w.len());
        assert_eq!(w.last().unwrap(), weird.as_os_str());
        for bad in ["sh", "-c", "cmd", "/C"] {
            assert!(!w.iter().any(|x| x == bad), "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_byte_exact() {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::OsStr::from_bytes(b"rec-\xff.webm");
        let out = std::env::temp_dir().join(name);
        let a = build_args(&spec(Format::WebM), &v9(), &out).unwrap();
        assert_eq!(a.last().unwrap().as_bytes(), out.as_os_str().as_bytes());
    }

    #[test]
    fn path_rejections() {
        let cases: Vec<(PathBuf, Format, Option<&str>)> = vec![
            (
                PathBuf::from("-rec.webm"),
                Format::WebM,
                Some("starts with '-'"),
            ),
            (
                PathBuf::from("rec.webm"),
                Format::WebM,
                Some("must be absolute"),
            ),
            (abs("rec.mp4"), Format::WebM, Some("extension")),
            (abs("rec"), Format::WebM, Some("extension")),
            (abs("REC.WEBM"), Format::WebM, None),
        ];
        for (p, f, want) in cases {
            let r = build_args(&spec(f), &v9(), &p);
            match want {
                None => assert!(r.is_ok(), "{p:?}"),
                Some(sub) => match r {
                    Err(EncoderError::BadOutputPath { reason, .. }) => {
                        assert!(reason.contains(sub), "{reason}")
                    }
                    other => panic!("{p:?}: {other:?}"),
                },
            }
        }
    }

    #[test]
    fn structural_invariants() {
        let out = abs("rec.webm");
        let a = strs(&build_args(&spec(Format::WebM), &v9(), &out).unwrap());
        let y = a.iter().position(|x| x == "-y").unwrap();
        assert!(y < a.len() - 1);
        let i = a.iter().position(|x| x == "-i").unwrap();
        assert_eq!(a[i + 1], "pipe:0");
        assert!(a.iter().any(|x| x == "-an"));
        assert!(!a.iter().any(|x| x == "-r"));
    }

    #[test]
    fn from_config_carries_values() {
        let c = RecordConfig {
            enabled: true,
            dir: PathBuf::from("d"),
            format: Format::Mp4,
            fps: 7,
            size: FrameSize {
                width: 640,
                height: 360,
            },
            quality: 70,
            tabs: crate::record::Tabs::Follow,
            queue_len: 16,
            name: None,
            artifacts_dir: PathBuf::from("a"),
        };
        let s = EncodeSpec::from_config(&c);
        assert_eq!(s.format, Format::Mp4);
        assert_eq!(s.fps, 7);
        assert_eq!(s.size.width, 640);
        assert_eq!(s.size.height, 360);
    }
}
