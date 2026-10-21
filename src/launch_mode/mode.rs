//! Pure parsing and layered resolution of the launch mode.

use std::path::PathBuf;

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
}
