//! Pure merge of the per-segment source reports into what `compose` needs.
//!
//! Why not pass one raw report: `Source::stop` turns a zero-frame capture into
//! `NoFrames`, so a tab the agent merely passed through would fail the whole
//! recording. Here only the video as a whole (no frame at all) is NoFrames; a
//! single silent segment is a failure only when it lasted long enough to be a
//! real stall, otherwise it is labelled `(no frames)`.

use super::super::msg;
use super::super::report::{compose, fields_line, Segment};
use crate::record::encoder::EncoderReport;
use crate::record::source::{SourceOutcome, SourceReport, SourceStats};
use std::time::Instant;

/// One finished capture generation.
#[derive(Debug, Clone)]
pub struct SegIn {
    pub tab: String,
    /// Timestamp (ms) of the first frame the video accepted from this tab.
    pub first_ts_ms: Option<u64>,
    /// Wall-clock offset of the segment start, relative to the first video frame.
    pub wall_ms: u64,
    /// How long the agent stayed on the tab.
    pub dur_ms: u64,
    pub report: SourceReport,
    /// Another segment followed: a lost connection was handled, not a failure.
    pub rebound: bool,
}

#[derive(Debug)]
pub struct MergeIn {
    pub segs: Vec<SegIn>,
    /// Timestamp of the first frame of the whole video.
    pub t0_ts_ms: Option<u64>,
    /// Failure texts the follower already collected (no tab left, browser lost,
    /// failed rebind). Each makes the recording not clean.
    pub failures: Vec<String>,
    pub notes: Vec<String>,
    pub no_frames_after_ms: u64,
}

#[derive(Debug)]
pub struct Merged {
    pub report: SourceReport,
    pub segments: Vec<Segment>,
    pub notes: Vec<String>,
    pub failures: Vec<String>,
    /// Video clock: arrival of the first frame the video accepted. Set by the
    /// caller (run.rs / files.rs) from the sink; merge() only sees timestamps.
    pub t0_at: Option<Instant>,
}

fn add(a: &mut SourceStats, b: &SourceStats) {
    a.frames_received += b.frames_received;
    a.acks_sent += b.acks_sent;
    a.acks_ok += b.acks_ok;
    a.ack_errors += b.ack_errors;
    a.forwarded += b.forwarded;
    a.queue_dropped += b.queue_dropped;
    a.capped += b.capped;
    a.lost += b.lost;
    a.lag_acks += b.lag_acks;
    a.bad_frames += b.bad_frames;
    a.ts_fallback += b.ts_fallback;
    a.longest_gap_ms = a.longest_gap_ms.max(b.longest_gap_ms);
    if a.methods_sent.is_empty() {
        a.methods_sent = b.methods_sent.clone();
    }
}

fn secs(ms: u64) -> f64 {
    ms as f64 / 1000.0
}

pub fn merge(i: MergeIn) -> Merged {
    let mut stats = SourceStats::default();
    let mut failures = i.failures;
    let mut segments = Vec::with_capacity(i.segs.len());
    let n = i.segs.len();
    for (k, s) in i.segs.iter().enumerate() {
        add(&mut stats, &s.report.stats);
        let at_ms = match (s.first_ts_ms, i.t0_ts_ms) {
            (Some(f), Some(t0)) => f.saturating_sub(t0),
            _ => s.wall_ms,
        };
        let silent = s.report.stats.frames_received == 0;
        let mut label = s.tab.clone();
        if silent {
            if s.dur_ms >= i.no_frames_after_ms {
                failures.push(msg::zero_frames(&s.tab, secs(s.dur_ms)));
            } else {
                label = format!("{} (no frames)", s.tab);
            }
        } else if let SourceOutcome::ConnectionLost { .. } = s.report.outcome {
            if !s.rebound && k + 1 == n {
                failures.push(s.report.verdict());
            }
        }
        segments.push(Segment {
            at_s: secs(at_ms),
            tab: label,
        });
    }
    let last = i.segs.last();
    let total = stats.frames_received;
    let outcome = if total == 0 {
        SourceOutcome::NoFrames {
            waited_ms: last.map_or(0, |s| s.wall_ms + s.dur_ms),
            hidden: last.is_some_and(|s| s.report.hidden),
        }
    } else {
        SourceOutcome::Captured
    };
    let first = i.segs.first();
    let report = SourceReport {
        outcome,
        stats,
        target_id: first.map(|s| s.tab.clone()).unwrap_or_default(),
        elapsed_ms: i.segs.iter().map(|s| s.report.elapsed_ms).sum(),
        last_frame_age_ms: last.and_then(|s| s.report.last_frame_age_ms),
        // Silence after a frame is judged on the last segment only.
        quiet_tail_ms: last.and_then(|s| s.report.quiet_tail_ms),
        hidden: last.is_some_and(|s| s.report.hidden),
    };
    Merged {
        report,
        segments,
        notes: i.notes,
        failures,
        t0_at: None,
    }
}

/// `compose` plus the follower's failure texts. With no failures this is
/// exactly `compose` (notes joined). A failure makes the result not clean and
/// is named first; a not-clean encoder keeps its own verdict (it names the
/// file state) with the failures appended as a note.
pub fn compose_follow(m: &Merged, enc: &EncoderReport, fps: u32) -> (bool, String) {
    let joined = (!m.notes.is_empty()).then(|| m.notes.join("; "));
    if m.failures.is_empty() {
        return compose(&m.report, enc, fps, &m.segments, joined.as_deref());
    }
    if !enc.ok() || !m.report.ok() {
        let mut notes = m.failures.clone();
        notes.extend(m.notes.iter().cloned());
        let (_, t) = compose(&m.report, enc, fps, &m.segments, Some(&notes.join("; ")));
        return (false, t);
    }
    let fields = fields_line(&m.report, enc, fps, &m.segments);
    let mut t = format!("recording not clean: {}; {fields}", m.failures.join("; "));
    if let Some(n) = joined {
        t.push_str(&format!(". Note: {n}"));
    }
    (false, msg::scrub(&t))
}
