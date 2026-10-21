//! `bladebro doctor` report for ffmpeg (OUT-02). Pure decision logic lives in
//! [`doctor_line`]; [`doctor_check`] is the impure wrapper the doctor hook calls.
//!
//! Status rule (D4-8): WARN when ffmpeg is missing/unusable, the encoder list
//! is unreadable, the configured format's encoder is missing, or the record
//! config could not be read. Everything else is OK. Never a failure: recording
//! is opt-in and doctor must not exit non-zero for it.

use super::lookup::{Found, Lookup, TargetOs};
use super::parse::{EncoderSet, VP9_ENCODER, X264_ENCODER};
use super::probe::{discover_cached, Discovery, ProbeInfo};
use crate::record::values::Format;
use crate::record::RecordOverrides;

/// Doctor check name (doctor pads names to 24 columns).
pub const CHECK_NAME: &str = "ffmpeg (recording)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
}

/// Mirrors doctor.rs printing: one line, plus a `fix:` line when `fix` is Some.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorLine {
    pub level: Level,
    pub detail: String,
    pub fix: Option<String>,
}

impl DoctorLine {
    pub fn line_count(&self) -> usize {
        1 + usize::from(self.fix.is_some())
    }
}

/// Per-OS install command (D4-11).
pub fn install_hint(os: TargetOs) -> &'static str {
    match os {
        TargetOs::MacOs => "brew install ffmpeg",
        TargetOs::Linux => "sudo apt install ffmpeg (or your distro's ffmpeg package)",
        TargetOs::Windows => "winget install ffmpeg",
    }
}

fn warn(detail: String, fix: String) -> DoctorLine {
    DoctorLine {
        level: Level::Warn,
        detail,
        fix: Some(fix),
    }
}

fn ok_missing(b: bool) -> &'static str {
    if b {
        "ok"
    } else {
        "missing"
    }
}

fn other(f: Format) -> Format {
    match f {
        Format::WebM => Format::Mp4,
        Format::Mp4 => Format::WebM,
    }
}

fn fmt_name(f: Format) -> &'static str {
    match f {
        Format::WebM => "WebM",
        Format::Mp4 => "MP4",
    }
}

fn has(enc: &EncoderSet, f: Format) -> bool {
    enc.has(f.encoder())
}

/// Decide the doctor line. `configured` is the record config's format or the
/// resolver error (D4-10: then evaluated as WebM).
pub fn doctor_line(d: &Discovery, configured: Result<Format, String>, os: TargetOs) -> DoctorLine {
    let hint = install_hint(os);
    let (found, probe) = match &d.lookup {
        Lookup::NotFound => {
            return warn(
                "not found (BLADE_FFMPEG unset; not on PATH or in known locations); recording is opt-in"
                    .to_string(),
                format!("{hint}, or set BLADE_FFMPEG=/path/to/ffmpeg"),
            )
        }
        Lookup::OverrideUnusable { value, reason } => {
            return warn(
                format!("BLADE_FFMPEG={value} unusable: {reason}; PATH not searched"),
                "fix or unset BLADE_FFMPEG (path to a working ffmpeg)".to_string(),
            )
        }
        Lookup::Found(f) => (f, d.probe.as_ref()),
    };
    let Found { path, source } = found;
    let path = path.display();
    let info: &ProbeInfo = match probe {
        None => {
            return warn(
                format!("{path} (via {}) not probed", source.label()),
                "re-run doctor, or point BLADE_FFMPEG at a working ffmpeg".to_string(),
            )
        }
        Some(Err(e)) => {
            return warn(
                format!("{path} (via {}) unusable: {e}", source.label()),
                format!("{hint}, or point BLADE_FFMPEG at a working ffmpeg"),
            )
        }
        Some(Ok(i)) => i,
    };
    if !info.encoders.listed {
        return warn(
            format!(
                "{path} ({} via {}) could not read the encoder list",
                info.version.display(),
                source.label()
            ),
            format!("{hint}, or point BLADE_FFMPEG at a working ffmpeg"),
        );
    }
    let (cfg_fmt, cfg_err) = match configured {
        Ok(f) => (f, None),
        Err(e) => (Format::WebM, Some(e)),
    };
    let vp9 = info.encoders.has(VP9_ENCODER);
    let x264 = info.encoders.has(X264_ENCODER);
    let old = if info.version.is_old() {
        " (old; 4+ recommended)"
    } else {
        ""
    };
    let note = if cfg_err.is_some() {
        "; record config unreadable, assumed webm"
    } else {
        ""
    };
    let detail = format!(
        "{path} ({}{old} via {}; WebM/VP9 {}, MP4/x264 {}){note}",
        info.version.display(),
        source.label(),
        ok_missing(vp9),
        ok_missing(x264)
    );
    let mut fixes: Vec<String> = Vec::new();
    if !has(&info.encoders, cfg_fmt) {
        let mut fix = format!(
            "{} recording needs {}: install an ffmpeg built with it ({hint}) or point BLADE_FFMPEG at one",
            fmt_name(cfg_fmt),
            cfg_fmt.encoder()
        );
        let o = other(cfg_fmt);
        if has(&info.encoders, o) {
            fix.push_str(&format!("; or set BLADE_RECORD_FORMAT={}", o.ext()));
        }
        fixes.push(fix);
    }
    if let Some(e) = cfg_err {
        fixes.push(e.replace('\n', " "));
    }
    if fixes.is_empty() {
        DoctorLine {
            level: Level::Ok,
            detail,
            fix: None,
        }
    } else {
        warn(detail, fixes.join("; "))
    }
}

/// Impure wrapper for the doctor hook: real discovery plus the real record
/// config. No unit test (it wires tested pieces); never panics; the first call
/// blocks at most 2 x PROBE_TIMEOUT (D4-1).
pub fn doctor_check() -> DoctorLine {
    doctor_line(
        discover_cached(),
        crate::record::resolve_from_env(&RecordOverrides::default()).map(|c| c.format),
        TargetOs::current(),
    )
}

#[cfg(test)]
mod tests {
    use super::super::lookup::Source;
    use super::super::parse::parse_version;
    use super::*;
    use std::path::PathBuf;

    fn found_with(
        path: &str,
        source: Source,
        probe: Option<Result<ProbeInfo, String>>,
    ) -> Discovery {
        Discovery {
            lookup: Lookup::Found(Found {
                path: PathBuf::from(path),
                source,
            }),
            probe,
        }
    }

    fn info(version_line: &str, encoders: &[&str], listed: bool) -> ProbeInfo {
        ProbeInfo {
            version: parse_version(version_line).expect("banner"),
            encoders: EncoderSet::from_names(encoders, listed),
        }
    }

    fn found(path: &str, source: Source, ver: &str, enc: &[&str]) -> Discovery {
        found_with(path, source, Some(Ok(info(ver, enc, true))))
    }

    const BOTH: [&str; 2] = [VP9_ENCODER, X264_ENCODER];
    const V902: &str = "ffmpeg version n9.0.2 Copyright (c) 2000-2026 the FFmpeg developers";
    const MAC: TargetOs = TargetOs::MacOs;
    const BREW: &str = "/opt/homebrew/bin/ffmpeg";

    fn webm() -> Result<Format, String> {
        Ok(Format::WebM)
    }

    fn contains_all(s: &str, parts: &[&str]) {
        for p in parts {
            assert!(s.contains(p), "{s:?} should contain {p:?}");
        }
    }

    struct Row {
        name: &'static str,
        d: Discovery,
        cfg: Result<Format, String>,
        os: TargetOs,
        level: Level,
        detail: &'static [&'static str],
        fix: Option<&'static [&'static str]>,
        not_in_fix: &'static [&'static str],
    }

    fn rows() -> Vec<Row> {
        let bad_cfg = || {
            Err("BLADE_RECORD_FORMAT: format \"vp8\" is not supported; use webm or mp4 (default webm)"
                .to_string())
        };
        let nf = |os| {
            (
                Discovery {
                    lookup: Lookup::NotFound,
                    probe: None,
                },
                os,
            )
        };
        let (nf_mac, _) = nf(MAC);
        let (nf_lin, _) = nf(TargetOs::Linux);
        let (nf_win, _) = nf(TargetOs::Windows);
        vec![
            Row {
                name: "1 all well",
                d: found(BREW, Source::Path, V902, &BOTH),
                cfg: webm(),
                os: MAC,
                level: Level::Ok,
                detail: &["/opt/homebrew/bin/ffmpeg (n9.0.2 via PATH; WebM/VP9 ok, MP4/x264 ok)"],
                fix: None,
                not_in_fix: &[],
            },
            Row {
                name: "2 vp9 only",
                d: found(BREW, Source::Path, V902, &[VP9_ENCODER]),
                cfg: webm(),
                os: MAC,
                level: Level::Ok,
                detail: &["WebM/VP9 ok, MP4/x264 missing"],
                fix: None,
                not_in_fix: &[],
            },
            Row {
                name: "3 x264 only, webm",
                d: found(BREW, Source::Path, V902, &[X264_ENCODER]),
                cfg: webm(),
                os: MAC,
                level: Level::Warn,
                detail: &["WebM/VP9 missing", "MP4/x264 ok"],
                fix: Some(&[
                    "libvpx-vp9",
                    "BLADE_FFMPEG",
                    "brew install ffmpeg",
                    "BLADE_RECORD_FORMAT=mp4",
                ]),
                not_in_fix: &[],
            },
            Row {
                name: "4 vp9 only, mp4",
                d: found(BREW, Source::Path, V902, &[VP9_ENCODER]),
                cfg: Ok(Format::Mp4),
                os: MAC,
                level: Level::Warn,
                detail: &["MP4/x264 missing"],
                fix: Some(&["libx264", "BLADE_FFMPEG", "BLADE_RECORD_FORMAT=webm"]),
                not_in_fix: &[],
            },
            Row {
                name: "5 no encoders",
                d: found(BREW, Source::Path, V902, &[]),
                cfg: webm(),
                os: MAC,
                level: Level::Warn,
                detail: &["WebM/VP9 missing, MP4/x264 missing"],
                fix: Some(&["libvpx-vp9", "BLADE_FFMPEG"]),
                not_in_fix: &["BLADE_RECORD_FORMAT"],
            },
            Row {
                name: "6 not found mac",
                d: nf_mac,
                cfg: webm(),
                os: MAC,
                level: Level::Warn,
                detail: &["not found", "recording is opt-in"],
                fix: Some(&["brew install ffmpeg", "BLADE_FFMPEG"]),
                not_in_fix: &[],
            },
            Row {
                name: "7 not found linux",
                d: nf_lin,
                cfg: webm(),
                os: TargetOs::Linux,
                level: Level::Warn,
                detail: &["not found"],
                fix: Some(&["apt install ffmpeg", "BLADE_FFMPEG"]),
                not_in_fix: &[],
            },
            Row {
                name: "8 not found windows",
                d: nf_win,
                cfg: webm(),
                os: TargetOs::Windows,
                level: Level::Warn,
                detail: &["not found"],
                fix: Some(&["winget install ffmpeg", "BLADE_FFMPEG"]),
                not_in_fix: &[],
            },
            Row {
                name: "9 override unusable",
                d: Discovery {
                    lookup: Lookup::OverrideUnusable {
                        value: "/nonexistent/ffmpeg".into(),
                        reason: "not found".into(),
                    },
                    probe: None,
                },
                cfg: webm(),
                os: MAC,
                level: Level::Warn,
                detail: &[
                    "BLADE_FFMPEG=/nonexistent/ffmpeg",
                    "not found",
                    "PATH not searched",
                ],
                fix: Some(&["BLADE_FFMPEG"]),
                not_in_fix: &[],
            },
            Row {
                name: "10 override probe err",
                d: found_with(
                    "/x/ffmpeg",
                    Source::Override,
                    Some(Err("timed out after 5s".into())),
                ),
                cfg: webm(),
                os: MAC,
                level: Level::Warn,
                detail: &["BLADE_FFMPEG", "/x/ffmpeg", "timed out"],
                fix: Some(&["BLADE_FFMPEG"]),
                not_in_fix: &[],
            },
            Row {
                name: "11 not ffmpeg",
                d: found_with(
                    BREW,
                    Source::Path,
                    Some(Err("does not look like ffmpeg (first line: hello)".into())),
                ),
                cfg: webm(),
                os: MAC,
                level: Level::Warn,
                detail: &[BREW, "does not look like ffmpeg"],
                fix: Some(&["BLADE_FFMPEG"]),
                not_in_fix: &[],
            },
            Row {
                name: "12 not listed",
                d: found_with(BREW, Source::Path, Some(Ok(info(V902, &[], false)))),
                cfg: webm(),
                os: MAC,
                level: Level::Warn,
                detail: &["could not read the encoder list"],
                fix: Some(&["BLADE_FFMPEG"]),
                not_in_fix: &[],
            },
            Row {
                name: "13 old",
                d: found(BREW, Source::Path, "ffmpeg version 3.4.11", &BOTH),
                cfg: webm(),
                os: MAC,
                level: Level::Ok,
                detail: &["3.4.11", "old; 4+ recommended"],
                fix: None,
                not_in_fix: &[],
            },
            Row {
                name: "14 git build",
                d: found(
                    BREW,
                    Source::Path,
                    "ffmpeg version N-118000-g1234abcd",
                    &BOTH,
                ),
                cfg: webm(),
                os: MAC,
                level: Level::Ok,
                detail: &["N-118000-g1234abcd"],
                fix: None,
                not_in_fix: &[],
            },
            Row {
                name: "15 unknown version",
                d: found(BREW, Source::Path, "ffmpeg version Copyright", &BOTH),
                cfg: webm(),
                os: MAC,
                level: Level::Ok,
                detail: &["version unknown"],
                fix: None,
                not_in_fix: &[],
            },
            Row {
                name: "16 bad config",
                d: found(BREW, Source::Path, V902, &BOTH),
                cfg: bad_cfg(),
                os: MAC,
                level: Level::Warn,
                detail: &["record config unreadable, assumed webm"],
                fix: Some(&["BLADE_RECORD_FORMAT"]),
                not_in_fix: &[],
            },
            Row {
                name: "17 bad config, x264 only",
                d: found(BREW, Source::Path, V902, &[X264_ENCODER]),
                cfg: bad_cfg(),
                os: MAC,
                level: Level::Warn,
                detail: &["WebM/VP9 missing", "assumed webm"],
                fix: Some(&["libvpx-vp9"]),
                not_in_fix: &[],
            },
            Row {
                name: "18 known location",
                d: found(
                    "/home/u/.local/ffmpeg/bin/ffmpeg",
                    Source::Known,
                    V902,
                    &BOTH,
                ),
                cfg: webm(),
                os: MAC,
                level: Level::Ok,
                detail: &["via known location"],
                fix: None,
                not_in_fix: &[],
            },
        ]
    }

    #[test]
    fn doctor_table() {
        for r in rows() {
            let line = doctor_line(&r.d, r.cfg.clone(), r.os);
            assert_eq!(line.level, r.level, "row {}: {line:?}", r.name);
            contains_all(&line.detail, r.detail);
            match (r.fix, &line.fix) {
                (None, None) => {}
                (Some(parts), Some(fix)) => {
                    contains_all(fix, parts);
                    for n in r.not_in_fix {
                        assert!(!fix.contains(n), "row {}: fix has {n}", r.name);
                    }
                }
                (want, got) => panic!("row {}: fix want {want:?} got {got:?}", r.name),
            }
            assert!(!line.detail.contains('\n'), "row {}", r.name);
            if let Some(f) = &line.fix {
                assert!(!f.contains('\n'), "row {}", r.name);
                if !r.name.starts_with("16") {
                    assert!(f.contains("BLADE_FFMPEG"), "row {}: {f}", r.name);
                }
            }
            assert_eq!(line.line_count() == 1, line.fix.is_none(), "row {}", r.name);
        }
    }

    #[test]
    fn all_well_is_one_exact_line() {
        let l = doctor_line(&found(BREW, Source::Path, V902, &BOTH), webm(), MAC);
        assert_eq!(l.line_count(), 1);
        assert_eq!(
            l.detail,
            "/opt/homebrew/bin/ffmpeg (n9.0.2 via PATH; WebM/VP9 ok, MP4/x264 ok)"
        );
    }

    #[test]
    fn check_name_fits_doctor_column() {
        assert!(CHECK_NAME.len() <= 24);
    }

    #[test]
    fn install_hints_non_empty() {
        for os in [TargetOs::MacOs, TargetOs::Linux, TargetOs::Windows] {
            assert!(!install_hint(os).is_empty());
        }
    }

    #[test]
    fn found_without_probe_warns_not_panics() {
        let l = doctor_line(&found_with(BREW, Source::Path, None), webm(), MAC);
        assert_eq!(l.level, Level::Warn);
        assert!(l.detail.contains("not probed"));
    }

    #[test]
    fn suggestions_never_mention_vp8() {
        for r in rows() {
            let l = doctor_line(&r.d, r.cfg, r.os);
            let all = format!("{} {}", l.detail, l.fix.unwrap_or_default());
            // The resolver error may echo the user's own "vp8"; we never suggest it.
            assert!(
                !all.contains("libvpx ") && !all.contains("libvpx\n"),
                "{all}"
            );
            assert!(!all.contains("=vp8") && !all.contains("set BLADE_RECORD_FORMAT=vp8"));
        }
    }
}
