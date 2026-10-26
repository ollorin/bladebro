//! The `state` tool schema gained exactly two enum values and text deltas
//! for recording (10-02 added the record-stop marker `value` and the poster /
//! timeline words). Guards the five-tool budget (non-negotiable 1) and the
//! added-text ceiling (Phase 1 D-01 232 chars; 10-PLAN-NOTES D10-6 <= 330).

use crate::mcp::tools::all_tools;

const ENUM_ADD: &str = r#", "record-start", "record-stop""#;
const LINE_ADD: &str = "RECORDING: record-start <name> (video of the active tab at start), record-stop [value=<marker note>] (returns path, bytes, duration, frames, drops, segments, poster PNG, timeline JSON). Needs ffmpeg; off unless BLADE_RECORD is set.";
const NAME_NEW: &str = "Cookie name, storage key, session name, or recording name.";
const NAME_OLD: &str = "Cookie name, storage key, or session name.";
const VALUE_NEW: &str = "Cookie or storage value, or record-stop marker note.";
const VALUE_OLD: &str = "Cookie or storage value.";
const DOCS: &str = include_str!("../../../docs/recording-and-launch-config.md");
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
fn description_mentions_poster_timeline_marker() {
    let s = state();
    assert!(
        s.description.contains(
            "record-stop [value=<marker note>] (returns path, bytes, duration, frames, drops, segments, poster PNG, timeline JSON)"
        ),
        "RECORDING line lacks the marker/poster/timeline wording"
    );
    let value = s.input_schema["properties"]["value"]["description"]
        .as_str()
        .expect("value description");
    assert_eq!(value, VALUE_NEW);
    assert!(value.contains("record-stop marker note"));
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
    assert!(
        collapsed.contains(VALUE_NEW),
        "missing in tools.rs: {VALUE_NEW}"
    );
    let name_delta = NAME_NEW.len() - NAME_OLD.len();
    let value_delta = VALUE_NEW.len() - VALUE_OLD.len();
    // The description line also carries a trailing "\n\" in the source (3 chars).
    let total = ENUM_ADD.len() + LINE_ADD.len() + 1 + name_delta + value_delta;
    eprintln!(
        "schema added chars: enum {} + line {} + name delta {} + value delta {} = {total} (~{} tokens)",
        ENUM_ADD.len(),
        LINE_ADD.len() + 1,
        name_delta,
        value_delta,
        total / 4
    );
    assert!(total <= 330, "added {total} chars, budget 330");
}

#[test]
fn docs_mention_poster_marker_timeline() {
    assert!(DOCS.contains("### Poster, marker and timeline"));
    for word in ["Poster:", "Marker at", "Timeline:", "value"] {
        assert!(DOCS.contains(word), "docs lack {word}");
    }
    assert!(DOCS.contains(r#""op": "record-stop", "value""#));
    assert!(DOCS.contains(".timeline.json"));
}
