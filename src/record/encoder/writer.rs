//! The stdin writer task (D5-11): owns `ChildStdin` and the `Pacer`, turns
//! timestamped frames into a constant-rate JPEG stream.
//!
//! Dropping `ChildStdin` is the only graceful stop: it delivers EOF, and
//! ffmpeg then finalizes the container. Every exit path of `run` drops it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};

use super::pace::Pacer;

/// While idle, the pending frame's hold is written this often, so `finish`
/// never faces more than about a second of backlog.
pub const HOLD_FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// One queued frame with its capture timestamp and arrival time.
pub(super) struct Frame {
    pub jpeg: Vec<u8>,
    pub ts_ms: u64,
    pub arrived: Instant,
}

/// State shared with `finish`. Atomics and `OnceLock` (no mutex) so the
/// counters and the first write error survive an aborted writer task.
#[derive(Default)]
pub(super) struct Shared {
    pub frames_written: AtomicU64,
    pub gaps_clamped: AtomicU64,
    /// Source frames whose every repeat was written. `queued - frames_done`
    /// is what an aborted writer never delivered.
    pub frames_done: AtomicU64,
    pub write_error: OnceLock<String>,
    /// When `finish` was called; the pending frame is held until then.
    pub end_at: OnceLock<Instant>,
}

/// Write `jpeg` `n` times. The first failure is recorded (first one only)
/// and returned; Rust ignores SIGPIPE, so a dead reader is an EPIPE value.
async fn write_n(stdin: &mut ChildStdin, jpeg: &[u8], n: u64, shared: &Shared) -> Result<(), ()> {
    for _ in 0..n {
        if let Err(e) = stdin.write_all(jpeg).await {
            let _ = shared.write_error.set(e.to_string());
            return Err(());
        }
        shared.frames_written.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Run until the channel closes or a write fails. `stdin` is dropped on
/// every return path (EOF).
pub(super) async fn run(
    mut rx: mpsc::Receiver<Frame>,
    mut stdin: ChildStdin,
    fps: u32,
    shared: Arc<Shared>,
) {
    let mut pacer = Pacer::new(fps);
    let mut pending: Option<Frame> = None;
    let mut tick = interval(HOLD_FLUSH_INTERVAL);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            msg = rx.recv() => match msg {
                Some(frame) => {
                    // Hold ticks count from the latest arrival (covers the first).
                    tick.reset();
                    let n = pacer.on_frame(frame.ts_ms);
                    shared.gaps_clamped.store(pacer.clamped(), Ordering::Relaxed);
                    if let Some(prev) = pending.replace(frame) {
                        if write_n(&mut stdin, &prev.jpeg, n, &shared).await.is_err() {
                            return;
                        }
                        shared.frames_done.fetch_add(1, Ordering::Relaxed);
                    }
                }
                None => break,
            },
            _ = tick.tick(), if pending.is_some() && rx.is_empty() => {
                if let Some(p) = pending.as_ref() {
                    // Once finish/mark_end fixed the end, a tick in the stop
                    // grace must not hold the frame past it (live +5 % on 8 s).
                    let mut held = p.arrived.elapsed();
                    if let Some(e) = shared.end_at.get() {
                        held = held.min(e.saturating_duration_since(p.arrived));
                    }
                    let est = p.ts_ms.saturating_add(ms(held));
                    let n = pacer.advance_to(est);
                    if write_n(&mut stdin, &p.jpeg, n, &shared).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
    if let Some(p) = pending.as_ref() {
        let held = shared
            .end_at
            .get()
            .map(|e| ms(e.saturating_duration_since(p.arrived)))
            .unwrap_or(0);
        let n = pacer.on_end(p.ts_ms.saturating_add(held));
        shared
            .gaps_clamped
            .store(pacer.clamped(), Ordering::Relaxed);
        if write_n(&mut stdin, &p.jpeg, n, &shared).await.is_err() {
            return;
        }
        shared.frames_done.fetch_add(1, Ordering::Relaxed);
    }
    if let Err(e) = stdin.flush().await {
        let _ = shared.write_error.set(e.to_string());
    }
}
