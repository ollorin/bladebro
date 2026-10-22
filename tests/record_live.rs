//! LIVE driver for the screencast source + encoder (06-02, D6-12).
//!
//! `#[ignore]`: invisible to `cargo test`, never shipped, not a CLI command or
//! tool. It attaches the recorder's public lib API to the Chrome a running
//! `bladebro nav` daemon launched (found by port) and prints one
//! `REC_LIVE_REPORT <json>` line. Run it through
//! `tools/record_sidecar/scenarios/source_live.sh` (headless, scratch home,
//! local fixtures only).
//!
//! Inputs (harness inputs of this test, not product options):
//!   REC_LIVE_PORT       Chrome debugging port (unset: the test is skipped)
//!   REC_LIVE_TARGET     target id (default: first real page target, 15 s retry)
//!   REC_LIVE_OUT        absolute .webm/.mp4 path
//!   REC_LIVE_SECS       recording length, default 20
//!   REC_LIVE_EXTRA_MS   added to the length, so the stop is not on a whole second
//!   REC_LIVE_MODE       plain | resize | trace | slow
//!                       trace: a test-only WS proxy between the Source and the page logs every
//!                              client->Chrome method (report: wire_methods, wire_first)
//!                       slow:  a SlowSink (300 ms per frame, queue 16) stands in for the encoder
//!                              feed (report: slow_sink {accepted, refused, max_len})
//!   Every mode also reports frame_sizes (distinct JPEG pixel sizes seen by the sink).
//!   REC_LIVE_NO_FRAMES_MS  optional no-frames period override
//!
//! SIGINT (ctrl-c) stops the recording gracefully and still reports.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bladebro::cdp::discovery::{list_page_targets, version};
use bladebro::cdp::CdpClient;
use bladebro::record::values::{DEFAULT_FPS, DEFAULT_QUALITY};
use bladebro::record::{
    resolve_ffmpeg, Encoder, EncoderConfig, EncoderReport, Endpoint, Format, FrameSink, FrameSize,
    FrameSource, RecordConfig, SourceConfig, SourceReport,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, connect_async};

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.trim().is_empty())
}

async fn pick_target(base: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(ts) = list_page_targets(base).await {
            if let Some(t) = ts.iter().find(|t| !t.url.starts_with("devtools://")) {
                return t.id.clone();
            }
        }
        assert!(Instant::now() < deadline, "no page target on {base}");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// HARNESS ONLY. Emulation and window-bounds changes are page-visible (they
/// change innerWidth, matchMedia, window state); they exist so a recording can
/// be driven through a size change on purpose and must NEVER move into src/
/// (SAFE-01 is about the recorder's own messages). Runs on SEPARATE clients,
/// never the recorder's connection.
async fn resize_script(base: String, target: String, log: Arc<std::sync::Mutex<Vec<Value>>>) {
    let note = |step: &str, r: Result<Value, String>| {
        let v = match r {
            Ok(v) => json!({"step": step, "ok": v}),
            Err(e) => json!({"step": step, "error": e}),
        };
        log.lock().expect("log").push(v);
    };
    let page_ws = match list_page_targets(&base).await {
        Ok(ts) => ts
            .into_iter()
            .find(|t| t.id == target)
            .and_then(|t| t.web_socket_debugger_url),
        Err(_) => None,
    };
    let Some(page_ws) = page_ws else {
        note("page ws", Err("target not found".into()));
        return;
    };
    let Ok(page) = CdpClient::connect(&page_ws).await else {
        note("page connect", Err("connect failed".into()));
        return;
    };
    let browser = match version(&base).await {
        Ok(v) => match v.web_socket_debugger_url {
            Some(u) => CdpClient::connect(&u).await.ok(),
            None => None,
        },
        Err(_) => None,
    };
    let t0 = Instant::now();
    let at = |s: u64| async move {
        let target = Duration::from_secs(s);
        if let Some(d) = target.checked_sub(t0.elapsed()) {
            tokio::time::sleep(d).await;
        }
    };
    at(4).await;
    let r = page
        .send(
            "Emulation.setDeviceMetricsOverride",
            Some(json!({"width":1001,"height":641,"deviceScaleFactor":0,"mobile":false})),
        )
        .await
        .map_err(|e| e.to_string());
    note("emulate 1001x641", r);
    at(9).await;
    let r = page
        .send("Emulation.clearDeviceMetricsOverride", Some(json!({})))
        .await
        .map_err(|e| e.to_string());
    note("emulate clear", r);
    at(12).await;
    let Some(browser) = browser else {
        note("browser connect", Err("no browser ws".into()));
        return;
    };
    let win = browser
        .send(
            "Browser.getWindowForTarget",
            Some(json!({"targetId": target})),
        )
        .await
        .map_err(|e| e.to_string());
    let wid = win.as_ref().ok().and_then(|v| v["windowId"].as_i64());
    note("getWindowForTarget", win);
    if let Some(window_id) = wid {
        // Restore first: a minimized/maximized window ignores explicit sizes.
        let r = browser
            .send(
                "Browser.setWindowBounds",
                Some(json!({"windowId": window_id, "bounds": {"windowState": "normal"}})),
            )
            .await
            .map_err(|e| e.to_string());
        note("window normal", r);
        let r = browser
            .send(
                "Browser.setWindowBounds",
                Some(json!({"windowId": window_id, "bounds": {"width": 1101, "height": 701}})),
            )
            .await
            .map_err(|e| e.to_string());
        note("window 1101x701", r);
    }
}

/// `method` of a CDP request text frame (None for responses, events, garbage).
fn method_of(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    v.get("method")?.as_str().map(str::to_string)
}

/// Pixel size from the first JPEG SOF marker (None if it is not a JPEG).
fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let m = b[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC {
            if i + 9 > b.len() {
                return None;
            }
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        i += 2 + len;
    }
    None
}

/// Per distinct frame size: (count, first ms, last ms) since the tap started.
type Sizes = BTreeMap<(u32, u32), (u64, u64, u64)>;

/// Sink wrapper that records the pixel sizes of the frames Chrome delivered, so a
/// mid-recording size change is evidenced instead of assumed.
struct SizeTap {
    inner: Arc<dyn FrameSink>,
    sizes: Arc<Mutex<Sizes>>,
    t0: Instant,
}

impl FrameSink for SizeTap {
    fn send_frame(&self, jpeg: Vec<u8>, ts_ms: u64) -> bool {
        if let Some(sz) = jpeg_size(&jpeg) {
            let now = self.t0.elapsed().as_millis() as u64;
            let mut m = self.sizes.lock().expect("sizes");
            let e = m.entry(sz).or_insert((0, now, now));
            e.0 += 1;
            e.2 = now;
        }
        self.inner.send_frame(jpeg, ts_ms)
    }

    fn mark_end(&self) {
        self.inner.mark_end()
    }
}

/// Slow consumer (D6-15): its own bounded queue of 16 filled by try_send, drained
/// at 300 ms per frame into the real encoder.
#[derive(Default)]
struct SlowStats {
    accepted: AtomicU64,
    refused: AtomicU64,
    max_len: AtomicU64,
}

struct SlowSink {
    tx: tokio::sync::mpsc::Sender<(Vec<u8>, u64)>,
    stats: Arc<SlowStats>,
}

const SLOW_QUEUE: usize = 16;

impl FrameSink for SlowSink {
    fn send_frame(&self, jpeg: Vec<u8>, ts_ms: u64) -> bool {
        match self.tx.try_send((jpeg, ts_ms)) {
            Ok(()) => {
                self.stats.accepted.fetch_add(1, Ordering::Relaxed);
                let len = (SLOW_QUEUE - self.tx.capacity()) as u64;
                self.stats.max_len.fetch_max(len, Ordering::Relaxed);
                true
            }
            Err(_) => {
                self.stats.refused.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
}

fn slow_sink(enc: Arc<Encoder>) -> (SlowSink, Arc<SlowStats>, tokio::task::JoinHandle<()>) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(Vec<u8>, u64)>(SLOW_QUEUE);
    let stats = Arc::new(SlowStats::default());
    let join = tokio::spawn(async move {
        while let Some((jpeg, ts)) = rx.recv().await {
            tokio::time::sleep(Duration::from_millis(300)).await;
            enc.send_frame(jpeg, ts);
        }
    });
    (
        SlowSink {
            tx,
            stats: stats.clone(),
        },
        stats,
        join,
    )
}

/// TEST-ONLY wire trace (D6-14): accepts the Source's one connection on a local
/// port, connects to the real page WebSocket and forwards both ways, logging the
/// `method` of every client->Chrome text frame in order.
async fn start_proxy(page_ws: String, log: Arc<Mutex<Vec<String>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("proxy bind");
    let url = format!("ws://{}/", listener.local_addr().expect("proxy addr"));
    tokio::spawn(async move {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let Ok(client) = accept_async(stream).await else {
            return;
        };
        let Ok((chrome, _)) = connect_async(page_ws.as_str()).await else {
            return;
        };
        let (mut c_tx, mut c_rx) = client.split();
        let (mut h_tx, mut h_rx) = chrome.split();
        let up = async {
            while let Some(Ok(m)) = c_rx.next().await {
                if let Message::Text(t) = &m {
                    if let Some(method) = method_of(t.as_str()) {
                        log.lock().expect("wire log").push(method);
                    }
                }
                let close = matches!(m, Message::Close(_));
                if h_tx.send(m).await.is_err() || close {
                    break;
                }
            }
        };
        let down = async {
            while let Some(Ok(m)) = h_rx.next().await {
                let close = matches!(m, Message::Close(_));
                if c_tx.send(m).await.is_err() || close {
                    break;
                }
            }
        };
        tokio::select! { _ = up => {}, _ = down => {} }
    });
    url
}

#[test]
fn method_of_parses_requests_only() {
    assert_eq!(
        method_of(r#"{"id":3,"method":"Page.enable"}"#).as_deref(),
        Some("Page.enable")
    );
    assert_eq!(method_of(r#"{"id":3,"result":{}}"#), None);
    assert_eq!(method_of("not json {"), None);
    assert_eq!(method_of(r#"{"method":7}"#), None);
}

#[test]
fn jpeg_size_reads_sof() {
    // SOI, APP0 (len 4), SOF0 (len 11) height 0x0140=320 width 0x0200=512.
    let b = [
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x01, 0x40,
        0x02, 0x00, 0x01, 0x01, 0x11, 0x00,
    ];
    assert_eq!(jpeg_size(&b), Some((512, 320)));
    assert_eq!(jpeg_size(b"nope"), None);
    assert_eq!(jpeg_size(&[0xFF, 0xD8, 0xFF, 0xC0]), None);
}

fn source_json(r: &SourceReport) -> Value {
    let s = &r.stats;
    json!({
        "outcome": format!("{:?}", r.outcome),
        "ok": r.ok(),
        "verdict": r.verdict(),
        "elapsed_ms": r.elapsed_ms,
        "hidden": r.hidden,
        "frames_received": s.frames_received,
        "acks_sent": s.acks_sent,
        "acks_ok": s.acks_ok,
        "ack_errors": s.ack_errors,
        "unacked": s.unacked,
        "forwarded": s.forwarded,
        "capped": s.capped,
        "queue_dropped": s.queue_dropped,
        "lost": s.lost,
        "bad_frames": s.bad_frames,
        "ts_fallback": s.ts_fallback,
        "first_frame_ms": s.first_frame_ms,
        "last_frame_ms": s.last_frame_ms,
        "last_frame_age_ms": r.last_frame_age_ms,
        "quiet_tail_ms": r.quiet_tail_ms,
        "longest_gap_ms": s.longest_gap_ms,
        "methods_sent": s.methods_sent.iter().map(|(m, n)| json!([m, n])).collect::<Vec<_>>(),
    })
}

fn encoder_json(r: &EncoderReport) -> Value {
    json!({
        "outcome": format!("{:?}", r.outcome),
        "ok": r.ok(),
        "verdict": r.verdict(),
        "path": r.path.display().to_string(),
        "bytes": r.bytes,
        "frames_queued": r.frames_queued,
        "frames_dropped": r.frames_dropped,
        "frames_written": r.frames_written,
        "finalize_ms": r.finalize_ms,
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "live: needs a running bladebro Chrome (tools/record_sidecar/scenarios/source_live.sh)"]
async fn record_live() {
    let Some(port) = env("REC_LIVE_PORT") else {
        eprintln!("record_live: skipped (REC_LIVE_PORT unset)");
        return;
    };
    // The source's no-frames warning goes through tracing; show it on stderr.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let base = format!("127.0.0.1:{port}");
    let out = PathBuf::from(env("REC_LIVE_OUT").expect("REC_LIVE_OUT (absolute path) is required"));
    let secs: u64 = env("REC_LIVE_SECS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let extra_ms: u64 = env("REC_LIVE_EXTRA_MS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mode = env("REC_LIVE_MODE").unwrap_or_else(|| "plain".into());
    let target = match env("REC_LIVE_TARGET") {
        Some(t) => t,
        None => pick_target(&base).await,
    };

    let format = if out.extension().and_then(|e| e.to_str()) == Some("mp4") {
        Format::Mp4
    } else {
        Format::WebM
    };
    let rc = RecordConfig {
        enabled: true,
        dir: out.parent().unwrap_or(Path::new(".")).to_path_buf(),
        format,
        fps: DEFAULT_FPS,
        size: FrameSize {
            width: 1280,
            height: 720,
        },
        quality: DEFAULT_QUALITY,
        queue_len: 16,
        name: None,
        artifacts_dir: PathBuf::from("/nonexistent-artifacts"),
    };
    let ffmpeg = resolve_ffmpeg().await.expect("ffmpeg resolves");
    let enc =
        Arc::new(Encoder::start(&EncoderConfig::new(ffmpeg, &rc), &out).expect("encoder starts"));

    let mut scfg = SourceConfig::from_record(&rc);
    if let Some(ms) = env("REC_LIVE_NO_FRAMES_MS").and_then(|v| v.parse::<u64>().ok()) {
        scfg.no_frames_after = Duration::from_millis(ms);
    }
    let sizes: Arc<Mutex<Sizes>> = Arc::default();
    let mut slow: Option<(Arc<SlowStats>, tokio::task::JoinHandle<()>)> = None;
    let inner: Arc<dyn FrameSink> = if mode == "slow" {
        let (s, st, j) = slow_sink(enc.clone());
        slow = Some((st, j));
        Arc::new(s)
    } else {
        enc.clone()
    };
    let sink: Arc<dyn FrameSink> = Arc::new(SizeTap {
        inner,
        sizes: sizes.clone(),
        t0: Instant::now(),
    });
    let wire_log: Arc<Mutex<Vec<String>>> = Arc::default();
    let started = if mode == "trace" {
        let ts = list_page_targets(&base).await.expect("targets");
        let ws = ts
            .into_iter()
            .find(|t| t.id == target)
            .and_then(|t| t.web_socket_debugger_url)
            .expect("page ws url");
        let proxy = start_proxy(ws, wire_log.clone()).await;
        FrameSource::start_ws(&proxy, &target, scfg, sink).await
    } else {
        FrameSource::start(&Endpoint::Port { base: base.clone() }, &target, scfg, sink).await
    };
    let src = match started {
        Ok(s) => s,
        Err(e) => panic!("source did not start: {e}"),
    };
    let t0 = Instant::now();
    let harness: Arc<std::sync::Mutex<Vec<Value>>> = Arc::default();
    let hjoin = (mode == "resize")
        .then(|| tokio::spawn(resize_script(base.clone(), target.clone(), harness.clone())));

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(secs) + Duration::from_millis(extra_ms)) => {}
        _ = tokio::signal::ctrl_c() => eprintln!("record_live: SIGINT, stopping"),
    }
    let health_at_stop = format!("{:?}", src.health());
    // D6-13: wall clock = source start to the stop() call.
    let wall_ms = t0.elapsed().as_millis() as u64;
    let src_report = src.stop().await;
    // The slow consumer drains what is still queued (<= 16 frames at 300 ms) before the encoder is released.
    let slow_json = match slow.take() {
        Some((st, j)) => {
            let _ = j.await;
            Some(json!({
                "accepted": st.accepted.load(Ordering::Relaxed),
                "refused": st.refused.load(Ordering::Relaxed),
                "max_len": st.max_len.load(Ordering::Relaxed),
            }))
        }
        None => None,
    };
    if let Some(h) = hjoin {
        h.abort();
    }
    let enc = Arc::try_unwrap(enc)
        .ok()
        .expect("source released the encoder");
    let enc_report = enc.finish().await;

    let report = json!({
        "target": target,
        "mode": mode,
        "secs_requested": secs,
        "wall_ms": wall_ms,
        "health_at_stop": health_at_stop,
        "source": source_json(&src_report),
        "encoder": encoder_json(&enc_report),
        "harness": harness.lock().expect("harness").clone(),
        "frame_sizes": sizes.lock().expect("sizes").iter().map(|((w, h), (n, a, b))| json!({"size": format!("{w}x{h}"), "count": n, "first_ms": a, "last_ms": b})).collect::<Vec<_>>(),
    });
    let mut report = report;
    if let Some(sj) = slow_json {
        report["slow_sink"] = sj;
    }
    if mode == "trace" {
        let log = wire_log.lock().expect("wire log").clone();
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for m in &log {
            *counts.entry(m.clone()).or_default() += 1;
        }
        report["wire_methods"] = json!(counts);
        report["wire_first"] = json!(log.iter().take(10).collect::<Vec<_>>());
    }
    println!("REC_LIVE_REPORT {report}");
}
