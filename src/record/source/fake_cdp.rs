//! In-process fake CDP endpoint (tests only): a real WebSocket server on
//! loopback that logs every request, replies `{}` to all of them and lets a
//! test inject arbitrary server->client text frames. No Chrome involved.

use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Default, Clone)]
pub(super) struct FakeOpts {
    /// `(method, message)`: answer that method with a CDP error.
    pub fail: Option<(&'static str, &'static str)>,
}

pub(super) struct FakeCdp {
    pub url: String,
    /// Every `(method, params)` the client sent, in order.
    pub log: Arc<Mutex<Vec<(String, Value)>>>,
    push_tx: mpsc::UnboundedSender<String>,
    // Read by capture_tests (Task 3): the close-observation tests.
    #[allow(dead_code)]
    closed: Arc<Notify>,
}

impl FakeCdp {
    pub async fn start(opts: FakeOpts) -> FakeCdp {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let log: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let closed = Arc::new(Notify::new());
        let (push_tx, mut push_rx) = mpsc::unbounded_channel::<String>();
        let (log2, closed2) = (log.clone(), closed.clone());
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
                        Some(t) => { if sink.send(Message::Text(t.into())).await.is_err() { break; } }
                        None => break,
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
                    }
                }
            }
            closed2.notify_one();
        });
        FakeCdp {
            url: format!("ws://{addr}/devtools/page/fake"),
            log,
            push_tx,
            closed,
        }
    }

    /// Inject a raw server->client text frame.
    pub fn push_raw(&self, text: String) {
        let _ = self.push_tx.send(text);
    }

    /// Resolves when the client side closed the socket (used from Task 3).
    #[allow(dead_code)]
    pub async fn closed(&self) {
        self.closed.notified().await;
    }
}
