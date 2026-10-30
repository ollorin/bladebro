//! Client-side launch note for the CLI daemon path, and the honest wording
//! when a raw `--headless` flag (BLADE_CHROME_FLAGS) decides the mode.
//!
//! The auto-started daemon's stderr is /dev/null, so the note printed by the
//! browser launch never reaches the user on the default CLI path. The client
//! therefore prints it once, only on the invocation that started the daemon
//! (later commands reuse the daemon and stay quiet), to stderr only.

use super::display::launch_note;
use super::mode::LaunchMode;

/// True when the raw Chrome flags carry `--headless` / `--headless=...`.
pub fn raw_headless_flag(flags: Option<&str>) -> bool {
    flags.is_some_and(|f| {
        f.split_whitespace()
            .any(|t| t == "--headless" || t.starts_with("--headless="))
    })
}

/// The one-line launch note. A raw `--headless` flag with BLADE_HEADLESS not
/// forcing headless is reported as what it is: headless, with the stealth
/// caveat (the launched-headless flag stays false, so headless-only masks are
/// skipped) and the fix.
pub fn note_text(m: &LaunchMode, headful: bool, linux: bool, flags: Option<&str>) -> String {
    if raw_headless_flag(flags) && !m.forced_headless() {
        return format!(
            "[bladebro] browser mode: headless via BLADE_CHROME_FLAGS (BLADE_HEADLESS not set to 1, so headless-only stealth masks are not applied); set BLADE_HEADLESS=1 instead (source: {})",
            m.source.label()
        );
    }
    launch_note(m, headful, linux)
}

/// True when a launch error came from a launch-mode setting, not from Chrome.
fn is_setting_error(e: &str) -> bool {
    e.starts_with("BLADE_HEADLESS:") || e.contains(".bladebro.json") || e.contains("launch.json")
}

/// Tail of the MCP "Could not launch Chrome" message. The stock tail blames a
/// missing Chromium, which is false (and sends the agent the wrong way) when
/// the launch setting was the problem.
pub fn launch_error_suffix(e: &str) -> &'static str {
    if is_setting_error(e) {
        " Fix or unset that launch setting and call again; no browser was started."
    } else {
        " Check that Chromium is installed and try again."
    }
}

/// Pure core of [`daemon_start_note`]. None on the real lane (headless does
/// not apply) and on an invalid setting (`request_tag` already refused it).
pub fn start_note(
    lane_real: bool,
    res: Result<LaunchMode, String>,
    linux: bool,
    flags: Option<&str>,
) -> Option<String> {
    if lane_real {
        return None;
    }
    let m = res.ok()?;
    // The daemon launches its browser lazily; what the client can state is the
    // requested mode (on Linux the virtual display may still fall back).
    let headful = !m.forced_headless();
    Some(note_text(&m, headful, linux, flags))
}

/// The note the CLI prints after it started the daemon.
pub fn daemon_start_note() -> Option<String> {
    start_note(
        super::daemon::lane_is_real(),
        super::state::effective(),
        cfg!(any(target_os = "linux", target_os = "android")),
        std::env::var("BLADE_CHROME_FLAGS").ok().as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch_mode::mode::Source;

    fn m(h: Option<bool>, s: Source) -> LaunchMode {
        LaunchMode {
            headless: h,
            source: s,
        }
    }

    #[test]
    fn raw_flag_detection() {
        assert!(raw_headless_flag(Some("--headless=new")));
        assert!(raw_headless_flag(Some("--foo --headless")));
        assert!(!raw_headless_flag(Some("--no-headless-x --disable-gpu")));
        assert!(!raw_headless_flag(None));
    }

    #[test]
    fn raw_flag_is_never_called_headed() {
        let d = m(None, Source::Default);
        let t = note_text(&d, true, false, Some("--headless=new"));
        assert!(t.contains("headless via BLADE_CHROME_FLAGS"), "{t}");
        assert!(t.contains("BLADE_HEADLESS=1") && !t.contains("browser mode: headed"));
        assert!(!t.contains('\n'));
        // Forced headless already: the normal note, no caveat.
        let f = m(Some(true), Source::Env);
        let t = note_text(&f, false, false, Some("--headless=new"));
        assert_eq!(
            t,
            "[bladebro] browser mode: headless (source: env BLADE_HEADLESS)"
        );
        // No raw flag: the normal note.
        assert!(note_text(&d, true, false, None).contains("headed (source: default)"));
    }

    #[test]
    fn launch_error_suffix_blames_the_setting_not_chromium() {
        let bad = crate::launch_mode::parse_headless_env(Some("maybe")).unwrap_err();
        assert!(!launch_error_suffix(&bad).contains("Chromium"));
        assert!(!launch_error_suffix("/x/.bladebro.json: invalid JSON").contains("Chromium"));
        assert!(launch_error_suffix("Chrome exited with code 1").contains("Chromium"));
    }

    #[test]
    fn start_note_rows() {
        let f = m(Some(true), Source::Env);
        assert_eq!(
            start_note(false, Ok(f.clone()), false, None).unwrap(),
            "[bladebro] browser mode: headless (source: env BLADE_HEADLESS)"
        );
        assert!(start_note(true, Ok(f), false, None).is_none());
        assert!(start_note(false, Err("bad".into()), false, None).is_none());
        let d = start_note(false, Ok(m(None, Source::Default)), true, None).unwrap();
        assert!(d.contains("headed (virtual display)") && !d.contains('\n'));
    }
}
