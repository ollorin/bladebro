//! Pure parsers for `ffmpeg -version` and `ffmpeg -encoders` output. No I/O:
//! the probe feeds text in, tests feed fixtures in.

use std::collections::BTreeSet;

/// VP9 encoder name. Must equal `Format::WebM.encoder()` (tested).
pub const VP9_ENCODER: &str = "libvpx-vp9";
/// H.264 encoder name. Must equal `Format::Mp4.encoder()` (tested).
pub const X264_ENCODER: &str = "libx264";
/// Release majors below this are shown with a note by doctor; not enforced.
pub const MIN_RECOMMENDED_MAJOR: u32 = 4;

/// Parsed ffmpeg version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FfmpegVersion {
    /// A numbered release (`7.1`, `n9.0.2`, `5.1.6-0+deb12u1`).
    Release { display: String, major: u32 },
    /// A git / nightly build (`N-118000-g...`, gyan `2024-11-25-git-...`).
    Git { display: String },
    /// An ffmpeg banner without a usable version token.
    Unknown,
}

impl FfmpegVersion {
    /// Text for humans.
    pub fn display(&self) -> String {
        match self {
            Self::Release { display, .. } | Self::Git { display } => display.clone(),
            Self::Unknown => "version unknown".to_string(),
        }
    }

    /// True for a release older than [`MIN_RECOMMENDED_MAJOR`].
    pub fn is_old(&self) -> bool {
        matches!(self, Self::Release { major, .. } if *major < MIN_RECOMMENDED_MAJOR)
    }
}

/// Parse the first non-blank line of `ffmpeg -version`. `None` when it is not
/// an ffmpeg banner.
///
/// D4-6: date-stamped (gyan) and `N-`/`git-` builds are `Git`, never a release
/// with major 2024.
pub fn parse_version(text: &str) -> Option<FfmpegVersion> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let rest = line.strip_prefix("ffmpeg version")?;
    let token = match rest.split_whitespace().next() {
        None | Some("Copyright") => return Some(FfmpegVersion::Unknown),
        Some(t) => t,
    };
    let display = token.to_string();
    if token.starts_with("N-") || token.starts_with("git") || token.contains("-git-") {
        return Some(FfmpegVersion::Git { display });
    }
    let num = token.strip_prefix('n').unwrap_or(token);
    let digits: String = num.chars().take_while(|c| c.is_ascii_digit()).collect();
    let boundary_ok = matches!(num[digits.len()..].chars().next(), None | Some('.' | '-'));
    if boundary_ok {
        if let Ok(major) = digits.parse::<u32>() {
            if (1..=99).contains(&major) {
                return Some(FfmpegVersion::Release { display, major });
            }
        }
    }
    if token.contains("git") {
        Some(FfmpegVersion::Git { display })
    } else {
        Some(FfmpegVersion::Unknown)
    }
}

/// Video encoders reported by `ffmpeg -encoders`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EncoderSet {
    names: BTreeSet<String>,
    /// False when the output had no encoder table (garbage / wrong binary).
    pub listed: bool,
}

impl EncoderSet {
    /// Whether the named video encoder is present.
    pub fn has(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// Test constructor: a set from encoder names.
    #[cfg(test)]
    pub(crate) fn from_names(names: &[&str], listed: bool) -> Self {
        Self {
            names: names.iter().map(|n| n.to_string()).collect(),
            listed,
        }
    }
}

/// Parse `ffmpeg -encoders` (D4-7): only video encoders (flags start with `V`)
/// after the `------` separator; the legend above it is ignored.
pub fn parse_encoders(text: &str) -> EncoderSet {
    let mut set = EncoderSet::default();
    for raw in text.lines() {
        let line = raw.trim();
        if !set.listed {
            if line.len() >= 2 && line.chars().all(|c| c == '-') {
                set.listed = true;
            }
            continue;
        }
        let mut it = line.split_whitespace();
        if let (Some(flags), Some(name)) = (it.next(), it.next()) {
            if flags.len() == 6 && flags.starts_with('V') {
                set.names.insert(name.to_string());
            }
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::values::Format;

    const MAC_VERSION: &str = r#"ffmpeg version n9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
built with Apple clang version 21.0.0 (clang-2100.3.34.2)
configuration: --prefix='/opt/homebrew/Cellar/ffmpeg/9.0.2' --enable-gpl --enable-libx264 --enable-libvpx --disable-doc --disable-ffplay --disable-network
libavutil      61.  1.102 / 61.  1.102
"#;

    const LEGEND: &str = r#"Encoders:
 V..... = Video
 A..... = Audio
 S..... = Subtitle
 .F.... = Frame-level multithreading
 ..S... = Slice-level multithreading
 ...X.. = Codec is experimental
 ....B. = Supports draw_horiz_band
 .....D = Supports direct rendering method 1
 ------
"#;

    fn with_legend(body: &str) -> String {
        format!("{LEGEND}{body}")
    }

    fn mac_encoders() -> String {
        with_legend(
            r#" V....D a64multi             Multicolor charset for Commodore 64 (codec a64_multi)
 V..... libx264              libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (codec h264)
 V..... libx264rgb           libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 RGB (codec h264)
 V....D h264_videotoolbox    VideoToolbox H.264 Encoder (codec h264)
 V..... libvpx               libvpx VP8 (codec vp8)
 V..... libvpx-vp9           libvpx VP9 (codec vp9)
 A....D aac                  AAC (Advanced Audio Coding)
 A..... libopus              libopus Opus (codec opus)
"#,
        )
    }

    const DEBIAN_VERSION: &str = r#"ffmpeg version 5.1.6-0+deb12u1 Copyright (c) 2000-2024 the FFmpeg developers
built with gcc 12 (Debian 12.2.0-14)
configuration: --prefix=/usr --extra-version=0+deb12u1 --toolchain=hardened --enable-libvpx --enable-libx264
"#;

    fn debian_encoders() -> String {
        with_legend(
            r#" V..... libx264              libx264 H.264 (codec h264)
 V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)
 V....D h264_qsv             H.264 / AVC (Intel Quick Sync Video acceleration) (codec h264)
 V....D h264_vaapi           H.264/AVC (VAAPI) (codec h264)
 V..... libvpx               libvpx VP8 (codec vp8)
 V..... libvpx-vp9           libvpx VP9 (codec vp9)
 V.S... mpeg4                MPEG-4 part 2
"#,
        )
    }

    const GARBAGE: &str = "Segmentation fault (core dumped)\n<html>oops</html>\n";

    fn rel(display: &str, major: u32) -> Option<FfmpegVersion> {
        Some(FfmpegVersion::Release {
            display: display.into(),
            major,
        })
    }

    fn git(display: &str) -> Option<FfmpegVersion> {
        Some(FfmpegVersion::Git {
            display: display.into(),
        })
    }

    #[test]
    fn version_table() {
        let rows: Vec<(&str, Option<FfmpegVersion>)> = vec![
            (MAC_VERSION, rel("n9.0.2", 9)),
            ("ffmpeg version 7.1 Copyright (c) 2000-2024", rel("7.1", 7)),
            (
                "ffmpeg version 6.1.1-3ubuntu5 Copyright",
                rel("6.1.1-3ubuntu5", 6),
            ),
            (DEBIAN_VERSION, rel("5.1.6-0+deb12u1", 5)),
            (
                "ffmpeg version 4.4.2-0ubuntu0.22.04.1 Copyright",
                rel("4.4.2-0ubuntu0.22.04.1", 4),
            ),
            ("ffmpeg version 3.4.11 Copyright", rel("3.4.11", 3)),
            (
                "ffmpeg version 7.1-full_build-www.gyan.dev Copyright",
                rel("7.1-full_build-www.gyan.dev", 7),
            ),
            (
                "ffmpeg version N-118000-g1234abcd Copyright",
                git("N-118000-g1234abcd"),
            ),
            (
                "ffmpeg version 2024-11-25-git-04ce01df0b-full_build-www.gyan.dev Copyright",
                git("2024-11-25-git-04ce01df0b-full_build-www.gyan.dev"),
            ),
            (
                "ffmpeg version git-2020-01-01-abc Copyright",
                git("git-2020-01-01-abc"),
            ),
            (
                "ffmpeg version Copyright (c) 2000",
                Some(FfmpegVersion::Unknown),
            ),
            ("ffmpeg version", Some(FfmpegVersion::Unknown)),
            (
                "\r\n  ffmpeg version 7.1 Copyright\r\nbuilt with",
                rel("7.1", 7),
            ),
            ("ffprobe version 7.1", None),
            (GARBAGE, None),
            ("", None),
        ];
        for (text, expected) in rows {
            assert_eq!(parse_version(text), expected, "input: {text:?}");
        }
    }

    #[test]
    fn display_and_is_old() {
        assert_eq!(FfmpegVersion::Unknown.display(), "version unknown");
        assert_eq!(
            parse_version("ffmpeg version 3.4.11 C").unwrap().display(),
            "3.4.11"
        );
        assert!(parse_version("ffmpeg version 3.4.11 C").unwrap().is_old());
        assert!(!parse_version("ffmpeg version 4.4.2 C").unwrap().is_old());
        assert!(!parse_version("ffmpeg version N-1-gabc C").unwrap().is_old());
        assert!(!FfmpegVersion::Unknown.is_old());
    }

    #[test]
    fn mac_encoders_parse() {
        let e = parse_encoders(&mac_encoders());
        assert!(e.listed);
        for n in [
            "libvpx-vp9",
            "libx264",
            "libvpx",
            "h264_videotoolbox",
            "a64multi",
        ] {
            assert!(e.has(n), "missing {n}");
        }
        for n in ["aac", "libopus", "=", "Video", "Encoders:"] {
            assert!(!e.has(n), "unexpected {n}");
        }
    }

    #[test]
    fn debian_encoders_parse() {
        let e = parse_encoders(&debian_encoders());
        for n in [
            "libvpx-vp9",
            "libx264",
            "h264_nvenc",
            "h264_qsv",
            "h264_vaapi",
        ] {
            assert!(e.has(n), "missing {n}");
        }
    }

    #[test]
    fn missing_encoders_are_absent() {
        let no_vpx = parse_encoders(&with_legend(
            " V..... libx264 x\n V....D h264_videotoolbox y\n V.S... mpeg4 z\n",
        ));
        assert!(no_vpx.has("libx264") && !no_vpx.has("libvpx-vp9") && !no_vpx.has("libvpx"));
        let no_x264 = parse_encoders(&with_legend(
            " V....D h264_videotoolbox y\n V..... libvpx a\n V..... libvpx-vp9 b\n V.S... mpeg4 z\n",
        ));
        assert!(no_x264.has("libvpx-vp9") && !no_x264.has("libx264"));
    }

    #[test]
    fn garbage_and_legend_only_are_not_listed() {
        for t in [GARBAGE, "", "Encoders:\n V..... = Video\n"] {
            let e = parse_encoders(t);
            assert!(!e.listed, "input: {t:?}");
            assert_eq!(e, EncoderSet::default());
        }
    }

    #[test]
    fn crlf_encoders_parse_identically() {
        let lf = mac_encoders();
        assert_eq!(
            parse_encoders(&lf.replace('\n', "\r\n")),
            parse_encoders(&lf)
        );
    }

    #[test]
    fn odd_flag_columns_are_ignored() {
        let e = parse_encoders(&with_legend(
            " XXXXXX foo\n V.... short\n VVVVVVV long\n V..... ok\n",
        ));
        assert!(e.has("ok"));
        assert!(!e.has("foo") && !e.has("short") && !e.has("long"));
    }

    #[test]
    fn encoder_names_match_format() {
        assert_eq!(Format::WebM.encoder(), VP9_ENCODER);
        assert_eq!(Format::Mp4.encoder(), X264_ENCODER);
    }
}
