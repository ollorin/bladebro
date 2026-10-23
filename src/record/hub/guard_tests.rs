//! Recording guard scenarios (LIFE-02): browser killed, relaunch, lost
//! recorder connection, ffmpeg killed, stale guard. Fake ffmpeg (`cat`),
//! FakeCdp and a scripted tab list: no Chrome. Unix only.

use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use super::env::Attach;
use super::follow_tests::{
    err, fake, guard as guard40, okay, started, until, view, Rig, LONG, OK_SCRIPT,
};
use super::guard;
use super::machine::{Hub, Interrupt};
use super::machine_tests::{env, frames, guard as guard20, OK_SCRIPT as M_OK};
use crate::record::encoder::test_util::{pid_gone, Scratch};
use crate::record::source::fake_cdp::FakeCdp;
use crate::record::Tabs;

const TICK: Duration = Duration::from_millis(50);
const GUARD_SRC: &str = include_str!("guard.rs");

/// Move the rig's hub behind the mutex the guard needs.
fn shared(r: &mut Rig) -> Arc<Mutex<Hub>> {
    Arc::new(Mutex::new(std::mem::replace(&mut r.hub, Hub::new())))
}

async fn active(h: &Arc<Mutex<Hub>>) -> bool {
    h.lock().await.is_active()
}

async fn until_idle(h: &Arc<Mutex<Hub>>, what: &str) {
    for _ in 0..600 {
        if !active(h).await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("hub still active: {what}");
}

fn pids_of(script: &std::path::Path) -> Vec<u32> {
    let o = Command::new("pgrep")
        .args(["-f", &script.display().to_string()])
        .output()
        .expect("pgrep");
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

async fn start_guarded(
    r: &Rig,
    h: &Arc<Mutex<Hub>>,
) -> (std::path::PathBuf, tokio::task::JoinHandle<()>) {
    let t = okay(h.lock().await.start(&r.env, "g").await);
    let gen = h.lock().await.active_gen().expect("gen");
    let rest = t.split_once("): ").map_or(t.as_str(), |(_, r)| r);
    let rest = t.split("recording started: ").nth(1).unwrap_or(rest);
    let path = std::path::PathBuf::from(rest.split(" (tab ").next().expect("path"));
    (path, guard::spawn(h.clone(), gen, TICK))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chrome_killed_no_calls() {
    guard40(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let mut r = Rig::with("gk", &a, OK_SCRIPT, Tabs::Follow, LONG).await;
        let h = shared(&mut r);
        let (path, _g) = start_guarded(&r, &h).await;
        until("screencast", || started(&a)).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        // Chrome dies: the page socket closes and the tab list stops answering.
        a.close_now();
        r.fail(true);
        until_idle(&h, "browser killed").await;
        assert!(std::fs::metadata(&path).expect("file").len() > 0);
        assert_eq!(r.encoders_alive(), 0, "ffmpeg finalized and gone");
        let t = err(h.lock().await.stop(&r.env).await);
        assert!(
            t.starts_with("previous recording interrupted: browser connection lost at "),
            "{t}"
        );
        assert!(t.contains("partial file kept: "), "{t}");
        assert!(t.contains(&path.display().to_string()), "{t}");
        assert!(!t.contains("recording saved"), "{t}");
        assert!(!active(&h).await);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn relaunch_mid_recording() {
    guard20(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-gr", M_OK);
        let f1 = FakeCdp::start(frames(30)).await;
        let e1 = env(&s, &script, Attach::Ws(f1.url.clone()));
        let h = Arc::new(Mutex::new(Hub::new()));
        okay(h.lock().await.start(&e1, "one").await);
        let gen = h.lock().await.active_gen().expect("gen");
        let g1 = guard::spawn(h.clone(), gen, TICK);
        tokio::time::sleep(Duration::from_millis(300)).await;
        // The 09-01 relaunch hook.
        h.lock().await.finish_orphan().await;
        tokio::time::timeout(Duration::from_secs(2), g1)
            .await
            .expect("guard 1 exits")
            .expect("join");
        let f2 = FakeCdp::start(frames(10)).await;
        let e2 = env(&s, &script, Attach::Ws(f2.url.clone()));
        let t = okay(h.lock().await.start(&e2, "two").await);
        assert!(
            t.starts_with("previous recording ended with the browser: "),
            "{t}"
        );
        assert!(h.lock().await.is_active(), "no second finalize by guard 1");
        okay(h.lock().await.stop(&e2).await);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lost_cdp_only() {
    // Recorder connection dropped, tab list still fine (Fixed probe: no
    // rebind in this rig). The guard does not misreport a browser loss; the
    // hub is never active forever: stop ends it and a new start works.
    guard20(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-gl", M_OK);
        let f1 = FakeCdp::start(frames(50)).await;
        let e1 = env(&s, &script, Attach::Ws(f1.url.clone()));
        let h = Arc::new(Mutex::new(Hub::new()));
        okay(h.lock().await.start(&e1, "one").await);
        let gen = h.lock().await.active_gen().expect("gen");
        let _g = guard::spawn(h.clone(), gen, TICK);
        tokio::time::sleep(Duration::from_millis(300)).await;
        f1.close_now();
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(
            active(&h).await,
            "a lost socket alone is not a browser loss"
        );
        let _ = h.lock().await.stop(&e1).await;
        assert!(!active(&h).await);
        let f2 = FakeCdp::start(frames(5)).await;
        let e2 = env(&s, &script, Attach::Ws(f2.url.clone()));
        okay(h.lock().await.start(&e2, "two").await);
        f2.wait_emitter_done().await;
        okay(h.lock().await.stop(&e2).await);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn kill_ffmpeg() {
    guard40(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let mut r = Rig::with("gf", &a, OK_SCRIPT, Tabs::Follow, LONG).await;
        let h = shared(&mut r);
        let (_path, _g) = start_guarded(&r, &h).await;
        until("screencast", || started(&a)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let pids = pids_of(&r.script);
        assert!(!pids.is_empty(), "fake ffmpeg running");
        for p in &pids {
            // SAFETY: SIGKILL to the fake encoder this test started.
            unsafe { libc::kill(*p as libc::pid_t, libc::SIGKILL) };
        }
        until_idle(&h, "ffmpeg killed").await;
        let t = err(h.lock().await.stop(&r.env).await);
        assert!(
            t.starts_with("previous recording interrupted: encoder exited at "),
            "{t}"
        );
        assert!(!t.contains("recording saved"), "{t}");
        for p in pids {
            assert!(pid_gone(p), "fake encoder {p} reaped");
        }
        // The hub is reusable.
        let s = Scratch::new();
        let script = s.script("fake-enc-gf2", M_OK);
        let f2 = FakeCdp::start(frames(5)).await;
        let e2 = env(&s, &script, Attach::Ws(f2.url.clone()));
        okay(h.lock().await.start(&e2, "again").await);
        f2.wait_emitter_done().await;
        okay(h.lock().await.stop(&e2).await);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stale_guard() {
    guard20(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-gs", M_OK);
        let f1 = FakeCdp::start(frames(10)).await;
        let e1 = env(&s, &script, Attach::Ws(f1.url.clone()));
        let h = Arc::new(Mutex::new(Hub::new()));
        okay(h.lock().await.start(&e1, "one").await);
        let g1_gen = h.lock().await.active_gen().expect("gen");
        // Slow tick: it fires only after generation 2 runs.
        let g1 = guard::spawn(h.clone(), g1_gen, Duration::from_millis(600));
        f1.wait_emitter_done().await;
        okay(h.lock().await.stop(&e1).await);
        let f2 = FakeCdp::start(frames(40)).await;
        let e2 = env(&s, &script, Attach::Ws(f2.url.clone()));
        okay(h.lock().await.start(&e2, "two").await);
        let g2_gen = h.lock().await.active_gen().expect("gen");
        assert_ne!(g1_gen, g2_gen);
        // A stale interrupt does nothing.
        assert!(
            !h.lock()
                .await
                .interrupt_if(g1_gen, Interrupt::BrowserLost)
                .await
        );
        tokio::time::timeout(Duration::from_secs(3), g1)
            .await
            .expect("stale guard exits")
            .expect("join");
        assert!(active(&h).await, "generation 2 still recording");
        let t = okay(h.lock().await.stop(&e2).await);
        assert!(t.starts_with("recording saved: "), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn normal_stop_guard_exits() {
    guard20(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-gn", M_OK);
        let f = FakeCdp::start(frames(8)).await;
        let e = env(&s, &script, Attach::Ws(f.url.clone()));
        let h = Arc::new(Mutex::new(Hub::new()));
        okay(h.lock().await.start(&e, "n").await);
        let gen = h.lock().await.active_gen().expect("gen");
        let g = guard::spawn(h.clone(), gen, TICK);
        f.wait_emitter_done().await;
        okay(h.lock().await.stop(&e).await);
        tokio::time::sleep(TICK * 3).await;
        assert!(g.is_finished(), "guard exits after a normal stop");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn per_tab_interrupted() {
    guard40(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::with("gp", &a, OK_SCRIPT, Tabs::PerTab, LONG).await;
        let h = shared(&mut r);
        let (path, _g) = start_guarded(&r, &h).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.fail(true);
        until_idle(&h, "per-tab browser lost").await;
        assert_eq!(r.encoders_alive(), 0, "every file's ffmpeg finalized");
        let t = err(h.lock().await.stop(&r.env).await);
        assert!(
            t.starts_with("previous recording interrupted: browser connection lost at "),
            "{t}"
        );
        assert!(t.contains(&path.display().to_string()), "{t}");
        assert_eq!(t.matches(" (bytes ").count(), 2, "both files listed: {t}");
    })
    .await;
}

#[test]
fn browsing_unaffected() {
    assert!(!GUARD_SRC.contains("crate::page"));
    assert!(!GUARD_SRC.contains("CdpSession"));
    assert!(!GUARD_SRC.contains("crate::cdp"));
}

#[test]
fn texts_scrubbed() {
    use super::msg::{interrupted, reason_browser_lost, reason_encoder_dead, scrub};
    for r in [reason_browser_lost(), reason_encoder_dead()] {
        let t = interrupted(
            r,
            3.2,
            "/tmp/x.webm",
            "recording saved: /tmp/x.webm (frames 3)",
        );
        assert_eq!(scrub(&t), t);
        assert!(t.starts_with("previous recording interrupted: "), "{t}");
        assert!(!t.contains("recording saved"), "{t}");
        assert!(t.contains("finalized: "), "{t}");
    }
}
