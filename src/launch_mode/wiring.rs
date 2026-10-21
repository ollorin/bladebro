//! Hook checks: every upstream edit of this module must stay in place.

const LAUNCH: &str = include_str!("../browser/launch.rs");
const PIPE: &str = include_str!("../browser/pipe.rs");
const ALERTS: &str = include_str!("../browser/alerts.rs");

#[test]
fn lib_declares_module() {
    assert!(include_str!("../lib.rs").contains("pub mod launch_mode;"));
}

#[test]
fn managed_launch_paths_consume_the_one_decision() {
    for (name, src) in [("launch.rs", LAUNCH), ("pipe.rs", PIPE)] {
        for needle in [
            "crate::launch_mode::effective()",
            "crate::launch_mode::plan_display(",
            "crate::launch_mode::note_launch(",
        ] {
            assert_eq!(src.matches(needle).count(), 1, "{name}: {needle}");
        }
        assert!(src.contains("plan.try_xvfb"), "{name}");
        assert!(src.contains("let headful = plan.native_headful;"), "{name}");
        assert!(
            !src.contains("let headful = true; // macOS/Windows have native window servers"),
            "{name}: old unconditional headful"
        );
        assert_eq!(
            src.matches("set_launched_headless(!headful);").count(),
            1,
            "{name}"
        );
        let plan = src.find("plan_display(").unwrap();
        let flag = src.find("set_launched_headless(!headful);").unwrap();
        assert!(
            plan < flag,
            "{name}: stealth flag must consume the decision"
        );
    }
}

#[test]
fn real_lane_is_untouched() {
    let a = LAUNCH.find("pub async fn launch_real").unwrap();
    let b = LAUNCH.find("pub async fn launch_lane").unwrap();
    let (a, b) = (a.min(b), a.max(b));
    assert!(!LAUNCH[a..b].contains("launch_mode"));
}

#[test]
fn alerts_skip_false_xvfb_advisory_when_headless_requested() {
    assert!(ALERTS.contains("!crate::launch_mode::headless_by_request()"));
}
