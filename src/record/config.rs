//! Recording configuration: `record.json` loading and the precedence resolver.
//!
//! Precedence, highest first: per-call > env > project file > `record.json` >
//! default (implemented in `layered.rs`; `resolve` here is the no-project delegate).
//! `enabled` and `dir` have no per-call layer (D3-3): the caller is an LLM,
//! and the output directory is an operator decision.
//!
//! The resolver is pure (env is an injected closure, the blade dir is a
//! parameter) so tests never mutate process env or touch a real `~/.blade`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[cfg(test)]
use super::values::check_fps;
use super::values::{Format, FrameSize, Tabs, CONFIG_FILE};

/// A six-field file; anything bigger is a mistake, not a config.
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

/// `<blade_dir>/record.json`, as written by the operator. Unknown fields are
/// an error (D3-2): a typo such as `"fsp": 12` must not be silently ignored.
/// Values stay raw here and go through the same validators as env and
/// per-call input, so every layer has one validation path.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    /// Turn recording on or off.
    pub enabled: Option<bool>,
    /// Output folder. Must be absolute.
    pub dir: Option<PathBuf>,
    /// `webm` or `mp4`.
    pub format: Option<String>,
    /// Frames per second, 5..=15.
    pub fps: Option<i64>,
    /// `WIDTHxHEIGHT`, both even, 160x120..=3840x2160.
    pub size: Option<String>,
    /// JPEG quality, 1..=100. File-only (D3-5).
    pub quality: Option<i64>,
    /// `follow` or `per-tab`.
    pub tabs: Option<String>,
}

/// Per-call (LLM-supplied) overrides as raw strings. They are parsed and
/// validated by `resolve`. There is deliberately NO per-call `dir` (D3-3).
/// `name` is passed through raw; the sanitizer (03-02) owns its rules.
#[derive(Debug, Clone, Default)]
pub struct RecordOverrides {
    /// Requested recording name (sanitized later).
    pub name: Option<String>,
    /// `webm` or `mp4`.
    pub format: Option<String>,
    /// Frames per second, 5..=15.
    pub fps: Option<String>,
    /// `WIDTHxHEIGHT`.
    pub size: Option<String>,
    /// `follow` or `per-tab`. Resolver parity only: the `state` tool has no
    /// such parameter (every schema word is billed to every user).
    pub tabs: Option<String>,
}

/// Fully resolved recording settings.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordConfig {
    /// Whether recording is on.
    pub enabled: bool,
    /// Directory the recordings are written to.
    pub dir: PathBuf,
    /// Output container and codec.
    pub format: Format,
    /// Frames per second.
    pub fps: u32,
    /// Output frame size.
    pub size: FrameSize,
    /// JPEG quality.
    pub quality: u8,
    /// Multi-tab mode.
    pub tabs: Tabs,
    /// Frame queue depth (constant, see `QUEUE_LEN`).
    pub queue_len: usize,
    /// Requested name, raw (sanitized in 03-02).
    pub name: Option<String>,
    /// `<blade_dir>/artifacts`; `prepare_dir_guarded` refuses a recordings dir
    /// that resolves into it (symlinks, `..`, case-insensitive FS included).
    pub artifacts_dir: PathBuf,
}

/// Load `record.json`. A missing file is `Ok(None)`: no config is a normal
/// state. Any other problem is an error that names the file.
pub fn load_file_config(path: &Path) -> Result<Option<FileConfig>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "cannot read recording config {}: {e}",
                path.display()
            ))
        }
    };
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(format!(
            "recording config {} is {} bytes; the limit is 64 KiB",
            path.display(),
            bytes.len()
        ));
    }
    serde_json::from_slice::<FileConfig>(&bytes)
        .map(Some)
        .map_err(|e| {
            format!(
                "invalid recording config {}: {e} (fields: enabled, dir, format, fps, size, quality)",
                path.display()
            )
        })
}

/// Resolve with no project layer: kept for existing callers and tests; the
/// logic lives in `layered::resolve_layered`.
pub fn resolve(
    over: &RecordOverrides,
    env: &dyn Fn(&str) -> Option<String>,
    file: Option<&FileConfig>,
    blade_dir: &Path,
) -> Result<RecordConfig, String> {
    super::layered::resolve_layered(over, env, None, file, blade_dir)
}

/// Impure wrapper: reads the real env, `<blade_dir>/record.json` and the
/// project file (`.bladebro.json` found from the cwd), then calls
/// `resolve_layered`. The project `record.dir` was already vetted by
/// `launch_mode::vet_project_dir` (inside the project or `<data root>/recordings`).
/// It only wires already-tested pieces, so it has no unit test of its own.
pub fn resolve_from_env(over: &RecordOverrides) -> Result<RecordConfig, String> {
    let bd = crate::platform::blade_dir();
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let file = load_file_config(&bd.join(CONFIG_FILE))?;
    // No cwd (deleted directory): no project layer.
    let cwd = std::env::current_dir().unwrap_or_default();
    let home = crate::platform::home_dir();
    let (project, warnings) = crate::launch_mode::project_in(&cwd, Some(&home), &bd)?;
    for w in warnings {
        eprintln!("[bladebro] warning: {w}");
    }
    let rec = project.as_ref().and_then(|l| {
        l.record.as_ref().map(|cfg| super::layered::ProjectRecord {
            cfg,
            label: l.path.display().to_string(),
        })
    });
    super::layered::resolve_layered(over, &env, rec.as_ref(), file.as_ref(), &bd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Absolute-path literal that is absolute on the current OS (see values.rs).
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

    fn file(f: FileConfig) -> Option<FileConfig> {
        Some(f)
    }

    /// Resolve with explicit env pairs, optional file, blank per-call.
    fn go(
        env: &[(&str, &str)],
        f: Option<FileConfig>,
        over: RecordOverrides,
    ) -> Result<RecordConfig, String> {
        let map: HashMap<String, String> = env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let closure = move |k: &str| map.get(k).cloned();
        resolve(&over, &closure, f.as_ref(), &bh())
    }

    fn ok(env: &[(&str, &str)], f: Option<FileConfig>) -> RecordConfig {
        go(env, f, RecordOverrides::default()).expect("resolve should succeed")
    }

    fn err_of(env: &[(&str, &str)], f: Option<FileConfig>, over: RecordOverrides) -> String {
        go(env, f, over).expect_err("resolve should fail")
    }

    #[test]
    fn off_by_default_with_documented_defaults() {
        let c = ok(&[], None);
        assert!(!c.enabled);
        assert_eq!(c.dir, bh().join("recordings"));
        assert_eq!(c.format, Format::WebM);
        assert_eq!(c.fps, 10);
        assert_eq!(
            c.size,
            FrameSize {
                width: 1280,
                height: 720
            }
        );
        assert_eq!(c.quality, 70);
        assert_eq!(c.queue_len, 16);
        assert_eq!(c.name, None);
    }

    #[test]
    fn blade_record_env_switches() {
        let c = ok(&[("BLADE_RECORD", abs("/abs/rec").as_str())], None);
        assert!(c.enabled);
        assert_eq!(c.dir, PathBuf::from(abs("/abs/rec")));

        let c = ok(&[("BLADE_RECORD", "1")], None);
        assert!(c.enabled);
        assert_eq!(c.dir, bh().join("recordings"));

        // env off wins over file enabled, but the file dir still applies.
        let c = ok(
            &[("BLADE_RECORD", "off")],
            file(FileConfig {
                enabled: Some(true),
                dir: Some(PathBuf::from(abs("/f"))),
                ..Default::default()
            }),
        );
        assert!(!c.enabled);
        assert_eq!(c.dir, PathBuf::from(abs("/f")));
    }

    #[test]
    fn file_enabled_and_dir_alone() {
        let c = ok(
            &[],
            file(FileConfig {
                enabled: Some(true),
                ..Default::default()
            }),
        );
        assert!(c.enabled);
        assert_eq!(c.dir, bh().join("recordings"));

        let c = ok(
            &[],
            file(FileConfig {
                dir: Some(PathBuf::from(abs("/f"))),
                ..Default::default()
            }),
        );
        assert!(!c.enabled);
        assert_eq!(c.dir, PathBuf::from(abs("/f")));
    }

    #[test]
    fn format_precedence_each_layer_wins_in_turn() {
        let file_mp4 = || {
            file(FileConfig {
                format: Some("mp4".into()),
                ..Default::default()
            })
        };
        // default only
        assert_eq!(ok(&[], None).format, Format::WebM);
        // file beats default
        assert_eq!(ok(&[], file_mp4()).format, Format::Mp4);
        // env beats file
        assert_eq!(
            ok(&[("BLADE_RECORD_FORMAT", "webm")], file_mp4()).format,
            Format::WebM
        );
        // per-call beats env and file
        let over = RecordOverrides {
            format: Some("mp4".into()),
            ..Default::default()
        };
        let c = go(&[("BLADE_RECORD_FORMAT", "webm")], file_mp4(), over).unwrap();
        assert_eq!(c.format, Format::Mp4);
    }

    #[test]
    fn fps_precedence_each_layer_wins_in_turn() {
        let file_6 = || {
            file(FileConfig {
                fps: Some(6),
                ..Default::default()
            })
        };
        assert_eq!(ok(&[], None).fps, 10);
        assert_eq!(ok(&[], file_6()).fps, 6);
        assert_eq!(ok(&[("BLADE_RECORD_FPS", "8")], file_6()).fps, 8);
        let over = RecordOverrides {
            fps: Some("12".into()),
            ..Default::default()
        };
        let c = go(&[("BLADE_RECORD_FPS", "8")], file_6(), over).unwrap();
        assert_eq!(c.fps, 12);
    }

    #[test]
    fn size_precedence_each_layer_wins_in_turn() {
        let file_640 = || {
            file(FileConfig {
                size: Some("640x480".into()),
                ..Default::default()
            })
        };
        assert_eq!(
            ok(&[], None).size,
            FrameSize {
                width: 1280,
                height: 720
            }
        );
        assert_eq!(
            ok(&[], file_640()).size,
            FrameSize {
                width: 640,
                height: 480
            }
        );
        assert_eq!(
            ok(&[("BLADE_RECORD_SIZE", "800x600")], file_640()).size,
            FrameSize {
                width: 800,
                height: 600
            }
        );
        let over = RecordOverrides {
            size: Some("1024x768".into()),
            ..Default::default()
        };
        let c = go(&[("BLADE_RECORD_SIZE", "800x600")], file_640(), over).unwrap();
        assert_eq!(
            c.size,
            FrameSize {
                width: 1024,
                height: 768
            }
        );
    }

    #[test]
    fn quality_comes_from_file_or_default_only() {
        assert_eq!(ok(&[], None).quality, 70);
        let c = ok(
            &[],
            file(FileConfig {
                quality: Some(85),
                ..Default::default()
            }),
        );
        assert_eq!(c.quality, 85);
    }

    #[test]
    fn name_passes_through_raw() {
        let over = RecordOverrides {
            name: Some("My Run/../x".into()),
            ..Default::default()
        };
        let c = go(&[], None, over).unwrap();
        assert_eq!(c.name.as_deref(), Some("My Run/../x"));
    }

    #[test]
    fn invalid_values_error_and_name_their_source() {
        let e = err_of(&[("BLADE_RECORD_FPS", "16")], None, Default::default());
        assert!(e.starts_with("BLADE_RECORD_FPS:"), "{e}");

        let e = err_of(
            &[],
            file(FileConfig {
                fps: Some(4),
                ..Default::default()
            }),
            Default::default(),
        );
        assert!(e.contains("record.json"), "{e}");

        let over = RecordOverrides {
            fps: Some("0".into()),
            ..Default::default()
        };
        let e = err_of(&[], None, over);
        assert!(e.contains("per-call"), "{e}");

        let e = err_of(
            &[],
            file(FileConfig {
                quality: Some(101),
                ..Default::default()
            }),
            Default::default(),
        );
        assert!(e.contains("quality"), "{e}");

        let e = err_of(
            &[("BLADE_RECORD_SIZE", "garbage")],
            None,
            Default::default(),
        );
        assert!(e.starts_with("BLADE_RECORD_SIZE:"), "{e}");

        let e = err_of(&[("BLADE_RECORD_FORMAT", "avi")], None, Default::default());
        assert!(e.starts_with("BLADE_RECORD_FORMAT:"), "{e}");
    }

    #[test]
    fn relative_dirs_are_rejected() {
        let e = err_of(&[("BLADE_RECORD", "rel/dir")], None, Default::default());
        assert!(e.contains("absolute"), "{e}");

        let e = err_of(
            &[],
            file(FileConfig {
                dir: Some(PathBuf::from("rel/dir")),
                ..Default::default()
            }),
            Default::default(),
        );
        assert!(e.contains("absolute"), "{e}");
    }

    #[test]
    fn broken_lower_layer_errors_even_when_overridden() {
        // A broken env var must surface, not hide behind a valid per-call value.
        let over = RecordOverrides {
            fps: Some("10".into()),
            ..Default::default()
        };
        let e = err_of(&[("BLADE_RECORD_FPS", "abc")], None, over);
        assert!(e.starts_with("BLADE_RECORD_FPS:"), "{e}");
    }

    #[test]
    fn artifacts_dir_is_rejected_but_sibling_names_pass() {
        for bad in ["/bh/artifacts", "/bh/artifacts/x"] {
            let e = err_of(
                &[("BLADE_RECORD", abs(bad).as_str())],
                None,
                Default::default(),
            );
            assert!(e.contains("artifacts"), "{bad}: {e}");
        }
        let c = ok(&[("BLADE_RECORD", abs("/bh/artifacts2").as_str())], None);
        assert_eq!(c.dir, PathBuf::from(abs("/bh/artifacts2")));
    }

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A unique scratch directory under the system temp dir. Never a real
    /// blade home.
    fn scratch() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("bladebro-rec-cfg-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&d).expect("create scratch dir");
        d
    }

    #[test]
    fn load_file_config_cases() {
        let d = scratch();
        let path = d.join("record.json");

        // missing file is not an error
        assert_eq!(load_file_config(&path), Ok(None));

        // valid, all six fields
        let full = format!(
            r#"{{"enabled":true,"dir":{:?},"format":"mp4","fps":8,"size":"800x600","quality":80}}"#,
            abs("/abs/x")
        );
        std::fs::write(&path, full).unwrap();
        let f = load_file_config(&path).unwrap().expect("some");
        assert_eq!(f.enabled, Some(true));
        assert_eq!(f.dir, Some(PathBuf::from(abs("/abs/x"))));
        assert_eq!(f.format.as_deref(), Some("mp4"));
        assert_eq!(f.fps, Some(8));
        assert_eq!(f.size.as_deref(), Some("800x600"));
        assert_eq!(f.quality, Some(80));

        // empty object is valid and all-None
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(
            load_file_config(&path).unwrap(),
            Some(FileConfig::default())
        );

        // unknown field is an error naming the field and the file
        std::fs::write(&path, r#"{"fsp":12}"#).unwrap();
        let e = load_file_config(&path).unwrap_err();
        assert!(
            e.contains("fsp") && e.contains(&path.display().to_string()),
            "{e}"
        );

        // not JSON
        std::fs::write(&path, "not json").unwrap();
        let e = load_file_config(&path).unwrap_err();
        assert!(e.contains(&path.display().to_string()), "{e}");

        // wrong type
        std::fs::write(&path, r#"{"fps":"ten"}"#).unwrap();
        assert!(load_file_config(&path).is_err());

        // a directory where the file should be
        let e = load_file_config(&d).unwrap_err();
        assert!(e.contains(&d.display().to_string()), "{e}");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn check_fps_is_not_a_clamp() {
        assert_eq!(check_fps(10), Ok(10));
        assert!(check_fps(4).is_err());
        assert!(check_fps(16).is_err());
        assert!(check_fps(-1).is_err());
    }
}
