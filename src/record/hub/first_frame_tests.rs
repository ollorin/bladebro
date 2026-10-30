//! record-stop's bounded first-frame wait (Phase 11.1) over the fake ffmpeg
//! script and FakeCdp (no Chrome): a late first frame is awaited and saved,
//! a missing one keeps the honest no-frames verdict, a recording with frames
//! does not wait, and the exit paths (extras None) never wait.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::env::Attach;
use super::follow_tests::{fake, num_after, Rig, LONG};
use super::machine::{Hub, Interrupt, Knobs, Limit};
use super::machine_tests::{env, err, guard, okay, started_path, OK_SCRIPT};
use crate::record::encoder::test_util::Scratch;
use crate::record::source::fake_cdp::{FakeCdp, FakeOpts};
use crate::record::Tabs;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Production timings except a short stop grace and the given first-frame wait.
fn hub_waiting(wait: Duration) -> Hub {
    Hub::with_knobs(Knobs {
        stop_grace: Some(ms(100)),
        finish_timeout: Some(Duration::from_secs(10)),
        first_frame_wait: Some(wait),
        ..Default::default()
    })
}

/// Chrome's first frame `first_ms` after startScreencast, then 4 more.
fn late(first_ms: u64) -> FakeOpts {
    FakeOpts {
        frames: 5,
        interval: ms(120),
        first_delay: ms(first_ms),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn late_first_frame_is_awaited_and_saved() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ff-late", OK_SCRIPT);
        let fake = FakeCdp::start(late(600)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub_waiting(Duration::from_secs(2));
        let path = started_path(&okay(h.start(&e, "late").await));
        let t0 = Instant::now();
        let t = okay(h.stop(&e).await);
        let took = t0.elapsed();
        assert!(t.starts_with("recording saved: "), "{t}");
        assert!(num_after(&t, "frames ") >= 1.0, "{t}");
        assert!(std::fs::metadata(&path).expect("file").len() > 0, "{t}");
        assert!(
            !t.contains("(no frames)") && !t.contains("agent was on tab"),
            "{t}"
        );
        assert!(took >= ms(500) && took < ms(2600), "{took:?} {t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_frame_in_the_wait_keeps_the_honest_verdict() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ff-none", OK_SCRIPT);
        let fake = FakeCdp::start(FakeOpts::default()).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub_waiting(ms(800));
        okay(h.start(&e, "none").await);
        let t0 = Instant::now();
        let t = err(h.stop(&e).await);
        let took = t0.elapsed();
        assert!(t.contains("no frames reached the encoder"), "{t}");
        assert!(!t.contains("agent was on tab"), "{t}");
        assert!(took >= ms(750) && took < ms(3000), "{took:?} {t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recording_with_frames_stops_without_waiting() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ff-busy", OK_SCRIPT);
        let fake = FakeCdp::start(FakeOpts {
            frames: 20,
            interval: ms(120),
            ..Default::default()
        })
        .await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub_waiting(Duration::from_secs(3));
        okay(h.start(&e, "busy").await);
        let deadline = Instant::now() + Duration::from_secs(5);
        while fake.state.emitted.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
            tokio::time::sleep(ms(10)).await;
        }
        tokio::time::sleep(ms(200)).await;
        let t0 = Instant::now();
        let t = okay(h.stop(&e).await);
        let took = t0.elapsed();
        assert!(t.starts_with("recording saved: "), "{t}");
        assert!(took < ms(1500), "{took:?} {t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_wait_after_the_source_no_frames_verdict() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ff-nfv", OK_SCRIPT);
        let fake = FakeCdp::start(FakeOpts::default()).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = Hub::with_knobs(Knobs {
            stop_grace: Some(ms(100)),
            finish_timeout: Some(Duration::from_secs(10)),
            no_frames_after: Some(ms(300)),
            first_frame_wait: Some(Duration::from_secs(3)),
            ..Default::default()
        });
        okay(h.start(&e, "nfv").await);
        tokio::time::sleep(ms(500)).await;
        let t0 = Instant::now();
        let t = err(h.stop(&e).await);
        let took = t0.elapsed();
        assert!(t.contains("no frames"), "{t}");
        assert!(took < ms(1500), "{took:?} {t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_wait_when_the_tab_connection_ended() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ff-closed", OK_SCRIPT);
        let fake = FakeCdp::start(FakeOpts::default()).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub_waiting(Duration::from_secs(3));
        okay(h.start(&e, "closed").await);
        fake.close_now();
        tokio::time::sleep(ms(400)).await;
        let t0 = Instant::now();
        let _ = err(h.stop(&e).await);
        let took = t0.elapsed();
        assert!(took < ms(1500), "{took:?}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exit_paths_never_wait() {
    guard(async {
        // finish_orphan: the browser went away with a silent recording.
        let s = Scratch::new();
        let script = s.script("fake-enc-ff-orphan", OK_SCRIPT);
        let fake = FakeCdp::start(FakeOpts::default()).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub_waiting(Duration::from_secs(3));
        okay(h.start(&e, "orphan").await);
        let t0 = Instant::now();
        h.finish_orphan().await;
        let took = t0.elapsed();
        assert!(took < ms(1500), "finish_orphan {took:?}");
        let o = h.orphaned_text().expect("orphaned text");
        assert!(o.contains("no frames"), "{o}");

        // interrupt_if: a lost browser ends the recording without a wait.
        let s2 = Scratch::new();
        let script2 = s2.script("fake-enc-ff-interrupt", OK_SCRIPT);
        let fake2 = FakeCdp::start(FakeOpts::default()).await;
        let e2 = env(&s2, &script2, Attach::Ws(fake2.url.clone()));
        let mut h2 = hub_waiting(Duration::from_secs(3));
        okay(h2.start(&e2, "interrupt").await);
        let gen = h2.active_gen().expect("gen");
        let t0 = Instant::now();
        let hit = h2.interrupt_if(gen, Interrupt::BrowserLost).await;
        let took = t0.elapsed();
        assert!(hit, "interrupt_if did not end the recording");
        assert!(took < ms(1500), "interrupt_if {took:?}");

        // stop_on_limit: the max duration ends the recording without a wait.
        let s3 = Scratch::new();
        let script3 = s3.script("fake-enc-ff-limit", OK_SCRIPT);
        let fake3 = FakeCdp::start(FakeOpts::default()).await;
        let e3 = env(&s3, &script3, Attach::Ws(fake3.url.clone()));
        let mut h3 = hub_waiting(Duration::from_secs(3));
        okay(h3.start(&e3, "limit").await);
        let gen = h3.active_gen().expect("gen");
        let t0 = Instant::now();
        let hit = h3.stop_on_limit(gen, Limit::MaxDuration).await;
        let took = t0.elapsed();
        assert!(hit, "stop_on_limit did not end the recording");
        assert!(took < ms(1500), "stop_on_limit {took:?}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn per_tab_late_first_frame_is_awaited() {
    guard(async {
        let ff = FakeCdp::start(FakeOpts {
            first_delay: ms(600),
            ..fake(5, 0.0)
        })
        .await;
        // Production default wait: the knob is not set on this rig's hub.
        let mut r = Rig::with("ff-pt", &ff, OK_SCRIPT, Tabs::PerTab, LONG).await;
        r.start().await;
        let t0 = Instant::now();
        let t = okay(r.hub.stop(&r.env).await);
        let took = t0.elapsed();
        assert!(t.starts_with("recording saved (per-tab"), "{t}");
        assert!(num_after(&t, "frames ") >= 1.0, "{t}");
        assert!(took >= ms(500) && took < ms(2600), "{took:?} {t}");
    })
    .await;
}
