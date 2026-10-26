//! CLI parse tests for `state record-start|record-stop`; compiled into
//! `cli::args::state` (a private module) via a `#[path]` test mod there.

use super::parse_state_args;
use crate::error::BladeError;
use crate::record::hub::record_op;
use serde_json::json;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn usage_text(r: crate::error::Result<serde_json::Value>) -> String {
    match r {
        Err(BladeError::Usage(t)) => t,
        other => panic!("expected usage error, got {other:?}"),
    }
}

#[test]
fn record_start_parses_positional_and_flag() {
    let want = json!({"op":"record-start","name":"demo"});
    assert_eq!(
        parse_state_args(&s(&["record-start", "demo"])).unwrap(),
        want
    );
    assert_eq!(
        parse_state_args(&s(&["record-start", "--name", "demo"])).unwrap(),
        want
    );
}

#[test]
fn record_start_needs_exactly_one_name() {
    assert!(
        usage_text(parse_state_args(&s(&["record-start"]))).contains("record-start needs a name")
    );
    assert!(
        usage_text(parse_state_args(&s(&["record-start", "a", "b"])))
            .contains("unexpected extra argument 'b'")
    );
}

#[test]
fn record_stop_takes_nothing() {
    assert_eq!(
        parse_state_args(&s(&["record-stop"])).unwrap(),
        json!({"op":"record-stop"})
    );
}

#[test]
fn record_stop_takes_value_flag() {
    // OUT-06: the marker note rides in the existing `value` param (D10-2).
    assert_eq!(
        parse_state_args(&s(&["record-stop", "--value", "a b"])).unwrap(),
        json!({"op":"record-stop","value":"a b"})
    );
}

#[test]
fn record_stop_positional_still_refused() {
    let t = usage_text(parse_state_args(&s(&["record-stop", "x"])));
    assert!(
        t.contains("unexpected extra argument 'x' for record-stop"),
        "{t}"
    );
}

#[test]
fn unknown_op_lists_record_ops() {
    let t = usage_text(parse_state_args(&s(&["bogus"])));
    assert!(
        t.contains("record-start") && t.contains("record-stop"),
        "{t}"
    );
}

#[test]
fn existing_ops_parse_unchanged_and_are_not_claimed() {
    let cases: Vec<(Vec<&str>, serde_json::Value)> = vec![
        (vec!["tabs"], json!({"op":"tabs"})),
        (
            vec!["open-tab", "http://x"],
            json!({"op":"open-tab","url":"http://x"}),
        ),
        (vec!["save", "n"], json!({"op":"save","name":"n"})),
        (
            vec!["block", "--classes", "images"],
            json!({"op":"block","classes":"images"}),
        ),
    ];
    for (a, want) in cases {
        let got = parse_state_args(&s(&a)).unwrap();
        assert_eq!(got, want);
        assert_eq!(record_op(&got), None);
    }
}
