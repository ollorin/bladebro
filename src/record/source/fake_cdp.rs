//! In-process fake CDP endpoint (tests only): a real WebSocket server on
//! loopback that logs every request, replies `{}` to all of them, can inject
//! arbitrary server->client frames and emulates Chrome's screencast emitter
//! (including its "at most N unacked frames in flight" rule). No Chrome.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]
pub(super) struct FakeOpts {
    /// `(method, message)`: answer that method with a CDP error.
    pub fail: Option<(&'static str, &'static str)>,
    /// Frames emitted after `Page.startScreencast` (0 = none).
    pub frames: usize,
    pub frame_bytes: usize,
    pub interval: Duration,
    pub screencast_session: i64,
    /// `Some(2)` = Chrome's rule: stop emitting while that many are unacked.
    pub max_in_flight: Option<usize>,
    /// Extra frames still emitted after `Page.stopScreencast` arrived.
    pub frames_after_stop: usize,
    /// Every n-th frame carries undecodable base64.
    pub garbage_every: Option<usize>,
}

impl Default for FakeOpts {
    fn default() -> Self {
        Self {
            fail: None,
            frames: 0,
            frame_bytes: 30_000,
            interval: Duration::from_millis(2),
            screencast_session: 1,
            max_in_flight: None,
            frames_after_stop: 0,
            garbage_every: None,
        }
    }
}

#[derive(Default)]
pub(super) struct FakeState {
    /// `params.sessionId` of every `Page.screencastFrameAck`, in order.
    pub acks: Mutex<Vec<i64>>,
    pub stopped: AtomicBool,
    pub emitted: AtomicUsize,
    pub done: AtomicBool,
}

pub(super) struct FakeCdp {
    pub url: String,
    /// Every `(method, params)` the client sent, in order.
    pub log: Arc<Mutex<Vec<(String, Value)>>>,
    pub state: Arc<FakeState>,
    /// `None` closes the socket from the server side.
    push_tx: mpsc::UnboundedSender<Option<String>>,
    closed: Arc<Notify>,
}

impl FakeCdp {
    pub async fn start(opts: FakeOpts) -> FakeCdp {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let log: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let state: Arc<FakeState> = Arc::default();
        let closed = Arc::new(Notify::new());
        let (push_tx, mut push_rx) = mpsc::unbounded_channel::<Option<String>>();
        let (log2, state2, closed2, tx2) =
            (log.clone(), state.clone(), closed.clone(), push_tx.clone());
        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(ws) = accept_async(stream).await else {
                return;
            };
            let (mut sink, mut stream) = ws.split();
            loop {
                tokio::select! {
                    pushed = push_rx.recv() => match pushed {
                        Some(Some(t)) => {
                            if sink.send(Message::Text(t.into())).await.is_err() { break; }
                        }
                        _ => { let _ = sink.close().await; break; }
                    },
                    msg = stream.next() => {
                        let Some(Ok(msg)) = msg else { break };
                        let text = match msg {
                            Message::Text(t) => t.to_string(),
                            Message::Close(_) => break,
                            _ => continue,
                        };
                        let Ok(req) = serde_json::from_str::<Value>(&text) else { continue };
                        let id = req["id"].as_u64().unwrap_or(0);
                        let method = req["method"].as_str().unwrap_or("").to_string();
                        log2.lock().expect("log").push((method.clone(), req["params"].clone()));
                        let resp = match opts.fail {
                            Some((m, e)) if m == method => {
                                json!({"id": id, "error": {"code": -32000, "message": e}})
                            }
                            _ => json!({"id": id, "result": {}}),
                        };
                        if sink.send(Message::Text(resp.to_string().into())).await.is_err() {
                            break;
                        }
                        match method.as_str() {
                            "Page.screencastFrameAck" => {
                                state2.acks.lock().expect("acks")
                                    .push(req["params"]["sessionId"].as_i64().unwrap_or(-1));
                            }
                            "Page.stopScreencast" => state2.stopped.store(true, Ordering::SeqCst),
                            "Page.startScreencast" if opts.fail.is_none() => {
                                tokio::spawn(emit(opts.clone(), state2.clone(), tx2.clone()));
                            }
                            _ => {}
                        }
                    }
                }
            }
            closed2.notify_one();
        });
        FakeCdp {
            url: format!("ws://{addr}/devtools/page/fake"),
            log,
            state,
            push_tx,
            closed,
        }
    }

    /// Inject a raw server->client text frame.
    pub fn push_raw(&self, text: String) {
        let _ = self.push_tx.send(Some(text));
    }

    /// Close the socket from the server side (tab closed / browser gone).
    pub fn close_now(&self) {
        let _ = self.push_tx.send(None);
    }

    /// Resolves when the connection ended (client closed it, or `close_now`).
    pub async fn closed(&self) {
        self.closed.notified().await;
    }

    /// Wait until the emitter has sent everything it was going to send.
    pub async fn wait_emitter_done(&self) {
        while !self.state.done.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    pub fn ack_ids(&self) -> Vec<i64> {
        self.state.acks.lock().expect("acks").clone()
    }

    pub fn methods(&self) -> Vec<String> {
        self.log
            .lock()
            .expect("log")
            .iter()
            .map(|(m, _)| m.clone())
            .collect()
    }
}

/// One frame event as Chrome would send it. `ts` is epoch seconds.
pub(super) fn frame_event(data: &str, session: i64, ts: f64) -> String {
    json!({"method": "Page.screencastFrame", "params": {
        "data": data,
        "metadata": {"timestamp": ts, "deviceWidth": 1280, "deviceHeight": 720},
        "sessionId": session
    }})
    .to_string()
}

/// Chrome's timestamp for frame `i` (tests recompute it with the same math).
pub(super) fn frame_ts(i: usize, interval: Duration) -> f64 {
    1.7e9 + i as f64 * interval.as_secs_f64()
}

async fn emit(o: FakeOpts, st: Arc<FakeState>, tx: mpsc::UnboundedSender<Option<String>>) {
    let bytes: Vec<u8> = (0..o.frame_bytes).map(|i| (i * 31 % 251) as u8).collect();
    let good = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mut extra_left = o.frames_after_stop;
    for i in 0..o.frames {
        if st.stopped.load(Ordering::SeqCst) {
            if extra_left == 0 {
                break;
            }
            extra_left -= 1;
        }
        if let Some(max) = o.max_in_flight {
            // Chrome stops sending while `max` frames are unacked.
            while st.emitted.load(Ordering::SeqCst) - st.acks.lock().expect("acks").len() >= max
                && !st.stopped.load(Ordering::SeqCst)
            {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
        let garbage = o.garbage_every.is_some_and(|n| n > 0 && (i + 1) % n == 0);
        let data = if garbage { "!!!" } else { good.as_str() };
        let _ = tx.send(Some(frame_event(
            data,
            o.screencast_session,
            frame_ts(i, o.interval),
        )));
        st.emitted.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(o.interval).await;
    }
    st.done.store(true, Ordering::SeqCst);
}
