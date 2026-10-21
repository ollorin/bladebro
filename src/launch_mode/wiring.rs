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

const MAIN: &str = include_str!("../main.rs");
const CLI: &str = include_str!("../cli.rs");
const HELP: &str = include_str!("../cli/help.rs");
const DOCTOR: &str = include_str!("../updater/doctor.rs");

#[test]
fn main_routes_headless_to_the_cli() {
    assert!(MAIN.matches("\"headless\"").count() >= 2);
}

#[test]
fn cli_dispatches_headless() {
    assert!(CLI.contains("\"headless\" => crate::launch_mode::run_cli(rest, json_mode)"));
}

#[test]
fn help_surfaces_headless() {
    for n in [
        "crate::launch_mode::HELP_TEXT",
        "crate::launch_mode::help_json()",
        "headless on|off|status",
    ] {
        assert!(HELP.contains(n), "{n}");
    }
    let v: serde_json::Value = serde_json::from_str(&crate::cli::help_json(None).unwrap()).unwrap();
    assert!(v["env"]["BLADE_HEADLESS"].is_string());
    let t = crate::cli::command_help_text("headless").expect("help text");
    assert!(t.contains("USAGE"));
}

#[test]
fn doctor_runs_the_launch_mode_check_after_ffmpeg() {
    let push = "checks.push(check_launch_mode());";
    assert_eq!(DOCTOR.matches(push).count(), 1);
    assert!(DOCTOR.find("checks.push(check_ffmpeg());").unwrap() < DOCTOR.find(push).unwrap());
    assert!(DOCTOR.contains("fn check_launch_mode() -> Check"));
}

const DAEMON: &str = include_str!("../cli/daemon.rs");

#[test]
fn cli_tags_daemon_requests_and_daemon_guards_after_lane_refresh() {
    assert_eq!(CLI.matches("crate::launch_mode::request_tag()").count(), 1);
    assert_eq!(CLI.matches("\"launch\":").count(), 1);
    assert_eq!(
        DAEMON
            .matches("crate::launch_mode::daemon_mismatch(&req)")
            .count(),
        1
    );
    let refresh = DAEMON.find("crate::realbrowser::refresh_lane()").unwrap();
    let guard = DAEMON
        .find("crate::launch_mode::daemon_mismatch(&req)")
        .unwrap();
    let launch = DAEMON.find("let need_launch").unwrap();
    assert!(refresh < guard && guard < launch);
}
