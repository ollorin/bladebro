//! Popup following (CAPT-06) over fake CDP: the opener rule, the settle, the
//! return to the opener and the stop note. Knobs: poll 30 ms, confirm 10 ms,
//! popup settle 150 ms. Every FakeCdp accepts one socket, so a tab visited
//! twice needs a second fake.

use std::process::Command;
use std::time::{Duration, Instant};

use super::follow_tests::{
    err, fake, guard, num_after, okay, segments, started, until, view, Rig, LONG,
};
use crate::record::source::fake_cdp::FakeCdp;

const SETTLE: u64 = 150;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

async fn rig(name: &str, a: &FakeCdp) -> Rig {
    Rig::new(name, a, LONG).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_popup_opened_by_the_recorded_tab_is_followed_whatever_its_kind() {
    guard(async {
        // One rule for a link popup and a window-feature popup: both are
        // `openerId`-linked tabs that stay in front.
        for kind in ["link", "window"] {
            let a = FakeCdp::start(fake(400, 0.0)).await;
            let p = FakeCdp::start(fake(400, 3.0)).await;
            let mut r = rig(kind, &a).await;
            r.start().await;
            tokio::time::sleep(ms(300)).await;
            r.opener("P", "A");
            let t0 = Instant::now();
            r.set(vec![view("P", &p.url), view("A", &a.url)]);
            until("P started", || started(&p)).await;
            let delay = t0.elapsed();
            assert!(
                delay < ms(SETTLE + 600),
                "{kind}: follow delay {delay:?} (poll 30 + settle {SETTLE} + rebind)"
            );
            assert!(
                delay >= ms(SETTLE - 20),
                "{kind}: followed before the settle"
            );
            tokio::time::sleep(ms(300)).await;
            *r.env.tab.lock().expect("tab") = "P".into();
            let t = okay(r.hub.stop(&r.env).await);
            assert_eq!(segments(&t), "0.0 s tab A, 3.0 s tab P", "{kind}: {t}");
            assert!(!t.contains("not recorded"), "{t}");
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_popup_re_fronted_in_time_is_not_followed_and_is_named() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("flash", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        tokio::time::sleep(ms(70)).await;
        r.set(vec![view("A", &a.url), view("P", &p.url)]);
        tokio::time::sleep(ms(600)).await;
        assert!(!started(&p));
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A", "{t}");
        assert!(
            t.contains("1 popup(s) not recorded (never in front): P; switch-tab to record one"),
            "{t}"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_tab_nobody_opened_takes_the_short_confirm() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let n = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("opentab", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.set(vec![view("N", &n.url), view("A", &a.url)]);
        until("N started", || started(&n)).await;
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A, 3.0 s tab N", "{t}");
        assert!(!t.contains("not recorded"), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_popup_of_an_unrelated_tab_is_ignored_without_a_note() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let x = FakeCdp::start(fake(400, 3.0)).await;
        let z = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("foreign", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("X", "Z");
        r.set(vec![
            view("X", &x.url),
            view("A", &a.url),
            view("Z", &z.url),
        ]);
        tokio::time::sleep(ms(700)).await;
        assert!(!started(&x));
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A", "{t}");
        assert!(!t.contains("not recorded"), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_burst_follows_only_the_popup_still_in_front() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p1 = FakeCdp::start(fake(400, 3.0)).await;
        let p2 = FakeCdp::start(fake(400, 3.0)).await;
        let p3 = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("burst", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        for id in ["P1", "P2", "P3"] {
            r.opener(id, "A");
        }
        let (v1, v2, v3, va) = (
            view("P1", &p1.url),
            view("P2", &p2.url),
            view("P3", &p3.url),
            view("A", &a.url),
        );
        r.set(vec![v1.clone(), va.clone()]);
        tokio::time::sleep(ms(60)).await;
        r.set(vec![v2.clone(), v1.clone(), va.clone()]);
        tokio::time::sleep(ms(60)).await;
        r.set(vec![v3, v2, v1, va]);
        until("P3 started", || started(&p3)).await;
        assert!(!started(&p1) && !started(&p2));
        tokio::time::sleep(ms(200)).await;
        *r.env.tab.lock().expect("tab") = "P3".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A, 3.0 s tab P3", "{t}");
        assert!(
            t.contains("2 popup(s) not recorded (never in front): P1, P2;"),
            "{t}"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_popup_of_a_followed_popup_is_followed_and_closing_it_returns_to_its_opener() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 2.0)).await;
        let q = FakeCdp::start(fake(400, 4.0)).await;
        let p2 = FakeCdp::start(fake(400, 6.0)).await;
        let mut r = rig("chain", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.opener("Q", "P");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        r.set(vec![
            view("Q", &q.url),
            view("P", &p.url),
            view("A", &a.url),
        ]);
        until("Q started", || started(&q)).await;
        tokio::time::sleep(ms(300)).await;
        q.close_now();
        r.set(vec![view("P", &p2.url), view("A", &a.url)]);
        until("P again", || started(&p2)).await;
        tokio::time::sleep(ms(300)).await;
        *r.env.tab.lock().expect("tab") = "P".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(
            segments(&t).matches(" s tab ").count(),
            4,
            "A, P, Q, P: {t}"
        );
        assert!(t.contains("popup Q closed at "), "{t}");
        assert!(t.contains("; back on opener P"), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_a_followed_popup_goes_back_to_the_opener_with_a_note() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let a2 = FakeCdp::start(fake(400, 6.0)).await;
        let mut r = rig("closep", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        tokio::time::sleep(ms(300)).await;
        p.close_now();
        r.set(vec![view("A", &a2.url)]);
        until("A again", || started(&a2)).await;
        tokio::time::sleep(ms(300)).await;
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved: "), "{t}");
        assert_eq!(segments(&t), "0.0 s tab A, 3.0 s tab P, 6.0 s tab A", "{t}");
        assert!(t.contains("popup P closed at "), "{t}");
        assert!(t.contains("; back on opener A"), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_a_popup_while_an_unrelated_tab_is_in_front_goes_to_the_opener() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let a2 = FakeCdp::start(fake(400, 6.0)).await;
        let z = FakeCdp::start(fake(400, 6.0)).await;
        let mut r = rig("closez", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.opener("Z", "W"); // opened from outside the recording
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        tokio::time::sleep(ms(300)).await;
        p.close_now();
        r.set(vec![view("Z", &z.url), view("A", &a2.url)]);
        until("A again", || started(&a2)).await;
        assert!(!started(&z), "the unrelated tab must not take the video");
        tokio::time::sleep(ms(300)).await;
        *r.env.tab.lock().expect("tab") = "A".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.contains("popup P closed at "), "{t}");
        assert!(t.contains("; back on opener A"), "{t}");
        assert!(!started(&z), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_the_opener_stays_on_the_popup_then_the_popup_is_no_tab_left() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("closeo", &a).await;
        let path = r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        tokio::time::sleep(ms(200)).await;
        a.close_now();
        r.set(vec![view("P", &p.url)]);
        tokio::time::sleep(ms(500)).await;
        // Still one segment on P; the video did not end or restart.
        p.close_now();
        r.set(vec![]);
        tokio::time::sleep(ms(400)).await;
        let t = err(r.hub.stop(&r.env).await);
        assert!(t.contains("no tab left at "), "{t}");
        assert!(t.contains(&path.display().to_string()), "{t}");
        assert!(std::fs::metadata(&path).expect("kept").len() > 0);
        assert_eq!(
            segments(&t).matches(" s tab ").count(),
            2,
            "no segment was added when the opener closed: {t}"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_blocked_popup_costs_nothing() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let mut r = rig("blocked", &a).await;
        r.start().await;
        tokio::time::sleep(ms(1000)).await;
        let (_, targets) = r.calls();
        assert_eq!(targets, 0, "an unchanged list asks no opener");
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A", "{t}");
        assert!(!t.contains("popup"), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cross_origin_navigation_of_the_popup_is_followed_through_the_gone_path() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let p2 = FakeCdp::start(fake(400, 6.0)).await;
        let mut r = rig("xorigin", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.opener("P2", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        tokio::time::sleep(ms(200)).await;
        p.close_now();
        r.set(vec![view("P2", &p2.url), view("A", &a.url)]);
        until("P2 started", || started(&p2)).await;
        tokio::time::sleep(ms(300)).await;
        *r.env.tab.lock().expect("tab") = "P2".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(
            segments(&t),
            "0.0 s tab A, 3.0 s tab P, 6.0 s tab P2",
            "{t}"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn segment_offsets_increase_and_stay_inside_the_duration() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 1.0)).await;
        let a2 = FakeCdp::start(fake(400, 2.0)).await;
        let mut r = rig("offsets", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        tokio::time::sleep(ms(300)).await;
        p.close_now();
        r.set(vec![view("A", &a2.url)]);
        until("A again", || started(&a2)).await;
        tokio::time::sleep(ms(400)).await;
        let t = okay(r.hub.stop(&r.env).await);
        let offs: Vec<f64> = segments(&t)
            .split(", ")
            .map(|s| {
                s.split(" s tab")
                    .next()
                    .expect("off")
                    .parse()
                    .expect("number")
            })
            .collect();
        assert_eq!(offs.len(), 3, "{t}");
        assert!(offs.windows(2).all(|w| w[0] < w[1]), "{t}");
        assert!(offs[2] - offs[0] <= num_after(&t, "duration "), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn finish_orphan_while_on_a_popup_lists_both_segments() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("orph", &a).await;
        r.start().await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        tokio::time::sleep(ms(300)).await;
        r.hub.finish_orphan().await;
        assert!(!r.hub.is_active());
        let t = r.hub.orphaned_text().expect("orphan text").to_string();
        assert!(t.starts_with("recording saved: "), "{t}");
        assert_eq!(segments(&t).matches(" s tab ").count(), 2, "{t}");
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dropping_the_hub_while_on_a_popup_leaves_no_encoder() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("dropp", &a).await;
        r.start().await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        let script = r.script.clone();
        let Rig { hub, .. } = r;
        drop(hub);
        for _ in 0..200 {
            let o = Command::new("pgrep")
                .args(["-f", &script.display().to_string()])
                .output()
                .expect("pgrep");
            if o.stdout.is_empty() {
                return;
            }
            tokio::time::sleep(ms(25)).await;
        }
        panic!("encoder still alive after the hub was dropped");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn with_no_recording_nothing_is_polled_and_the_flag_is_off() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let r = rig("off", &a).await;
        r.opener("P", "A");
        r.set(vec![view("P", "ws://127.0.0.1:1/x"), view("A", &a.url)]);
        tokio::time::sleep(ms(300)).await;
        assert!(!super::recording_active());
        assert!(!r.hub.is_active());
        assert_eq!(r.calls(), (0, 0));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unknown_opener_needs_the_settle_and_a_flash_is_ignored_silently() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let f = FakeCdp::start(fake(400, 2.0)).await;
        let u = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("unknown", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.targets_fail(true);
        // A flash shorter than the settle is never followed and, its opener
        // being unknown, never named in the note either.
        r.set(vec![view("F", &f.url), view("A", &a.url)]);
        tokio::time::sleep(ms(70)).await;
        r.set(vec![view("A", &a.url), view("F", &f.url)]);
        tokio::time::sleep(ms(400)).await;
        assert!(!started(&f));
        // One that stays in front is followed after the settle.
        let t0 = Instant::now();
        r.set(vec![
            view("U", &u.url),
            view("A", &a.url),
            view("F", &f.url),
        ]);
        until("U started", || started(&u)).await;
        assert!(
            t0.elapsed() >= ms(SETTLE - 20),
            "followed before the settle"
        );
        tokio::time::sleep(ms(300)).await;
        *r.env.tab.lock().expect("tab") = "U".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A, 3.0 s tab U", "{t}");
        assert!(!t.contains("not recorded"), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_names_the_agent_tab_when_the_video_is_on_a_popup() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("note", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        tokio::time::sleep(ms(300)).await;
        // The agent is still on A (its tab is not in front): no catch-up.
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A, 3.0 s tab P", "{t}");
        assert!(
            t.contains("agent was on tab A at stop; video followed tab P"),
            "{t}"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_front_agent_tab_is_caught_up_at_stop_without_a_note() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("catch", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        *r.env.tab.lock().expect("tab") = "B".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert!(segments(&t).ends_with("tab B"), "{t}");
        assert!(!t.contains("agent was on tab"), "{t}");
    })
    .await;
}
