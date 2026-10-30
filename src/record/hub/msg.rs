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

/// Append a marker note (`. Marker ...`) to a stop text that may already end with a
/// period, so an agent never reads `first.. Marker` (Phase 10 finding).
pub fn join_note(text: &str, note: &str) -> String {
    match (text.strip_suffix('.'), note.starts_with(". ")) {
        (Some(head), true) => format!("{head}{note}"),
        _ => format!("{text}{note}"),
    }
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

/// A followed popup closed and the video went back to the tab that opened it.
pub fn popup_closed(popup: &str, at_s: f64, opener: &str) -> String {
    format!("popup {popup} closed at {at_s:.1} s; back on opener {opener}")
}

/// Suffix `merge` gives the segment label of a tab that got no frame.
pub const NO_FRAMES_LABEL: &str = " (no frames)";

/// At stop the agent's tab and the tab the video ended on differ (only then).
/// `video` is a segment label: a zero-frame tab's label ends with
/// `NO_FRAMES_LABEL`, so the ids are compared without it; otherwise a
/// zero-frame stop on the agent's own tab named that tab twice (11-03 gap 1).
pub fn agent_vs_video(agent: Option<&str>, video: Option<&str>) -> Option<String> {
    match (agent, video) {
        (Some(a), Some(v)) if a != v.strip_suffix(NO_FRAMES_LABEL).unwrap_or(v) => Some(format!(
            "agent was on tab {a} at stop; video followed tab {v}"
        )),
        _ => None,
    }
}

/// Popups of the recording that never stayed in front (at most 3 ids named).
pub fn popups_unfollowed(n: usize, ids: &[String]) -> String {
    let shown: Vec<&str> = ids.iter().take(3).map(String::as_str).collect();
    let more = if n > shown.len() {
        format!(", +{} more", n - shown.len())
    } else {
        String::new()
    };
    format!(
        "{n} popup(s) not recorded (never in front): {}{more}; switch-tab to record one",
        shown.join(", ")
    )
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
/// A recording the guard stopped on a limit (LIFE-04), finalized.
const STOPPED: &str = "previous recording stopped: ";

/// The stored orphan text as the next record op prints it.
pub fn orphan_text(t: &str) -> String {
    if t.starts_with(INTERRUPTED) || t.starts_with(STOPPED) {
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

pub fn reason_disk_low(free_mib: u64) -> String {
    format!("disk low ({free_mib} MiB free)")
}

pub fn reason_max_duration() -> String {
    format!(
        "maximum duration ({}) reached",
        crate::record::disk::MAX_DURATION_TEXT
    )
}

pub fn reason_max_size(file: Option<&str>) -> String {
    let by = file.map(|f| format!(" by {f}")).unwrap_or_default();
    format!(
        "size limit ({}) reached{by}",
        crate::record::disk::MAX_FILE_TEXT
    )
}

/// Refusal at record-start: nothing is created and no ffmpeg is spawned.
pub fn disk_low_refusal(free_mib: u64, dir: &str) -> String {
    format!(
        "recording not started: disk low ({free_mib} MiB free in {dir}, need {} MiB)",
        crate::record::disk::DISK_LOW_MIB
    )
}

/// LIFE-04: the guard stopped a recording on a limit. The file was finalized;
/// `finish` is the finalize report ("recording saved: <path> (...)").
pub fn stopped(reason: &str, path: &str, finish: &str) -> String {
    // Single file: "recording saved: <p>"; per-tab: "recording saved (per-tab,
    // N files): <p>; <p>". Both read "saved..." after the reason.
    let rest = finish
        .strip_prefix("recording saved")
        .map_or_else(|| finish.to_string(), |r| format!("saved{r}"));
    // A failed finalize keeps its own report; the path is then added.
    if rest.starts_with("saved") {
        format!("{STOPPED}{reason}; {rest}")
    } else {
        format!("{STOPPED}{reason}; {rest} (file: {path})")
    }
}
