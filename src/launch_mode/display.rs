//! The single display decision and the launch-note wording (all pure).

use super::mode::LaunchMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayPlan {
    /// Linux: try to start bladebro's private Xvfb.
    pub try_xvfb: bool,
    /// macOS/Windows: launch with a native window (not headless).
    pub native_headful: bool,
}

/// The one place that decides how the browser gets (or skips) a display.
///
/// | headless | linux            | other OS          |
/// |----------|------------------|-------------------|
/// | None/No  | Xvfb, else fallback | native window  |
/// | Some(true)| no Xvfb, headless | headless         |
///
/// "Headed" on Linux means headful on the private Xvfb, never a window on the
/// user's own desktop (the managed lane pins X11 to Xvfb by design).
pub fn plan_display(headless: Option<bool>, linux: bool) -> DisplayPlan {
    let forced = headless == Some(true);
    DisplayPlan {
        try_xvfb: linux && !forced,
        native_headful: !linux && !forced,
    }
}

pub fn describe(m: &LaunchMode, headful: bool, linux: bool) -> &'static str {
    match (headful, linux) {
        (false, _) if m.forced_headless() => "headless",
        (false, _) => "headless (no Xvfb; fallback)",
        (true, true) => "headed (virtual display)",
        (true, false) => "headed",
    }
}

/// The once-per-process stderr line; stdout stays a machine contract.
pub fn launch_note(m: &LaunchMode, headful: bool, linux: bool) -> String {
    format!(
        "[bladebro] browser mode: {} (source: {})",
        describe(m, headful, linux),
        m.source.label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch_mode::mode::Source;
    use std::path::PathBuf;

    fn m(h: Option<bool>, s: Source) -> LaunchMode {
        LaunchMode {
            headless: h,
            source: s,
        }
    }

    #[test]
    fn plan_table() {
        let t = |h, l, x, n| {
            assert_eq!(
                plan_display(h, l),
                DisplayPlan {
                    try_xvfb: x,
                    native_headful: n
                },
                "{h:?} {l}"
            )
        };
        t(None, true, true, false);
        t(Some(false), true, true, false);
        t(Some(true), true, false, false);
        t(None, false, false, true);
        t(Some(false), false, false, true);
        t(Some(true), false, false, false);
        for l in [true, false] {
            let p = plan_display(Some(true), l);
            assert!(!p.try_xvfb && !p.native_headful);
        }
    }

    #[test]
    fn notes() {
        let env = m(Some(true), Source::Env);
        assert_eq!(
            launch_note(&env, false, false),
            "[bladebro] browser mode: headless (source: env BLADE_HEADLESS)"
        );
        assert_eq!(
            launch_note(&m(None, Source::Default), true, false),
            "[bladebro] browser mode: headed (source: default)"
        );
        assert!(
            launch_note(&m(None, Source::Default), true, true).contains("headed (virtual display)")
        );
        assert!(launch_note(&m(None, Source::Default), false, true)
            .contains("headless (no Xvfb; fallback)"));
        let p = m(Some(false), Source::Project(PathBuf::from("/x/p.json")));
        assert!(launch_note(&p, true, false).contains("project file /x/p.json"));
        let u = m(Some(false), Source::User(PathBuf::from("/x/u.json")));
        let n = launch_note(&u, true, false);
        assert!(n.contains("user config /x/u.json") && !n.contains('\n'));
    }
}
