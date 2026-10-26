//! Per-tab mode with popups (CAPT-06 + CAPT-04): a followed popup gets its own
//! file, a popup that never reaches the front gets none, a closed popup's file
//! is finalized while the recording continues. Same rig as `per_tab_tests`.

use std::time::Duration;

use super::follow::MAX_TAB_FILES;
use super::follow_tests::{
    fake, guard, num_after, okay, segments, started, until, view, Rig, FRAME_BYTES, LONG, OK_SCRIPT,
};
use crate::record::source::fake_cdp::FakeCdp;
use crate::record::Tabs;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

async fn rig(name: &str, a: &FakeCdp) -> Rig {
    Rig::with(name, a, OK_SCRIPT, Tabs::PerTab, LONG).await
}

fn files_in(r: &Rig) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(r.s.0.join("recordings"))
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn size_of(r: &Rig, tag: &str) -> u64 {
    let f = files_in(r)
        .into_iter()
        .find(|f| f.contains(tag))
        .unwrap_or_else(|| panic!("no file {tag}"));
    std::fs::metadata(r.s.0.join("recordings").join(f))
        .expect("meta")
        .len()
}

/// The `(bytes ... segments [...])` fact of the file named `-tabN-`.
fn line_of<'a>(t: &'a str, tag: &str) -> &'a str {
    let at = t.find(tag).unwrap_or_else(|| panic!("no {tag} in {t}"));
    let end = t[at..].find(')').map_or(t.len(), |e| at + e + 1);
    &t[at..end]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_followed_popup_gets_its_own_file_finalized_when_it_closes() {
    guard(async {
        let a = FakeCdp::start(fake(500, 0.0)).await;
        let p = FakeCdp::start(fake(500, 3.0)).await;
        let a2 = FakeCdp::start(fake(500, 6.0)).await;
        let mut r = rig("close", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        assert_eq!(r.encoders_alive(), 2, "one encoder per followed tab");
        tokio::time::sleep(ms(300)).await;
        p.close_now();
        r.set(vec![view("A", &a2.url)]);
        until("A again", || started(&a2)).await;
        until("P's encoder finished", || r.encoders_alive() == 1).await;
        let size = size_of(&r, "-tab2-");
        assert!(size > 0 && size % FRAME_BYTES == 0, "{size}");
        tokio::time::sleep(ms(300)).await;
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved (per-tab, 2 files): "), "{t}");
        let (one, two) = (line_of(&t, "-tab1-"), line_of(&t, "-tab2-"));
        assert_eq!(segments(one).matches(" s tab A").count(), 2, "{t}");
        assert_eq!(segments(two), "0.0 s tab P", "{t}");
        assert_eq!(num_after(two, "bytes ") as u64, size, "{t}");
        assert!(t.contains("popup P closed at "), "{t}");
        assert_eq!(files_in(&r).len(), 2);
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_burst_makes_files_only_for_the_opener_and_the_popup_in_front() {
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
        assert_eq!(r.encoders_alive(), 2);
        tokio::time::sleep(ms(200)).await;
        *r.env.tab.lock().expect("tab") = "P3".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved (per-tab, 2 files): "), "{t}");
        assert!(
            t.contains("2 popup(s) not recorded (never in front): P1, P2;"),
            "{t}"
        );
        assert_eq!(files_in(&r).len(), 2);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_chain_makes_three_files_and_returns_q_then_p_then_a() {
    guard(async {
        let a = FakeCdp::start(fake(600, 0.0)).await;
        let p = FakeCdp::start(fake(600, 2.0)).await;
        let q = FakeCdp::start(fake(600, 4.0)).await;
        let p2 = FakeCdp::start(fake(600, 6.0)).await;
        let a2 = FakeCdp::start(fake(600, 8.0)).await;
        let mut r = rig("chain", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        tokio::time::sleep(ms(300)).await;
        r.opener("Q", "P");
        r.set(vec![
            view("Q", &q.url),
            view("P", &p.url),
            view("A", &a.url),
        ]);
        until("Q started", || started(&q)).await;
        assert_eq!(r.encoders_alive(), 3);
        tokio::time::sleep(ms(300)).await;
        // Q closes: back on its opener P (second connection), P's file stays open.
        q.close_now();
        r.set(vec![view("P", &p2.url), view("A", &a.url)]);
        until("P again", || started(&p2)).await;
        until("Q's encoder finished", || r.encoders_alive() == 2).await;
        assert!(size_of(&r, "-tab3-") > 0);
        tokio::time::sleep(ms(300)).await;
        p2.close_now();
        r.set(vec![view("A", &a2.url)]);
        until("A again", || started(&a2)).await;
        until("P's encoder finished", || r.encoders_alive() == 1).await;
        tokio::time::sleep(ms(300)).await;
        *r.env.tab.lock().expect("tab") = "A".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved (per-tab, 3 files): "), "{t}");
        assert_eq!(
            segments(line_of(&t, "-tab2-")).matches("tab P").count(),
            2,
            "{t}"
        );
        assert_eq!(segments(line_of(&t, "-tab3-")), "0.0 s tab Q", "{t}");
        assert!(
            t.contains("popup Q closed at ") && t.contains("popup P closed at "),
            "{t}"
        );
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hidden_popup_never_gets_a_file_or_an_encoder() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("hidden", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        tokio::time::sleep(ms(70)).await;
        r.set(vec![view("A", &a.url), view("P", &p.url)]);
        for _ in 0..8 {
            tokio::time::sleep(ms(100)).await;
            assert_eq!(r.encoders_alive(), 1, "no second encoder, ever");
        }
        assert!(!started(&p));
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved (per-tab, 1 file): "), "{t}");
        assert!(
            t.contains("1 popup(s) not recorded (never in front): P;"),
            "{t}"
        );
        assert_eq!(files_in(&r).len(), 1);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_ninth_popup_gets_no_file_and_a_freed_slot_is_reused() {
    guard(async {
        assert_eq!(MAX_TAB_FILES, 8);
        let mut fakes = Vec::new();
        for i in 0..11 {
            fakes.push(FakeCdp::start(fake(700, i as f64 * 1.5)).await);
        }
        let id = |i: usize| format!("T{}", i + 1);
        let mut r = rig("cap", &fakes[0]).await;
        r.start().await;
        let mut tabs = vec![view("A", &fakes[0].url)];
        #[allow(clippy::needless_range_loop)] // `i` also names the tab
        for i in 1..9 {
            tokio::time::sleep(ms(150)).await;
            r.opener(&id(i), "A");
            tabs.insert(0, view(&id(i), &fakes[i].url));
            r.set(tabs.clone());
            if i < 8 {
                until(&format!("T{} started", i + 1), || started(&fakes[i])).await;
            } else {
                tokio::time::sleep(ms(700)).await;
                assert!(!started(&fakes[i]), "the 9th popup must not be recorded");
            }
        }
        assert_eq!(r.encoders_alive(), 8);
        tabs.retain(|v| v.id != "T3");
        r.set(tabs.clone());
        until("T3's encoder finished", || r.encoders_alive() == 7).await;
        r.opener("T10", "A");
        tabs.insert(0, view("T10", &fakes[9].url));
        r.set(tabs.clone());
        until("T10 started", || started(&fakes[9])).await;
        assert_eq!(r.encoders_alive(), 8);
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved (per-tab, 9 files): "), "{t}");
        assert!(
            t.contains("tab T9 not recorded from ")
                && t.contains("per-tab cap of 8 open files reached"),
            "{t}"
        );
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_on_a_popup_finalizes_every_file_and_names_the_agent_tab() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("stop", &a).await;
        r.start().await;
        tokio::time::sleep(ms(300)).await;
        r.opener("P", "A");
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        tokio::time::sleep(ms(300)).await;
        let t = okay(r.hub.stop(&r.env).await); // the agent is still on A
        assert!(t.starts_with("recording saved (per-tab, 2 files): "), "{t}");
        assert!(
            t.contains("agent was on tab A at stop; video followed tab P"),
            "{t}"
        );
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn finish_orphan_on_a_popup_finalizes_every_file() {
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
        let t = r.hub.orphaned_text().expect("text").to_string();
        assert!(t.contains("(per-tab, 2 files)"), "{t}");
        assert_eq!(r.encoders_alive(), 0);
        assert_eq!(files_in(&r).len(), 2);
    })
    .await;
}
