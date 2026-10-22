//! Tab-following scenarios over the fake ffmpeg script (`cat`), one FakeCdp
//! per tab connection and a scripted tab list. No Chrome. Every FakeCdp
//! accepts one socket, so a return to an earlier tab needs a second fake;
//! `ts_base` places each fake on one shared Chrome clock.

use std::future::Future;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::env::{Attach, HubEnv, Lane};
use super::follow::{Scripted, TabProbe, TabView};
use super::machine::{Hub, HubError, Knobs};
use crate::record::encoder::test_util::{fake_ffmpeg, Scratch};
use crate::record::source::fake_cdp::{FakeCdp, FakeOpts};
use crate::record::values::{Format, FrameSize};
use crate::record::{EncoderError, RecordConfig, ResolvedFfmpeg};

const T0: f64 = 1.7e9;
const OK_SCRIPT: &str = r#"for a; do out="$a"; done; cat > "$out""#;
const FRAME_BYTES: u64 = 30_000;
const DEAD: [&str; 4] = [
    "Target closed",
    "No target with given id",
    "Session closed",
    "Target.detachedFromTarget",
];

async fn guard<F: Future>(f: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(40), f)
        .await
        .expect("scenario timed out")
}

struct Env {
    cfg: RecordConfig,
    ffmpeg: ResolvedFfmpeg,
    tab: Mutex<String>,
    first_ws: String,
    probe: TabProbe,
}

impl HubEnv for Env {
    fn config(&self, name: &str) -> Result<RecordConfig, String> {
        let mut c = self.cfg.clone();
        c.name = Some(name.to_string());
        Ok(c)
    }
    async fn ffmpeg(&self) -> Result<ResolvedFfmpeg, EncoderError> {
        Ok(self.ffmpeg.clone())
    }
    fn lane(&self) -> Lane {
        Lane::Ws
    }
    async fn current_tab(&self) -> Result<String, String> {
        Ok(self.tab.lock().expect("tab").clone())
    }
    fn attach(&self) -> Attach {
        Attach::Ws(self.first_ws.clone())
    }
    fn probe(&self) -> TabProbe {
        self.probe.clone()
    }
}

fn view(id: &str, ws: &str) -> TabView {
    TabView {
        id: id.into(),
        url: format!("http://127.0.0.1/{id}"),
        attached: false,
        ws_url: ws.into(),
    }
}

fn fake(frames: usize, base_off: f64) -> FakeOpts {
    FakeOpts {
        frames,
        interval: Duration::from_millis(120),
        ts_base: T0 + base_off,
        ..Default::default()
    }
}

struct Rig {
    _s: Scratch,
    script: PathBuf,
    hub: Hub,
    env: Env,
    list: Arc<Mutex<Scripted>>,
}

impl Rig {
    async fn new(name: &str, first: &FakeCdp, no_frames_after: Duration) -> Rig {
        let s = Scratch::new();
        let script = s.script(&format!("fake-enc-{name}"), OK_SCRIPT);
        let (probe, list) = TabProbe::scripted(vec![view("A", &first.url)]);
        let env = Env {
            cfg: RecordConfig {
                enabled: true,
                dir: s.0.join("recordings"),
                format: Format::WebM,
                fps: 10,
                size: FrameSize {
                    width: 1280,
                    height: 720,
                },
                quality: 70,
                tabs: crate::record::Tabs::Follow,
                queue_len: 16,
                name: None,
                artifacts_dir: s.0.join("artifacts"),
            },
            ffmpeg: fake_ffmpeg(&script, &["libx264", "libvpx-vp9"]),
            tab: Mutex::new("A".into()),
            first_ws: first.url.clone(),
            probe,
        };
        let hub = Hub::with_knobs(Knobs {
            stop_grace: Some(Duration::from_millis(100)),
            finish_timeout: Some(Duration::from_secs(10)),
            poll: Some(Duration::from_millis(30)),
            confirm: Some(Duration::from_millis(10)),
            new_confirm: Some(Duration::from_millis(150)),
            no_frames_after: Some(no_frames_after),
        });
        Rig {
            _s: s,
            script,
            hub,
            env,
            list,
        }
    }

    fn set(&self, tabs: Vec<TabView>) {
        self.list.lock().expect("list").tabs = tabs;
    }

    async fn start(&mut self) -> PathBuf {
        let t = okay(self.hub.start(&self.env, "follow").await);
        let rest = t.split("recording started: ").nth(1).expect("started");
        PathBuf::from(rest.split(" (tab ").next().expect("path"))
    }

    /// ffmpeg processes of THIS rig's fake script.
    fn encoders_alive(&self) -> usize {
        let o = Command::new("pgrep")
            .args(["-f", &self.script.display().to_string()])
            .output()
            .expect("pgrep");
        String::from_utf8_lossy(&o.stdout).lines().count()
    }
}

fn clean(s: &str) -> &str {
    for d in DEAD {
        assert!(!s.contains(d), "{d} in {s}");
    }
    s
}

fn okay(r: Result<String, HubError>) -> String {
    match r {
        Ok(t) => clean(&t).to_string(),
        Err(e) => panic!("expected Ok, got Err: {e}"),
    }
}

fn err(r: Result<String, HubError>) -> String {
    match r {
        Err(e) => clean(&e.to_string()).to_string(),
        Ok(t) => panic!("expected Err, got Ok: {t}"),
    }
}

fn num_after(t: &str, key: &str) -> f64 {
    let rest = t
        .split(key)
        .nth(1)
        .unwrap_or_else(|| panic!("{key} in {t}"));
    let n: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    n.parse()
        .unwrap_or_else(|_| panic!("number after {key} in {t}"))
}

/// The inside of `segments [...]`.
fn segments(t: &str) -> String {
    let rest = t
        .split("segments [")
        .nth(1)
        .unwrap_or_else(|| panic!("segments in {t}"));
    rest.split(']').next().expect("close").to_string()
}

async fn until(what: &str, mut f: impl FnMut() -> bool) {
    for _ in 0..1500 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

fn started(f: &FakeCdp) -> bool {
    f.methods().iter().any(|m| m == "Page.startScreencast")
}

const LONG: Duration = Duration::from_secs(5);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn switch_a_b_a_makes_three_segments_in_one_file_and_one_encoder() {
    guard(async {
        let a1 = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 4.2)).await;
        let a2 = FakeCdp::start(fake(400, 9.8)).await;
        let mut r = Rig::new("sw", &a1, LONG).await;
        let path = r.start().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        r.set(vec![view("B", &b.url), view("A", &a1.url)]);
        until("B started", || started(&b)).await;
        assert_eq!(r.encoders_alive(), 1, "one encoder across the switch");
        tokio::time::sleep(Duration::from_millis(500)).await;
        r.set(vec![view("A", &a2.url), view("B", &b.url)]);
        until("A2 started", || started(&a2)).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(r.encoders_alive(), 1, "still one encoder");
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved: "), "{t}");
        assert!(t.contains(&path.display().to_string()), "{t}");
        assert_eq!(segments(&t), "0.0 s tab A, 4.2 s tab B, 9.8 s tab A", "{t}");
        // The video continued across both switches (mark_end only at the stop).
        assert!(num_after(&t, "duration ") >= 9.9, "{t}");
        // One file: its bytes are exactly the frames the encoder wrote.
        let frames = num_after(&t, "frames ") as u64;
        assert_eq!(num_after(&t, "bytes ") as u64, frames * FRAME_BYTES, "{t}");
        assert_eq!(
            std::fs::metadata(&path).expect("file").len(),
            frames * FRAME_BYTES
        );
        assert_eq!(r.encoders_alive(), 0, "encoder finished");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn open_tab_adds_a_segment() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let c = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::new("open", &a, LONG).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        r.set(vec![view("C", &c.url), view("A", &a.url)]);
        until("C started", || started(&c)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        *r.env.tab.lock().expect("tab") = "C".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A, 3.0 s tab C", "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_popup_behind_the_opener_is_not_a_segment_until_it_takes_the_front() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::new("popup", &a, LONG).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        // The opener stays first (a target=_blank link, or the opener re-activated).
        r.set(vec![view("A", &a.url), view("P", &p.url)]);
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!started(&p), "an unactivated popup must not be recorded");
        // The agent switches to the popup: it takes the front.
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        until("P started", || started(&p)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A, 3.0 s tab P", "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_popup_that_the_opener_re_fronts_in_time_is_not_a_segment() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::new("flash", &a, LONG).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        // The popup is first right after the click; the opener's re-activation
        // (after the click verdict) puts it back, well inside new_confirm.
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        tokio::time::sleep(Duration::from_millis(70)).await;
        r.set(vec![view("A", &a.url), view("P", &p.url)]);
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(!started(&p), "a re-fronted popup must not be recorded");
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A", "{t}");
        // Once the popup is known, switching to it is followed after the short
        // confirm only (the agent chose it).
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_head_that_already_differed_at_record_start_is_not_followed() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let p = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::new("stale", &a, LONG).await;
        // A lingering window popup is first in Chrome's list at record-start.
        r.set(vec![view("P", &p.url), view("A", &a.url)]);
        r.start().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!started(&p), "the stale head must not be recorded");
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A", "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_transient_flip_makes_no_segment() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::new("flip", &a, LONG).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        for _ in 0..5 {
            r.set(vec![view("B", &b.url), view("A", &a.url)]);
            tokio::time::sleep(Duration::from_millis(4)).await;
            r.set(vec![view("A", &a.url), view("B", &b.url)]);
            tokio::time::sleep(Duration::from_millis(70)).await;
        }
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t), "0.0 s tab A", "{t}");
        assert!(!started(&b));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rapid_a_b_c_ends_on_c() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let c = FakeCdp::start(fake(400, 6.0)).await;
        let mut r = Rig::new("rapid", &a, LONG).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        tokio::time::sleep(Duration::from_millis(15)).await;
        r.set(vec![
            view("C", &c.url),
            view("B", &b.url),
            view("A", &a.url),
        ]);
        until("C started", || started(&c)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        *r.env.tab.lock().expect("tab") = "C".into();
        let t = okay(r.hub.stop(&r.env).await);
        let s = segments(&t);
        assert!(s.starts_with("0.0 s tab A"), "{t}");
        assert!(s.ends_with("s tab C"), "the latest tab wins: {t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_the_recorded_tab_rebinds_with_a_note() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::new("close", &a, LONG).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        a.close_now();
        r.set(vec![view("B", &b.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        *r.env.tab.lock().expect("tab") = "B".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved: "), "{t}");
        assert!(t.contains("Note: tab A gone at "), "{t}");
        assert!(t.contains("; following tab B"), "{t}");
        assert_eq!(segments(&t), "0.0 s tab A, 3.0 s tab B", "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_target_id_change_rebinds_to_the_new_id() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let a2 = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::new("swap", &a, LONG).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        // Same page, new target id (a cross-process swap): A disappears, A2 takes over.
        a.close_now();
        r.set(vec![view("A2", &a2.url)]);
        until("A2 started", || started(&a2)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.contains("tab A gone at "), "{t}");
        assert!(t.contains("following tab A2"), "{t}");
        assert_eq!(segments(&t), "0.0 s tab A, 3.0 s tab A2", "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_tab_left_is_not_clean_and_keeps_the_file() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let mut r = Rig::new("none", &a, LONG).await;
        let path = r.start().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        a.close_now();
        r.set(vec![]);
        tokio::time::sleep(Duration::from_millis(400)).await;
        let t = err(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording not clean: "), "{t}");
        assert!(t.contains("no tab left at "), "{t}");
        assert!(t.contains(&path.display().to_string()), "{t}");
        assert!(std::fs::metadata(&path).expect("kept").len() > 0);
        assert!(!t.contains("recording saved"), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_browser_that_stops_answering_is_not_clean() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let mut r = Rig::new("gone", &a, LONG).await;
        let path = r.start().await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        r.list.lock().expect("list").fail = true;
        let calls = r.list.lock().expect("list").calls;
        until("three failed polls", || {
            r.list.lock().expect("list").calls >= calls + 3
        })
        .await;
        let t = err(r.hub.stop(&r.env).await);
        assert!(t.contains("browser connection lost at "), "{t}");
        assert!(t.contains(&path.display().to_string()), "{t}");
        assert!(std::fs::metadata(&path).expect("kept").len() > 0);
        // Polling stopped after the third miss.
        let after = r.list.lock().expect("list").calls;
        assert!(after <= calls + 8, "kept polling a dead browser: {after}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_rebind_is_named_and_the_first_tab_is_kept() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let mut r = Rig::new("rbf", &a, LONG).await;
        let path = r.start().await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        // Nothing listens on port 1.
        r.set(vec![view("B", "ws://127.0.0.1:1/x"), view("A", &a.url)]);
        tokio::time::sleep(Duration::from_millis(900)).await;
        let t = err(r.hub.stop(&r.env).await);
        assert!(t.contains("tab B not recorded from "), "{t}");
        assert!(num_after(&t, "frames ") > 0.0, "A's frames are kept: {t}");
        assert!(std::fs::metadata(&path).expect("kept").len() > 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_long_zero_frame_segment_is_not_clean_a_short_one_is_labelled() {
    guard(async {
        for (stay_ms, ok) in [(1500u64, false), (300, true)] {
            let a1 = FakeCdp::start(fake(400, 0.0)).await;
            let b = FakeCdp::start(FakeOpts::default()).await; // never sends a frame
            let a2 = FakeCdp::start(fake(400, 8.0)).await;
            let mut r = Rig::new("zero", &a1, Duration::from_secs(1)).await;
            r.start().await;
            tokio::time::sleep(Duration::from_millis(400)).await;
            r.set(vec![view("B", &b.url), view("A", &a1.url)]);
            until("B started", || started(&b)).await;
            tokio::time::sleep(Duration::from_millis(stay_ms)).await;
            r.set(vec![view("A", &a2.url), view("B", &b.url)]);
            until("A2 started", || started(&a2)).await;
            tokio::time::sleep(Duration::from_millis(400)).await;
            let res = r.hub.stop(&r.env).await;
            if ok {
                let t = okay(res);
                assert!(t.starts_with("recording saved: "), "{t}");
                assert!(segments(&t).contains("tab B (no frames)"), "{t}");
            } else {
                let t = err(res);
                assert!(t.contains("tab B: no frames for "), "{t}");
                assert!(t.contains("hidden or background tab"), "{t}");
            }
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_encoder_comes_back_after_ten_switches() {
    guard(async {
        let first = FakeCdp::start(fake(400, 0.0)).await;
        let mut fakes = Vec::new();
        for k in 1..=10 {
            fakes.push(FakeCdp::start(fake(400, 2.0 * k as f64)).await);
        }
        let mut r = Rig::new("ten", &first, LONG).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        for (k, f) in fakes.iter().enumerate() {
            let id = format!("T{}", k + 1);
            r.set(vec![view(&id, &f.url), view("A", &first.url)]);
            until("switch", || started(f)).await;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        *r.env.tab.lock().expect("tab") = "T10".into();
        let t = okay(r.hub.stop(&r.env).await);
        assert_eq!(segments(&t).matches(" s tab ").count(), 11, "{t}");
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_second_start_while_following_is_refused_naming_the_file() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::new("dbl", &a, LONG).await;
        let path = r.start().await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        let second = err(r.hub.start(&r.env, "two").await);
        assert!(second.contains(&path.display().to_string()), "{second}");
        assert!(second.contains("record-stop"), "{second}");
        assert_eq!(r.encoders_alive(), 1);
        okay(r.hub.stop(&r.env).await);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn finish_orphan_stops_the_follower_and_the_encoder() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::new("orph", &a, LONG).await;
        r.start().await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
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
async fn dropping_the_hub_while_following_leaves_no_encoder() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::new("drop", &a, LONG).await;
        r.start().await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
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
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("encoder still alive after the hub was dropped");
    })
    .await;
}
