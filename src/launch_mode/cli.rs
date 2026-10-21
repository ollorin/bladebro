//! `bladebro headless on|off|status`, its help text and the doctor line.
//!
//! The wording and decisions are pure and unit-tested; `run_cli` and
//! `doctor_check` only wire them to the real environment.

use std::path::Path;

use serde_json::{json, Value};

use super::mode::{resolve_in, LaunchMode, Source};
use super::user::{load_user_launch, write_user_launch, USER_LAUNCH_FILE};
use crate::error::{BladeError, Result};

const LINUX: bool = cfg!(any(target_os = "linux", target_os = "android"));

pub const USAGE: &str = "usage: bladebro headless on|off|status";

const PRECEDENCE: &str = "Precedence: BLADE_HEADLESS > .bladebro.json (found from the current directory upward) > <data root>/launch.json (this toggle) > platform default.";

pub const HELP_TEXT: &str = r#"bladebro headless — hide or show the managed browser (suite-level)

USAGE
  bladebro headless status       effective mode and which setting decided it
  bladebro headless on           save headless (hidden browser) to launch.json
  bladebro headless off          save headed (visible browser) to launch.json
  --json                         one machine-readable result

Precedence: BLADE_HEADLESS > .bladebro.json (found from the current
directory upward) > <data root>/launch.json (this toggle) > platform default.

The setting applies at the next browser launch. If a daemon is running, run
`bladebro stop` first; a command whose mode differs from the running daemon
is refused with that instruction. The real-browser lane ignores it: use
`rb visible on|off`. Full reference: docs/recording-and-launch-config.md
"#;

pub fn help_json() -> Value {
    json!({
        "usage": USAGE,
        "tool": null,
        "examples": [
            "bladebro headless status",
            "bladebro headless on",
            "BLADE_HEADLESS=1 bladebro nav example.com"
        ],
        "notes": [PRECEDENCE]
    })
}

fn word(h: Option<bool>, linux: bool) -> &'static str {
    match (h, linux) {
        (Some(true), _) => "headless",
        (Some(false), true) => "headed (virtual display)",
        (Some(false), false) => "headed",
        (None, _) => "platform default",
    }
}

/// Human status: the effective mode and source, plus a second line when the
/// user's launch.json is overridden by a higher layer.
pub fn status_text(mode: &LaunchMode, user: Option<(bool, &Path)>, linux: bool) -> String {
    let mut out = format!(
        "browser mode: {} (source: {})",
        word(mode.headless, linux),
        mode.source.label()
    );
    if let Some((on, path)) = user {
        if !matches!(mode.source, Source::User(_)) {
            out.push_str(&format!(
                "\nuser config {} says {} but {} takes precedence",
                path.display(),
                if on { "on" } else { "off" },
                mode.source.label()
            ));
        }
    }
    out
}

/// Write launch.json for `on`/`off`. Returns the first and last output lines
/// separated by a newline (the fresh status line goes between them).
pub fn toggle_in(blade_dir: &Path, sub: &str) -> std::result::Result<String, String> {
    let on = match sub {
        "on" => true,
        "off" => false,
        _ => return Err(USAGE.to_string()),
    };
    let path = write_user_launch(blade_dir, on)?;
    Ok(format!(
        "headless: {sub} (saved to {})\napplies at the next browser launch; run `bladebro stop` first if a daemon is running",
        path.display()
    ))
}

fn resolve_real() -> std::result::Result<(LaunchMode, Vec<String>), String> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let home = crate::platform::home_dir();
    resolve_in(
        &|k| std::env::var(k).ok(),
        &cwd,
        Some(&home),
        &crate::platform::blade_dir(),
    )
}

fn emit(text: &str, json_mode: bool) {
    if json_mode {
        println!("{}", json!({"ok": true, "is_error": false, "text": text}));
    } else {
        println!("{text}");
    }
}

pub fn run_cli(rest: &[String], json_mode: bool) -> Result<()> {
    let sub = rest
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(String::as_str)
        .unwrap_or("status");
    let bd = crate::platform::blade_dir();
    let saved = match sub {
        "status" => None,
        "on" | "off" => Some(toggle_in(&bd, sub).map_err(BladeError::Other)?),
        // BladeError::Usage already prints the `usage: ` prefix.
        _ => {
            return Err(BladeError::Usage(
                USAGE.trim_start_matches("usage: ").to_string(),
            ))
        }
    };
    // Fresh resolution, never the process-pinned mode.
    let (mode, warnings) = resolve_real().map_err(BladeError::Other)?;
    for w in warnings {
        eprintln!("[bladebro] warning: {w}");
    }
    let upath = bd.join(USER_LAUNCH_FILE);
    let user = load_user_launch(&upath)
        .map_err(BladeError::Other)?
        .and_then(|f| f.headless);
    let status = status_text(&mode, user.map(|b| (b, upath.as_path())), LINUX);
    let text = match saved {
        None => status,
        Some(s) => match s.split_once('\n') {
            Some((first, last)) => format!("{first}\n{status}\n{last}"),
            None => format!("{s}\n{status}"),
        },
    };
    emit(&text, json_mode);
    Ok(())
}

pub const CHECK_NAME: &str = "Browser launch mode";

pub struct ModeCheck {
    pub ok: bool,
    pub detail: String,
    pub fix: Option<String>,
}

pub fn doctor_line(res: std::result::Result<LaunchMode, String>, linux: bool) -> ModeCheck {
    match res {
        Ok(m) => {
            let what = if m.forced_headless() {
                "headless"
            } else if linux {
                "Xvfb headful, headless fallback"
            } else {
                "headed"
            };
            ModeCheck {
                ok: true,
                detail: format!("{what} (source: {})", m.source.label()),
                fix: None,
            }
        }
        Err(e) => ModeCheck {
            ok: false,
            detail: e.replace('\n', " "),
            fix: Some(
                "fix or remove the named setting; see docs/recording-and-launch-config.md".into(),
            ),
        },
    }
}

pub fn doctor_check() -> ModeCheck {
    match resolve_real() {
        Ok((m, warnings)) => {
            for w in warnings {
                eprintln!("[bladebro] warning: {w}");
            }
            doctor_line(Ok(m), LINUX)
        }
        Err(e) => doctor_line(Err(e), LINUX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn m(h: Option<bool>, s: Source) -> LaunchMode {
        LaunchMode {
            headless: h,
            source: s,
        }
    }

    #[test]
    fn status_lines() {
        let s = status_text(&m(Some(true), Source::Env), None, false);
        assert!(
            s.starts_with("browser mode: headless (source: env BLADE_HEADLESS)"),
            "{s}"
        );
        assert_eq!(
            status_text(&m(None, Source::Default), None, false),
            "browser mode: platform default (source: default)"
        );
        let u = PathBuf::from("/u/launch.json");
        let s = status_text(&m(Some(false), Source::Env), Some((true, &u)), false);
        assert_eq!(s.lines().count(), 2, "{s}");
        assert!(s.lines().nth(1).unwrap().contains("takes precedence"));
        // not overridden: no second line
        let s = status_text(
            &m(Some(true), Source::User(u.clone())),
            Some((true, &u)),
            false,
        );
        assert_eq!(s.lines().count(), 1);
    }

    #[test]
    fn toggle_writes_user_file() {
        let d = std::env::temp_dir().join(format!("bladebro-toggle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let t = toggle_in(&d, "on").unwrap();
        let (first, last) = t.split_once('\n').unwrap();
        assert!(first.contains("saved to"), "{first}");
        assert!(last.contains("bladebro stop"), "{last}");
        let f = load_user_launch(&d.join(USER_LAUNCH_FILE))
            .unwrap()
            .unwrap();
        assert_eq!(f.headless, Some(true));
        toggle_in(&d, "off").unwrap();
        let f = load_user_launch(&d.join(USER_LAUNCH_FILE))
            .unwrap()
            .unwrap();
        assert_eq!(f.headless, Some(false));
        let e = toggle_in(&d, "maybe").unwrap_err();
        assert!(e.contains("on|off|status"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn doctor_rows() {
        let r = doctor_line(Ok(m(Some(true), Source::Env)), false);
        assert!(r.ok && r.detail == "headless (source: env BLADE_HEADLESS)");
        let r = doctor_line(Ok(m(None, Source::Default)), false);
        assert_eq!(r.detail, "headed (source: default)");
        let r = doctor_line(Ok(m(None, Source::Default)), true);
        assert_eq!(
            r.detail,
            "Xvfb headful, headless fallback (source: default)"
        );
        let r = doctor_line(Err("bad\nthing".into()), false);
        assert!(!r.ok && !r.detail.contains('\n'));
        assert!(r
            .fix
            .unwrap()
            .contains("docs/recording-and-launch-config.md"));
        assert!(CHECK_NAME.len() <= 24);
    }

    #[test]
    fn help_text_content() {
        for n in [
            "USAGE",
            "BLADE_HEADLESS",
            ".bladebro.json",
            "launch.json",
            "rb visible",
        ] {
            assert!(HELP_TEXT.contains(n), "{n}");
        }
        assert!(!HELP_TEXT.contains("\"#"));
        assert!(help_json()["usage"]
            .as_str()
            .unwrap()
            .contains("on|off|status"));
    }
}
