//! Recording hub: one active recording per process, started and stopped by
//! the `state` ops `record-start` / `record-stop` (CTRL-01/04/05, OUT-05).
//!
//! Module map:
//! - `msg`: message texts and `scrub` (no dead-tab substrings, ever).
//! - `report`: pure (source report, encoder report) -> one honest stop line.
//! - `report_tests` (tests only): outcome table.
//!
//! Fork-only module (SYNC-01): no upstream file is touched by it.

pub mod msg;
pub mod report;
#[cfg(test)]
mod report_tests;

use serde_json::Value;

/// The two ops this hub owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOp {
    Start,
    Stop,
}

/// Claim a `state` call: Some only for `record-start` / `record-stop`. A bare
/// string compare on `args["op"]` with no env read, lock or I/O, so every
/// other state op (and recording-off sessions) pays nothing (CTRL-01).
pub fn record_op(args: &Value) -> Option<RecordOp> {
    match args.get("op")?.as_str()? {
        "record-start" => Some(RecordOp::Start),
        "record-stop" => Some(RecordOp::Stop),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn existing_state_ops_are_never_claimed() {
        for op in [
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
            "compress",
        ] {
            assert_eq!(record_op(&json!({ "op": op })), None, "{op}");
        }
    }

    #[test]
    fn malformed_args_are_not_claimed() {
        assert_eq!(record_op(&json!({})), None);
        assert_eq!(record_op(&json!({ "op": 3 })), None);
        assert_eq!(record_op(&json!({ "op": null })), None);
        assert_eq!(record_op(&json!("record-start")), None);
        assert_eq!(record_op(&json!(null)), None);
        assert_eq!(record_op(&json!(["record-start"])), None);
    }

    #[test]
    fn record_ops_are_claimed() {
        assert_eq!(
            record_op(&json!({ "op": "record-start", "name": "x" })),
            Some(RecordOp::Start)
        );
        assert_eq!(
            record_op(&json!({ "op": "record-stop" })),
            Some(RecordOp::Stop)
        );
    }
}
