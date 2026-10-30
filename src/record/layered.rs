//! The recording precedence resolver, with the project-file layer.
//!
//! Precedence, highest first: per-call, env, project file `record` section,
//! `record.json`, default. `enabled` and `dir` have no per-call layer (D3-3);
//! `quality` has no per-call or env layer (D3-5).
//!
//! Pure: env is an injected closure and the blade dir a parameter. The project
//! layer arrives with its `dir` already vetted by
//! `launch_mode::vet_project_dir` (inside the project or `<data root>/recordings`).

use std::path::{Path, PathBuf};

use super::config::{FileConfig, RecordConfig, RecordOverrides};
use super::values::{
    check_fps, parse_fps, parse_quality, parse_record_env, Format, FrameSize, RecordEnv, Tabs,
    Toggle, DEFAULT_FPS, DEFAULT_QUALITY, ENV_RECORD, ENV_RECORD_FORMAT, ENV_RECORD_FPS,
    ENV_RECORD_SIZE, ENV_RECORD_TABS, QUEUE_LEN, RECORDINGS_SUBDIR,
};

/// The project file's `record` section plus its path for error messages.
#[derive(Debug, Clone)]
pub struct ProjectRecord<'a> {
    pub cfg: &'a FileConfig,
    pub label: String,
}

/// One parsed file layer (values validated, source-named errors).
#[derive(Default)]
struct Parsed {
    enabled: Option<bool>,
    dir: Option<PathBuf>,
    format: Option<Format>,
    fps: Option<u32>,
    size: Option<FrameSize>,
    quality: Option<u8>,
    tabs: Option<Tabs>,
}

/// Parse and validate a file layer; `name(key)` builds the error prefix
/// (`record.json fps` for the user file, `<path> record.fps` for a project).
fn parse_layer(f: Option<&FileConfig>, name: &dyn Fn(&str) -> String) -> Result<Parsed, String> {
    let Some(f) = f else {
        return Ok(Parsed::default());
    };
    let dir = match &f.dir {
        Some(d) if !d.is_absolute() => {
            return Err(format!(
                "{} {} must be an absolute path",
                name("dir"),
                d.display()
            ))
        }
        other => other.clone(),
    };
    Ok(Parsed {
        enabled: f.enabled,
        dir,
        format: f
            .format
            .as_deref()
            .map(|v| Format::parse(v).map_err(|e| format!("{}: {e}", name("format"))))
            .transpose()?,
        fps: f
            .fps
            .map(|n| check_fps(n).map_err(|e| format!("{}: {e}", name("fps"))))
            .transpose()?,
        size: f
            .size
            .as_deref()
            .map(|v| FrameSize::parse(v).map_err(|e| format!("{}: {e}", name("size"))))
            .transpose()?,
        quality: f
            .quality
            .map(|q| parse_quality(q).map_err(|e| format!("{}: {e}", name("quality"))))
            .transpose()?,
        tabs: f
            .tabs
            .as_deref()
            .map(|v| Tabs::parse(v).map_err(|e| format!("{}: {e}", name("tabs"))))
            .transpose()?,
    })
}

/// Resolve the final settings. Every present layer is parsed before any
/// precedence is applied, so a broken lower layer is an error even when a
/// higher layer overrides it. Errors name their source.
pub fn resolve_layered(
    over: &RecordOverrides,
    env: &dyn Fn(&str) -> Option<String>,
    project: Option<&ProjectRecord>,
    user: Option<&FileConfig>,
    blade_dir: &Path,
) -> Result<RecordConfig, String> {
    // Blank env values mean unset, matching platform.rs.
    let nonblank = |k: &str| env(k).filter(|v| !v.trim().is_empty());

    // --- env layer (parse all of it first) ---
    let rec_env = parse_record_env(nonblank(ENV_RECORD).as_deref())
        .map_err(|e| format!("{ENV_RECORD}: {e}"))?;
    let env_format = nonblank(ENV_RECORD_FORMAT)
        .map(|v| Format::parse(&v).map_err(|e| format!("{ENV_RECORD_FORMAT}: {e}")))
        .transpose()?;
    let env_fps = nonblank(ENV_RECORD_FPS)
        .map(|v| parse_fps(&v).map_err(|e| format!("{ENV_RECORD_FPS}: {e}")))
        .transpose()?;
    let env_size = nonblank(ENV_RECORD_SIZE)
        .map(|v| FrameSize::parse(&v).map_err(|e| format!("{ENV_RECORD_SIZE}: {e}")))
        .transpose()?;
    let env_tabs = nonblank(ENV_RECORD_TABS)
        .map(|v| Tabs::parse(&v).map_err(|e| format!("{ENV_RECORD_TABS}: {e}")))
        .transpose()?;

    // --- file layers ---
    let proj = match project {
        Some(p) => parse_layer(Some(p.cfg), &|k| format!("{} record.{k}", p.label))?,
        None => Parsed::default(),
    };
    let usr = parse_layer(user, &|k| format!("record.json {k}"))?;

    // --- per-call layer ---
    let call_format = over
        .format
        .as_deref()
        .map(|v| Format::parse(v).map_err(|e| format!("per-call format: {e}")))
        .transpose()?;
    let call_fps = over
        .fps
        .as_deref()
        .map(|v| parse_fps(v).map_err(|e| format!("per-call fps: {e}")))
        .transpose()?;
    let call_size = over
        .size
        .as_deref()
        .map(|v| FrameSize::parse(v).map_err(|e| format!("per-call size: {e}")))
        .transpose()?;
    let call_tabs = over
        .tabs
        .as_deref()
        .map(|v| Tabs::parse(v).map_err(|e| format!("per-call tabs: {e}")))
        .transpose()?;

    // --- precedence ---
    // env `off` says nothing about the dir, so the file dir still applies.
    let enabled = match &rec_env {
        RecordEnv::Set(Toggle::Off) => false,
        RecordEnv::Set(Toggle::On(_)) => true,
        RecordEnv::Unset => proj.enabled.or(usr.enabled).unwrap_or(false),
    };
    let dir = match &rec_env {
        RecordEnv::Set(Toggle::On(Some(d))) => d.clone(),
        _ => proj
            .dir
            .or(usr.dir)
            .unwrap_or_else(|| blade_dir.join(RECORDINGS_SUBDIR)),
    };

    // Artifacts rotate at 300 files (src/artifacts.rs); a recording inside
    // that folder would be deleted by a later rotation. Path::starts_with is
    // component-wise, so `artifacts2` is not caught.
    let artifacts = blade_dir.join("artifacts");
    if dir.starts_with(&artifacts) {
        return Err(format!(
            "recordings dir {} is inside {}, which rotates at 300 files; choose another dir",
            dir.display(),
            artifacts.display()
        ));
    }

    Ok(RecordConfig {
        enabled,
        dir,
        format: call_format
            .or(env_format)
            .or(proj.format)
            .or(usr.format)
            .unwrap_or_default(),
        fps: call_fps
            .or(env_fps)
            .or(proj.fps)
            .or(usr.fps)
            .unwrap_or(DEFAULT_FPS),
        size: call_size
            .or(env_size)
            .or(proj.size)
            .or(usr.size)
            .unwrap_or_default(),
        quality: proj.quality.or(usr.quality).unwrap_or(DEFAULT_QUALITY),
        tabs: call_tabs
            .or(env_tabs)
            .or(proj.tabs)
            .or(usr.tabs)
            .unwrap_or_default(),
        queue_len: QUEUE_LEN,
        name: over.name.clone(),
        artifacts_dir: artifacts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn abs(p: &str) -> String {
        if cfg!(windows) {
            format!("C:{}", p.replace('/', "\\"))
        } else {
            p.to_string()
        }
    }

    fn bh() -> PathBuf {
        PathBuf::from(abs("/bh"))
    }

    fn label() -> String {
        abs("/p/.bladebro.json")
    }

    fn go_p(
        env: &[(&str, &str)],
        project: Option<FileConfig>,
        user: Option<FileConfig>,
        over: RecordOverrides,
    ) -> Result<RecordConfig, String> {
        let map: HashMap<String, String> = env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let closure = move |k: &str| map.get(k).cloned();
        let pr = project.as_ref().map(|cfg| ProjectRecord {
            cfg,
            label: label(),
        });
        resolve_layered(&over, &closure, pr.as_ref(), user.as_ref(), &bh())
    }

    fn okp(
        env: &[(&str, &str)],
        project: Option<FileConfig>,
        user: Option<FileConfig>,
    ) -> RecordConfig {
        go_p(env, project, user, RecordOverrides::default()).expect("resolve")
    }

    fn fc() -> FileConfig {
        FileConfig::default()
    }

    #[test]
    fn format_fps_size_precedence_with_project() {
        let proj = || {
            Some(FileConfig {
                format: Some("mp4".into()),
                fps: Some(6),
                size: Some("640x480".into()),
                ..fc()
            })
        };
        let usr = || {
            Some(FileConfig {
                format: Some("webm".into()),
                fps: Some(12),
                size: Some("800x600".into()),
                ..fc()
            })
        };
        let c = okp(&[], proj(), usr());
        assert_eq!((c.format, c.fps), (Format::Mp4, 6));
        assert_eq!(c.size.width, 640);
        // user alone still works
        let c = okp(&[], None, usr());
        assert_eq!((c.format, c.fps, c.size.width), (Format::WebM, 12, 800));
        // env beats project
        let c = okp(
            &[
                ("BLADE_RECORD_FORMAT", "webm"),
                ("BLADE_RECORD_FPS", "9"),
                ("BLADE_RECORD_SIZE", "320x240"),
            ],
            proj(),
            usr(),
        );
        assert_eq!((c.format, c.fps, c.size.width), (Format::WebM, 9, 320));
        // per-call beats all
        let over = RecordOverrides {
            format: Some("mp4".into()),
            fps: Some("14".into()),
            size: Some("1024x768".into()),
            ..Default::default()
        };
        let c = go_p(
            &[("BLADE_RECORD_FORMAT", "webm"), ("BLADE_RECORD_FPS", "9")],
            proj(),
            usr(),
            over,
        )
        .unwrap();
        assert_eq!((c.format, c.fps, c.size.width), (Format::Mp4, 14, 1024));
    }

    #[test]
    fn tabs_parse_table() {
        for (i, want) in [
            ("follow", Tabs::Follow),
            (" FOLLOW ", Tabs::Follow),
            ("per-tab", Tabs::PerTab),
            ("Per-Tab", Tabs::PerTab),
        ] {
            assert_eq!(Tabs::parse(i), Ok(want), "{i:?}");
        }
        for i in ["", "pertab", "per_tab", "tabs"] {
            let e = Tabs::parse(i).unwrap_err();
            assert!(e.contains("follow") && e.contains("per-tab"), "{e}");
            assert!(e.contains(&format!("{:?}", i.trim())), "{e}");
        }
    }

    #[test]
    fn tabs_precedence_default_and_errors() {
        let t = |v: &str| {
            Some(FileConfig {
                tabs: Some(v.into()),
                ..fc()
            })
        };
        assert_eq!(okp(&[], None, None).tabs, Tabs::Follow);
        let over = RecordOverrides {
            tabs: Some("per-tab".into()),
            ..Default::default()
        };
        let c = go_p(
            &[("BLADE_RECORD_TABS", "follow")],
            t("follow"),
            t("follow"),
            over,
        )
        .unwrap();
        assert_eq!(c.tabs, Tabs::PerTab);
        // env wins over project over user
        let c = okp(
            &[("BLADE_RECORD_TABS", "per-tab")],
            t("follow"),
            t("follow"),
        );
        assert_eq!(c.tabs, Tabs::PerTab);
        assert_eq!(okp(&[], t("per-tab"), t("follow")).tabs, Tabs::PerTab);
        assert_eq!(okp(&[], t("follow"), t("per-tab")).tabs, Tabs::Follow);
        assert_eq!(okp(&[], None, t("per-tab")).tabs, Tabs::PerTab);
        // blank env = unset
        assert_eq!(
            okp(&[("BLADE_RECORD_TABS", "  ")], None, t("per-tab")).tabs,
            Tabs::PerTab
        );
        // a broken lower layer errors even when env overrides it
        let e = go_p(
            &[("BLADE_RECORD_TABS", "per-tab")],
            None,
            t("x"),
            Default::default(),
        )
        .unwrap_err();
        assert!(e.starts_with("record.json tabs:"), "{e}");
        let e = go_p(&[], t("x"), None, Default::default()).unwrap_err();
        assert!(e.starts_with(&format!("{} record.tabs:", label())), "{e}");
        let e = go_p(
            &[("BLADE_RECORD_TABS", "x")],
            None,
            None,
            Default::default(),
        )
        .unwrap_err();
        assert!(e.starts_with("BLADE_RECORD_TABS:"), "{e}");
        let over = RecordOverrides {
            tabs: Some("x".into()),
            ..Default::default()
        };
        let e = go_p(&[], None, None, over).unwrap_err();
        assert!(e.starts_with("per-call tabs:"), "{e}");
    }

    #[test]
    fn enabled_precedence() {
        let t = || {
            Some(FileConfig {
                enabled: Some(true),
                ..fc()
            })
        };
        let f = || {
            Some(FileConfig {
                enabled: Some(false),
                ..fc()
            })
        };
        assert!(okp(&[], t(), f()).enabled);
        assert!(!okp(&[("BLADE_RECORD", "off")], t(), None).enabled);
        assert!(okp(&[], None, t()).enabled);
        assert!(!okp(&[], None, f()).enabled);
        assert!(!okp(&[], Some(fc()), None).enabled);
    }

    #[test]
    fn dir_precedence() {
        let pd = || {
            Some(FileConfig {
                dir: Some(PathBuf::from(abs("/p/rec"))),
                ..fc()
            })
        };
        let ud = || {
            Some(FileConfig {
                dir: Some(PathBuf::from(abs("/u/rec"))),
                ..fc()
            })
        };
        assert_eq!(okp(&[], pd(), ud()).dir, PathBuf::from(abs("/p/rec")));
        let c = okp(&[("BLADE_RECORD", abs("/e/rec").as_str())], pd(), ud());
        assert_eq!(c.dir, PathBuf::from(abs("/e/rec")));
        // project dir dropped (None) -> user dir, else default
        assert_eq!(okp(&[], Some(fc()), ud()).dir, PathBuf::from(abs("/u/rec")));
        assert_eq!(okp(&[], Some(fc()), None).dir, bh().join("recordings"));
    }

    #[test]
    fn quality_precedence() {
        let q = |n| {
            Some(FileConfig {
                quality: Some(n),
                ..fc()
            })
        };
        assert_eq!(okp(&[], q(90), q(85)).quality, 90);
        assert_eq!(okp(&[], q(90), None).quality, 90);
        assert_eq!(okp(&[], None, q(85)).quality, 85);
        assert_eq!(okp(&[], None, None).quality, 70);
    }

    #[test]
    fn project_errors_name_the_file_and_key() {
        let e = go_p(
            &[],
            Some(FileConfig {
                fps: Some(4),
                ..fc()
            }),
            None,
            Default::default(),
        )
        .unwrap_err();
        assert!(e.starts_with(&label()) && e.contains("record.fps"), "{e}");
        // broken project layer errors even when env overrides it
        let e = go_p(
            &[("BLADE_RECORD_FPS", "9")],
            Some(FileConfig {
                fps: Some(4),
                ..fc()
            }),
            None,
            Default::default(),
        )
        .unwrap_err();
        assert!(e.contains("record.fps"), "{e}");
        // relative project dir is an error (the loader makes it absolute)
        let e = go_p(
            &[],
            Some(FileConfig {
                dir: Some(PathBuf::from("rel")),
                ..fc()
            }),
            None,
            Default::default(),
        )
        .unwrap_err();
        assert!(e.contains("absolute"), "{e}");
    }

    #[test]
    fn artifacts_guard_applies_to_project_dir() {
        let e = go_p(
            &[],
            Some(FileConfig {
                dir: Some(PathBuf::from(abs("/bh/artifacts/x"))),
                ..fc()
            }),
            None,
            Default::default(),
        )
        .unwrap_err();
        assert!(e.contains("artifacts"), "{e}");
    }
}
