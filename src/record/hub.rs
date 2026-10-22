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

#[cfg(test)]
mod adapter_tests;
pub mod env;
pub mod live_env;
pub mod machine;
#[cfg(all(test, unix))]
mod machine_tests;
pub mod msg;
pub mod report;
#[cfg(test)]
mod report_tests;
#[cfg(test)]
mod schema_tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use serde_json::Value;
use tokio::sync::Mutex;

use crate::error::BladeError;
use crate::page::Page;

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

/// `state tabs` hook, part 1: while a recording is active the recorder's own
/// CDP connection makes its tab report `attached`, so the agent's tab is read
/// from its own session instead. None (and no CDP call) when nothing records.
pub async fn own_target(cdp: &crate::cdp::CdpSession) -> Option<String> {
    let m = HUB.get()?;
    if !m.lock().await.is_active() {
        return None;
    }
    let info = cdp.send("Target.getTargetInfo", None).await.ok()?;
    info["targetInfo"]["targetId"].as_str().map(String::from)
}

/// `state tabs` hook, part 2: is this tab the agent's current one? Without a
/// recording (`own` None) this is exactly Chrome's `attached` flag.
pub fn is_current(attached: bool, target_id: &str, own: Option<&str>) -> bool {
    own.map_or(attached, |o| o == target_id)
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

/// HubError -> BladeError: never `Closed`/`Transport`, so a refused or unclean
/// recording is not mistaken for a dead browser by the self-heal paths.
pub(crate) fn map_err(e: HubError) -> BladeError {
    match e {
        HubError::Usage(t) => BladeError::Usage(t),
        HubError::Failed(t) => BladeError::Other(t),
    }
}

/// `record-start` needs a string name; checked before the lock so a bad call
/// never initializes the hub.
pub(crate) fn check_name(args: &Value) -> Result<&str, BladeError> {
    args.get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| BladeError::Usage(msg::usage()))
}

/// CLI `record-start` name: `--name x` or exactly one positional.
pub fn cli_name(flag: Option<&str>, pos: &[String]) -> crate::error::Result<String> {
    let extra = pos.get(usize::from(flag.is_none()));
    if let Some(x) = extra {
        return Err(BladeError::Usage(format!(
            "unexpected extra argument '{x}' for record-start"
        )));
    }
    flag.map(String::from)
        .or_else(|| pos.first().cloned())
        .ok_or_else(|| {
            BladeError::Usage(
                "record-start needs a name, e.g. bladebro state record-start login-flow".into(),
            )
        })
}

/// The `state` hook: None for every op the hub does not own (nothing read,
/// locked or sent), Some(result) for record-start / record-stop.
pub async fn try_handle(args: &Value, page: &Page) -> Option<crate::error::Result<String>> {
    let op = record_op(args)?;
    let env = live_env::PageEnv { page };
    let name = match op {
        RecordOp::Start => match check_name(args) {
            Ok(n) => Some(n),
            Err(e) => return Some(Err(e)),
        },
        RecordOp::Stop => None,
    };
    let mut h = hub().lock().await;
    let r = match name {
        Some(n) => h.start(&env, n).await,
        None => h.stop(&env).await,
    };
    Some(r.map_err(map_err))
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

#[cfg(test)]
mod tabs_marker_tests {
    use super::is_current;

    #[test]
    fn off_mode_follows_attached() {
        assert!(is_current(true, "A", None));
        assert!(!is_current(false, "A", None));
    }

    #[test]
    fn recorded_tab_is_not_current_when_agent_is_elsewhere() {
        // Recording on A (attached by the recorder), agent on B (attached).
        let own = Some("B");
        assert!(!is_current(true, "A", own));
        assert!(is_current(true, "B", own));
        assert!(!is_current(false, "C", own));
    }

    #[test]
    fn agent_on_recorded_tab_marks_it() {
        assert!(is_current(true, "A", Some("A")));
    }
}
