//! Exit-path matrix tests (LIFE-01), declared from `hub.rs` so they can reuse
//! the fake-ffmpeg / FakeCdp rigs of the hub tests. No Chrome, no daemon: the
//! daemon and MCP hooks are one call of `finish_best_effort`, so the SAME
//! machine sequence is driven on a private `Hub`, and the literal hook lines
//! are pinned by reading the upstream sources. Unix only (fake ffmpeg is a
//! shell script); Windows is reasoned in `exit.rs`, never claimed tested.

use std::time::{Duration, Instant};

use super::env::{Attach, Lane};
use super::follow_tests::{
    guard as guard40, started, until, view, Rig, LONG, OK_SCRIPT as FOLLOW_OK,
};
use super::machine::{Hub, Knobs};
use super::machine_tests::{env, err, files_in, frames, guard, hub, okay, started_path, OK_SCRIPT};
use crate::record::encoder::test_util::Scratch;
use crate::record::exit::MATRIX;
use crate::record::hub::msg;
use crate::record::source::fake_cdp::{FakeCdp, FakeOpts};
use crate::record::Tabs;

const DAEMON: &str = include_str!("../cli/daemon.rs");
const BOOT: &str = include_str!("../mcp/server/boot.rs");
const SERVE: &str = include_str!("../mcp/server/serve.rs");
const CLI: &str = include_str!("../cli.rs");
const THIS: &str = include_str!("exit_tests.rs");
const PREFIX: &str = "previous recording ended with the browser: ";

fn fake(n: usize, off: f64) -> FakeOpts {
    FakeOpts {
        frames: n,
        interval: Duration::from_millis(120),
        ts_base: 1.7e9 + off,
        ..Default::default()
    }
}

/// start -> frames -> the surface goes away (`finish_orphan`, what every hook
/// calls) -> the next stop returns the prefixed report -> a new start works.
async fn orphan_cycle(tag: &str) {
    guard(async {
        let s = Scratch::new();
        let script = s.script(&format!("fake-enc-{tag}"), OK_SCRIPT);
        let fake1 = FakeCdp::start(frames(10)).await;
        let e = env(&s, &script, Attach::Ws(fake1.url.clone()));
        let mut h = hub();
        let path = started_path(&okay(h.start(&e, "exit").await));
        fake1.wait_emitter_done().await;
        h.finish_orphan().await;
        assert!(!h.is_active(), "never active forever");
        assert!(std::fs::metadata(&path).expect("file").len() > 0);
        let t = okay(h.stop(&e).await);
        assert!(t.starts_with(PREFIX), "{t}");
        assert!(t.contains("recording saved: "), "{t}");
        assert!(t.contains(&path.display().to_string()), "{t}");
        let fake2 = FakeCdp::start(frames(5)).await;
        let e2 = env(&s, &script, Attach::Ws(fake2.url.clone()));
        okay(h.start(&e2, "after").await);
        fake2.wait_emitter_done().await;
        okay(h.stop(&e2).await);
    })
    .await;
}

#[test]
fn hooks_present_in_daemon() {
    assert_eq!(DAEMON.matches("finish_best_effort").count(), 4);
    let lines: Vec<&str> = DAEMON.lines().collect();
    // The three relaunch/idle sites; the 4th is the post-loop exit (order
    // checked below) and the 5th the launch-failure cleanup, where no page
    // ever existed.
    let sites: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("move || b.shutdown()"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(sites.len(), 5, "{sites:?}");
    for &i in &sites[..3] {
        assert!(
            lines[i - 3..i]
                .iter()
                .any(|l| l.contains("finish_best_effort")),
            "line {} has no finish within 3 lines",
            i + 1
        );
    }
    // Post-loop: the finalize comes before the login snapshot.
    let cleanup = DAEMON
        .find("// Cleanup: persist live logins")
        .expect("cleanup");
    let tail = &DAEMON[cleanup - 200..];
    let fin = tail.find("finish_best_effort").expect("finish");
    let snap = tail[200..].find("logins::snapshot").expect("snapshot") + 200;
    assert!(fin < snap, "finalize must precede the snapshot");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_record_stop_mcp() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-rs-mcp", OK_SCRIPT);
        let f = FakeCdp::start(frames(10)).await;
        let e = env(&s, &script, Attach::Ws(f.url.clone()));
        let mut h = hub();
        let path = started_path(&okay(h.start(&e, "rs").await));
        f.wait_emitter_done().await;
        let r = okay(h.stop(&e).await);
        assert!(r.starts_with("recording saved: "), "{r}");
        assert!(std::fs::metadata(&path).expect("file").len() > 0);
        assert!(!h.is_active());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_record_stop_daemon() {
    // Same hub path as MCP (one hub per process, same `Hub::stop`); the lane
    // differs only in who calls it. Second round on the same hub: a stop does
    // not poison the next recording.
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-rs-d", OK_SCRIPT);
        let mut h = hub();
        for n in ["one", "two"] {
            let f = FakeCdp::start(frames(8)).await;
            let e = env(&s, &script, Attach::Ws(f.url.clone()));
            okay(h.start(&e, n).await);
            f.wait_emitter_done().await;
            assert!(okay(h.stop(&e).await).starts_with("recording saved: "));
        }
        assert_eq!(files_in(&s.0.join("recordings")).len(), 2);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_stdin_eof_mcp() {
    // serve.rs breaks out of the loop on EOF into `shutdown_browser`.
    assert!(BOOT.contains("pub(super) async fn shutdown_browser"));
    let i = BOOT.find("async fn shutdown_browser").expect("fn");
    assert!(BOOT[i..i + 300].contains("finish_best_effort"));
    assert!(SERVE.matches("shutdown_browser(b).await").count() >= 4);
    orphan_cycle("eof").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_signal_mcp() {
    assert!(BOOT.contains("SignalKind::terminate()") && BOOT.contains("SignalKind::interrupt()"));
    assert!(SERVE.contains("wait_for_shutdown_signal"));
    orphan_cycle("sig-mcp").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_signal_daemon() {
    assert!(
        DAEMON.contains("SignalKind::terminate()") && DAEMON.contains("SignalKind::interrupt()")
    );
    orphan_cycle("sig-d").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_idle_mcp() {
    // Idle shutdown in serve.rs ends in `shutdown_browser` (finish hook).
    assert!(SERVE.contains("shutting down Chrome to save memory"));
    orphan_cycle("idle-mcp").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_idle_daemon() {
    assert!(DAEMON.contains("idle timeout ("));
    orphan_cycle("idle-d").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_relaunch_mcp() {
    orphan_cycle("rel-mcp").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_relaunch_daemon() {
    orphan_cycle("rel-d").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_relaunch_daemon_dead_source() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-rel-dead", OK_SCRIPT);
        let f = FakeCdp::start(frames(5)).await;
        let e = env(&s, &script, Attach::Ws(f.url.clone()));
        let mut h = hub();
        let path = started_path(&okay(h.start(&e, "dead").await));
        f.wait_emitter_done().await;
        f.close_now();
        tokio::time::sleep(Duration::from_millis(700)).await;
        h.finish_orphan().await;
        assert!(!h.is_active());
        let t = err(h.stop(&e).await);
        assert!(t.starts_with(PREFIX), "{t}");
        assert!(t.contains("recording not clean"), "{t}");
        assert!(path.exists(), "partial file kept");
        let f2 = FakeCdp::start(frames(5)).await;
        let e2 = env(&s, &script, Attach::Ws(f2.url.clone()));
        okay(h.start(&e2, "later").await);
        f2.wait_emitter_done().await;
        okay(h.stop(&e2).await);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_bladebro_stop_daemon() {
    // Client wait (daemon.rs stop read timeout) must exceed the finish bound:
    // FINISH_TIMEOUT 10 s + kill reap 5 s + stderr drain 1 s.
    assert!(DAEMON.contains("set_read_timeout(Some(std::time::Duration::from_secs(30)))"));
    let worst = crate::record::encoder::FINISH_TIMEOUT.as_secs() + 5 + 1;
    assert!(worst < 30, "{worst}");
    // The bound really holds: an encoder that ignores EOF is killed and
    // reported unfinalized well inside the shortened finish timeout + reap.
    guard(async {
        let s = Scratch::new();
        let script = s.script(
            "fake-enc-stop-hang",
            r#"for a; do out="$a"; done; : > "$out"; exec sleep 4242.7"#,
        );
        let f = FakeCdp::start(frames(5)).await;
        let e = env(&s, &script, Attach::Ws(f.url.clone()));
        let mut h = Hub::with_knobs(Knobs {
            stop_grace: Some(Duration::from_millis(100)),
            finish_timeout: Some(Duration::from_millis(300)),
            ..Default::default()
        });
        okay(h.start(&e, "hang").await);
        f.wait_emitter_done().await;
        let t0 = Instant::now();
        h.finish_orphan().await;
        assert!(t0.elapsed() < Duration::from_secs(8), "{:?}", t0.elapsed());
        let t = err(h.stop(&e).await);
        assert!(t.starts_with(PREFIX), "{t}");
        assert!(!h.is_active());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn row_tab_close_follow() {
    guard40(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::with("tcf", &a, FOLLOW_OK, Tabs::Follow, LONG).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        a.close_now();
        r.set(vec![view("B", &b.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        *r.env.tab.lock().expect("tab") = "B".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved: "), "{t}");
        assert!(t.contains("tab A gone at "), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn row_per_tab_close() {
    guard40(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let c = FakeCdp::start(fake(400, 6.0)).await;
        let mut r = Rig::with("ptc", &a, FOLLOW_OK, Tabs::PerTab, LONG).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![
            view("C", &c.url),
            view("B", &b.url),
            view("A", &a.url),
        ]);
        until("C started", || started(&c)).await;
        assert_eq!(r.encoders_alive(), 3);
        // B closes while the agent is on C: only B's file is finalized.
        r.set(vec![view("C", &c.url), view("A", &a.url)]);
        until("B's ffmpeg finished", || r.encoders_alive() == 2).await;
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved (per-tab, 3 files): "), "{t}");
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_oneshot_refused() {
    assert!(CLI.contains("hub::set_ephemeral()"));
    let s = Scratch::new();
    let script = s.script("fake-enc-oneshot", OK_SCRIPT);
    let mut e = env(&s, &script, Attach::Ws("ws://127.0.0.1:1/x".into()));
    e.lane = Lane::Ephemeral;
    let mut h = hub();
    let t = err(h.start(&e, "x").await);
    assert!(t.contains("long-lived session"), "{t}");
    assert_eq!(e.ffmpeg_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(
        !s.0.join("recordings").exists(),
        "no file, no ffmpeg spawned"
    );
    assert!(!h.is_active());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn row_kill_ffmpeg() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-killff", OK_SCRIPT);
        let f = FakeCdp::start(frames(30)).await;
        let e = env(&s, &script, Attach::Ws(f.url.clone()));
        let mut h = hub();
        let path = started_path(&okay(h.start(&e, "kill").await));
        tokio::time::sleep(Duration::from_millis(600)).await;
        // Only the scratch script of THIS test (unique name) is signalled.
        let o = std::process::Command::new("pgrep")
            .args(["-f", &script.display().to_string()])
            .output()
            .expect("pgrep");
        let pids: Vec<i32> = String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter_map(|l| l.trim().parse().ok())
            .collect();
        assert!(!pids.is_empty(), "fake encoder not found");
        for p in pids {
            // SAFETY: pid of the fake encoder this test spawned.
            unsafe { libc::kill(p, libc::SIGKILL) };
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        let t = err(h.stop(&e).await);
        assert!(!t.starts_with("recording saved"), "{t}");
        assert!(t.contains(&path.display().to_string()), "{t}");
        assert!(!h.is_active(), "hub cleared");
        let f2 = FakeCdp::start(frames(5)).await;
        let e2 = env(&s, &script, Attach::Ws(f2.url.clone()));
        okay(h.start(&e2, "again").await);
        f2.wait_emitter_done().await;
        okay(h.stop(&e2).await);
    })
    .await;
}

#[test]
fn matrix_complete() {
    for r in MATRIX {
        let ok = ["live:", "n/a:", "planned: 09-0"]
            .iter()
            .any(|m| r.test.starts_with(m))
            || THIS.contains(&format!("async fn {}(", r.test))
            || THIS.contains(&format!("fn {}(", r.test));
        assert!(
            ok,
            "row `{} / {}` has no test or marker: {}",
            r.path, r.lane, r.test
        );
    }
}

#[test]
fn matrix_texts_scrubbed() {
    for r in MATRIX {
        for t in [r.path, r.lane, r.outcome] {
            assert_eq!(msg::scrub(t), t);
        }
    }
}
