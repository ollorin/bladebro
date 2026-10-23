//! Tab following (CAPT-03): ONE recording, ONE encoder, capture rebound to
//! whichever tab the agent is on.
//!
//! Module map:
//! - `probe`: `TabProbe` (HTTP `/json`, Fixed, Scripted) and `TabView`; no CDP method.
//! - `pick`: pure `agent_tab` rules, `Confirm` debounce, `Baseline` (lingering popup at start).
//! - `sink`: `FollowShared` + `SegmentSink` (generation and monotonic filters, no-op mark_end).
//! - `merge`: pure merge of per-segment reports and `compose_follow`.
//! - `files`: sink policy (one encoder, or one file per tab with a cap of 8) and the per-tab lifecycle.
//! - `run`: the follower task (poll, rebind, gone/lost handling, catch-up stop).
//! - `unit_tests` (tests only): tables for pick, sink, merge, msg.
//!
//! Rules: the follower reads only `GET /json` on the debugging port (live: the
//! `attached` flag is absent, so the agent's tab is the first page in Chrome's
//! order, confirmed on two samples ~50 ms apart, a brand-new tab only after it
//! stayed in front for 1.5 s); a popup that does not take
//! the front never becomes a segment; each switch starts a new `Source` on the
//! new tab BEFORE the old one is retired in the background.

mod files;
mod merge;
mod pick;
mod probe;
mod run;
mod sink;
#[cfg(test)]
mod unit_tests;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub use files::{Entered, Files, Policy, Skip, MAX_TAB_FILES};
pub use merge::{compose_follow, merge, MergeIn, Merged as FollowResult, SegIn};
pub use pick::{agent_tab, Baseline, Confirm, Rule};
pub use probe::{Scripted, TabProbe, TabView};
pub use sink::{FollowShared, SegmentSink};

use super::follow_report::PerTabResult;

use crate::record::source::{
    Health, Source, SourceConfig, SourceOutcome, SourceReport, SourceStats,
};

/// What a stopped follower hands back: one merged capture, or one result per file.
pub enum Outcome {
    One(Box<FollowResult>),
    PerTab(PerTabResult),
}

/// The rule in force; 08-01-discovery.txt explains why not `Attached`.
pub const RULE: Rule = Rule::ListOrder;
/// How often the tab list is read.
pub const POLL: Duration = Duration::from_millis(200);
/// Gap between the two samples that confirm a switch to a known tab.
pub const CONFIRM: Duration = Duration::from_millis(50);
/// A tab that just appeared in front must stay there this long (popups are
/// re-fronted by their opener within about a second; see `run.rs`).
pub const NEW_TAB_CONFIRM: Duration = Duration::from_millis(1500);

/// Live view for `msg::active` and the hub.
pub(super) struct Snap {
    pub tab: String,
    pub health: Health,
    /// Per-tab mode: every file so far, in tab order.
    pub files: Vec<std::path::PathBuf>,
    /// The follower gave up on Chrome (3 failed tab-list reads in a row).
    pub lost: bool,
}

pub struct SpawnArgs {
    /// The source already started on `tab` (generation 1 of `shared`).
    pub first: Source,
    pub tab: String,
    /// Where frames go: the one encoder (its only handle in the follower is
    /// dropped before the result is returned so the hub can take it back), or
    /// the per-tab files.
    pub policy: Policy,
    pub probe: TabProbe,
    pub scfg: SourceConfig,
    pub poll: Duration,
    pub confirm: Duration,
    /// `first_view` sampled before the first source started.
    pub initial: Initial,
    pub new_confirm: Duration,
}

/// Chrome's tab list at record-start.
#[derive(Debug, Default)]
pub struct Initial {
    pub head: Option<String>,
    pub known: Vec<String>,
}

/// Chrome's list right now: its first real page is the baseline for `Baseline`,
/// every id in it counts as already open (empty with `Fixed` or on an error).
pub async fn first_view(probe: &TabProbe, tab: &str) -> Initial {
    if probe.is_fixed() {
        return Initial::default();
    }
    match tokio::time::timeout(Duration::from_secs(2), probe.list(tab)).await {
        Ok(Ok(l)) => Initial {
            head: agent_tab(&l, &[], Rule::ListOrder),
            known: l.into_iter().map(|v| v.id).collect(),
        },
        _ => Initial::default(),
    }
}

pub struct Tracker {
    stop_tx: Option<oneshot::Sender<Option<String>>>,
    task: Option<JoinHandle<Outcome>>,
    snap: Arc<Mutex<Snap>>,
    cap: Duration,
    per_tab: bool,
}

impl Tracker {
    pub fn spawn(a: SpawnArgs) -> Tracker {
        let snap = Arc::new(Mutex::new(Snap {
            tab: a.tab.clone(),
            health: a.first.health(),
            files: a.policy.files(),
            lost: false,
        }));
        let per_tab = matches!(a.policy, Policy::PerTab(_));
        let extra = match &a.policy {
            Policy::PerTab(f) => f.cap(),
            Policy::One { .. } => Duration::ZERO,
        };
        let cap = a.scfg.stop_grace * 2 + Duration::from_secs(14) + extra;
        let (stop_tx, stop_rx) = oneshot::channel();
        let p = run::Params {
            no_frames_after: a.scfg.no_frames_after,
            first: a.first,
            tab: a.tab,
            policy: a.policy,
            probe: a.probe,
            scfg: a.scfg,
            poll: a.poll,
            confirm: a.confirm,
            snap: snap.clone(),
            head: a.initial.head,
            known: a.initial.known,
            new_confirm: a.new_confirm,
        };
        let task = tokio::spawn(run::run(p, stop_rx));
        Tracker {
            stop_tx: Some(stop_tx),
            task: Some(task),
            snap,
            cap,
            per_tab,
        }
    }

    /// Health of the current capture (refreshed every poll).
    pub fn health(&self) -> Health {
        self.snap
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .health
            .clone()
    }

    /// Chrome stopped answering the tab list (the recording guard's trigger).
    pub fn browser_lost(&self) -> bool {
        self.snap.lock().unwrap_or_else(|e| e.into_inner()).lost
    }

    /// Per-tab mode: the files so far (empty in follow mode).
    pub fn files(&self) -> Vec<std::path::PathBuf> {
        self.snap
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .files
            .clone()
    }

    /// The tab capture is bound to right now.
    pub fn current_tab(&self) -> String {
        self.snap
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .tab
            .clone()
    }

    /// End the recording: `hint` is the tab the agent is on now (catch-up when
    /// the last switch is younger than one poll). Bounded; a follower that does
    /// not answer is aborted and reported honestly.
    pub async fn stop(mut self, hint: Option<String>) -> Outcome {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(hint);
        }
        // No facts about the frames: not clean, named, never "saved".
        let paths = self.files();
        let stuck = || {
            let mut text = super::msg::follower_stuck();
            if !paths.is_empty() {
                let l: Vec<_> = paths.iter().map(|p| p.display().to_string()).collect();
                text.push_str(&format!(" (files: {})", l.join(", ")));
            }
            if self.per_tab {
                return Outcome::PerTab(PerTabResult {
                    failures: vec![text],
                    ..Default::default()
                });
            }
            let one = FollowResult {
                report: SourceReport {
                    outcome: SourceOutcome::Captured,
                    stats: SourceStats::default(),
                    target_id: String::new(),
                    elapsed_ms: 0,
                    last_frame_age_ms: None,
                    quiet_tail_ms: None,
                    hidden: false,
                },
                segments: Vec::new(),
                notes: Vec::new(),
                failures: vec![text],
            };
            Outcome::One(Box::new(one))
        };
        let Some(mut task) = self.task.take() else {
            return stuck();
        };
        match tokio::time::timeout(self.cap, &mut task).await {
            Ok(Ok(r)) => r,
            _ => {
                task.abort();
                stuck()
            }
        }
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        // Aborting drops every Source (their loops abort) and the sinks; the
        // encoder's kill_on_drop is the backstop for the process itself.
        if let Some(t) = self.task.take() {
            t.abort();
        }
    }
}
