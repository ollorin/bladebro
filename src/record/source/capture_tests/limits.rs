//! SC2 scenarios: bounded queue, counted drops, a reader that never blocks.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use super::super::fake_cdp::FakeOpts;
use super::*;

/// try_send into a bounded queue whose receiver is never polled.
struct StuckSink {
    tx: mpsc::Sender<Vec<u8>>,
}
impl FrameSink for StuckSink {
    fn send_frame(&self, jpeg: Vec<u8>, _: u64) -> bool {
        self.tx.try_send(jpeg).is_ok()
    }
}

#[tokio::test]
async fn stuck_consumer_is_bounded_acked_and_never_blocks_the_client() {
    guard(async {
        let fake = fake_with(FakeOpts {
            frames: 2000,
            interval: Duration::from_millis(1),
            ..Default::default()
        })
        .await;
        let (tx, _rx_never_polled) = mpsc::channel(16);
        let src = start(&fake, 15, Arc::new(StuckSink { tx })).await;
        let client = src.client_for_test();
        // While frames flood, the recorder's own connection stays responsive.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let t = Instant::now();
        client
            .send_with_timeout("Page.enable", None, Duration::from_secs(2))
            .await
            .expect("command during flood");
        assert!(
            t.elapsed() < Duration::from_millis(250),
            "{:?}",
            t.elapsed()
        );
        fake.wait_emitter_done().await;
        let rep = src.stop().await;
        let s = &rep.stats;
        assert_eq!(s.forwarded, 16, "exactly the queue bound is accepted");
        assert!(s.queue_dropped >= 1);
        assert_eq!(s.frames_received, s.acks_sent);
        assert_eq!(
            s.frames_received,
            s.forwarded + s.queue_dropped + s.capped + s.bad_frames
        );
    })
    .await;
}

#[tokio::test]
async fn slow_consumer_accounting_identity_holds() {
    guard(async {
        let fake = fake_with(FakeOpts {
            frames: 200,
            interval: Duration::from_millis(5),
            ..Default::default()
        })
        .await;
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(2);
        let drain = tokio::spawn(async move {
            while rx.recv().await.is_some() {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        });
        let began = Instant::now();
        let src = start(&fake, 15, Arc::new(StuckSink { tx })).await;
        fake.wait_emitter_done().await;
        let rep = src.stop().await;
        let s = &rep.stats;
        assert!(began.elapsed() < Duration::from_secs(5));
        assert_eq!(s.frames_received, 200);
        assert_eq!(
            s.frames_received,
            s.forwarded + s.queue_dropped + s.capped + s.bad_frames
        );
        assert!(s.queue_dropped >= 1);
        drain.abort();
    })
    .await;
}

#[tokio::test]
async fn flood_keeps_the_fps_cadence_so_the_timer_is_not_starved() {
    guard(async {
        // Pins the biased select: with frames always ready, held frames still
        // reach the sink at the gate's cadence (arrival path + timer path).
        let fake = fake_with(FakeOpts {
            frames: 600,
            interval: Duration::from_millis(2),
            ..Default::default()
        })
        .await;
        let sink = Arc::new(CountingSink::default());
        let started = Instant::now();
        let src = start(&fake, 10, sink.clone()).await;
        fake.wait_emitter_done().await;
        let elapsed = started.elapsed();
        let rep = src.stop().await;
        let floor = (elapsed.as_millis() / 100).saturating_sub(1) as u64;
        assert!(
            rep.stats.forwarded >= floor,
            "{} < {floor}",
            rep.stats.forwarded
        );
        assert_eq!(rep.stats.frames_received, 600);
    })
    .await;
}
