//! Per-event logic (sync, unit-testable): ack decision, fps gate, decode,
//! sink hand-off and counters. The async loop in `capture` only moves data
//! in and out of this.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast::error::RecvError;

use super::frame::{decode_jpeg, parse_frame_owned};
use super::gate::{FpsGate, GateStep};
use super::report::SourceStats;
use super::sink::FrameSink;
use super::watch::Liveness;
use crate::cdp::CdpEvent;

/// A frame waiting for the gate or the sink: base64 stays encoded until it is
/// actually forwarded (decode cost <= fps per second).
pub(super) type Held = (String, u64);

/// What the loop must do for one received event.
#[derive(Default)]
pub(super) struct Step {
    /// Ack to issue FIRST, before anything else (D6-4).
    pub ack: Option<i64>,
    pub forward: Option<Held>,
    /// The bus is closed: end the loop.
    pub end: bool,
}

pub(super) type SharedStats = Arc<Mutex<SourceStats>>;
pub(super) type SharedLive = Arc<Mutex<Liveness>>;

pub(super) fn lock(s: &SharedStats) -> MutexGuard<'_, SourceStats> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) struct Handler {
    gate: FpsGate<Held>,
    stats: SharedStats,
    live: SharedLive,
    last_ack_id: Option<i64>,
}

impl Handler {
    pub fn new(fps: u32, stats: SharedStats, live: SharedLive) -> Self {
        Self {
            gate: FpsGate::new(fps),
            stats,
            live,
            last_ack_id: None,
        }
    }

    pub fn on_recv(&mut self, r: Result<CdpEvent, RecvError>, now_ms: u64) -> Step {
        let ev = match r {
            Ok(ev) => ev,
            Err(RecvError::Lagged(n)) => {
                // Counted, not hidden: these frames were never acked either.
                lock(&self.stats).lost += n;
                return Step::default();
            }
            Err(RecvError::Closed) => {
                return Step {
                    end: true,
                    ..Step::default()
                }
            }
        };
        if ev.method == "Page.screencastVisibilityChanged" {
            // Chrome volunteers this on the screencast session; nothing is
            // requested to get it (SAFE-01) and nothing is sent in reply.
            if let Some(v) = ev.params.get("visible").and_then(|v| v.as_bool()) {
                self.live
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .on_visibility(v, now_ms);
            }
            return Step::default();
        }
        if ev.method != "Page.screencastFrame" {
            return Step::default();
        }
        self.live
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_frame(now_ms);
        // Ack decision before anything else touches the frame: Chrome stops
        // sending after 2 unacked, so a missing ack stalls the whole screencast.
        let frame = parse_frame_owned(ev.params);
        let mut st = lock(&self.stats);
        st.frames_received += 1;
        let ack = match frame.ack_id {
            Some(id) => {
                self.last_ack_id = Some(id);
                Some(id)
            }
            None => match self.last_ack_id {
                // Above the hook's byte cap params are stripped; the screencast
                // session id is stable, so the last one still acks it.
                Some(id) => {
                    st.acked_with_cached_id += 1;
                    Some(id)
                }
                None => {
                    st.unacked += 1;
                    None
                }
            },
        };
        if ack.is_some() {
            st.acks_sent += 1;
        }
        if st.first_frame_ms.is_none() {
            st.first_frame_ms = Some(now_ms);
        }
        st.last_frame_ms = Some(now_ms);
        let Some(data) = frame.data_b64 else {
            st.bad_frames += 1;
            return Step {
                ack,
                ..Step::default()
            };
        };
        let ts = frame.ts_ms.unwrap_or_else(|| {
            st.ts_fallback += 1;
            epoch_ms()
        });
        drop(st);
        let forward = match self.gate.on_frame(now_ms, (data, ts)) {
            GateStep::Forward(h) => Some(h),
            GateStep::Held { .. } => None,
        };
        lock(&self.stats).capped = self.gate.capped();
        Step {
            ack,
            forward,
            end: false,
        }
    }

    /// Arrival-time ms at which the held frame is due, if one is held.
    pub fn due_at(&self) -> Option<u64> {
        self.gate.due_at()
    }

    pub fn on_timer(&mut self, now_ms: u64) -> Option<Held> {
        self.gate.on_due(now_ms)
    }

    /// Stop flush: the final visible state always reaches the encoder.
    pub fn take_held(&mut self) -> Option<Held> {
        self.gate.take_held()
    }

    pub fn forward(&mut self, (b64, ts): Held, sink: &dyn FrameSink) {
        let decoded = decode_jpeg(&b64);
        let mut st = lock(&self.stats);
        match decoded {
            Err(_) => st.bad_frames += 1,
            Ok(jpeg) => {
                if sink.send_frame(jpeg, ts) {
                    st.forwarded += 1;
                } else {
                    st.queue_dropped += 1;
                }
            }
        }
    }
}

/// Same clock domain as CDP's `metadata.timestamp` (epoch), so the Pacer's
/// gap math stays consistent when a frame lacks metadata (D6-7).
fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    struct TestSink {
        accept: AtomicBool,
        got: AtomicU64,
    }
    impl FrameSink for TestSink {
        fn send_frame(&self, _: Vec<u8>, _: u64) -> bool {
            self.got.fetch_add(1, Ordering::SeqCst);
            self.accept.load(Ordering::SeqCst)
        }
    }

    fn handler(fps: u32) -> (Handler, SharedStats) {
        let s: SharedStats = Arc::default();
        (Handler::new(fps, s.clone(), Arc::default()), s)
    }

    fn ev(method: &str, params: Value) -> Result<CdpEvent, RecvError> {
        Ok(CdpEvent {
            method: method.into(),
            params,
            session_id: None,
        })
    }

    fn frame(id: Option<i64>, ts: f64) -> Result<CdpEvent, RecvError> {
        let mut p = json!({"data": "/9j/", "metadata": {"timestamp": ts}});
        if let Some(id) = id {
            p["sessionId"] = json!(id);
        }
        ev("Page.screencastFrame", p)
    }

    #[test]
    fn lagged_adds_to_lost_without_ack_or_forward() {
        let (mut h, s) = handler(10);
        let step = h.on_recv(Err(RecvError::Lagged(7)), 0);
        assert!(step.ack.is_none() && step.forward.is_none() && !step.end);
        assert_eq!(lock(&s).lost, 7);
        h.on_recv(Err(RecvError::Lagged(3)), 1);
        assert_eq!(lock(&s).lost, 10);
    }

    #[test]
    fn closed_ends_the_loop() {
        let (mut h, _) = handler(10);
        assert!(h.on_recv(Err(RecvError::Closed), 0).end);
    }

    #[test]
    fn other_events_are_ignored() {
        let (mut h, s) = handler(10);
        let step = h.on_recv(ev("Page.frameNavigated", json!({})), 0);
        assert!(step.ack.is_none() && step.forward.is_none());
        assert_eq!(lock(&s).frames_received, 0);
    }

    #[test]
    fn frame_is_acked_with_its_own_session_id() {
        let (mut h, s) = handler(10);
        let step = h.on_recv(frame(Some(7), 1.7e9), 5);
        assert_eq!(step.ack, Some(7));
        assert_eq!(step.forward, Some(("/9j/".to_string(), 1_700_000_000_000)));
        let st = lock(&s);
        assert_eq!((st.frames_received, st.acks_sent), (1, 1));
        assert_eq!(st.first_frame_ms, Some(5));
    }

    #[test]
    fn cached_ack_id_covers_stripped_frames() {
        let (mut h, s) = handler(10);
        h.on_recv(frame(Some(9), 1.7e9), 0);
        let step = h.on_recv(ev("Page.screencastFrame", json!({})), 1);
        assert_eq!(step.ack, Some(9));
        let st = lock(&s);
        assert_eq!(
            (st.acked_with_cached_id, st.unacked, st.bad_frames),
            (1, 0, 1)
        );
        assert_eq!(st.frames_received, st.acks_sent + st.unacked);
    }

    #[test]
    fn no_id_known_counts_unacked() {
        let (mut h, s) = handler(10);
        let step = h.on_recv(ev("Page.screencastFrame", json!({})), 0);
        assert!(step.ack.is_none());
        assert_eq!(lock(&s).unacked, 1);
    }

    #[test]
    fn missing_timestamp_falls_back_to_epoch_now() {
        let (mut h, s) = handler(10);
        let step = h.on_recv(frame(Some(1), 0.0), 0);
        let (_, ts) = step.forward.expect("forwarded");
        assert!(ts > 1_600_000_000_000);
        assert_eq!(lock(&s).ts_fallback, 1);
    }

    #[test]
    fn gate_holds_then_arrival_after_due_flushes_without_timer() {
        // Pins the arrival path of the trailing flush: even if the timer arm
        // never wins the select, a frame arriving past the due time forwards.
        let (mut h, s) = handler(10);
        assert!(h.on_recv(frame(Some(1), 1.7e9), 0).forward.is_some());
        assert!(h.on_recv(frame(Some(1), 1.7e9), 50).forward.is_none());
        assert_eq!(h.due_at(), Some(100));
        assert!(h.on_recv(frame(Some(1), 1.7e9), 120).forward.is_some());
        assert_eq!(lock(&s).capped, 1);
    }

    #[test]
    fn forward_counts_sink_outcomes_and_bad_base64() {
        let (mut h, s) = handler(10);
        let sink = TestSink {
            accept: AtomicBool::new(true),
            got: AtomicU64::new(0),
        };
        h.forward(("/9j/".into(), 1), &sink);
        sink.accept.store(false, Ordering::SeqCst);
        h.forward(("/9j/".into(), 2), &sink);
        h.forward(("!!!".into(), 3), &sink);
        let st = lock(&s);
        assert_eq!((st.forwarded, st.queue_dropped, st.bad_frames), (1, 1, 1));
        assert_eq!(sink.got.load(Ordering::SeqCst), 2);
    }
}
