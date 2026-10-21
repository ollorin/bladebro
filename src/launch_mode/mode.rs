//! Pure parsing and layered resolution of the launch mode.

use std::path::{Path, PathBuf};

use crate::record::values::parse_switch;

pub const ENV_HEADLESS: &str = "BLADE_HEADLESS";

/// Parse `BLADE_HEADLESS`. Blank counts as unset; an invalid value is an error
/// (never silently ignored: the agent asked for something specific).
pub fn parse_headless_env(raw: Option<&str>) -> Result<Option<bool>, String> {
    let t = match raw.map(str::trim) {
        None | Some("") => return Ok(None),
        Some(t) => t,
    };
    parse_switch(t).map(Some).ok_or_else(|| {
        format!(
            "{ENV_HEADLESS}: \"{t}\" is not valid; use 1/on/true/yes (hidden browser) or \
             0/off/false/no (visible browser), or unset it for the platform default"
        )
    })
}

/// Which layer decided the mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Env,
    Project(PathBuf),
    User(PathBuf),
    Default,
}

impl Source {
    pub fn label(&self) -> String {
        match self {
            Source::Env => format!("env {ENV_HEADLESS}"),
            Source::Project(p) => format!("project file {}", p.display()),
            Source::User(p) => format!("user config {}", p.display()),
            Source::Default => "default".to_string(),
        }
    }
}

/// A config-file layer's opinion (filled by the file loaders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub headless: Option<bool>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchMode {
    /// `Some(true)` = headless forced; `Some(false)`/`None` = platform default.
    pub headless: Option<bool>,
    pub source: Source,
}

impl LaunchMode {
    pub fn forced_headless(&self) -> bool {
        self.headless == Some(true)
    }
}

/// env > project > user > default. The env layer is parsed first so a broken
/// higher layer is an error even when a lower layer would have answered.
pub fn resolve_mode(
    env: &dyn Fn(&str) -> Option<String>,
    project: Option<&Layer>,
    user: Option<&Layer>,
) -> Result<LaunchMode, String> {
    if let Some(h) = parse_headless_env(env(ENV_HEADLESS).as_deref())? {
        return Ok(LaunchMode {
            headless: Some(h),
            source: Source::Env,
        });
    }
    if let Some(l) = project {
        if l.headless.is_some() {
            return Ok(LaunchMode {
                headless: l.headless,
                source: Source::Project(l.path.clone()),
            });
        }
    }
    if let Some(l) = user {
        if l.headless.is_some() {
            return Ok(LaunchMode {
                headless: l.headless,
                source: Source::User(l.path.clone()),
            });
        }
    }
    Ok(LaunchMode {
        headless: None,
        source: Source::Default,
    })
}

/// Resolve from explicit inputs: reads only the given paths (project walk-up
/// from `cwd`, `<blade_dir>/launch.json`). Warnings (an ignored untrusted
/// project file) are returned for the caller to print.
pub fn resolve_in(
    env: &dyn Fn(&str) -> Option<String>,
    cwd: &Path,
    home: Option<&Path>,
    blade_dir: &Path,
) -> Result<(LaunchMode, Vec<String>), String> {
    let (project, warnings) = super::project::project_in(cwd, home, blade_dir)?;
    let project_layer = project.map(|l| Layer {
        headless: l.file.headless,
        path: l.path,
    });
    let user_path = blade_dir.join(super::user::USER_LAUNCH_FILE);
    let user_layer = super::user::load_user_launch(&user_path)?.map(|f| Layer {
        headless: f.headless,
        path: user_path,
    });
    let mode = resolve_mode(env, project_layer.as_ref(), user_layer.as_ref())?;
    Ok((mode, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(v: Option<&str>) -> impl Fn(&str) -> Option<String> {
        let mut m = HashMap::new();
        if let Some(v) = v {
            m.insert(ENV_HEADLESS.to_string(), v.to_string());
        }
        move |k: &str| m.get(k).cloned()
    }

    fn layer(h: Option<bool>, p: &str) -> Layer {
        Layer {
            headless: h,
            path: PathBuf::from(p),
        }
    }

    #[test]
    fn env_parsing() {
        assert_eq!(parse_headless_env(None), Ok(None));
        assert_eq!(parse_headless_env(Some("  ")), Ok(None));
        for t in ["1", "on", "true", "yes"] {
            assert_eq!(parse_headless_env(Some(t)), Ok(Some(true)));
        }
        for t in ["0", "off", "false", "no"] {
            assert_eq!(parse_headless_env(Some(t)), Ok(Some(false)));
        }
        let e = parse_headless_env(Some("maybe")).unwrap_err();
        assert!(
            e.starts_with("BLADE_HEADLESS: \"maybe\" is not valid"),
            "{e}"
        );
        assert!(e.contains("1/on/true/yes") && e.contains("unset"));
    }

    #[test]
    fn precedence() {
        let r = resolve_mode(&env_of(None), None, None).unwrap();
        assert_eq!((r.headless, r.source), (None, Source::Default));

        let u = layer(Some(true), "/u.json");
        let r = resolve_mode(&env_of(None), None, Some(&u)).unwrap();
        assert_eq!(r.headless, Some(true));
        assert_eq!(r.source, Source::User(PathBuf::from("/u.json")));

        let p = layer(Some(false), "/p.json");
        let r = resolve_mode(&env_of(None), Some(&p), Some(&u)).unwrap();
        assert_eq!(r.headless, Some(false));
        assert_eq!(r.source, Source::Project(PathBuf::from("/p.json")));

        let r = resolve_mode(&env_of(Some("1")), Some(&p), Some(&u)).unwrap();
        assert_eq!((r.headless, r.source), (Some(true), Source::Env));

        let r = resolve_mode(&env_of(Some(" ")), Some(&p), Some(&u)).unwrap();
        assert_eq!(r.headless, Some(false));

        let none = layer(None, "/p.json");
        let r = resolve_mode(&env_of(None), Some(&none), Some(&u)).unwrap();
        assert_eq!(r.source, Source::User(PathBuf::from("/u.json")));

        assert!(resolve_mode(&env_of(Some("maybe")), Some(&p), None).is_err());
    }

    #[test]
    fn forced_headless_only_for_some_true() {
        let m = |h| LaunchMode {
            headless: h,
            source: Source::Default,
        };
        assert!(m(Some(true)).forced_headless());
        assert!(!m(Some(false)).forced_headless());
        assert!(!m(None).forced_headless());
    }

    fn scratch(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("bladebro-resolve-in-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("scratch");
        std::fs::canonicalize(&d).expect("canon")
    }

    fn write_file(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
    }

    #[test]
    fn resolve_in_layers() {
        let s = scratch("layers");
        let (p, bd, home) = (s.join("p"), s.join("bd"), s.join("home"));
        let cwd = p.join("a").join("b");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::create_dir_all(&bd).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let none = env_of(None);

        // user layer only
        write_file(&bd.join("launch.json"), r#"{"headless":true}"#);
        let (m, w) = resolve_in(&none, &cwd, Some(&home), &bd).unwrap();
        assert_eq!(m.headless, Some(true));
        assert_eq!(m.source, Source::User(bd.join("launch.json")));
        assert!(w.is_empty());

        // project beats user
        write_file(&p.join(".bladebro.json"), r#"{"headless":false}"#);
        let (m, _) = resolve_in(&none, &cwd, Some(&home), &bd).unwrap();
        assert_eq!(m.headless, Some(false));
        assert_eq!(m.source, Source::Project(p.join(".bladebro.json")));

        // env beats project
        let (m, _) = resolve_in(&env_of(Some("1")), &cwd, Some(&home), &bd).unwrap();
        assert_eq!((m.headless, m.source), (Some(true), Source::Env));

        // project without the key falls through to user
        write_file(&p.join(".bladebro.json"), r#"{"record":{"fps":8}}"#);
        let (m, _) = resolve_in(&none, &cwd, Some(&home), &bd).unwrap();
        assert_eq!(m.source, Source::User(bd.join("launch.json")));

        // an untrusted file yields a warning, not a print
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let f = p.join(".bladebro.json");
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o666)).unwrap();
            let (m, w) = resolve_in(&none, &cwd, Some(&home), &bd).unwrap();
            assert_eq!(m.source, Source::User(bd.join("launch.json")));
            assert_eq!(w.len(), 1, "{w:?}");
        }
        let _ = std::fs::remove_dir_all(&s);
    }
}
