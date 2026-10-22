//! SC1 and SAFE-01 scenarios: ack-first, accounting, the wire allowlist.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::super::fake_cdp::{frame_event, frame_ts, FakeOpts};
use super::*;

#[tokio::test]
async fn every_frame_is_acked_with_its_session_id() {
    guard(async {
        let fake = fake_with(FakeOpts {
            frames: 500,
            interval: Duration::from_millis(2),
            screencast_session: 42,
            max_in_flight: Some(2),
            ..Default::default()
        })
        .await;
        let sink = Arc::new(CountingSink::default());
        let src = start(&fake, 15, sink.clone()).await;
        fake.wait_emitter_done().await;
        let rep = src.stop().await;
        let s = &rep.stats;
        assert!(rep.ok(), "{}", rep.verdict());
        assert_eq!(s.frames_received, 500);
        assert_eq!(s.acks_sent, 500);
        assert_eq!(s.acks_ok, 500);
        assert_eq!(s.ack_errors, 0);
        let acks = fake.ack_ids();
        assert_eq!(acks.len(), 500);
        assert!(acks.iter().all(|&a| a == 42));
        assert_eq!(s.forwarded + s.capped, 500);
        assert_eq!(s.queue_dropped, 0);
        assert_eq!(sink.n.load(Ordering::SeqCst), s.forwarded);
    })
    .await;
}

#[tokio::test]
async fn acks_are_sent_even_when_the_sink_and_decode_fail() {
    guard(async {
        // 80 ms apart: every frame clears the 66 ms gate and reaches forward().
        let fake = fake_with(FakeOpts {
            frames: 12,
            interval: Duration::from_millis(80),
            garbage_every: Some(2),
            ..Default::default()
        })
        .await;
        struct Refuse;
        impl FrameSink for Refuse {
            fn send_frame(&self, _: Vec<u8>, _: u64) -> bool {
                false
            }
        }
        let src = start(&fake, 15, Arc::new(Refuse)).await;
        fake.wait_emitter_done().await;
        let rep = src.stop().await;
        let s = &rep.stats;
        assert_eq!(s.acks_sent, s.frames_received);
        assert_eq!(s.frames_received, 12);
        assert_eq!(s.bad_frames, 6);
        assert_eq!(s.queue_dropped, 6);
        assert_eq!(s.forwarded, 0);
        assert!(rep.verdict().contains(", 6 unreadable"));
    })
    .await;
}

#[tokio::test]
async fn only_the_four_screencast_methods_reach_the_wire() {
    guard(async {
        let fake = fake_with(FakeOpts {
            frames: 20,
            ..Default::default()
        })
        .await;
        let src = start(&fake, 15, Arc::new(CountingSink::default())).await;
        fake.wait_emitter_done().await;
        let rep = src.stop().await;
        let all = fake.methods();
        let allowed = [
            "Page.enable",
            "Page.startScreencast",
            "Page.screencastFrameAck",
            "Page.stopScreencast",
        ];
        assert!(all.iter().all(|m| allowed.contains(&m.as_str())), "{all:?}");
        let mut dedup: Vec<&str> = vec![];
        for m in &all {
            if !dedup.contains(&m.as_str()) {
                dedup.push(m);
            }
        }
        assert_eq!(dedup, allowed);
        let pos = |m: &str| all.iter().position(|x| x == m).expect("present");
        assert!(pos("Page.enable") < pos("Page.startScreencast"));
        let start_params = fake.log.lock().expect("log")[pos("Page.startScreencast")]
            .1
            .clone();
        assert_eq!(
            start_params,
            json!({"format":"jpeg","quality":70,"maxWidth":1280,"maxHeight":720,"everyNthFrame":1})
        );
        // The counters agree with the wire.
        let count = |m: &str| all.iter().filter(|x| *x == m).count() as u64;
        let sent: std::collections::HashMap<_, _> =
            rep.stats.methods_sent.iter().cloned().collect();
        for m in allowed {
            assert_eq!(sent.get(m).copied(), Some(count(m)), "{m}");
        }
    })
    .await;
}

#[tokio::test]
async fn held_trailing_frame_is_flushed_by_the_timer() {
    guard(async {
        let iv = Duration::from_millis(10);
        let fake = fake_with(FakeOpts {
            frames: 3,
            interval: iv,
            ..Default::default()
        })
        .await;
        let sink = Arc::new(CountingSink::default());
        let src = start(&fake, 10, sink.clone()).await;
        fake.wait_emitter_done().await;
        // Silence after the burst: only the timer can deliver frame 2.
        tokio::time::sleep(Duration::from_millis(400)).await;
        let seen = sink.seen.lock().expect("seen").clone();
        assert_eq!(seen.len(), 2, "frame 0 now, newest frame later");
        let ms = |i| (frame_ts(i, iv) * 1000.0).round() as u64;
        assert_eq!((seen[0].1, seen[1].1), (ms(0), ms(2)));
        let gap = seen[1].0.duration_since(seen[0].0);
        assert!(gap >= Duration::from_millis(60), "timer slot, got {gap:?}");
        assert_eq!(src.stats().capped, 1);
        let rep = src.stop().await;
        assert_eq!(rep.stats.forwarded, 2);
    })
    .await;
}

#[tokio::test]
async fn stripped_frame_reuses_the_cached_ack_id() {
    guard(async {
        let fake = fake_with(FakeOpts::default()).await;
        let src = start(&fake, 15, Arc::new(CountingSink::default())).await;
        fake.push_raw(frame_event("/9j/", 9, 1.7e9));
        fake.push_raw(json!({"method":"Page.screencastFrame","params":{}}).to_string());
        tokio::time::sleep(Duration::from_millis(200)).await;
        let rep = src.stop().await;
        assert_eq!(fake.ack_ids(), vec![9, 9]);
        assert_eq!(rep.stats.acked_with_cached_id, 1);
        assert_eq!(rep.stats.unacked, 0);
    })
    .await;
}

/// 06-04 gap 4: frames the bus overwrote were never acked, and Chrome stops
/// after 2 unacked. A Lagged event must ack (bounded) with the cached id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lagged_bus_still_acks_and_the_wire_stays_allowlisted() {
    guard(async {
        struct BlockOnce(std::sync::Once, Arc<std::sync::atomic::AtomicBool>);
        impl FrameSink for BlockOnce {
            fn send_frame(&self, _: Vec<u8>, _: u64) -> bool {
                // Stall the recorder's loop (not the CDP reader) once, until
                // released, so the bus (16384) overflows behind it.
                self.0.call_once(|| {
                    let until = std::time::Instant::now() + Duration::from_secs(15);
                    while !self.1.load(Ordering::SeqCst) && std::time::Instant::now() < until {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                });
                true
            }
        }
        let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fake = fake_with(FakeOpts {
            frames: 1,
            screencast_session: 7,
            ..Default::default()
        })
        .await;
        let src = start(
            &fake,
            15,
            Arc::new(BlockOnce(std::sync::Once::new(), release.clone())),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut probe = crate::cdp::CdpSession::root(src.client_for_test()).subscribe();
        for i in 0..19_000 {
            fake.push_raw(frame_event("!!!", 7, 1.7e9 + 1.0 + i as f64 * 0.001));
        }
        // A second, prompt subscriber counts what the reader has put on the
        // bus; release the stalled loop once that is past the capacity.
        let mut seen = 0u32;
        let _ = tokio::time::timeout(Duration::from_secs(12), async {
            while seen < 18_000 {
                if probe.recv().await.is_ok() {
                    seen += 1;
                }
            }
        })
        .await;
        release.store(true, Ordering::SeqCst);
        let t0 = std::time::Instant::now();
        loop {
            let s = src.stats();
            if s.lag_acks > 0 && fake.ack_ids().len() as u64 == s.acks_sent + s.lag_acks {
                break;
            }
            assert!(t0.elapsed() < Duration::from_secs(14), "{s:?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let rep = src.stop().await;
        let s = &rep.stats;
        assert!(s.lost > 0, "the bus really lagged: {s:?}");
        assert!(s.lag_acks >= 2 && s.lag_acks <= 2 * s.lost);
        // Seen frames keep the strict identity; lag acks are separate.
        assert_eq!(s.frames_received, s.acks_sent);
        let acks = fake.ack_ids();
        assert_eq!(acks.len() as u64, s.acks_sent + s.lag_acks);
        assert!(acks.iter().all(|&a| a == 7), "cached id used");
        let allowed = [
            "Page.enable",
            "Page.startScreencast",
            "Page.screencastFrameAck",
            "Page.stopScreencast",
        ];
        assert!(fake.methods().iter().all(|m| allowed.contains(&m.as_str())));
    })
    .await;
}
