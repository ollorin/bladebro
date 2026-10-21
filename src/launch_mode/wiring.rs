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
    // Two uses: the pre-daemon validation and the per-request tag.
    assert_eq!(CLI.matches("crate::launch_mode::request_tag()").count(), 2);
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

#[test]
fn run_tool_validates_before_auto_starting_a_daemon() {
    let a = CLI.find("async fn run_tool(").unwrap();
    let body = &CLI[a..];
    let validate = body.find("crate::launch_mode::request_tag()").unwrap();
    let start = body.find("auto_start_daemon()?").unwrap();
    assert!(
        validate < start,
        "an invalid setting must never start a daemon"
    );
    // Validation also precedes the first send to an existing daemon.
    assert!(validate < body.find("send_to_daemon(tool, args)").unwrap());
}

#[test]
fn run_tool_prints_the_launch_note_only_after_it_started_the_daemon() {
    let a = CLI.find("async fn run_tool(").unwrap();
    let body = &CLI[a..];
    assert_eq!(
        body.matches("crate::launch_mode::daemon_start_note()")
            .count(),
        1
    );
    let note = body
        .find("crate::launch_mode::daemon_start_note()")
        .unwrap();
    let start = body.find("auto_start_daemon()?").unwrap();
    assert!(
        start < note,
        "note belongs after the auto-start, not on a reused daemon"
    );
    // stderr only: a stdout print would break --json.
    let line_end = body[note..].find('\n').unwrap();
    let after = &body[note..note + line_end + 120];
    assert!(after.contains("eprintln!"), "{after}");
}

const SERVE: &str = include_str!("../mcp/server/serve.rs");

#[test]
fn mcp_launch_failure_suffix_is_launch_mode_aware() {
    assert!(SERVE.contains("crate::launch_mode::launch_error_suffix(&e.to_string())"));
}

const ATTACH: &str = include_str!("../page/attach.rs");

#[test]
fn ua_metadata_is_headless_by_request_only() {
    let g = ATTACH
        .find("crate::launch_mode::headless_by_request()")
        .unwrap();
    let m = ATTACH.find("ua_params[\"userAgentMetadata\"]").unwrap();
    assert!(g < m);
    assert!(!ATTACH.contains("PROBE_JS"));
}
