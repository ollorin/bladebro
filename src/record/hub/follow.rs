//! Tab following (CAPT-03): ONE recording, ONE encoder, capture rebound to
//! whichever tab the agent is on.
//!
//! Module map:
//! - `probe`: `TabProbe` (HTTP `/json`, Fixed, Scripted) and `TabView`; no CDP method.
//! - `pick`: pure `agent_tab` rules, `Confirm` debounce, `Baseline` (lingering popup at start).
//! - `sink`: `FollowShared` + `SegmentSink` (generation and monotonic filters, no-op mark_end).
//! - `merge`: pure merge of per-segment reports and `compose_follow`.
//! - `run`: the follower task (poll, rebind, gone/lost handling, catch-up stop).
//! - `unit_tests` (tests only): tables for pick, sink, merge, msg.
//!
//! Rules: the follower reads only `GET /json` on the debugging port (live: the
//! `attached` flag is absent, so the agent's tab is the first page in Chrome's
//! order, confirmed on two samples ~50 ms apart); a popup that does not take
//! the front never becomes a segment; each switch starts a new `Source` on the
//! new tab BEFORE the old one is retired in the background.

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

pub use merge::{compose_follow, merge, MergeIn, Merged as FollowResult, SegIn};
pub use pick::{agent_tab, Baseline, Confirm, Rule};
pub use probe::{Scripted, TabProbe, TabView};
pub use sink::{FollowShared, SegmentSink};

use crate::record::source::{
    FrameSink, Health, Source, SourceConfig, SourceOutcome, SourceReport, SourceStats,
};

/// The rule in force; 08-01-discovery.txt explains why not `Attached`.
pub const RULE: Rule = Rule::ListOrder;
/// How often the tab list is read.
pub const POLL: Duration = Duration::from_millis(200);
/// Gap between the two samples that confirm a switch.
pub const CONFIRM: Duration = Duration::from_millis(50);

/// Live view for `msg::active` and the hub.
pub(super) struct Snap {
    pub tab: String,
    pub health: Health,
}

pub struct SpawnArgs {
    /// The source already started on `tab` (generation 1 of `shared`).
    pub first: Source,
    pub tab: String,
    pub shared: Arc<FollowShared>,
    /// The encoder as a sink: the follower's only handle to it, dropped before
    /// the result is returned so the hub can take the encoder back.
    pub inner: Arc<dyn FrameSink>,
    pub probe: TabProbe,
    pub scfg: SourceConfig,
    pub poll: Duration,
    pub confirm: Duration,
}

pub struct Tracker {
    stop_tx: Option<oneshot::Sender<Option<String>>>,
    task: Option<JoinHandle<FollowResult>>,
    snap: Arc<Mutex<Snap>>,
    cap: Duration,
}

impl Tracker {
    pub fn spawn(a: SpawnArgs) -> Tracker {
        let snap = Arc::new(Mutex::new(Snap {
            tab: a.tab.clone(),
            health: a.first.health(),
        }));
        let cap = a.scfg.stop_grace * 2 + Duration::from_secs(14);
        let (stop_tx, stop_rx) = oneshot::channel();
        let p = run::Params {
            no_frames_after: a.scfg.no_frames_after,
            first: a.first,
            tab: a.tab,
            shared: a.shared,
            inner: a.inner,
            probe: a.probe,
            scfg: a.scfg,
            poll: a.poll,
            confirm: a.confirm,
            snap: snap.clone(),
        };
        let task = tokio::spawn(run::run(p, stop_rx));
        Tracker {
            stop_tx: Some(stop_tx),
            task: Some(task),
            snap,
            cap,
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
    pub async fn stop(mut self, hint: Option<String>) -> FollowResult {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(hint);
        }
        // No facts about the frames: not clean, named, never "saved".
        let stuck = || FollowResult {
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
            failures: vec![super::msg::follower_stuck()],
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
