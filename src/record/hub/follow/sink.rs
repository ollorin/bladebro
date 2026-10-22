//! One sink per capture generation over the shared encoder.
//!
//! Why: a `Source` calls `mark_end` on its sink when it stops, which would end
//! the whole video at the first switch, and a retiring source still delivers
//! in-flight frames that would move the pacer's clock backwards. The wrapper
//! drops frames of a retired generation and out-of-order timestamps, records
//! each segment's first accepted frame (true video offsets) and ignores
//! `mark_end`; the follower ends the video itself exactly once.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::record::FrameSink;

#[derive(Default)]
struct State {
    last_ts: Option<u64>,
    t0_ts: Option<u64>,
    t0_at: Option<Instant>,
    first_ts: HashMap<u64, u64>,
    stale: u64,
}

/// State shared by every generation of one recording.
pub struct FollowShared {
    current_gen: AtomicU64,
    st: Mutex<State>,
}

impl FollowShared {
    /// Generation 1 is current.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            current_gen: AtomicU64::new(1),
            st: Mutex::default(),
        })
    }

    fn st(&self) -> std::sync::MutexGuard<'_, State> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn current_gen(&self) -> u64 {
        self.current_gen.load(Ordering::SeqCst)
    }

    pub fn set_gen(&self, g: u64) {
        self.current_gen.store(g, Ordering::SeqCst);
    }

    pub fn sink(self: &Arc<Self>, inner: Arc<dyn FrameSink>, gen: u64) -> Arc<SegmentSink> {
        Arc::new(SegmentSink {
            inner,
            gen,
            shared: self.clone(),
        })
    }

    /// Timestamp (ms) of the first frame accepted for `gen`.
    pub fn first_ts(&self, gen: u64) -> Option<u64> {
        self.st().first_ts.get(&gen).copied()
    }

    /// Timestamp of the first accepted frame of the whole video.
    pub fn t0_ts(&self) -> Option<u64> {
        self.st().t0_ts
    }

    pub fn t0_at(&self) -> Option<Instant> {
        self.st().t0_at
    }

    /// Frames discarded as stale (old generation or out of order).
    pub fn stale(&self) -> u64 {
        self.st().stale
    }
}

pub struct SegmentSink {
    inner: Arc<dyn FrameSink>,
    gen: u64,
    shared: Arc<FollowShared>,
}

impl FrameSink for SegmentSink {
    fn send_frame(&self, jpeg: Vec<u8>, ts_ms: u64) -> bool {
        let mut st = self.shared.st();
        // `true` on a discard: the source must not count a queue drop for a
        // frame of a tab the agent already left.
        if self.gen != self.shared.current_gen() || st.last_ts.is_some_and(|l| ts_ms < l) {
            st.stale += 1;
            return true;
        }
        let accepted = self.inner.send_frame(jpeg, ts_ms);
        if accepted {
            st.last_ts = Some(ts_ms);
            if st.t0_ts.is_none() {
                st.t0_ts = Some(ts_ms);
                st.t0_at = Some(Instant::now());
            }
            st.first_ts.entry(self.gen).or_insert(ts_ms);
        }
        accepted
    }

    // mark_end stays the trait's no-op on purpose (see the module doc).
}
