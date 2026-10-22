//! The screencast source: own CDP connection, ack-first loop, stop.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::{broadcast::error::RecvError, oneshot};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::Instant;

use super::handler::{lock, Handler, SharedLive, SharedStats};
use super::report::{SourceError, SourceOutcome, SourceReport, SourceStats};
use super::sink::FrameSink;
use super::watch::{Health, NO_FRAMES_AFTER};
use super::wire::{RecMethod, Wire};
use crate::cdp::discovery::list_page_targets;
use crate::cdp::{CdpClient, CdpSession};
use crate::record::config::RecordConfig;

/// Per-ack deadline. A lost ack only costs a counted `ack_errors`.
pub const ACK_TIMEOUT: Duration = Duration::from_secs(2);
/// After `Page.stopScreencast`, keep acking in-flight frames this long.
pub const STOP_GRACE: Duration = Duration::from_millis(500);
/// Deadline for the start-up commands.
const START_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a silent connection is checked for closure (the client's event
/// bus never reports Closed while a client handle is alive).
const LIVENESS_TICK: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub fps: u32,
    pub quality: u8,
    pub max_width: u32,
    pub max_height: u32,
    pub ack_timeout: Duration,
    pub stop_grace: Duration,
    /// No first frame after this long is `NoFrames` (D6-10). A const default,
    /// not a user option; tests shorten it.
    pub no_frames_after: Duration,
}

impl SourceConfig {
    pub fn from_record(rc: &RecordConfig) -> Self {
        Self {
            fps: rc.fps,
            quality: rc.quality,
            max_width: rc.size.width,
            max_height: rc.size.height,
            ack_timeout: ACK_TIMEOUT,
            stop_grace: STOP_GRACE,
            no_frames_after: NO_FRAMES_AFTER,
        }
    }
}

/// Where the recorder connects. Phase 7 maps `launched_pipe()` to `Pipe`.
#[derive(Debug, Clone)]
pub enum Endpoint {
    /// `host:port` of Chrome's debugging port (WebSocket transport).
    Port {
        base: String,
    },
    Pipe,
}

pub struct Source {
    wire: Arc<Wire>,
    stats: SharedStats,
    live: SharedLive,
    no_frames_after: Duration,
    stop_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<SourceOutcome>>,
    target_id: String,
    started: Instant,
    stop_grace: Duration,
}

impl Source {
    /// Resolve the tab's page WebSocket by target id (plain `/json` HTTP, not a
    /// CDP method) and start recording it.
    pub async fn start(
        ep: &Endpoint,
        target_id: &str,
        cfg: SourceConfig,
        sink: Arc<dyn FrameSink>,
    ) -> Result<Source, SourceError> {
        let base = match ep {
            // D6-2: the pipe has no port for a second connection. No flat
            // session fallback: an honest limitation, not a hack.
            Endpoint::Pipe => return Err(SourceError::PipeTransport),
            Endpoint::Port { base } => base,
        };
        let mut found = None;
        // A just-created target can lag in /json discovery (same retry as
        // attach_tab_target).
        for attempt in 0..5 {
            let targets = list_page_targets(base)
                .await
                .map_err(|e| SourceError::Connect {
                    url: format!("http://{base}/json"),
                    error: e.to_string(),
                })?;
            found = targets.into_iter().find(|t| t.id == target_id);
            if found.is_some() {
                break;
            }
            if attempt < 4 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        let t = found.ok_or_else(|| SourceError::TargetNotFound {
            target_id: target_id.to_string(),
            base: base.clone(),
        })?;
        let ws = t.ws_url().map_err(|e| SourceError::Connect {
            url: format!("http://{base}/json"),
            error: e.to_string(),
        })?;
        Self::start_ws(ws, target_id, cfg, sink).await
    }

    /// Start on an explicit page WebSocket URL.
    pub async fn start_ws(
        ws_url: &str,
        target_id: &str,
        cfg: SourceConfig,
        sink: Arc<dyn FrameSink>,
    ) -> Result<Source, SourceError> {
        // D6-1: the recorder's OWN connection, never Page's session or bus.
        // Events are per CDP session, so no other subscriber sees a frame.
        let client = CdpClient::connect(ws_url)
            .await
            .map_err(|e| SourceError::Connect {
                url: ws_url.to_string(),
                error: e.to_string(),
            })?;
        let session = CdpSession::root(client);
        // Subscribe before starting so the first frame cannot be missed.
        let sub = session.subscribe();
        let wire = Arc::new(Wire::new(session));
        let cdp_err = |method: RecMethod, e: crate::error::BladeError| SourceError::Cdp {
            method: method.as_str(),
            error: e.to_string(),
        };
        // Page.enable first: attaching without it failed with -32000 on a
        // page that was still starting (Phase 2).
        wire.send(RecMethod::PageEnable, None, START_TIMEOUT)
            .await
            .map_err(|e| cdp_err(RecMethod::PageEnable, e))?;
        let params = json!({
            "format": "jpeg",
            "quality": cfg.quality,
            "maxWidth": cfg.max_width,
            "maxHeight": cfg.max_height,
            "everyNthFrame": 1,
        });
        wire.send(RecMethod::StartScreencast, Some(params), START_TIMEOUT)
            .await
            .map_err(|e| cdp_err(RecMethod::StartScreencast, e))?;

        let started = Instant::now();
        let stats: SharedStats = Arc::default();
        let live: SharedLive = Arc::default();
        let (stop_tx, stop_rx) = oneshot::channel();
        let lp = Loop {
            sub,
            wire: wire.clone(),
            handler: Handler::new(cfg.fps, stats.clone(), live.clone()),
            sink,
            stats: stats.clone(),
            live: live.clone(),
            no_frames_after: cfg.no_frames_after,
            warned: false,
            started,
            ack_timeout: cfg.ack_timeout,
            stop_grace: cfg.stop_grace,
        };
        let task = tokio::spawn(lp.run(stop_rx));
        Ok(Source {
            wire,
            stats,
            live,
            no_frames_after: cfg.no_frames_after,
            stop_tx: Some(stop_tx),
            task: Some(task),
            target_id: target_id.to_string(),
            started,
            stop_grace: cfg.stop_grace,
        })
    }

    pub fn stats(&self) -> SourceStats {
        snapshot(&self.stats, &self.live, &self.wire)
    }

    /// Live state of the capture: waiting, no frames, receiving or quiet.
    pub fn health(&self) -> Health {
        let now = self.started.elapsed().as_millis() as u64;
        self.live
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .health(now, self.no_frames_after.as_millis() as u64)
    }

    /// The recorder's own connection (tests prove it stays responsive).
    #[cfg(test)]
    pub(super) fn client_for_test(&self) -> CdpClient {
        self.wire.session().client().clone()
    }

    /// Stop the screencast, keep acking in-flight frames for the grace, flush
    /// the held frame, close the connection and release the sink so the
    /// caller can `Arc::try_unwrap` the encoder and finish it.
    pub async fn stop(mut self) -> SourceReport {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        let after_ms = self.started.elapsed().as_millis() as u64;
        let outcome = match self.task.take() {
            Some(task) => {
                // Bounded: grace to drain + grace to await acks + stop command.
                let cap = self.stop_grace * 2 + Duration::from_secs(3);
                match tokio::time::timeout(cap, task).await {
                    Ok(Ok(o)) => o,
                    _ => SourceOutcome::ConnectionLost { after_ms },
                }
            }
            None => SourceOutcome::Captured,
        };
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        let (frames, last_age, hidden) = {
            let l = self.live.lock().unwrap_or_else(|e| e.into_inner());
            (l.frames(), l.last_age(elapsed_ms), l.hidden())
        };
        // D6-10 rule 1: zero frames is a blank capture at ANY duration. A
        // known cause (lost connection) keeps its own, more specific outcome.
        let outcome = match outcome {
            SourceOutcome::Captured if frames == 0 => SourceOutcome::NoFrames {
                waited_ms: elapsed_ms,
                hidden,
            },
            o => o,
        };
        // Rule 3: silence after a frame is reported, never a failure.
        let quiet_tail_ms = last_age.filter(|a| *a >= self.no_frames_after.as_millis() as u64);
        SourceReport {
            outcome,
            stats: snapshot(&self.stats, &self.live, &self.wire),
            target_id: std::mem::take(&mut self.target_id),
            elapsed_ms,
            last_frame_age_ms: last_age,
            quiet_tail_ms,
            hidden,
        }
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        // Aborting drops the loop's connection handles; the WS closes.
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn snapshot(stats: &SharedStats, live: &SharedLive, wire: &Wire) -> SourceStats {
    let mut s = lock(stats).clone();
    s.longest_gap_ms = live
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .longest_gap_ms();
    s.methods_sent = wire.methods_sent();
    s
}

struct Loop {
    sub: crate::cdp::SessionSubscription,
    wire: Arc<Wire>,
    handler: Handler,
    sink: Arc<dyn FrameSink>,
    stats: SharedStats,
    live: SharedLive,
    no_frames_after: Duration,
    /// The no-frames warning goes out once.
    warned: bool,
    started: Instant,
    ack_timeout: Duration,
    stop_grace: Duration,
}

impl Loop {
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// D6-4: the ack is spawned BEFORE decode/gate/sink on every path, so a
    /// held, refused or undecodable frame is still acked.
    fn spawn_ack(&self, acks: &mut JoinSet<()>, id: i64) {
        let (wire, stats, t) = (self.wire.clone(), self.stats.clone(), self.ack_timeout);
        acks.spawn(async move {
            let r = wire
                .send(
                    RecMethod::ScreencastFrameAck,
                    Some(json!({ "sessionId": id })),
                    t,
                )
                .await;
            let mut st = lock(&stats);
            if r.is_ok() {
                st.acks_ok += 1;
            } else {
                st.ack_errors += 1;
            }
        });
    }

    /// One stderr warning when no first frame arrived within the period;
    /// recording continues (stdout is untouched).
    fn warn_if_no_frames(&mut self) {
        if self.warned {
            return;
        }
        let h = self
            .live
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .health(self.now_ms(), self.no_frames_after.as_millis() as u64);
        if let Health::NoFrames { for_ms, hidden } = h {
            self.warned = true;
            tracing::warn!(
                target: "bladebro.record",
                "no screencast frames for {:.1} s{}; the tab may be in the background, minimized or covered",
                for_ms as f64 / 1000.0,
                if hidden { " (Chrome reported it hidden)" } else { "" }
            );
        }
    }

    async fn run(mut self, mut stop_rx: oneshot::Receiver<()>) -> SourceOutcome {
        let mut acks: JoinSet<()> = JoinSet::new();
        let mut stopping: Option<Instant> = None;
        let mut lost = false;
        let mut liveness = tokio::time::interval(LIVENESS_TICK);
        loop {
            let due = self
                .handler
                .due_at()
                .map(|ms| self.started + Duration::from_millis(ms));
            // D6-5: the only awaits are this select and ack spawns; the CDP
            // reader task never waits on us (it only broadcasts).
            // Order matters (biased): stop first, then frames (their acks are
            // latency-critical), then the trailing-frame timer, then liveness.
            tokio::select! {
                biased;
                _ = &mut stop_rx, if stopping.is_none() => {
                    stopping = Some(Instant::now() + self.stop_grace);
                    let (wire, t) = (self.wire.clone(), Duration::from_secs(2));
                    acks.spawn(async move {
                        let _ = wire.send(RecMethod::StopScreencast, None, t).await;
                    });
                }
                r = self.sub.recv() => {
                    let closed = matches!(r, Err(RecvError::Closed));
                    let now = self.now_ms();
                    let step = self.handler.on_recv(r, now);
                    if let Some(id) = step.ack {
                        self.spawn_ack(&mut acks, id);
                    }
                    if let Some((id, k)) = step.lag_acks {
                        for _ in 0..k {
                            self.spawn_ack(&mut acks, id);
                        }
                    }
                    if let Some(h) = step.forward {
                        self.handler.forward(h, self.sink.as_ref());
                    }
                    if step.end || closed {
                        lost = true;
                        break;
                    }
                }
                _ = tokio::time::sleep_until(due.unwrap_or_else(Instant::now)), if due.is_some() => {
                    let now = self.now_ms();
                    if let Some(h) = self.handler.on_timer(now) {
                        self.handler.forward(h, self.sink.as_ref());
                    }
                }
                _ = async { tokio::time::sleep_until(stopping.unwrap_or_else(Instant::now)).await }, if stopping.is_some() => {
                    break;
                }
                _ = liveness.tick() => {
                    if self.wire.session().is_closed() {
                        lost = true;
                        break;
                    }
                    self.warn_if_no_frames();
                }
            }
            // Reap finished ack tasks so a long recording does not grow the set.
            while acks.try_join_next().is_some() {}
        }
        if let Some(h) = self.handler.take_held() {
            self.handler.forward(h, self.sink.as_ref());
        }
        // Outstanding acks and the stop command, bounded by the grace.
        let _ = tokio::time::timeout(self.stop_grace, async {
            while acks.join_next().await.is_some() {}
        })
        .await;
        acks.abort_all();
        let after_ms = self.now_ms();
        // `wire` (the connection) and `sink` drop with `self` here.
        if lost && stopping.is_none() {
            SourceOutcome::ConnectionLost { after_ms }
        } else {
            SourceOutcome::Captured
        }
    }
}
