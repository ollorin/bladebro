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

/// Per-tab mode: the first file is the start tab's; more appear as the agent
/// visits tabs.
pub fn started_per_tab(path: &str, tab: &str) -> String {
    format!("recording started (per-tab files): {path} (tab {tab}); first frame not awaited. record-stop saves every tab's file.")
}

/// CTRL-05 in per-tab mode: the second start names the first file and the count.
pub fn active_per_tab(path: &str, files: usize, age_ms: u64, h: &Health) -> String {
    format!(
        "recording already active: {path} (per-tab, {files} file{}, started {:.1} s ago, {}). record-stop it first.",
        if files == 1 { "" } else { "s" },
        age_ms as f64 / 1000.0,
        health_label(h)
    )
}

/// The 9th open tab: an operator limit, named with the time it went unrecorded.
pub fn tab_capped(tab: &str, at_s: f64, cap: usize) -> String {
    format!("tab {tab} not recorded from {at_s:.1} s: per-tab cap of {cap} open files reached (close a tab to free one)")
}

/// A tab's file was finished because the tab closed.
pub fn tab_closed(tab: &str, at_s: f64) -> String {
    format!("tab {tab} closed at {at_s:.1} s")
}

/// A tab whose file was already finished is in front again.
pub fn tab_file_done(tab: &str, at_s: f64) -> String {
    format!("tab {tab} not recorded from {at_s:.1} s: its file was already finished")
}

pub fn not_finalized(path: &str) -> String {
    format!(
        "recording not finalized: the frame source did not release the encoder within 1 s; the file {path} may be incomplete. Browsing is unaffected."
    )
}

/// The recorded tab went away and the agent's next tab took over.
pub fn tab_gone(from: &str, at_s: f64, to: &str) -> String {
    format!("tab {from} gone at {at_s:.1} s; following tab {to}")
}

/// The recorded tab is still listed but its recorder connection ended.
pub fn tab_reconnected(tab: &str, at_s: f64) -> String {
    format!("recorder connection to tab {tab} ended at {at_s:.1} s; reconnected")
}

pub fn no_tab(at_s: f64) -> String {
    format!("no tab left at {at_s:.1} s")
}

pub fn browser_lost(at_s: f64) -> String {
    format!("browser connection lost at {at_s:.1} s")
}

pub fn rebind_failed(tab: &str, at_s: f64, err: &str) -> String {
    format!("tab {tab} not recorded from {at_s:.1} s: {err}")
}

pub fn zero_frames(tab: &str, for_s: f64) -> String {
    format!("tab {tab}: no frames for {for_s:.1} s (hidden or background tab)")
}

/// The follower did not answer the stop in time.
pub fn follower_stuck() -> String {
    "the tab follower did not stop in time; the recording may be missing its last seconds"
        .to_string()
}

pub fn orphan_prefix() -> &'static str {
    "previous recording ended with the browser: "
}

/// Parked results that already say what happened start with this; they are
/// not prefixed with `orphan_prefix` (it would claim the browser ended them).
const INTERRUPTED: &str = "previous recording interrupted: ";

/// The stored orphan text as the next record op prints it.
pub fn orphan_text(t: &str) -> String {
    if t.starts_with(INTERRUPTED) {
        t.to_string()
    } else {
        format!("{}{t}", orphan_prefix())
    }
}

pub fn reason_browser_lost() -> &'static str {
    "browser connection lost"
}

pub fn reason_encoder_dead() -> &'static str {
    "encoder exited"
}

/// LIFE-02: the guard ended a recording early. `finish` is the finalize report
/// (never "saved": the recording did not end by the agent's stop).
pub fn interrupted(reason: &str, at_s: f64, path: &str, empty: bool, finish: &str) -> String {
    let finish = match finish.strip_prefix("recording saved: ") {
        Some(rest) => format!("finalized: {rest}"),
        None => finish.to_string(),
    };
    // An encoder killed before it wrote anything leaves a 0-byte file: say so
    // instead of promising a playable partial.
    let file = if empty {
        format!("file is empty, nothing playable: {path}")
    } else {
        format!("partial file kept: {path}")
    };
    format!("{INTERRUPTED}{reason} at {at_s:.1} s; {file}. {finish}")
}
