//! Hub message texts (product surface: an LLM reads them mid-task).
//!
//! Every failure names the recovery path and ends "Browsing is unaffected."
//! (or "Nothing was recorded." when no file exists). None may contain the
//! substrings the MCP/daemon loops read as a dead tab ("Target closed", "No
//! target with given id", "Session closed", "Target.detachedFromTarget"):
//! they would trigger `recover_dead_tab` for a message that is only text.
//! `scrub` enforces that on every outgoing string.

use crate::record::source::{Health, SourceError};

/// Substrings that make the surrounding loops treat a result as a dead tab.
const DEAD_TAB: [&str; 4] = [
    "Target closed",
    "No target with given id",
    "Session closed",
    "Target.detachedFromTarget",
];

/// Replace every dead-tab marker with neutral text.
pub fn scrub(s: &str) -> String {
    let mut out = s.to_string();
    for m in DEAD_TAB {
        if out.contains(m) {
            out = out.replace(m, "tab gone");
        }
    }
    out
}

pub fn off() -> String {
    "recording is off: the operator has not enabled it. Set BLADE_RECORD=1 (or record.enabled in .bladebro.json / record.json), then run bladebro stop (or restart the MCP server) so it is re-read. Nothing was recorded.".to_string()
}

pub fn ephemeral() -> String {
    "recording needs a long-lived session: use the MCP server or the daemon (bladebro without --no-daemon/--port); one-shot CLI exits after each command. Nothing was recorded.".to_string()
}

pub fn pipe() -> String {
    SourceError::PipeTransport.to_string()
}

pub fn real_lane() -> String {
    "recording is not available in the real-browser lane (it would capture your own browser). Nothing was recorded.".to_string()
}

pub fn usage() -> String {
    "record-start needs name (a string), e.g. state record-start login-flow.".to_string()
}

pub fn none() -> String {
    "no active recording; record-start <name> first.".to_string()
}

/// Operator-facing label of the live capture state.
pub fn health_label(h: &Health) -> String {
    match h {
        Health::Waiting { .. } => "waiting for first frame".to_string(),
        Health::NoFrames { .. } => "no frames yet (tab hidden or in background?)".to_string(),
        Health::Receiving { .. } => "receiving".to_string(),
        Health::Quiet { last_age_ms, .. } => {
            format!("quiet {:.1} s", *last_age_ms as f64 / 1000.0)
        }
    }
}

/// CTRL-05: the second start names the file that is still being written.
pub fn active(path: &str, age_ms: u64, h: &Health) -> String {
    format!(
        "recording already active: {path} (started {:.1} s ago, {}). record-stop it first.",
        age_ms as f64 / 1000.0,
        health_label(h)
    )
}

/// D7-8: a start does not wait for a first frame (a static page may send one).
pub fn started(path: &str, tab: &str) -> String {
    format!("recording started: {path} (tab {tab}); first frame not awaited. record-stop saves it.")
}

pub fn not_finalized(path: &str) -> String {
    format!(
        "recording not finalized: the frame source did not release the encoder within 1 s; the file {path} may be incomplete. Browsing is unaffected."
    )
}

/// Phase 8 adds tab following; until then the video stays on the bound tab.
pub fn tab_moved(bound: &str, now: &str) -> String {
    format!(
        "agent is now on tab {now}; the video shows only tab {bound} (tab following is not implemented yet)"
    )
}

pub fn orphan_prefix() -> &'static str {
    "previous recording ended with the browser: "
}
