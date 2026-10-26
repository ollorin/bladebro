//! Live MCP stdio checks of the 10-02 evidence extras: the record-stop result carries the poster, the
//! timeline and the marker; a recording-off session writes no sidecar. All `#[ignore]`: they need Chrome +
//! ffmpeg and local fixtures, and run through `tools/record_sidecar/scenarios/evidence_live.sh` (id `mcp`,
//! headless, scratch homes, 127.0.0.1 fixtures only). Harness inputs as in tests/record_popup.rs.

mod record_support;

use std::path::PathBuf;
use std::time::Duration;

use record_support::*;
use serde_json::{json, Value};

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn sleep(s: f64) {
    std::thread::sleep(Duration::from_secs_f64(s));
}

/// The path that follows `key` in the result text. The poster path is followed by `. Timeline:`
/// with no space, so a trailing dot is part of the separator, not the name.
fn path_after(text: &str, key: &str) -> PathBuf {
    let rest = text
        .split(key)
        .nth(1)
        .unwrap_or_else(|| panic!("no {key:?} in {text}"));
    let tok = rest.split_whitespace().next().expect("a path token");
    PathBuf::from(tok.trim_end_matches('.'))
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/evidence_live.sh)"]
fn mcp_stop_has_poster_timeline_marker() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("evm", &live_env(&[("BLADE_RECORD", "1")], &ff));
    assert!(!mcp.nav(&format!("{fix}/mark.html")).0);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"ev"}));
    assert!(!e, "{t}");
    sleep(1.5);
    let (e, t) = mcp.call("act", json!({"action":"click","text":"turn blue"}));
    assert!(!e, "{t}");
    sleep(1.5);
    let (e, stop) = mcp.state(json!({"op":"record-stop","value":"x"}));
    eprintln!("RESULT mcp stop: {stop}");
    assert!(!e && stop.contains("recording saved: "), "{stop}");
    assert!(
        stop.contains(". Poster: ")
            && stop.contains(". Timeline: ")
            && stop.contains(". Marker at "),
        "{stop}"
    );

    // Poster: a PNG at the path the result names, created next to the video.
    let video = saved_path(&stop);
    let poster = path_after(&stop, ". Poster: ");
    let bytes = std::fs::read(&poster).expect("poster file exists");
    assert!(bytes.starts_with(&PNG_MAGIC), "poster is a PNG: {poster:?}");
    assert_eq!(
        poster.parent(),
        video.parent(),
        "poster sits next to the video"
    );

    // Timeline: version 1 JSON with the click (ok) and the marker.
    let tl_path = path_after(&stop, ". Timeline: ");
    let tl: Value =
        serde_json::from_str(&std::fs::read_to_string(&tl_path).expect("timeline file exists"))
            .expect("timeline is JSON");
    eprintln!("RESULT mcp timeline: {tl}");
    assert_eq!(tl["version"], 1, "{tl}");
    let entries = tl["entries"].as_array().expect("entries array");
    assert!(
        entries
            .iter()
            .any(|e| e["kind"] == "action" && e["action"] == "click" && e["ok"] == true),
        "a successful click entry: {tl}"
    );
    assert!(
        entries
            .iter()
            .any(|e| e["kind"] == "marker" && e["note"] == "x"),
        "the marker entry carries the note: {tl}"
    );

    // Exactly the poster and the timeline beside the one video.
    assert_eq!(mcp.recordings().len(), 1, "one video");
    assert_eq!(mcp.sidecars().len(), 2, "poster + timeline");
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/evidence_live.sh)"]
fn mcp_recording_off_has_no_sidecars() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    // BLADE_RECORD unset: record-start is refused and nothing may be written.
    let mut mcp = Mcp::spawn("evo", &live_env(&[], &ff));
    assert!(!mcp.nav(&format!("{fix}/mark.html")).0);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"off"}));
    eprintln!("RESULT mcp off record-start: {t}");
    assert!(e, "record-start is refused with recording off: {t}");
    let (e, t) = mcp.call("act", json!({"action":"click","text":"turn blue"}));
    assert!(!e, "{t}");
    let (_, stop) = mcp.state(json!({"op":"record-stop","value":"x"}));
    eprintln!("RESULT mcp off record-stop: {stop}");
    assert!(
        !stop.contains("Poster:") && !stop.contains("Timeline:") && !stop.contains("Marker at"),
        "no evidence text with recording off: {stop}"
    );
    assert!(mcp.recordings().is_empty(), "no video with recording off");
    assert!(mcp.sidecars().is_empty(), "no sidecar with recording off");
}
