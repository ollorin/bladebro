//! No-frames watchdog scenarios against the fake CDP endpoint, with the
//! no-frames period shortened to 300 ms. Each runs under a 10 s ceiling.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use super::capture::{Source, SourceConfig};
use super::fake_cdp::{FakeCdp, FakeOpts};
use super::report::SourceOutcome;
use super::sink::FrameSink;
use super::watch::Health;

struct Sink;
impl FrameSink for Sink {
    fn send_frame(&self, _: Vec<u8>, _: u64) -> bool {
        true
    }
}

async fn guard<F: Future>(f: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(10), f)
        .await
        .expect("scenario timed out")
}

async fn start(fake: &FakeCdp) -> Source {
    let cfg = SourceConfig {
        fps: 10,
        quality: 70,
        max_width: 1280,
        max_height: 720,
        ack_timeout: Duration::from_secs(2),
        stop_grace: Duration::from_millis(50),
        no_frames_after: Duration::from_millis(300),
    };
    Source::start_ws(&fake.url, "T1", cfg, Arc::new(Sink))
        .await
        .map_err(|e| e.to_string())
        .expect("source starts")
}

fn sleep(ms: u64) -> tokio::time::Sleep {
    tokio::time::sleep(Duration::from_millis(ms))
}

#[tokio::test]
async fn zero_frames_waits_then_reports_no_frames() {
    guard(async {
        let fake = FakeCdp::start(FakeOpts::default()).await;
        let src = start(&fake).await;
        sleep(100).await;
        assert!(matches!(src.health(), Health::Waiting { .. }));
        sleep(300).await;
        assert!(matches!(
            src.health(),
            Health::NoFrames { hidden: false, .. }
        ));
        let rep = src.stop().await;
        assert!(!rep.ok());
        match rep.outcome {
            SourceOutcome::NoFrames { waited_ms, .. } => assert!(waited_ms >= 300, "{waited_ms}"),
            o => panic!("{o:?}"),
        }
        assert!(rep.verdict().starts_with("capture failed: no frames for"));
    })
    .await;
}

#[tokio::test]
async fn short_zero_frame_capture_is_still_no_frames() {
    guard(async {
        let fake = FakeCdp::start(FakeOpts::default()).await;
        let src = start(&fake).await;
        sleep(100).await;
        let rep = src.stop().await;
        assert!(matches!(rep.outcome, SourceOutcome::NoFrames { .. }));
        assert!(!rep.ok());
    })
    .await;
}

#[tokio::test]
async fn one_frame_then_silence_is_quiet_not_failure() {
    guard(async {
        let fake = FakeCdp::start(FakeOpts {
            frames: 1,
            ..Default::default()
        })
        .await;
        let src = start(&fake).await;
        sleep(600).await;
        assert!(matches!(src.health(), Health::Quiet { .. }));
        let rep = src.stop().await;
        assert!(rep.ok(), "{}", rep.verdict());
        assert!(rep.quiet_tail_ms.is_some());
        assert!(
            rep.verdict().contains("no new frames for"),
            "{}",
            rep.verdict()
        );
        assert!(rep.stats.longest_gap_ms < 300);
    })
    .await;
}

#[tokio::test]
async fn steady_frames_are_receiving_without_a_quiet_note() {
    guard(async {
        let fake = FakeCdp::start(FakeOpts {
            frames: 25,
            interval: Duration::from_millis(20),
            ..Default::default()
        })
        .await;
        let src = start(&fake).await;
        sleep(450).await;
        assert!(matches!(src.health(), Health::Receiving { .. }));
        let rep = src.stop().await;
        assert!(rep.ok());
        assert!(rep.quiet_tail_ms.is_none());
        assert!(!rep.verdict().contains("no new frames"));
    })
    .await;
}

#[tokio::test]
async fn hidden_event_is_named_and_sends_nothing() {
    guard(async {
        let fake = FakeCdp::start(FakeOpts::default()).await;
        let src = start(&fake).await;
        fake.push_raw(
            r#"{"method":"Page.screencastVisibilityChanged","params":{"visible":false}}"#
                .to_string(),
        );
        sleep(450).await;
        assert!(matches!(
            src.health(),
            Health::NoFrames { hidden: true, .. }
        ));
        let rep = src.stop().await;
        assert!(!rep.ok());
        assert!(rep.hidden);
        assert!(rep.verdict().contains("Chrome reported it hidden"));
        assert_eq!(
            fake.methods(),
            ["Page.enable", "Page.startScreencast", "Page.stopScreencast"]
        );
    })
    .await;
}
