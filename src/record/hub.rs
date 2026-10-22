//! Recording hub: one active recording per process, started and stopped by
//! the `state` ops `record-start` / `record-stop` (CTRL-01/04/05, OUT-05).
//!
//! Module map:
//! - `msg`: message texts and `scrub` (no dead-tab substrings, ever).
//! - `report`: pure (source report, encoder report) -> one honest stop line.
//! - `env`: `HubEnv` (config, ffmpeg, lane, current tab, attach) injected into the machine.
//! - `machine`: `Hub` state machine (start/stop/finish_orphan, start-failure cleanup, encoder hand-back).
//! - `report_tests`, `machine_tests` (tests only): outcome table; scenarios over fake ffmpeg + FakeCdp.
//!
//! The process-global lives here and is a thin wrapper: all logic is on `Hub`
//! instances so tests never share state through it.
//!
//! Fork-only module (SYNC-01): no upstream file is touched by it.

pub mod env;
pub mod machine;
#[cfg(all(test, unix))]
mod machine_tests;
pub mod msg;
pub mod report;
#[cfg(test)]
mod report_tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use serde_json::Value;
use tokio::sync::Mutex;

pub use machine::{Hub, HubError};

static HUB: OnceLock<Mutex<Hub>> = OnceLock::new();
static EPHEMERAL: AtomicBool = AtomicBool::new(false);

/// The one hub of this process (one browser, one recording).
pub fn hub() -> &'static Mutex<Hub> {
    HUB.get_or_init(|| Mutex::new(Hub::new()))
}

/// Mark this process as a one-shot CLI call: it exits after the command, so
/// a recording could never be stopped.
pub fn set_ephemeral() {
    EPHEMERAL.store(true, Ordering::Relaxed);
}

pub fn is_ephemeral() -> bool {
    EPHEMERAL.load(Ordering::Relaxed)
}

/// Surface exit / browser shutdown: finalize an active recording and keep its
/// report for the next call. Never initializes the hub, so a session that
/// never recorded pays nothing (CTRL-01).
pub async fn finish_best_effort() {
    let Some(m) = HUB.get() else {
        return;
    };
    let mut h = m.lock().await;
    if !h.is_active() {
        return;
    }
    h.finish_orphan().await;
    if let Some(t) = h.orphaned_text() {
        tracing::warn!(target: "bladebro.record", "{t}");
    }
}

#[cfg(test)]
pub(crate) fn initialized() -> bool {
    HUB.get().is_some()
}

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
        // Claiming never touches the global (CTRL-01: zero cost when off).
        assert!(!initialized());
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
