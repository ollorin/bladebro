//! No-frames watchdog state (CAPT-05, D6-10). Pure: times are ms since the
//! source started, so every rule is table-testable without a clock.
//!
//! Why silence after a first frame is NOT a failure (rule 3): an idle static
//! page legitimately yields about one frame and then nothing, and from the
//! outside that is indistinguishable from a stalled screencast. Telling them
//! apart would need touching the page (injected JS, an overlay, an extra CDP
//! method), which SAFE-01 forbids. So after >= 1 frame silence is only
//! reported (last-frame age, longest gap), never turned into a failure. Zero
//! frames is different: a visible page sends its first frame within tens of ms
//! (Phase 2), so none at all is a blank video and always a failure.

use std::time::Duration;

/// A capture with no first frame after this long is `NoFrames` while running.
pub const NO_FRAMES_AFTER: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    /// No frame yet, still inside the grace period.
    Waiting {
        for_ms: u64,
    },
    /// No frame and the period passed: the video would be blank.
    NoFrames {
        for_ms: u64,
        hidden: bool,
    },
    Receiving {
        last_age_ms: u64,
    },
    /// Frames seen, but the last one is older than the period.
    Quiet {
        last_age_ms: u64,
        hidden: bool,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Liveness {
    frames: u64,
    last_ms: Option<u64>,
    longest_gap_ms: u64,
    hidden: bool,
}

impl Liveness {
    pub fn new() -> Self {
        Self::default()
    }

    /// The first gap is measured from the start of the source.
    pub fn on_frame(&mut self, now_ms: u64) {
        let prev = self.last_ms.unwrap_or(0);
        self.longest_gap_ms = self.longest_gap_ms.max(now_ms.saturating_sub(prev));
        self.last_ms = Some(now_ms);
        self.frames += 1;
    }

    /// `Page.screencastVisibilityChanged`: the latest report wins.
    pub fn on_visibility(&mut self, visible: bool, _now_ms: u64) {
        self.hidden = !visible;
    }

    pub fn health(&self, now_ms: u64, period_ms: u64) -> Health {
        match self.last_age(now_ms) {
            None if now_ms >= period_ms => Health::NoFrames {
                for_ms: now_ms,
                hidden: self.hidden,
            },
            None => Health::Waiting { for_ms: now_ms },
            Some(age) if age >= period_ms => Health::Quiet {
                last_age_ms: age,
                hidden: self.hidden,
            },
            Some(age) => Health::Receiving { last_age_ms: age },
        }
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn last_age(&self, now_ms: u64) -> Option<u64> {
        self.last_ms.map(|l| now_ms.saturating_sub(l))
    }

    pub fn hidden(&self) -> bool {
        self.hidden
    }

    pub fn longest_gap_ms(&self) -> u64 {
        self.longest_gap_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: u64 = 5000;

    #[test]
    fn health_table() {
        let none = Liveness::new();
        assert_eq!(none.health(4999, P), Health::Waiting { for_ms: 4999 });
        assert_eq!(
            none.health(5000, P),
            Health::NoFrames {
                for_ms: 5000,
                hidden: false
            }
        );

        let mut hid = Liveness::new();
        hid.on_visibility(false, 100);
        assert_eq!(
            hid.health(6000, P),
            Health::NoFrames {
                for_ms: 6000,
                hidden: true
            }
        );

        let mut f = Liveness::new();
        f.on_frame(20);
        assert_eq!(f.health(1000, P), Health::Receiving { last_age_ms: 980 });
        assert_eq!(
            f.health(5020, P),
            Health::Quiet {
                last_age_ms: 5000,
                hidden: false
            }
        );

        f.on_frame(40_000);
        assert_eq!(f.health(40_001, P), Health::Receiving { last_age_ms: 1 });
        assert_eq!(f.longest_gap_ms(), 39_980);
        assert_eq!(f.frames(), 2);

        let mut v = Liveness::new();
        v.on_frame(20);
        v.on_visibility(false, 30);
        v.on_visibility(true, 50);
        assert!(matches!(v.health(100, P), Health::Receiving { .. }));
        assert!(!v.hidden());
    }

    #[test]
    fn first_gap_is_measured_from_start() {
        let mut l = Liveness::new();
        l.on_frame(700);
        assert_eq!(l.longest_gap_ms(), 700);
        assert_eq!(l.last_age(900), Some(200));
        assert_eq!(Liveness::new().last_age(900), None);
    }
}
