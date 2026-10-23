//! Hub scenarios over a fake ffmpeg script and the in-process FakeCdp
//! endpoint (no Chrome). Every test builds its own `Hub`; the global is never
//! touched.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::env::{Attach, HubEnv, Lane};
use super::follow::{TabProbe, TabView};
use super::machine::{take_encoder, Hub, HubError, Knobs};
use crate::record::encoder::test_util::{cfg_for, fake_ffmpeg, pid_gone, start_retry, Scratch};
use crate::record::source::fake_cdp::{FakeCdp, FakeOpts};
use crate::record::values::{Format, FrameSize};
use crate::record::{EncoderError, RecordConfig, ResolvedFfmpeg};

const BOTH: &[&str] = &["libx264", "libvpx-vp9"];
pub(super) const OK_SCRIPT: &str = r#"for a; do out="$a"; done; cat > "$out""#;
const DEAD: [&str; 4] = [
    "Target closed",
    "No target with given id",
    "Session closed",
    "Target.detachedFromTarget",
];

pub(super) async fn guard<F: Future>(f: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(20), f)
        .await
        .expect("scenario timed out")
}

pub(super) struct TestEnv {
    pub(super) cfg: Result<RecordConfig, String>,
    pub(super) ffmpeg: Result<ResolvedFfmpeg, EncoderError>,
    pub(super) lane: Lane,
    pub(super) tab: Mutex<String>,
    pub(super) attach: Attach,
    probe: Option<TabProbe>,
    pub(super) config_calls: AtomicUsize,
    pub(super) ffmpeg_calls: AtomicUsize,
}

impl HubEnv for TestEnv {
    fn config(&self, name: &str) -> Result<RecordConfig, String> {
        self.config_calls.fetch_add(1, Ordering::SeqCst);
        self.cfg.clone().map(|mut c| {
            c.name = Some(name.to_string());
            c
        })
    }
    async fn ffmpeg(&self) -> Result<ResolvedFfmpeg, EncoderError> {
        self.ffmpeg_calls.fetch_add(1, Ordering::SeqCst);
        self.ffmpeg.clone()
    }
    fn lane(&self) -> Lane {
        self.lane
    }
    async fn current_tab(&self) -> Result<String, String> {
        Ok(self.tab.lock().expect("tab").clone())
    }
    fn attach(&self) -> Attach {
        self.attach.clone()
    }
    fn probe(&self) -> TabProbe {
        match &self.probe {
            Some(p) => p.clone(),
            None => match self.attach() {
                Attach::Port(base) => TabProbe::Http { base },
                Attach::Ws(_) => TabProbe::Fixed,
            },
        }
    }
}

impl TestEnv {
    fn with_probe(mut self, p: TabProbe) -> Self {
        self.probe = Some(p);
        self
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

fn rec_cfg(s: &Scratch, enabled: bool) -> RecordConfig {
    RecordConfig {
        enabled,
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
    }
}

pub(super) fn env(s: &Scratch, script: &Path, attach: Attach) -> TestEnv {
    TestEnv {
        cfg: Ok(rec_cfg(s, true)),
        ffmpeg: Ok(fake_ffmpeg(script, BOTH)),
        lane: Lane::Ws,
        tab: Mutex::new("T1".into()),
        attach,
        probe: None,
        config_calls: AtomicUsize::new(0),
        ffmpeg_calls: AtomicUsize::new(0),
    }
}

pub(super) fn hub() -> Hub {
    Hub::with_knobs(Knobs {
        stop_grace: Some(Duration::from_millis(100)),
        finish_timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    })
}

/// Frames slow enough to clear the 10 fps gate (100 ms).
pub(super) fn frames(n: usize) -> FakeOpts {
    FakeOpts {
        frames: n,
        interval: Duration::from_millis(120),
        ..Default::default()
    }
}

fn clean(s: &str) -> &str {
    for d in DEAD {
        assert!(!s.contains(d), "{d} in {s}");
    }
    s
}

pub(super) fn okay(r: Result<String, HubError>) -> String {
    match r {
        Ok(t) => clean(&t).to_string(),
        Err(e) => panic!("expected Ok, got Err: {e}"),
    }
}

pub(super) fn err(r: Result<String, HubError>) -> String {
    match r {
        Err(e) => clean(&e.to_string()).to_string(),
        Ok(t) => panic!("expected Err, got Ok: {t}"),
    }
}

/// The absolute path out of "recording started: <path> (tab ...".
pub(super) fn started_path(t: &str) -> PathBuf {
    let rest = t.split("recording started: ").nth(1).expect("started text");
    PathBuf::from(rest.split(" (tab ").next().expect("path"))
}

pub(super) fn files_in(dir: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => vec![],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_then_stop_reports_the_recording() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-a", OK_SCRIPT);
        let fake = FakeCdp::start(frames(20)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        let t = okay(h.start(&e, "demo").await);
        assert!(h.is_active());
        let path = started_path(&t);
        assert!(path.is_absolute(), "{t}");
        fake.wait_emitter_done().await;
        let r = okay(h.stop(&e).await);
        assert!(!h.is_active());
        assert!(r.starts_with("recording saved: "), "{r}");
        assert!(r.contains(&path.display().to_string()), "{r}");
        assert!(std::fs::metadata(&path).expect("file").len() > 0);
        assert!(r.contains("segments [0.0 s tab T1]"), "{r}");
        assert!(r.contains("duration ") && r.contains("frames "), "{r}");
        let again = err(h.stop(&e).await);
        assert!(again.contains("no active recording"), "{again}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_second_start_names_the_active_file_and_spawns_nothing() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-b", OK_SCRIPT);
        let fake = FakeCdp::start(frames(10)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        let t = okay(h.start(&e, "one").await);
        let path = started_path(&t);
        let second = err(h.start(&e, "two").await);
        assert!(second.contains(&path.display().to_string()), "{second}");
        assert!(second.contains("record-stop"), "{second}");
        // Refused before config/ffmpeg: no second encoder was resolved.
        assert_eq!(e.ffmpeg_calls.load(Ordering::SeqCst), 1);
        assert_eq!(files_in(&s.0.join("recordings")).len(), 1);
        okay(h.stop(&e).await);
    })
    .await;
}

#[tokio::test]
async fn c_recording_off_refuses_before_anything_happens() {
    let s = Scratch::new();
    let script = s.script("fake-enc-c", OK_SCRIPT);
    let mut e = env(&s, &script, Attach::Ws("ws://127.0.0.1:1/x".into()));
    e.cfg = Ok(rec_cfg(&s, false));
    let mut h = hub();
    let t = err(h.start(&e, "x").await);
    assert!(
        t.contains("BLADE_RECORD") && t.contains("bladebro stop"),
        "{t}"
    );
    assert_eq!(e.ffmpeg_calls.load(Ordering::SeqCst), 0);
    assert!(!s.0.join("recordings").exists());
    assert!(!h.is_active());
}

#[tokio::test]
async fn d_lane_refusals_touch_nothing() {
    for (lane, want) in [
        (Lane::Pipe, "BLADE_TRANSPORT=pipe"),
        (Lane::Ephemeral, "long-lived session"),
        (Lane::Real, "real-browser lane"),
    ] {
        let s = Scratch::new();
        let script = s.script("fake-enc-d", OK_SCRIPT);
        let mut e = env(&s, &script, Attach::Ws("ws://127.0.0.1:1/x".into()));
        e.lane = lane;
        let mut h = hub();
        let t = err(h.start(&e, "x").await);
        assert!(t.contains(want), "{lane:?}: {t}");
        assert_eq!(e.config_calls.load(Ordering::SeqCst), 0);
        assert_eq!(e.ffmpeg_calls.load(Ordering::SeqCst), 0);
        assert!(!s.0.join("recordings").exists());
    }
}

#[tokio::test]
async fn e_missing_ffmpeg_creates_no_file() {
    let s = Scratch::new();
    let script = s.script("fake-enc-e", OK_SCRIPT);
    let mut e = env(&s, &script, Attach::Ws("ws://127.0.0.1:1/x".into()));
    e.ffmpeg = Err(EncoderError::FfmpegNotFound {
        hint: "brew install ffmpeg",
    });
    let mut h = hub();
    let t = err(h.start(&e, "x").await);
    assert!(t.contains("ffmpeg not found"), "{t}");
    assert!(!s.0.join("recordings").exists());
    assert!(!h.is_active());
}

#[tokio::test]
async fn f_bad_names_are_usage_errors_and_call_nothing() {
    let s = Scratch::new();
    let script = s.script("fake-enc-f", OK_SCRIPT);
    let e = env(&s, &script, Attach::Ws("ws://127.0.0.1:1/x".into()));
    let mut h = hub();
    for n in ["", "   ", "\t\n"] {
        match h.start(&e, n).await {
            Err(HubError::Usage(t)) => assert!(t.contains("record-start needs name"), "{t}"),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(e.config_calls.load(Ordering::SeqCst), 0);
    assert_eq!(e.ffmpeg_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g_source_start_failure_cleans_up_and_hub_stays_idle() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-g", OK_SCRIPT);
        // Nothing listens on port 1: the encoder spawns, the source cannot.
        let mut e = env(&s, &script, Attach::Ws("ws://127.0.0.1:1/x".into()));
        let mut h = hub();
        let t = err(h.start(&e, "x").await);
        assert!(t.contains("recording not started"), "{t}");
        assert!(!h.is_active());
        assert!(
            files_in(&s.0.join("recordings")).is_empty(),
            "stray file left: {:?}",
            files_in(&s.0.join("recordings"))
        );
        // A following start works on the same hub.
        let fake = FakeCdp::start(frames(5)).await;
        e.attach = Attach::Ws(fake.url.clone());
        okay(h.start(&e, "again").await);
        fake.wait_emitter_done().await;
        okay(h.stop(&e).await);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_connection_lost_is_an_error_that_keeps_the_file() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-h", OK_SCRIPT);
        let fake = FakeCdp::start(frames(5)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        let path = started_path(&okay(h.start(&e, "lost").await));
        fake.wait_emitter_done().await;
        fake.close_now();
        // The source notices a silent close on its 250 ms liveness tick.
        tokio::time::sleep(Duration::from_millis(700)).await;
        let t = err(h.stop(&e).await);
        assert!(t.starts_with("recording not clean"), "{t}");
        assert!(t.contains(&path.display().to_string()), "{t}");
        assert!(path.exists());
        assert!(!h.is_active());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn i_encoder_exit_failure_is_reported() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-i", "cat >/dev/null; echo boom >&2; exit 1");
        let fake = FakeCdp::start(frames(5)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "bad").await);
        fake.wait_emitter_done().await;
        let t = err(h.stop(&e).await);
        assert!(t.contains("ffmpeg exited with exit code 1"), "{t}");
        assert!(!t.contains("recording saved"), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn j_take_encoder_fails_honestly_when_the_sink_is_still_held() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-j", OK_SCRIPT);
        let out = s.0.join("j.webm");
        let cfg = cfg_for(
            fake_ffmpeg(&script, BOTH),
            Format::WebM,
            10,
            8,
            Duration::from_secs(5),
        );
        let enc = Arc::new(start_retry(&cfg, &out).expect("start"));
        let pid = enc.pid().expect("pid");
        let extra = enc.clone();
        let m = take_encoder(enc, 3, Duration::from_millis(10))
            .await
            .map(|_| ())
            .expect_err("still shared");
        assert!(m.contains("did not release the encoder"), "{m}");
        assert!(m.contains(&out.display().to_string()), "{m}");
        clean(&m);
        // The last handle going away kills ffmpeg (kill_on_drop): no leak.
        drop(extra);
        for _ in 0..200 {
            if pid_gone(pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("ffmpeg {pid} still alive");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn k_stop_after_late_switch_shows_the_new_tab() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-k", OK_SCRIPT);
        let fake_a = FakeCdp::start(frames(30)).await;
        // One Chrome clock: the second tab's frames come later on it.
        let fake_b = FakeCdp::start(FakeOpts {
            ts_base: 1.7e9 + 10.0,
            ..frames(30)
        })
        .await;
        let (probe, list) = TabProbe::scripted(vec![view("T1", &fake_a.url)]);
        let e = env(&s, &script, Attach::Ws(fake_a.url.clone())).with_probe(probe);
        let mut h = Hub::with_knobs(Knobs {
            stop_grace: Some(Duration::from_millis(100)),
            finish_timeout: Some(Duration::from_secs(10)),
            // A poll that never fires inside this test: only the stop-time
            // catch-up can notice the switch.
            poll: Some(Duration::from_secs(60)),
            ..Default::default()
        });
        okay(h.start(&e, "tabs").await);
        tokio::time::sleep(Duration::from_millis(500)).await;
        list.lock().expect("list").tabs = vec![view("T2", &fake_b.url), view("T1", &fake_a.url)];
        *e.tab.lock().expect("tab") = "T2".into();
        let t = okay(h.stop(&e).await);
        assert!(t.starts_with("recording saved: "), "{t}");
        assert!(t.contains("tab T2]"), "last segment is the new tab: {t}");
        assert!(t.contains("tab T1,"), "{t}");
        assert!(
            !t.contains("tab following is not implemented"),
            "stale Phase 7 text: {t}"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l_orphan_result_is_kept_for_the_next_stop() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-l", OK_SCRIPT);
        let fake = FakeCdp::start(frames(10)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "orphan").await);
        fake.wait_emitter_done().await;
        h.finish_orphan().await;
        assert!(!h.is_active());
        assert!(h.orphaned_text().is_some());
        let t = okay(h.stop(&e).await);
        assert!(
            t.starts_with("previous recording ended with the browser: recording saved: "),
            "{t}"
        );
        let again = err(h.stop(&e).await);
        assert!(again.contains("no active recording"), "{again}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l2_orphan_result_prefixes_the_next_start() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-l2", OK_SCRIPT);
        let fake = FakeCdp::start(frames(10)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "first").await);
        fake.wait_emitter_done().await;
        h.finish_orphan().await;
        let fake2 = FakeCdp::start(frames(5)).await;
        let e2 = env(&s, &script, Attach::Ws(fake2.url.clone()));
        let t = okay(h.start(&e2, "second").await);
        let (first, second) = t.split_once('\n').expect("two lines");
        assert!(
            first.starts_with("previous recording ended with the browser: "),
            "{t}"
        );
        assert!(second.starts_with("recording started: "), "{t}");
        fake2.wait_emitter_done().await;
        okay(h.stop(&e2).await);
    })
    .await;
}

#[tokio::test]
async fn finish_orphan_without_a_recording_is_a_no_op() {
    let mut h = hub();
    h.finish_orphan().await;
    assert!(h.orphaned_text().is_none());
}
