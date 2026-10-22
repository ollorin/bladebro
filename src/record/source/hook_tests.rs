//! SC3 (hook half): screencast frames survive the CDP bus size bound through
//! a real `CdpClient` (fake WS server -> reader -> route_cdp_text -> bus).

use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use super::fake_cdp::{FakeCdp, FakeOpts};
use crate::cdp::client::MAX_SCREENCAST_EVENT_BYTES;
use crate::cdp::{protocol::CdpEvent, CdpClient};

fn frame_text(method: &str, data: &str) -> String {
    json!({"method": method, "params": {
        "data": data,
        "metadata": {"timestamp": 1.7e9, "deviceWidth": 1280, "deviceHeight": 720},
        "sessionId": 7
    }})
    .to_string()
}

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 251) as u8).collect()
}

async fn next_event(rx: &mut tokio::sync::broadcast::Receiver<CdpEvent>) -> CdpEvent {
    tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("event in time")
        .expect("bus open")
}

async fn setup() -> (
    FakeCdp,
    CdpClient,
    tokio::sync::broadcast::Receiver<CdpEvent>,
) {
    let fake = FakeCdp::start(FakeOpts::default()).await;
    let client = CdpClient::connect(&fake.url).await.expect("connect");
    let rx = client.subscribe();
    (fake, client, rx)
}

#[tokio::test]
async fn two_mb_screencast_frame_passes_intact() {
    let (fake, client, mut rx) = setup().await;
    let bytes = payload(1_500_000);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let text = frame_text("Page.screencastFrame", &b64);
    assert!(text.len() >= 2_000_000);
    fake.push_raw(text);
    let ev = next_event(&mut rx).await;
    assert_eq!(ev.method, "Page.screencastFrame");
    assert_eq!(ev.params["data"].as_str(), Some(b64.as_str()));
    assert_eq!(ev.params["sessionId"], 7);
    assert_eq!(ev.params["metadata"]["deviceWidth"], 1280);
    let back = base64::engine::general_purpose::STANDARD
        .decode(ev.params["data"].as_str().expect("data"))
        .expect("decode");
    assert_eq!(back, bytes);
    // The connection still serves requests after the big frame.
    client.send("Page.enable", None).await.expect("round trip");
    assert_eq!(fake.log.lock().expect("log")[0].0, "Page.enable");
}

#[tokio::test]
async fn just_over_16k_frame_is_intact() {
    let (fake, _client, mut rx) = setup().await;
    let b64 = base64::engine::general_purpose::STANDARD.encode(payload(20_000));
    fake.push_raw(frame_text("Page.screencastFrame", &b64));
    let ev = next_event(&mut rx).await;
    assert_eq!(ev.params["data"].as_str(), Some(b64.as_str()));
    assert_eq!(ev.params["sessionId"], 7);
}

#[tokio::test]
async fn frame_above_the_cap_is_still_stripped() {
    let (fake, _client, mut rx) = setup().await;
    let overhead = frame_text("Page.screencastFrame", "").len();
    let data = "A".repeat(MAX_SCREENCAST_EVENT_BYTES + 1024 - overhead);
    let text = frame_text("Page.screencastFrame", &data);
    assert_eq!(text.len(), MAX_SCREENCAST_EVENT_BYTES + 1024);
    fake.push_raw(text);
    let ev = next_event(&mut rx).await;
    assert_eq!(ev.method, "Page.screencastFrame");
    assert_eq!(ev.params, Value::Object(Default::default()));
}

#[tokio::test]
async fn other_oversized_events_are_still_stripped() {
    let (fake, _client, mut rx) = setup().await;
    let text = json!({"method": "Network.webSocketFrameReceived", "params": {
        "requestId": "1", "response": {"payloadData": "x".repeat(100_000)}
    }})
    .to_string();
    fake.push_raw(text);
    let ev = next_event(&mut rx).await;
    assert_eq!(ev.params, Value::Object(Default::default()));
}

#[tokio::test]
async fn small_other_event_is_intact() {
    let (fake, _client, mut rx) = setup().await;
    fake.push_raw(
        json!({"method": "Page.frameNavigated", "params": {"frame": {"url": "http://x/"}}})
            .to_string(),
    );
    let ev = next_event(&mut rx).await;
    assert_eq!(ev.params["frame"]["url"], "http://x/");
}
