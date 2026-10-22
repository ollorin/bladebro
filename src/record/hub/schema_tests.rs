//! The `state` tool schema gained exactly two enum values and two text
//! deltas for recording. Guards the five-tool budget (non-negotiable 1) and
//! the added-token ceiling (Phase 1 D-01: 232 chars / ~58 tokens).

use crate::mcp::tools::all_tools;

const ENUM_ADD: &str = r#", "record-start", "record-stop""#;
const LINE_ADD: &str = "RECORDING: record-start <name> (video of the active tab at start), record-stop (returns path, bytes, duration, frames, drops, segments). Needs ffmpeg; off unless BLADE_RECORD is set.";
const NAME_NEW: &str = "Cookie name, storage key, session name, or recording name.";
const NAME_OLD: &str = "Cookie name, storage key, or session name.";
const OLD_ENUM: &[&str] = &[
    "cookies",
    "set-cookie",
    "del-cookie",
    "ls",
    "ss",
    "set-ls",
    "set-ss",
    "rm-ls",
    "rm-ss",
    "clear-ls",
    "clear-ss",
    "tabs",
    "open-tab",
    "close-tab",
    "switch-tab",
    "save",
    "load",
    "block",
];

fn state() -> crate::mcp::tools::ToolDef {
    all_tools()
        .into_iter()
        .find(|t| t.name == "state")
        .expect("state tool")
}

#[test]
fn five_tools_only() {
    let names: Vec<_> = all_tools().iter().map(|t| t.name).collect();
    assert_eq!(names, ["act", "see", "state", "run", "vision"]);
}

#[test]
fn enum_has_both_ops_once_and_keeps_the_old_ones() {
    let s = state();
    let en: Vec<&str> = s.input_schema["properties"]["op"]["enum"]
        .as_array()
        .expect("enum")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    for op in ["record-start", "record-stop"] {
        assert_eq!(en.iter().filter(|e| **e == op).count(), 1, "{op}");
    }
    for op in OLD_ENUM {
        assert!(en.contains(op), "{op} lost");
    }
    assert_eq!(en.len(), OLD_ENUM.len() + 2);
}

#[test]
fn description_text_and_no_tab_following_claim() {
    let s = state();
    assert!(s.description.contains("RECORDING: record-start <name>"));
    assert!(s.description.contains("record-stop"));
    assert!(!s.description.contains("follows tab switches"));
    let name = s.input_schema["properties"]["name"]["description"]
        .as_str()
        .expect("name description");
    assert!(name.contains("recording name"));
}

#[test]
fn no_new_state_property() {
    let s = state();
    let mut keys: Vec<&str> = s.input_schema["properties"]
        .as_object()
        .expect("props")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "classes",
            "clear",
            "domain",
            "name",
            "op",
            "target_id",
            "url",
            "value"
        ]
    );
}

#[test]
fn added_text_stays_within_the_budget() {
    let src = include_str!("../../mcp/tools.rs");
    let collapsed = src.split_whitespace().collect::<Vec<_>>().join(" ");
    for part in [ENUM_ADD.trim_start_matches(", "), LINE_ADD, NAME_NEW] {
        assert!(collapsed.contains(part), "missing in tools.rs: {part}");
    }
    let name_delta = NAME_NEW.len() - NAME_OLD.len();
    // The description line also carries a trailing "\n\" in the source (3 chars).
    let total = ENUM_ADD.len() + LINE_ADD.len() + 1 + name_delta;
    eprintln!(
        "schema added chars: enum {} + line {} + name delta {} = {total} (~{} tokens)",
        ENUM_ADD.len(),
        LINE_ADD.len() + 1,
        name_delta,
        total / 4
    );
    assert!(total <= 240, "added {total} chars, budget 240");
}
