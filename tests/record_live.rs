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
//!   REC_LIVE_MODE       plain | resize
//!   REC_LIVE_NO_FRAMES_MS  optional no-frames period override
//!
//! SIGINT (ctrl-c) stops the recording gracefully and still reports.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bladebro::cdp::discovery::{list_page_targets, version};
use bladebro::cdp::CdpClient;
use bladebro::record::values::{DEFAULT_FPS, DEFAULT_QUALITY};
use bladebro::record::{
    resolve_ffmpeg, Encoder, EncoderConfig, EncoderReport, Endpoint, Format, FrameSize,
    FrameSource, RecordConfig, SourceConfig, SourceReport,
};
use serde_json::{json, Value};

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
    let src = match FrameSource::start(
        &Endpoint::Port { base: base.clone() },
        &target,
        scfg,
        enc.clone(),
    )
    .await
    {
        Ok(s) => s,
        Err(e) => panic!("source did not start: {e}"),
    };
    let t0 = Instant::now();
    let harness: Arc<std::sync::Mutex<Vec<Value>>> = Arc::default();
    let hjoin = (mode == "resize")
        .then(|| tokio::spawn(resize_script(base.clone(), target.clone(), harness.clone())));

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(secs)) => {}
        _ = tokio::signal::ctrl_c() => eprintln!("record_live: SIGINT, stopping"),
    }
    let health_at_stop = format!("{:?}", src.health());
    // D6-13: wall clock = source start to the stop() call.
    let wall_ms = t0.elapsed().as_millis() as u64;
    let src_report = src.stop().await;
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
    });
    println!("REC_LIVE_REPORT {report}");
}
