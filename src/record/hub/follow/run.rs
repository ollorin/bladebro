//! The follower task: polls the tab list, decides where the agent is and
//! rebinds capture by starting a new `Source` on that tab into the same
//! encoder (see `follow.rs` for the rules).
//!
//! Concurrency: the task owns every `Source`; the hub mutex is never taken
//! here. The only shared state is the snapshot mutex (never held across an
//! await) and `FollowShared` (short, non-async critical sections). A retiring
//! source holds only its own `SegmentSink`, so it cannot disturb the next
//! generation.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

use super::files::{Policy, Skip};
use super::merge::{merge, MergeIn, SegIn};
use super::opener::{OpenerState, Step};
use super::pick::{agent_tab, Baseline, Confirm};
use super::probe::{OpenerFeed, TabProbe, TabView};
use super::sink::FollowShared;
use super::{Outcome, Snap, RULE};
use crate::record::hub::msg;
use crate::record::source::{Source, SourceConfig, SourceOutcome, SourceReport, SourceStats};

/// A rebind that cannot finish within this is a failure, never a hang.
const REBIND_CAP: Duration = Duration::from_secs(4);
const LIST_CAP: Duration = Duration::from_secs(2);
/// Consecutive probe errors before the browser counts as gone.
const LOST_AFTER: u32 = 3;
/// Consecutive failed rebinds to one tab before it is reported.
const FAIL_AFTER: u32 = 3;
/// A switch made just before record-stop gets this long to deliver a frame.
const CATCHUP_WAIT: Duration = Duration::from_millis(400);

pub(super) struct Params {
    pub first: Source,
    pub tab: String,
    pub policy: Policy,
    pub probe: TabProbe,
    pub scfg: SourceConfig,
    pub poll: Duration,
    pub confirm: Duration,
    pub no_frames_after: Duration,
    pub snap: Arc<Mutex<Snap>>,
    /// Chrome's list at record-start: its first page (see `Baseline`) and
    /// every id already open (never a popup, see `opener`).
    pub head: Option<String>,
    pub known: Vec<String>,
    /// How long a popup of the recording must stay in front before it is followed.
    pub popup_settle: Duration,
}

struct Cur {
    src: Source,
    tab: String,
    seg: usize,
}

/// One visit to a tab (a capture generation).
pub struct SegRec {
    pub tab: String,
    pub gen: u64,
    start: Instant,
    end: Option<Instant>,
    report: Option<SourceReport>,
    rebound: bool,
}

/// The merge input for one finished visit.
pub fn seg_in(s: &SegRec, shared: &FollowShared, t0: Instant, now: Instant) -> SegIn {
    SegIn {
        tab: s.tab.clone(),
        first_ts_ms: shared.first_ts(s.gen),
        wall_ms: s.start.saturating_duration_since(t0).as_millis() as u64,
        dur_ms: s
            .end
            .unwrap_or(now)
            .saturating_duration_since(s.start)
            .as_millis() as u64,
        report: s.report.clone().unwrap_or_else(|| lost_report(&s.tab)),
        rebound: s.rebound,
    }
}

struct Run {
    cur: Option<Cur>,
    segs: Vec<SegRec>,
    pending: Vec<(usize, JoinHandle<SourceReport>)>,
    notes: Vec<String>,
    failures: Vec<String>,
    confirm: Confirm,
    baseline: Baseline,
    /// Opener facts, the settle clock and the popups never followed.
    op: OpenerState,
    feed: OpenerFeed,
    popup_settle: Duration,
    /// Per-tab mode: the tab the agent is on while it has no source (cap
    /// reached or its file could not be created); nothing is retried until
    /// the agent moves.
    parked: Option<String>,
    misses: u32,
    fail_streak: u32,
    failed: HashSet<String>,
    lost: bool,
    next_gen: u64,
    start: Instant,
    policy: Policy,
    probe: TabProbe,
    scfg: SourceConfig,
    poll: Duration,
    confirm_delay: Duration,
    no_frames_after: Duration,
    snap: Arc<Mutex<Snap>>,
}

pub(super) async fn run(p: Params, mut stop_rx: oneshot::Receiver<Option<String>>) -> Outcome {
    let mut r = Run::new(p);
    loop {
        tokio::select! {
            biased;
            hint = &mut stop_rx => return r.finish(hint.ok().flatten()).await,
            _ = sleep(r.poll) => r.tick().await,
        }
    }
}

fn lost_report(tab: &str) -> SourceReport {
    SourceReport {
        outcome: SourceOutcome::ConnectionLost { after_ms: 0 },
        stats: SourceStats::default(),
        target_id: tab.to_string(),
        elapsed_ms: 0,
        last_frame_age_ms: None,
        quiet_tail_ms: None,
        hidden: false,
    }
}

impl Run {
    fn new(p: Params) -> Run {
        let baseline = Baseline::new(p.head.as_deref(), &p.tab);
        let now = Instant::now();
        let op = OpenerState::new(&p.tab).with_known(&p.known);
        let feed = OpenerFeed::for_probe(&p.probe);
        Run {
            segs: vec![SegRec {
                tab: p.tab.clone(),
                gen: 1,
                start: now,
                end: None,
                report: None,
                rebound: false,
            }],
            cur: Some(Cur {
                src: p.first,
                tab: p.tab,
                seg: 0,
            }),
            pending: Vec::new(),
            notes: Vec::new(),
            failures: Vec::new(),
            confirm: Confirm::new(),
            baseline,
            op,
            feed,
            popup_settle: p.popup_settle,
            parked: None,
            misses: 0,
            fail_streak: 0,
            failed: HashSet::new(),
            lost: false,
            next_gen: 2,
            start: now,
            policy: p.policy,
            probe: p.probe,
            scfg: p.scfg,
            poll: p.poll,
            confirm_delay: p.confirm,
            no_frames_after: p.no_frames_after,
            snap: p.snap,
        }
    }

    /// Seconds on the video clock (it starts at the first frame).
    fn at_s(&self) -> f64 {
        self.policy
            .clock()
            .unwrap_or(self.start)
            .elapsed()
            .as_secs_f64()
    }

    /// Every tab the follower holds a socket to; the current one is last.
    fn bound_ids(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .pending
            .iter()
            .map(|(i, _)| self.segs[*i].tab.clone())
            .collect();
        if let Some(c) = &self.cur {
            v.push(c.tab.clone());
        }
        v
    }

    async fn reap(&mut self) {
        let mut i = 0;
        while i < self.pending.len() {
            if self.pending[i].1.is_finished() {
                let (idx, h) = self.pending.swap_remove(i);
                self.segs[idx].report = h.await.ok();
            } else {
                i += 1;
            }
        }
    }

    /// Stop the current source in the background (never await a stop on the
    /// tick path: it takes at least the stop grace).
    fn retire(&mut self, rebound: bool) {
        if let Some(c) = self.cur.take() {
            let seg = &mut self.segs[c.seg];
            seg.end = Some(Instant::now());
            seg.rebound = rebound;
            self.pending.push((c.seg, tokio::spawn(c.src.stop())));
        }
    }

    fn publish(&self) {
        let mut s = self.snap.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = &self.cur {
            s.tab = c.tab.clone();
            s.health = c.src.health();
        }
        s.files = self.policy.files();
        s.lost = self.lost;
    }

    async fn tick(&mut self) {
        self.reap().await;
        if self.lost || self.probe.is_fixed() {
            self.publish();
            return;
        }
        let hint = self.cur.as_ref().map(|c| c.tab.clone()).unwrap_or_default();
        match timeout(LIST_CAP, self.probe.list(&hint)).await {
            Ok(Ok(list)) => {
                self.misses = 0;
                let ids: Vec<String> = list.iter().map(|v| v.id.clone()).collect();
                let live = self.cur.as_ref().map(|c| &c.src);
                self.op
                    .refresh(&list, &self.feed, live.filter(|s| !s.is_finished()))
                    .await;
                self.decide(list).await;
                let keep = self.cur.as_ref().map(|c| c.tab.clone());
                let at = self.at_s();
                self.policy.close_missing(&ids, keep.as_deref(), at);
            }
            _ => {
                self.misses += 1;
                if self.misses >= LOST_AFTER {
                    // Keep the encoder and the sources; only stop polling.
                    self.lost = true;
                    let t = msg::browser_lost(self.at_s());
                    self.failures.push(t);
                }
            }
        }
        self.publish();
    }

    async fn decide(&mut self, list: Vec<TabView>) {
        let parked = self.cur.is_none().then(|| self.parked.clone()).flatten();
        let cur_tab = self.cur.as_ref().map(|c| c.tab.clone()).or(parked);
        let present = cur_tab
            .as_ref()
            .is_some_and(|t| list.iter().any(|v| &v.id == t));
        let finished = self.cur.as_ref().is_some_and(|c| c.src.is_finished());
        let bound = self.bound_ids();
        let cand = agent_tab(&list, &bound, RULE);
        let cand = match &cur_tab {
            Some(c) => self.baseline.filter(cand, c),
            None => cand,
        };
        let Some(cand) = cand else {
            if self.cur.is_some() {
                self.retire(false);
                let t = msg::no_tab(self.at_s());
                self.failures.push(t);
            }
            return;
        };
        let Some(cur) = cur_tab else {
            // Nothing bound (the tabs were all gone): bind the one that appeared.
            if let Some(v) = list.iter().find(|v| v.id == cand) {
                self.rebind(v, None).await;
            }
            return;
        };
        if !present || finished {
            let at = self.at_s();
            let prev: Vec<String> = self.segs.iter().map(|s| s.tab.clone()).collect();
            let (to, note) = if !present {
                self.op.gone(&cur, &list, &prev, at, &cand)
            } else {
                (cand.clone(), msg::tab_reconnected(&cand, at))
            };
            let Some(v) = list.iter().find(|v| v.id == to) else {
                return;
            };
            // Parked: the tab had no source to lose, so nothing to explain.
            let note = self.cur.is_some().then_some(note);
            self.rebind(v, note).await;
            return;
        }
        let step = self
            .op
            .step(&cand, &cur, self.start.elapsed(), self.popup_settle);
        if cand == cur {
            self.confirm.reset();
            return;
        }
        match step {
            Step::Stay => return,
            Step::Follow => {
                if let Some(v) = list.iter().find(|v| v.id == cand) {
                    self.rebind(v, None).await;
                }
                return;
            }
            Step::Confirm => {}
        }
        let target = if self.confirm.feed(&cand, &cur) {
            Some((cand, list))
        } else {
            // First sighting: look again shortly; the same candidate twice
            // is a switch, anything else is a transient.
            sleep(self.confirm_delay).await;
            match timeout(LIST_CAP, self.probe.list(&cur)).await {
                Ok(Ok(l2)) => {
                    let c2 = agent_tab(&l2, &bound, RULE);
                    match c2 {
                        Some(c2) if c2 != cur && self.confirm.feed(&c2, &cur) => Some((c2, l2)),
                        _ => None,
                    }
                }
                _ => None,
            }
        };
        if let Some((id, l)) = target {
            if let Some(v) = l.iter().find(|v| v.id == id) {
                self.rebind(v, None).await;
            }
        }
    }

    /// Start a new source on `v` and retire the old one. The generation is
    /// bumped BEFORE the start so the new tab's first frame (a static page
    /// sends exactly one) is not discarded as stale; a failed start restores
    /// the old generation and the old binding keeps recording.
    async fn rebind(&mut self, v: &TabView, note: Option<String>) -> bool {
        let gen = self.next_gen;
        self.next_gen += 1;
        let at = self.at_s();
        let entered = match self.policy.enter(v, gen, at) {
            Ok(e) => e,
            Err(skip) => {
                self.skipped(v, skip);
                return false;
            }
        };
        let started = timeout(
            REBIND_CAP,
            Source::start_ws(&v.ws_url, &v.id, self.scfg.clone(), entered.sink),
        )
        .await;
        match started {
            Ok(Ok(src)) => {
                self.retire(true);
                self.segs.push(SegRec {
                    tab: v.id.clone(),
                    gen,
                    start: Instant::now(),
                    end: None,
                    report: None,
                    rebound: false,
                });
                self.cur = Some(Cur {
                    src,
                    tab: v.id.clone(),
                    seg: self.segs.len() - 1,
                });
                self.op.note_followed(&v.id);
                self.parked = None;
                self.fail_streak = 0;
                self.confirm.reset();
                if let Some(n) = note {
                    self.notes.push(n);
                }
                true
            }
            other => {
                entered.shared.set_gen(entered.prev);
                let err = match other {
                    Ok(Err(e)) => e.to_string(),
                    _ => "timed out connecting".to_string(),
                };
                self.fail_streak += 1;
                if self.fail_streak >= FAIL_AFTER && self.failed.insert(v.id.clone()) {
                    let t = msg::rebind_failed(&v.id, self.at_s(), &err);
                    self.failures.push(t);
                }
                false
            }
        }
    }

    /// The agent is on a tab that gets no source (per-tab cap, file could not
    /// be created): the previous tab's source retires, the tab is named once
    /// and nothing is retried until the agent moves.
    fn skipped(&mut self, v: &TabView, skip: Skip) {
        match skip {
            Skip::Failed(t) => {
                if self.failed.insert(v.id.clone()) {
                    self.failures.push(t);
                }
            }
            Skip::Capped(t) => self.notes.push(t),
            Skip::Silent => {}
        }
        self.retire(true);
        self.parked = Some(v.id.clone());
        self.confirm.reset();
    }

    async fn finish(mut self, hint: Option<String>) -> Outcome {
        // Catch up a switch made less than one poll before the stop.
        if !self.lost && !self.probe.is_fixed() {
            if let Some(h) = hint {
                let cur = self.cur.as_ref().map(|c| c.tab.clone());
                if cur.as_deref() != Some(h.as_str()) {
                    if let Ok(Ok(list)) = timeout(LIST_CAP, self.probe.list(&h)).await {
                        if let Some(v) = list.iter().find(|v| v.id == h) {
                            if self.rebind(v, None).await {
                                self.wait_first_frame().await;
                            }
                        }
                    }
                }
            }
        }
        // The video ends at the stop request (once per file).
        self.policy.mark_end();
        self.retire(false);
        self.notes.extend(self.op.note());
        for (idx, h) in std::mem::take(&mut self.pending) {
            self.segs[idx].report = h.await.ok();
        }
        let no_frames_after = self.no_frames_after;
        match &mut self.policy {
            Policy::PerTab(files) => Outcome::PerTab(
                files
                    .finish(
                        &self.segs,
                        no_frames_after,
                        std::mem::take(&mut self.failures),
                        std::mem::take(&mut self.notes),
                    )
                    .await,
            ),
            Policy::One { shared, .. } => {
                let now = Instant::now();
                let t0 = shared.t0_at().unwrap_or(self.start);
                let segs = self
                    .segs
                    .iter()
                    .map(|s| seg_in(s, shared, t0, now))
                    .collect();
                Outcome::One(Box::new(merge(MergeIn {
                    segs,
                    t0_ts_ms: shared.t0_ts(),
                    failures: self.failures,
                    notes: self.notes,
                    no_frames_after_ms: no_frames_after.as_millis() as u64,
                })))
            }
        }
    }

    async fn wait_first_frame(&self) {
        let Some(c) = &self.cur else { return };
        let gen = self.segs[c.seg].gen;
        let end = Instant::now() + CATCHUP_WAIT;
        while self.policy.first_ts(&c.tab, gen).is_none() && Instant::now() < end {
            sleep(Duration::from_millis(10)).await;
        }
    }
}
