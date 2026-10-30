//! Timestamp pacing: turn frame timestamps into per-frame write counts (D5-1).
//!
//! Strategy: duplicate-on-write at a constant rate, NOT the sidecar's concat
//! spool. Frames arrive on stdin; concat would need every frame as a file on
//! disk (a spool dir, cleanup, sensitive frames left behind after a crash) and
//! would encode the whole recording at stop, so finalize would grow with
//! length and break the 10 s bound. (This deliberately departs from the
//! Phase 2 "concat" recommendation.)
//!
//! Mechanism: ffmpeg input `-f image2pipe -framerate <fps>` stamps the i-th
//! JPEG at i/fps, `-fps_mode passthrough` keeps every frame at that stamp and
//! there is no output `-r`, so video duration = frames_written / fps exactly.
//! The `Pacer` decides how many times each JPEG is written:
//! slots(t) = round(elapsed_ms * fps / 1000), cumulative from the first
//! frame, writes = slots(now) - written. Rounding therefore never
//! accumulates (total error < 1 slot); frames faster than fps get 0 writes
//! (that IS the fps cap); the final frame is held to the finish time and is
//! written at least once. Measured in planning on ffmpeg n9.0.2: 31 copies of
//! a 1001x641 JPEG at `-framerate 10` gave ffprobe duration 3.100 s for both
//! webm/vp9 and mp4/h264. Spans under about 1 s can exceed a 10 % duration
//! bar by one slot (the forced final write).
//!
//! The 05-02 writer calls `advance_to` once per second while idle so finish
//! never faces a large backlog (measured: ~1600-1900 held 1280x720 frames/s;
//! a 10-minute idle written only at finish would be 6000 frames, 3-4 s on a
//! fast host and possibly over the 10 s finish bound on a slow CI host).

use super::args::clamp_fps;

/// A single gap longer than this (10 min) is a clock anomaly. Real idle pages
/// are covered by the progressive hold flush, so the cap never shortens one.
pub const MAX_GAP_MS: u64 = 600_000;

/// Round-half-up slot count for an elapsed time at `fps`.
fn slots(elapsed_ms: u64, fps: u32) -> u64 {
    (elapsed_ms.saturating_mul(u64::from(fps)) + 500) / 1000
}

/// Stateful pacer: how many times to write each pending frame.
#[derive(Debug, Clone)]
pub struct Pacer {
    fps: u32,
    started: bool,
    last_ts: u64,
    elapsed_ms: u64,
    written: u64,
    cur_written: u64,
    clamped: u64,
}

impl Pacer {
    /// New pacer; `fps` is clamped into the supported range.
    pub fn new(fps: u32) -> Self {
        Self {
            fps: clamp_fps(fps),
            started: false,
            last_ts: 0,
            elapsed_ms: 0,
            written: 0,
            cur_written: 0,
            clamped: 0,
        }
    }

    /// Commit the gap up to `ts_ms` (backwards counts as 0, long gaps clamp).
    fn commit_gap(&mut self, ts_ms: u64) {
        let mut gap = ts_ms.saturating_sub(self.last_ts);
        if gap > MAX_GAP_MS {
            gap = MAX_GAP_MS;
            self.clamped += 1;
        }
        // Follow a reset clock rather than freezing on the old high mark.
        self.last_ts = ts_ms;
        self.elapsed_ms += gap;
    }

    /// A new frame arrived at `ts_ms`; returns the writes owed to the
    /// PREVIOUS pending frame (0 for the first frame).
    pub fn on_frame(&mut self, ts_ms: u64) -> u64 {
        if !self.started {
            self.started = true;
            self.last_ts = ts_ms;
            self.cur_written = 0;
            return 0;
        }
        self.commit_gap(ts_ms);
        let n = slots(self.elapsed_ms, self.fps).saturating_sub(self.written);
        self.written += n;
        self.cur_written = 0;
        n
    }

    /// Hold flush for the CURRENT pending frame up to `ts_ms`, without
    /// committing elapsed/last_ts. 0 before the first frame.
    pub fn advance_to(&mut self, ts_ms: u64) -> u64 {
        if !self.started {
            return 0;
        }
        let gap = ts_ms.saturating_sub(self.last_ts).min(MAX_GAP_MS);
        let n = slots(self.elapsed_ms + gap, self.fps).saturating_sub(self.written);
        self.written += n;
        self.cur_written += n;
        n
    }

    /// Recording ends at `end_ms`; returns the writes owed to the pending
    /// frame. The final visible state is always written at least once.
    pub fn on_end(&mut self, end_ms: u64) -> u64 {
        if !self.started {
            return 0;
        }
        self.commit_gap(end_ms);
        let mut n = slots(self.elapsed_ms, self.fps).saturating_sub(self.written);
        if n == 0 && self.cur_written == 0 {
            n = 1;
        }
        self.written += n;
        self.cur_written += n;
        n
    }

    /// Total writes decided so far.
    pub fn written(&self) -> u64 {
        self.written
    }

    /// How many gaps were clamped to `MAX_GAP_MS`.
    pub fn clamped(&self) -> u64 {
        self.clamped
    }
}

/// Writes per frame for a whole timestamp list (frame i is held until frame
/// i+1; the last until `end_ms`). Pure helper over `Pacer`.
pub fn repeat_counts(ts_ms: &[u64], end_ms: u64, fps: u32) -> Vec<u64> {
    let mut p = Pacer::new(fps);
    let mut out = Vec::with_capacity(ts_ms.len());
    for (i, &t) in ts_ms.iter().enumerate() {
        let n = p.on_frame(t);
        if i > 0 {
            out.push(n);
        }
    }
    if !ts_ms.is_empty() {
        out.push(p.on_end(end_ms));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(from: u64, to: u64, by: u64) -> Vec<u64> {
        (from..=to).step_by(by as usize).collect()
    }

    #[test]
    fn repeat_counts_table() {
        let t31 = step(0, 3000, 100);
        let rows: Vec<(u32, Vec<u64>, u64, Vec<u64>)> = vec![
            (10, vec![], 0, vec![]),
            (10, vec![0], 0, vec![1]),
            (10, vec![0], 1000, vec![10]),
            (10, vec![5000], 5000, vec![1]),
            (10, vec![0, 1000], 1000, vec![10, 1]),
            (10, t31, 3000, vec![1; 31]),
            (10, vec![0, 500, 200, 700], 700, vec![5, 0, 5, 1]),
            (10, vec![0, 3_600_000], 3_600_000, vec![6000, 1]),
            (15, vec![0, 1000], 1000, vec![15, 1]),
            (15, vec![0, 33, 66, 100], 100, vec![0, 1, 1, 1]),
            (5, vec![0, 1000, 1100], 1100, vec![5, 1, 1]),
            (0, vec![0, 1000], 1000, vec![5, 1]),
            (100, vec![0, 1000], 1000, vec![15, 1]),
        ];
        for (fps, ts, end, want) in rows {
            assert_eq!(repeat_counts(&ts, end, fps), want, "fps {fps} ts {ts:?}");
        }
    }

    #[test]
    fn fast_frames_are_capped() {
        let ts = step(0, 990, 10);
        let c = repeat_counts(&ts, 1000, 10);
        assert_eq!(c.len(), 100);
        assert_eq!(c.iter().sum::<u64>(), 11);
        assert!(c.iter().filter(|&&n| n == 0).count() >= 89);
        assert_eq!(*c.last().unwrap_or(&0), 1);
    }

    #[test]
    fn long_gap_is_clamped_and_counted() {
        let mut p = Pacer::new(10);
        assert_eq!(p.on_frame(0), 0);
        assert_eq!(p.on_frame(3_600_000), 6000);
        assert_eq!(p.clamped(), 1);
        assert_eq!(p.on_end(3_600_000), 1);
    }

    #[test]
    fn duration_tracks_span() {
        for fps in [5u64, 10, 15] {
            for by in [7u64, 33, 100, 250, 1000] {
                for span in [1000u64, 3000, 10_000] {
                    let ts = step(0, span, by);
                    let sum: u64 = repeat_counts(&ts, span, fps as u32).iter().sum();
                    let base = span * fps / 1000;
                    assert!(
                        sum >= base && sum <= base + 2,
                        "fps {fps} step {by} span {span}: {sum} vs {base}"
                    );
                }
            }
        }
    }

    #[test]
    fn advance_and_end_interplay() {
        let mut p = Pacer::new(10);
        assert_eq!(p.on_frame(0), 0);
        assert_eq!(p.advance_to(1000), 10);
        assert_eq!(p.advance_to(1000), 0);
        assert_eq!(p.advance_to(1500), 5);
        assert_eq!(p.on_frame(1600), 1);
        assert_eq!(p.on_end(1600), 1);
        assert_eq!(p.written(), 17);

        let mut p = Pacer::new(10);
        p.on_frame(0);
        assert_eq!(p.advance_to(2000), 20);
        assert_eq!(p.on_end(2000), 0);
        assert_eq!(p.written(), 20);

        let mut p = Pacer::new(10);
        p.on_frame(0);
        assert_eq!(p.advance_to(1000), 10);
        assert_eq!(p.on_frame(900), 0);
    }

    #[test]
    fn advance_before_first_frame_is_zero() {
        let mut p = Pacer::new(10);
        assert_eq!(p.advance_to(5000), 0);
        assert_eq!(p.on_end(5000), 0);
        assert_eq!(p.written(), 0);
    }
}
