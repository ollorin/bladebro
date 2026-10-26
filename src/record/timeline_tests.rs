//! Table tests for the action timeline (D10-3). Pure: the only global touched
//! is `hub::recording_active`, which no lib test ever sets.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::timeline::*;
use crate::error::BladeError;

fn entry(start: Instant, end: Instant, action: &str) -> Entry {
    Entry {
        start,
        end,
        tool: "act".into(),
        action: action.into(),
        steps: None,
        ok: true,
        verdict: "v".into(),
    }
}

#[test]
fn include_rules() {
    let act_click = json!({"action": "click"});
    type Want = Option<(&'static str, Option<usize>)>;
    let cases: Vec<(&str, Value, Want)> = vec![
        ("act", act_click, Some(("click", None))),
        ("act", json!({}), Some(("", None))),
        ("run", json!({"steps": [{}, {}]}), Some(("run", Some(2)))),
        ("state", json!({"op": "open-tab"}), Some(("open-tab", None))),
        (
            "state",
            json!({"op": "close-tab"}),
            Some(("close-tab", None)),
        ),
        (
            "state",
            json!({"op": "switch-tab"}),
            Some(("switch-tab", None)),
        ),
        ("state", json!({"op": "tabs"}), None),
        ("state", json!({"op": "cookies"}), None),
        ("state", json!({"op": "record-start"}), None),
        ("state", json!({"op": "record-stop"}), None),
        ("see", json!({"url": "x"}), None),
        ("vision", json!({}), None),
        ("nope", json!({}), None),
    ];
    for (tool, args, want) in cases {
        let got = include(tool, &args);
        let want = want.map(|(a, s)| (a.to_string(), s));
        assert_eq!(got, want, "include({tool}, {args})");
    }
}

#[test]
fn verdict_first_nonempty_line_clipped() {
    assert_eq!(
        verdict_line("\n\n  clicked \"Go\"\n--- delta ---"),
        "clicked \"Go\""
    );
    let long = "x".repeat(500);
    let v = verdict_line(&long);
    assert_eq!(v.chars().count(), MAX_VERDICT_CHARS);
    assert!(v.ends_with("..."));
    assert_eq!(verdict_line("a\u{202E}b"), "ab");
}

#[test]
fn begin_off_is_none() {
    let log = Log::new();
    let b = begin_with(false, Some(log.clone()), "act", &json!({"action": "click"}));
    assert!(b.is_none());
    assert_eq!(log.len(), 0);
}

#[test]
fn begin_without_log_is_none() {
    assert!(begin_with(true, None, "act", &json!({"action": "click"})).is_none());
}

#[test]
fn begin_excluded_tool_is_none() {
    let log = Log::new();
    assert!(begin_with(true, Some(log.clone()), "see", &json!({})).is_none());
    assert_eq!(log.len(), 0);
}

#[test]
fn end_none_is_noop() {
    end(None, &Ok("x".into()));
}

#[test]
fn end_pushes_ok_and_err() {
    let log = Log::new();
    let b1 = begin_with(true, Some(log.clone()), "act", &json!({"action": "click"}));
    end(b1, &Ok("clicked x\nmore".into()));
    let b2 = begin_with(true, Some(log.clone()), "act", &json!({"action": "type"}));
    end(b2, &Err(BladeError::Other("bad ref e9".into())));
    let (entries, dropped) = log.take();
    assert_eq!(dropped, 0);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].action, "click");
    assert!(entries[0].ok);
    assert_eq!(entries[0].verdict, "clicked x");
    assert_eq!(entries[1].action, "type");
    assert!(!entries[1].ok);
    assert_eq!(entries[1].verdict, "bad ref e9");
    assert!(entries[0].end >= entries[0].start);
}

#[test]
fn log_caps_entries() {
    let log = Log::new();
    let t = Instant::now();
    for _ in 0..MAX_ENTRIES + 5 {
        log.push(entry(t, t, "click"));
    }
    assert_eq!(log.len(), MAX_ENTRIES);
    let (entries, dropped) = log.take();
    assert_eq!(entries.len(), MAX_ENTRIES);
    assert_eq!(dropped, 5);
}

#[test]
fn closed_log_ignores_pushes() {
    let log = Log::new();
    let t = Instant::now();
    log.push(entry(t, t, "click"));
    let (first, d1) = log.take();
    assert_eq!(first.len(), 1);
    assert_eq!(d1, 0);
    log.push(entry(t, t, "type"));
    let (second, d2) = log.take();
    assert!(second.is_empty());
    assert_eq!(d2, d1);
}

#[test]
fn place_offsets_relative_to_clock() {
    let t0 = Instant::now();
    let clock = t0 + Duration::from_secs(1);
    let entries = vec![
        entry(
            t0 + Duration::from_millis(500),
            t0 + Duration::from_millis(600),
            "a",
        ),
        entry(
            clock + Duration::from_secs(1),
            clock + Duration::from_millis(1200),
            "b",
        ),
        entry(
            clock + Duration::from_secs(99),
            clock + Duration::from_secs(100),
            "c",
        ),
    ];
    let placed = place(&entries, clock, 10.0);
    let ats: Vec<f64> = placed
        .iter()
        .map(|p| match p {
            Placed::Action { at_s, .. } | Placed::Marker { at_s, .. } => *at_s,
        })
        .collect();
    assert_eq!(ats, vec![0.0, 1.0, 10.0]);
    for p in &placed {
        if let Placed::Action { at_s, end_s, .. } = p {
            assert!(end_s >= at_s);
        }
    }
}

fn sample_placed() -> Vec<Placed> {
    vec![
        Placed::Action {
            at_s: 2.5,
            end_s: 2.6,
            tool: "act".into(),
            action: "type".into(),
            steps: None,
            ok: false,
            verdict: "bad ref e9".into(),
        },
        Placed::Action {
            at_s: 0.5,
            end_s: 1.0,
            tool: "act".into(),
            action: "click".into(),
            steps: None,
            ok: true,
            verdict: "clicked x".into(),
        },
        Placed::Action {
            at_s: 1.2,
            end_s: 3.0,
            tool: "run".into(),
            action: "run".into(),
            steps: Some(4),
            ok: true,
            verdict: "run done".into(),
        },
        Placed::Marker {
            at_s: 3.0,
            note: "FAIL: x".into(),
        },
    ]
}

#[test]
fn json_shape_v1() {
    let files = vec![FileClock {
        file: "v.webm".into(),
        start_s: Some(0.0),
        duration_s: 4.0,
    }];
    let v = to_json(
        "v.webm",
        4.0,
        &files,
        &sample_placed(),
        0,
        CLOCK_FIRST_FRAME,
    );
    let obj = v.as_object().expect("object");
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "clock",
            "dropped",
            "duration_s",
            "entries",
            "files",
            "version",
            "video"
        ]
    );
    assert_eq!(v["version"], 1);
    assert_eq!(v["clock"], CLOCK_FIRST_FRAME);
    assert_eq!(v["video"], "v.webm");
    assert_eq!(v["dropped"], 0);
    assert_eq!(v["files"][0]["file"], "v.webm");
    assert_eq!(v["files"][0]["start_s"], 0.0);
    let entries = v["entries"].as_array().expect("entries");
    let ats: Vec<f64> = entries
        .iter()
        .map(|e| e["at_s"].as_f64().unwrap_or(-1.0))
        .collect();
    assert_eq!(ats, vec![0.5, 1.2, 2.5, 3.0]);
    let click = &entries[0];
    assert_eq!(click["kind"], "action");
    assert_eq!(click["tool"], "act");
    assert_eq!(click["action"], "click");
    assert_eq!(click["ok"], true);
    assert_eq!(click["verdict"], "clicked x");
    assert!(click.get("steps").is_none());
    assert_eq!(entries[1]["steps"], 4);
    assert_eq!(entries[3]["kind"], "marker");
    assert_eq!(entries[3]["note"], "FAIL: x");
    let mut mk: Vec<&str> = entries[3]
        .as_object()
        .expect("obj")
        .keys()
        .map(String::as_str)
        .collect();
    mk.sort_unstable();
    assert_eq!(mk, vec!["at_s", "kind", "note"]);
}

#[test]
fn json_has_no_absolute_paths() {
    let files = vec![FileClock {
        file: "/Users/me/recs/v-tab1.webm".into(),
        start_s: None,
        duration_s: 1.0,
    }];
    let v = to_json(
        "/Users/me/recs/v-tab1.webm",
        1.0,
        &files,
        &sample_placed(),
        0,
        CLOCK_START,
    );
    assert_eq!(v["video"], "v-tab1.webm");
    assert_eq!(v["files"][0]["file"], "v-tab1.webm");
    assert!(v["files"][0]["start_s"].is_null());
    let text = serde_json::to_string(&v).expect("ser");
    assert!(!text.contains("/Users"), "{text}");
    assert!(!text.contains('\\'), "{text}");
}

#[test]
fn json_escapes_hostile_text() {
    let placed = vec![
        Placed::Action {
            at_s: 0.0,
            end_s: 0.1,
            tool: "act".into(),
            action: "click".into(),
            steps: None,
            ok: true,
            verdict: "say \"hi\" \\ </script> \u{1}x".into(),
        },
        Placed::Marker {
            at_s: 0.2,
            note: "q\"uote\\ </script>".into(),
        },
    ];
    let v = to_json("v.webm", 1.0, &[], &placed, 3, CLOCK_START);
    let bytes = serde_json::to_vec_pretty(&v).expect("ser");
    let back: Value = serde_json::from_slice(&bytes).expect("parse");
    assert_eq!(back, v);
    assert_eq!(back["dropped"], 3);
}

#[test]
fn global_off_begin_is_none() {
    assert!(!crate::record::hub::recording_active());
    assert!(begin("act", &json!({"action": "click"})).is_none());
}

/// The two dispatch hooks are exactly three lines each (HOOKS.md rows): the
/// comment, `begin` right above the handler match, `end` right after it.
/// Pinned from source so a reformat or a moved hook fails here.
#[test]
fn hooks_are_three_lines_each() {
    let hooks = [
        ("proto", include_str!("../mcp/server/proto.rs")),
        ("cli", include_str!("../cli.rs")),
    ];
    for (name, src) in hooks {
        assert_eq!(
            src.matches("crate::record::timeline::begin(").count(),
            1,
            "{name}: one begin"
        );
        assert_eq!(
            src.matches("crate::record::timeline::end(tl, &result);")
                .count(),
            1,
            "{name}: one end"
        );
        let lines: Vec<&str> = src.lines().collect();
        let b = lines
            .iter()
            .position(|l| l.contains("crate::record::timeline::begin("))
            .expect("begin line");
        assert_eq!(
            lines[b - 1].trim(),
            "// fork hook (record): see .planning/HOOKS.md",
            "{name}: comment directly above begin"
        );
        assert!(lines[b].trim_start().starts_with("let tl = "), "{name}");
        assert!(
            lines[b + 1].trim_start().starts_with("let result = match"),
            "{name}: begin directly above the handler match"
        );
        let e = lines
            .iter()
            .position(|l| l.contains("crate::record::timeline::end(tl, &result);"))
            .expect("end line");
        assert_eq!(
            lines[e - 1].trim(),
            "};",
            "{name}: end right after the match"
        );
    }
}

/// Recording off: every tool the hooks see gets None from `begin`, so the
/// hooks do no work (one atomic load) and the output stays byte-identical.
#[test]
fn recording_off_hook_is_inert() {
    assert!(!crate::record::hub::recording_active());
    for tool in ["act", "see", "state", "run", "vision"] {
        let args = json!({ "action": "click", "op": "open-tab", "steps": [{}] });
        assert!(begin(tool, &args).is_none(), "{tool}");
        let log = Log::new();
        assert!(begin_with(false, Some(log.clone()), tool, &args).is_none());
        assert_eq!(log.len(), 0, "{tool}: nothing logged while off");
    }
}
