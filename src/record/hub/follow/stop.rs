//! The follower's stop path, split from run.rs (500-line rule): catch-up of a
//! late switch, then end the video and merge. A child of `run` (declared there
//! with `#[path]`), so it sees `Run`'s private fields and helpers.

use std::time::{Duration, Instant};

use tokio::time::{sleep, timeout};

use super::super::files::Policy;
use super::super::merge::{merge, MergeIn};
use super::super::Outcome;
use super::{seg_in, Run, CATCHUP_WAIT, LIST_CAP};
use crate::record::source::Health;

/// Poll step of the first-frame waits (as the catch-up wait always used).
const WAIT_STEP: Duration = Duration::from_millis(10);
/// The wait stops this far before the no-frames threshold (see wait_first_frame).
const THRESHOLD_MARGIN: Duration = Duration::from_millis(100);

impl Run {
    pub(super) async fn finish(
        mut self,
        hint: Option<String>,
        first_frame_wait: Duration,
    ) -> Outcome {
        let until = Instant::now() + first_frame_wait;
        // Catch up a switch made less than one poll before the stop.
        if !self.lost && !self.probe.is_fixed() {
            if let Some(h) = hint {
                let cur = self.cur.as_ref().map(|c| c.tab.clone());
                if cur.as_deref() != Some(h.as_str()) {
                    if let Ok(Ok(list)) = timeout(LIST_CAP, self.probe.list(&h)).await {
                        if let Some(v) = list.first().filter(|v| v.id == h) {
                            if self.rebind(v, None).await {
                                self.wait_first_frame(Instant::now() + CATCHUP_WAIT).await;
                            }
                        }
                    }
                }
            }
        }
        // Phase 11.1: an agent record-stop gives the current capture's first
        // frame a bounded chance before the video ends (zero on exit paths).
        if !first_frame_wait.is_zero() {
            self.wait_first_frame(until).await;
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
                let mut m = merge(MergeIn {
                    segs,
                    t0_ts_ms: shared.t0_ts(),
                    failures: self.failures,
                    notes: self.notes,
                    no_frames_after_ms: no_frames_after.as_millis() as u64,
                });
                m.t0_at = shared.t0_at();
                Outcome::One(Box::new(m))
            }
        }
    }

    /// Wait until the current capture's first frame reached its encoder, or
    /// `until`. Ends early when no frame can come: no current capture, the
    /// source loop ended (tab closed, connection lost), the source already
    /// gave its no-frames verdict, or Chrome is lost. Never stretches a silent
    /// segment to the no-frames threshold, so a short silent tab keeps its
    /// clean `(no frames)` label instead of becoming a failure.
    async fn wait_first_frame(&self, until: Instant) {
        let Some(c) = &self.cur else { return };
        let seg = &self.segs[c.seg];
        let gen = seg.gen;
        let threshold = (seg.start + self.no_frames_after)
            .checked_sub(THRESHOLD_MARGIN)
            .unwrap_or(seg.start);
        let end = until.min(threshold);
        while Instant::now() < end
            && !self.lost
            && self.policy.first_ts(&c.tab, gen).is_none()
            && !c.src.is_finished()
            && !matches!(c.src.health(), Health::NoFrames { .. })
        {
            sleep(WAIT_STEP).await;
        }
    }
}
