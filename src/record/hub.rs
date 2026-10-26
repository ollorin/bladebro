//! Recording hub: one active recording per process, started and stopped by
//! the `state` ops `record-start` / `record-stop` (CTRL-01/04/05, OUT-05).
//!
//! Module map:
//! - `msg`: message texts and `scrub` (no dead-tab substrings, ever).
//! - `report`: pure (source report, encoder report) -> one honest stop line.
//! - `env`: `HubEnv` (config, ffmpeg, lane, current tab, attach) injected into the machine.
//! - `follow`: tab following (CAPT-03): follower task, agent-tab rule, segment sink, merge.
//! - `follow_report`: pure per-tab stop result (CAPT-04), one compact line for every file.
//! - `guard`: per-recording task that finalizes early on a dead browser/encoder (LIFE-02).
//! - `machine`: `Hub` state machine (start/finish_orphan, start-failure cleanup, encoder hand-back).
//! - `finish` (child of `machine`): stop/stop_marked/finish_orphan and the shared finish path.
//! - `evidence`: stop extras (OUT-06): poster pick and run, marker offset on the video clock.
//! - `report_tests`, `machine_tests` (tests only): outcome table; scenarios over fake ffmpeg + FakeCdp.
//! - `listing_tests` (tests only): recordings-dir helpers list videos, never sidecars.
//!
//! The process-global lives here and is a thin wrapper: all logic is on `Hub`
//! instances so tests never share state through it.
//!
//! Fork-only module (SYNC-01): no upstream file is touched by it.

#[cfg(test)]
mod adapter_tests;
pub mod env;
pub mod evidence;
#[cfg(all(test, unix))]
mod evidence_tests;
#[cfg(all(test, unix))]
#[path = "exit_tests.rs"]
mod exit_tests;
pub mod follow;
pub mod follow_report;
#[cfg(test)]
mod follow_report_tests;
#[cfg(all(test, unix))]
mod follow_tests;
pub mod guard;
#[cfg(all(test, unix))]
mod guard_tests;
#[cfg(all(test, unix))]
mod listing_tests;
pub mod live_env;
pub mod machine;
#[cfg(all(test, unix))]
mod machine_tests;
pub mod msg;
#[cfg(all(test, unix))]
mod per_tab_popup_tests;
#[cfg(all(test, unix))]
mod per_tab_tests;
#[cfg(all(test, unix))]
mod popup_tests;
pub mod report;
#[cfg(test)]
mod report_tests;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod tabs_marker_tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use serde_json::Value;
use tokio::sync::Mutex;

use crate::error::BladeError;
use crate::page::Page;

pub use machine::{Hub, HubError};

static HUB: OnceLock<Mutex<Hub>> = OnceLock::new();
static EPHEMERAL: AtomicBool = AtomicBool::new(false);
/// Mirror of the global hub's `is_active()`, readable without the hub lock or
/// initializing it (the page layer asks it on every popup click).
static RECORDING: AtomicBool = AtomicBool::new(false);

/// True while the global hub holds an active recording.
pub fn recording_active() -> bool {
    RECORDING.load(Ordering::Relaxed)
}

/// Store the flag from the hub's state; called after every transition of the
/// global hub (start/stop, orphan finish, guard finalize).
pub(crate) fn publish_active(h: &Hub) {
    RECORDING.store(h.is_active(), Ordering::Relaxed);
    // The timeline hooks read the current log through this, never the hub.
    crate::record::timeline::publish(h.timeline_log());
}

/// Decision of the opener re-front after a click opened a tab: while recording
/// the popup keeps the front (stock Chrome behaviour) so it can be recorded.
pub(crate) fn refront_after_popup(active: bool) -> bool {
    !active
}

/// Upstream hook (`Page::act`, new-tab branch). Recording off: exactly
/// `page.activate()`, so the CDP method sequence is unchanged.
pub async fn activate_opener(page: &Page) -> crate::error::Result<()> {
    if refront_after_popup(recording_active()) {
        page.activate().await
    } else {
        Ok(())
    }
}

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
    publish_active(&h);
    if let Some(t) = h.orphaned_text() {
        tracing::warn!(target: "bladebro.record", "{t}");
    }
}

/// What `state tabs` marks while recording: the agent's tab and the tab the
/// video follows (None in per-tab mode while the followed tab has no file).
pub struct TabsView {
    pub agent: String,
    pub video: Option<String>,
}

/// `state tabs` hook, part 1: while a recording is active the recorder's own
/// CDP connection makes its tab report `attached`, so the agent's tab is read
/// from its own session instead. None (and no CDP call) when nothing records.
pub async fn own_target(cdp: &crate::cdp::CdpSession) -> Option<TabsView> {
    let m = HUB.get()?;
    let video = {
        let h = m.lock().await;
        if !h.is_active() {
            return None;
        }
        h.video_tab()
    };
    let info = cdp.send("Target.getTargetInfo", None).await.ok()?;
    let agent = info["targetInfo"]["targetId"].as_str()?.to_string();
    Some(TabsView { agent, video })
}

/// `state tabs` hook, part 2: the row marker. Without a recording (`own` None)
/// exactly Chrome's `attached` flag (`*` or space); while recording two
/// characters: `*` agent, `R` video.
pub fn tab_marker(attached: bool, id: &str, own: Option<&TabsView>) -> &'static str {
    let Some(o) = own else {
        return if attached { "*" } else { " " };
    };
    match (o.agent == id, o.video.as_deref() == Some(id)) {
        (true, true) => "*R",
        (true, false) => "* ",
        (false, true) => " R",
        (false, false) => "  ",
    }
}

/// `state tabs` hook, part 3: one extra line only when agent and video differ.
pub fn tabs_footer(own: Option<&TabsView>) -> &'static str {
    match own {
        Some(o) if o.video.as_deref().is_some_and(|v| v != o.agent) => {
            "(* agent, R video: differ; switch-tab to move the agent)\n"
        }
        _ => "",
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
        None => {
            h.stop_marked(&env, crate::record::marker::marker_from_args(args))
                .await
        }
    };
    publish_active(&h);
    // LIFE-02: a started recording gets its guard (generation-checked).
    if let (Some(_), Ok(_), Some(g)) = (name, &r, h.active_gen()) {
        guard::spawn(hub(), g, guard::GUARD_TICK);
    }
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
mod popup_hook_tests {
    use super::*;

    #[test]
    fn flag_defaults_off_without_initializing_the_hub() {
        assert!(!recording_active());
        assert!(!initialized());
    }

    #[test]
    fn refront_is_skipped_only_while_recording() {
        assert!(refront_after_popup(false));
        assert!(!refront_after_popup(true));
    }

    #[test]
    fn publish_reads_the_hub_state() {
        publish_active(&Hub::default());
        assert!(!recording_active());
    }

    #[test]
    fn upstream_line_is_the_only_hook() {
        let src = include_str!("../page/mod.rs");
        assert_eq!(src.matches("activate_opener(self)").count(), 1);
        assert!(src.contains("self.activate().await?;"));
    }
}
