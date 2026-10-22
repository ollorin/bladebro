//! Source tests against the in-process fake CDP endpoint (real WebSocket, no
//! Chrome). Shared helpers live here; scenarios are in `capture_tests/`.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::capture::{Source, SourceConfig};
use super::fake_cdp::{FakeCdp, FakeOpts};
use super::sink::FrameSink;

mod flow;
mod lifecycle;
mod limits;

/// Every scenario runs under a hard 20 s ceiling.
async fn guard<F: Future>(f: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(20), f)
        .await
        .expect("scenario timed out")
}

fn cfg(fps: u32) -> SourceConfig {
    SourceConfig {
        fps,
        quality: 70,
        max_width: 1280,
        max_height: 720,
        ack_timeout: Duration::from_secs(2),
        stop_grace: Duration::from_millis(100),
    }
}

/// Accepts every frame; remembers when and with which timestamp.
#[derive(Default)]
struct CountingSink {
    n: AtomicU64,
    seen: Mutex<Vec<(Instant, u64)>>,
}

impl FrameSink for CountingSink {
    fn send_frame(&self, _jpeg: Vec<u8>, ts_ms: u64) -> bool {
        self.n.fetch_add(1, Ordering::SeqCst);
        self.seen
            .lock()
            .expect("seen")
            .push((Instant::now(), ts_ms));
        true
    }
}

async fn start(fake: &FakeCdp, fps: u32, sink: Arc<dyn FrameSink>) -> Source {
    Source::start_ws(&fake.url, "T1", cfg(fps), sink)
        .await
        .map_err(|e| e.to_string())
        .expect("source starts")
}

async fn fake_with(opts: FakeOpts) -> FakeCdp {
    FakeCdp::start(opts).await
}
