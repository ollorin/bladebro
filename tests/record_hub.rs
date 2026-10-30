//! Integration tests for the `state` recording ops (`record-start` / `record-stop`)
//! over the real binary's MCP stdio server (07-03).
//!
//! The hermetic tests run in the normal suite and need no Chrome (the server
//! launches Chrome lazily, so `tools/list` and a stdin close touch no browser).
//! The live tests are `#[ignore]`: they need Chrome + ffmpeg and local fixtures,
//! and run through `tools/record_sidecar/scenarios/hub_live.sh`.
//!
//! Harness inputs of the live tests (not product options):
//!   REC_HUB_FIX      fixture base URL, e.g. http://127.0.0.1:8765 (unset: live tests skip)
//!   REC_HUB_FFMPEG   ffmpeg path, passed to the child as BLADE_FFMPEG
//!   REC_HUB_FFPROBE  ffprobe path (default: `ffprobe` on PATH)

mod record_support;

use std::process::Command;
use std::time::{Duration, Instant};

use record_support::*;
use serde_json::json;

// ---------------------------------------------------------------- hermetic

#[test]
fn mcp_tools_list_has_record_ops() {
    let mut mcp = Mcp::spawn("list", &[]);
    let resp = mcp.request("tools/list", json!({}), Duration::from_secs(20));
    let tools = resp["result"]["tools"].as_array().expect("tools array");
    let mut names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        ["act", "run", "see", "state", "vision"],
        "five tools, forever"
    );

    let state = tools
        .iter()
        .find(|t| t["name"] == "state")
        .expect("state tool");
    let ops: Vec<&str> = state["inputSchema"]["properties"]["op"]["enum"]
        .as_array()
        .expect("op enum")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(ops.iter().filter(|o| **o == "record-start").count(), 1);
    assert_eq!(ops.iter().filter(|o| **o == "record-stop").count(), 1);

    // No property was added for recording: `name` is the pre-existing one.
    let props: Vec<&str> = state["inputSchema"]["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .map(|k| k.as_str())
        .collect();
    for forbidden in ["record", "recording", "fps", "quality", "format", "output"] {
        assert!(
            !props.contains(&forbidden),
            "no new state property: {forbidden}"
        );
    }

    #[cfg(unix)]
    assert!(
        procs_with_home(&mcp.home).is_empty(),
        "tools/list must not start Chrome: {:?}",
        procs_with_home(&mcp.home)
    );
    mcp.close_stdin();
    assert!(
        mcp.wait_exit(Duration::from_secs(10)),
        "server must exit after stdin closes"
    );
}

#[test]
fn cli_record_start_without_name_is_usage() {
    let home = tmp_home("cli-usage");
    let out = Command::new(bin())
        .args(["state", "record-start"])
        .env("BLADE_HOME", &home)
        .env("BLADE_NO_UPDATE_CHECK", "1")
        .env("BLADE_HEADLESS", "1")
        .output()
        .expect("bladebro must run");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "must exit non-zero: {text}");
    assert!(text.contains("needs a name"), "usage text expected: {text}");
    #[cfg(unix)]
    assert!(
        procs_with_home(&home).is_empty(),
        "a parse error must start no daemon or Chrome: {:?}",
        procs_with_home(&home)
    );
    let _ = std::fs::remove_dir_all(&home);
}

const LIVE: &str = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)";

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn start_browse_stop() {
    let _ = LIVE;
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("sbs", &live_env(&[("BLADE_RECORD", "1")], &ff));
    let (e, t) = mcp.nav(&format!("{fix}/anim.html"));
    assert!(!e, "nav anim: {t}");
    let t0 = Instant::now();
    let (e, start) = mcp.state(json!({"op":"record-start","name":"t1"}));
    eprintln!("RESULT record-start: {start}");
    assert!(
        !e && start.contains("recording started: /"),
        "start: {start}"
    );
    let (e, t) = mcp.nav(&format!("{fix}/tick.html"));
    assert!(!e, "nav tick: {t}");
    std::thread::sleep(Duration::from_secs(6));
    let (e, stop) = mcp.state(json!({"op":"record-stop"}));
    let wall = t0.elapsed().as_secs_f64();
    eprintln!("RESULT record-stop: {stop}");
    assert!(!e, "stop must not be an error: {stop}");
    assert!(stop.contains("recording saved: "), "{stop}");

    let path = saved_path(&stop);
    assert!(path.is_absolute(), "absolute path: {path:?}");
    assert!(path.exists(), "file exists: {path:?}");
    assert!(
        path.starts_with(mcp.home.join("recordings"))
            || path.starts_with(
                std::fs::canonicalize(&mcp.home)
                    .unwrap_or_default()
                    .join("recordings")
            ),
        "under <home>/recordings: {path:?}"
    );
    let size = std::fs::metadata(&path).expect("stat").len() as f64;
    assert_eq!(
        field_after(&stop, "bytes "),
        size,
        "bytes equal file size: {stop}"
    );
    let dur = field_after(&stop, "duration ");
    assert!(
        (dur - wall).abs() <= wall * 0.30,
        "duration {dur} s within 30% of wall {wall:.1} s: {stop}"
    );
    assert!(field_after(&stop, "frames ") > 0.0, "{stop}");
    assert!(stop.contains("dropped "), "{stop}");
    assert!(stop.contains("segments [0.0 s tab "), "{stop}");
    let (codec, w, h) = probe(&path).expect("ffprobe reads the file");
    eprintln!("RESULT ffprobe: {codec} {w}x{h} wall={wall:.2}s duration={dur}s");
    assert_eq!(codec, "vp9");
    assert!(decodes_clean(&path), "full decode must be clean");
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn double_start_names_file() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("dbl", &live_env(&[("BLADE_RECORD", "1")], &ff));
    assert!(!mcp.nav(&format!("{fix}/anim.html")).0);
    let (e, a) = mcp.state(json!({"op":"record-start","name":"a"}));
    assert!(!e, "{a}");
    let first = a
        .split("recording started: ")
        .nth(1)
        .and_then(|r| r.split(" (tab").next())
        .expect("first path")
        .to_string();
    let (e, b) = mcp.state(json!({"op":"record-start","name":"b"}));
    eprintln!("RESULT double-start: {b}");
    assert!(e, "second start must be isError: {b}");
    assert!(b.contains(&first), "names the first file {first}: {b}");
    assert!(b.contains("record-stop"), "{b}");
    let files = mcp.recordings();
    assert_eq!(files.len(), 1, "exactly one file: {files:?}");
    // A stop before any frame is an honest "no frames" error (by design), so let
    // the first frames arrive before the clean stop.
    std::thread::sleep(Duration::from_secs(2));
    let (e, s) = mcp.state(json!({"op":"record-stop"}));
    assert!(!e, "{s}");
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn off_mode_refuses_and_creates_nothing() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("off", &live_env(&[], &ff));
    let pid = mcp.pid();
    let url = format!("{fix}/a.html");
    let (e, t) = mcp.nav(&url);
    assert!(!e, "{t}");
    assert!(ffmpeg_children(pid).is_empty(), "no ffmpeg after nav");
    let (e, t) = mcp.call("see", json!({"mode":"content"}));
    assert!(!e, "{t}");
    let (e, t) = mcp.state(json!({"op":"tabs"}));
    assert!(!e, "{t}");
    assert!(ffmpeg_children(pid).is_empty(), "no ffmpeg after tabs");
    let (e, t) = mcp.state(json!({"op":"record-start","name":"x"}));
    eprintln!("RESULT off record-start: {t}");
    assert!(
        e && t.contains("BLADE_RECORD") && t.contains("bladebro stop"),
        "{t}"
    );
    let (_, t) = mcp.state(json!({"op":"record-stop"}));
    eprintln!("RESULT off record-stop: {t}");
    assert!(t.contains("no active recording"), "{t}");
    assert!(ffmpeg_children(pid).is_empty(), "no ffmpeg after refusals");
    assert!(
        !mcp.home.join("recordings").exists(),
        "off mode must not create the recordings dir"
    );
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn missing_ffmpeg() {
    let Some(fix) = fix() else { return };
    let mut mcp = Mcp::spawn(
        "noff",
        &[
            ("BLADE_RECORD", "1"),
            ("BLADE_FFMPEG", "/nonexistent/ffmpeg"),
        ],
    );
    assert!(!mcp.nav(&format!("{fix}/a.html")).0);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"x"}));
    eprintln!("RESULT missing ffmpeg: {t}");
    assert!(e && t.contains("BLADE_FFMPEG"), "{t}");
    assert!(
        mcp.recordings().is_empty(),
        "no file: {:?}",
        mcp.recordings()
    );
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn pipe_refusal() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn(
        "pipe",
        &live_env(&[("BLADE_RECORD", "1"), ("BLADE_TRANSPORT", "pipe")], &ff),
    );
    let pid = mcp.pid();
    assert!(!mcp.nav(&format!("{fix}/a.html")).0);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"x"}));
    eprintln!("RESULT pipe refusal: {t}");
    assert!(e && t.to_lowercase().contains("pipe"), "{t}");
    assert!(ffmpeg_children(pid).is_empty(), "no ffmpeg child");
    assert!(
        mcp.recordings().is_empty(),
        "no file: {:?}",
        mcp.recordings()
    );
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/hub_live.sh)"]
fn stdin_eof_finalizes() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("eof", &live_env(&[("BLADE_RECORD", "1")], &ff));
    assert!(!mcp.nav(&format!("{fix}/anim.html")).0);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"eof"}));
    assert!(!e, "{t}");
    std::thread::sleep(Duration::from_secs(4));
    mcp.close_stdin();
    assert!(
        mcp.wait_exit(Duration::from_secs(40)),
        "server must exit within 40 s of stdin EOF"
    );
    let files = mcp.recordings();
    assert_eq!(files.len(), 1, "exactly one file: {files:?}");
    let (codec, w, h) = probe(&files[0]).expect("ffprobe reads the EOF-finalized file");
    eprintln!(
        "RESULT eof finalize: {} {codec} {w}x{h}",
        files[0].display()
    );
    assert!(decodes_clean(&files[0]), "full decode must be clean");
}
