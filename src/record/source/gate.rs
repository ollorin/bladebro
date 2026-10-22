//! fps cap with newest-wins and a trailing flush (D6-6).
//!
//! A capped final frame would freeze stale content in the video, so a held
//! frame is forwarded when its slot comes due (and at stop). Newest wins
//! because the latest frame is the one that shows the current page state.

use crate::record::encoder::clamp_fps;

#[derive(Debug, PartialEq, Eq)]
pub enum GateStep<T> {
    Forward(T),
    /// Held for the trailing flush; `replaced` is the older held item that
    /// was dropped (counted as capped).
    Held {
        replaced: Option<T>,
    },
}

pub struct FpsGate<T> {
    interval_ms: u64,
    last_forward: Option<u64>,
    held: Option<T>,
    capped: u64,
}

impl<T> FpsGate<T> {
    pub fn new(fps: u32) -> Self {
        Self {
            interval_ms: 1000 / u64::from(clamp_fps(fps)),
            last_forward: None,
            held: None,
            capped: 0,
        }
    }

    pub fn interval_ms(&self) -> u64 {
        self.interval_ms
    }

    pub fn on_frame(&mut self, now_ms: u64, item: T) -> GateStep<T> {
        let due = match self.last_forward {
            None => true,
            Some(last) => now_ms.saturating_sub(last) >= self.interval_ms,
        };
        if due {
            self.last_forward = Some(now_ms);
            // A fresher frame supersedes whatever was held.
            if self.held.take().is_some() {
                self.capped += 1;
            }
            return GateStep::Forward(item);
        }
        let replaced = self.held.replace(item);
        if replaced.is_some() {
            self.capped += 1;
        }
        GateStep::Held { replaced }
    }

    /// When the held frame's slot comes due (arrival-time ms).
    pub fn due_at(&self) -> Option<u64> {
        self.held
            .as_ref()
            .and_then(|_| self.last_forward.map(|l| l + self.interval_ms))
    }

    pub fn on_due(&mut self, now_ms: u64) -> Option<T> {
        if now_ms < self.due_at()? {
            return None;
        }
        self.last_forward = Some(now_ms);
        self.held.take()
    }

    /// Stop flush: the final visible state always reaches the encoder.
    pub fn take_held(&mut self) -> Option<T> {
        self.held.take()
    }

    pub fn capped(&self) -> u64 {
        self.capped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_forwards() {
        let mut g = FpsGate::new(10);
        assert_eq!(g.on_frame(0, 0u64), GateStep::Forward(0));
    }

    #[test]
    fn hundred_frames_at_10ms() {
        let mut g = FpsGate::new(10);
        let mut fwd = vec![];
        for t in (0..1000u64).step_by(10) {
            if let GateStep::Forward(x) = g.on_frame(t, t) {
                fwd.push(x);
            }
        }
        assert_eq!(fwd, (0..=900).step_by(100).collect::<Vec<u64>>());
        assert_eq!(g.capped(), 89);
        assert_eq!(g.take_held(), Some(990));
    }

    #[test]
    fn trailing_frame_due() {
        let mut g = FpsGate::new(10);
        g.on_frame(0, 0u64);
        assert_eq!(g.on_frame(50, 50), GateStep::Held { replaced: None });
        assert_eq!(g.due_at(), Some(100));
        assert_eq!(g.on_due(99), None);
        assert_eq!(g.on_due(100), Some(50));
        assert_eq!(g.due_at(), None);
    }

    #[test]
    fn newest_replaces_held() {
        let mut g = FpsGate::new(10);
        g.on_frame(0, 0u64);
        g.on_frame(50, 50);
        assert_eq!(g.on_frame(60, 60), GateStep::Held { replaced: Some(50) });
        assert_eq!(g.capped(), 1);
    }

    #[test]
    fn slow_frames_all_forward() {
        let mut g = FpsGate::new(10);
        assert_eq!(g.on_frame(0, 0u64), GateStep::Forward(0));
        assert_eq!(g.on_frame(250, 250), GateStep::Forward(250));
        assert_eq!(g.capped(), 0);
    }

    #[test]
    fn intervals_follow_clamped_fps() {
        assert_eq!(FpsGate::<u8>::new(15).interval_ms(), 66);
        assert_eq!(FpsGate::<u8>::new(0).interval_ms(), 200);
        assert_eq!(FpsGate::<u8>::new(100).interval_ms(), 66);
    }
}
