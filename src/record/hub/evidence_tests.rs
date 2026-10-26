//! Stop extras over the fake ffmpeg (OUT-06): the poster PNG and the marker
//! offset appear in the record-stop result; a failing extra never changes the
//! stop's ok/err and leaves no file behind. Rigs come from machine_tests and
//! follow_tests (fake CDP, no Chrome).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::env::Attach;
use super::follow_tests::{fake, guard as rig_guard, segments, started, until, view, Rig, LONG};
use super::machine::{Interrupt, Limit};
use super::machine_tests::{env, err, frames, guard, hub, okay, started_path, TestEnv, OK_SCRIPT};
use super::msg;
use crate::record::encoder::test_util::{fake_ffmpeg, Scratch};
use crate::record::marker::MarkerIn;
use crate::record::poster::is_png;
use crate::record::source::fake_cdp::{FakeCdp, FakeOpts};
use crate::record::timeline::{begin_with, end, Entry, Log, MAX_ENTRIES};
use crate::record::Tabs;

/// The poster run needs the png encoder listed next to the video encoder.
const PNG_ENCODERS: &[&str] = &["libvpx-vp9", "png"];

/// Video runs `cat` to the last argument; the poster run (argv has `-update`)
/// writes the 8 PNG magic bytes and exits 0.
const POSTER_OK: &str = r#"for a; do out="$a"; done
case " $* " in *" -update "*) printf '\211PNG\r\n\032\n' > "$out"; exit 0;; esac
cat > "$out""#;

/// Same, but the poster run fails (exit 1, nothing written).
const POSTER_FAIL: &str = r#"for a; do out="$a"; done
case " $* " in *" -update "*) exit 1;; esac
cat > "$out""#;

fn png_env(s: &Scratch, script: &Path, attach: Attach) -> TestEnv {
    let mut e = env(s, script, attach);
    e.ffmpeg = Ok(fake_ffmpeg(script, PNG_ENCODERS));
    e
}

/// Every entry name of the recordings dir (videos and sidecars), sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn mode(p: &Path) -> u32 {
    std::fs::metadata(p).expect("meta").permissions().mode() & 0o777
}

/// The poster path a video gets: `<video stem>.poster.png` next to it.
fn poster_of(video: &Path) -> PathBuf {
    let stem = video
        .file_stem()
        .expect("stem")
        .to_string_lossy()
        .into_owned();
    video.with_file_name(format!("{stem}.poster.png"))
}

/// The number right after `key` (e.g. ". Marker at " -> 1.23).
fn number_after(t: &str, key: &str) -> f64 {
    t.split(key)
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.trim_end_matches(',').parse().ok())
        .unwrap_or_else(|| panic!("no number after {key:?} in {t}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_reports_poster_path() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ev1", POSTER_OK);
        let fake = FakeCdp::start(frames(20)).await;
        let e = png_env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        let t = okay(h.start(&e, "demo").await);
        let video = started_path(&t);
        fake.wait_emitter_done().await;
        let r = okay(h.stop(&e).await);
        let poster = poster_of(&video);
        assert!(
            r.contains(&format!(". Poster: {}", poster.display())),
            "{r}"
        );
        let bytes = std::fs::read(&poster).expect("poster file");
        assert!(is_png(&bytes), "poster is not a PNG");
        assert_eq!(mode(&poster), 0o600);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poster_not_made_without_png_encoder() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ev2", POSTER_OK);
        let fake = FakeCdp::start(frames(20)).await;
        // env() lists the video encoders only: no png.
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        let t = okay(h.start(&e, "demo").await);
        let video = started_path(&t);
        fake.wait_emitter_done().await;
        let r = okay(h.stop(&e).await);
        assert!(r.starts_with("recording saved: "), "{r}");
        assert!(
            r.contains(". Poster: not made (ffmpeg lists no png encoder)"),
            "{r}"
        );
        assert!(!poster_of(&video).exists());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poster_failure_never_fails_the_stop() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ev3", POSTER_FAIL);
        let fake = FakeCdp::start(frames(20)).await;
        let e = png_env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        let t = okay(h.start(&e, "demo").await);
        let video = started_path(&t);
        fake.wait_emitter_done().await;
        let r = okay(h.stop(&e).await);
        assert!(r.starts_with("recording saved: "), "{r}");
        assert!(r.contains(". Poster: not made (ffmpeg exited"), "{r}");
        assert!(!poster_of(&video).exists());
        assert!(
            names_in(&s.0.join("recordings"))
                .iter()
                .all(|n| !n.ends_with(".poster.png")),
            "a sidecar was left behind"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn marker_offset_in_result() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ev4", OK_SCRIPT);
        let fake = FakeCdp::start(frames(40)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "marked").await);
        tokio::time::sleep(Duration::from_millis(600)).await;
        let r = okay(
            h.stop_marked(&e, MarkerIn::Note("demo note here".into()))
                .await,
        );
        let at = number_after(&r, ". Marker at ");
        let dur = number_after(&r, "duration ");
        assert!(
            at >= 0.3 && at <= dur + 0.05,
            "marker {at} vs duration {dur}: {r}"
        );
        // The note is the agent's text: it is never echoed back.
        assert!(!r.contains("demo note here"), "{r}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn marker_bad_value_never_blocks_stop() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ev5", OK_SCRIPT);
        let fake = FakeCdp::start(frames(20)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        let t = okay(h.start(&e, "bad").await);
        let video = started_path(&t);
        fake.wait_emitter_done().await;
        let r = okay(
            h.stop_marked(&e, MarkerIn::Bad("value must be a string"))
                .await,
        );
        assert!(r.starts_with("recording saved: "), "{r}");
        assert!(
            r.contains(". Marker not placed: value must be a string"),
            "{r}"
        );
        assert!(!h.is_active());
        assert!(std::fs::metadata(&video).expect("video").len() > 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn marker_without_recording() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ev6", OK_SCRIPT);
        let e = env(&s, &script, Attach::Ws(String::new()));
        let mut h = hub();
        let r = err(h.stop_marked(&e, MarkerIn::Note("x".into())).await);
        assert_eq!(
            r,
            "no active recording; record-start <name> first. Marker not placed: no active recording"
        );
        assert!(!r.contains(".."), "{r}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plain_stop_is_unchanged_without_marker() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ev7", OK_SCRIPT);
        let fake = FakeCdp::start(frames(20)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "plain").await);
        fake.wait_emitter_done().await;
        let r = okay(h.stop(&e).await);
        assert!(r.starts_with("recording saved: "), "{r}");
        assert!(!r.contains("Marker"), "{r}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_extras_on_orphan_paths() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ev8", POSTER_OK);
        let dir = s.0.join("recordings");
        // Each exit path gets its own fake browser: a fake connection is single-use.
        // Each exit path finalizes; its parked result must carry no extras.
        let fake = FakeCdp::start(frames(40)).await;
        let e = png_env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "orphan").await);
        h.finish_orphan().await;
        let t = match h.stop(&e).await {
            Ok(t) => t,
            Err(x) => x.to_string(),
        };
        assert!(!t.contains("Poster:") && !t.contains("Marker"), "{t}");

        let fake = FakeCdp::start(frames(40)).await;
        let e = png_env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "interrupt").await);
        let gen = h.active_gen().expect("gen");
        assert!(h.interrupt_if(gen, Interrupt::BrowserLost).await);
        let t = match h.stop(&e).await {
            Ok(t) => t,
            Err(x) => x.to_string(),
        };
        assert!(!t.contains("Poster:") && !t.contains("Marker"), "{t}");

        let fake = FakeCdp::start(frames(40)).await;
        let e = png_env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "limit").await);
        let gen = h.active_gen().expect("gen");
        assert!(h.stop_on_limit(gen, Limit::MaxDuration).await);
        let t = match h.stop(&e).await {
            Ok(t) => t,
            Err(x) => x.to_string(),
        };
        assert!(!t.contains("Poster:") && !t.contains("Marker"), "{t}");

        // No sidecar was written on any exit path.
        assert!(
            names_in(&dir).iter().all(|n| n.ends_with(".webm")),
            "{:?}",
            names_in(&dir)
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn per_tab_poster_from_the_followed_tab() {
    rig_guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::with("poster-tab", &a, POSTER_OK, Tabs::PerTab, LONG)
            .await
            .with_png();
        r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        // The agent is on tab 2 at stop: the poster comes from its file.
        *r.env.tab.lock().expect("tab") = "B".into();
        let t = okay(r.hub.stop(&r.env).await);
        let dir = r.s.0.join("recordings");
        let tab2 = names_in(&dir)
            .into_iter()
            .find(|n| n.contains("-tab2-") && n.ends_with(".webm"))
            .expect("tab2 file");
        let poster = format!("{}.poster.png", tab2.trim_end_matches(".webm"));
        assert!(t.contains(&poster), "{t}");
        assert_eq!(
            names_in(&dir)
                .iter()
                .filter(|n| n.ends_with(".poster.png"))
                .count(),
            1,
            "{:?}",
            names_in(&dir)
        );
        assert!(is_png(&std::fs::read(dir.join(&poster)).expect("poster")));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn result_is_scrubbed() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-ev9", OK_SCRIPT);
        let fake = FakeCdp::start(frames(20)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "scrub").await);
        fake.wait_emitter_done().await;
        // `okay` panics on any dead-tab substring; the note is never echoed.
        let r = okay(
            h.stop_marked(&e, MarkerIn::Note("Target closed".into()))
                .await,
        );
        assert!(!r.contains("Target closed"), "{r}");
        assert!(r.contains(". Marker at "), "{r}");
    })
    .await;
}

/// `<video stem>.timeline.json` next to the video.
fn timeline_of(video: &Path) -> PathBuf {
    let stem = video
        .file_stem()
        .expect("stem")
        .to_string_lossy()
        .into_owned();
    video.with_file_name(format!("{stem}.timeline.json"))
}

fn read_json(p: &Path) -> Value {
    let bytes = std::fs::read(p).expect("timeline file");
    serde_json::from_slice(&bytes).expect("timeline is JSON")
}

/// A finished call that started and ended at `at` (its verdict is a fixture).
fn entry_at(at: Instant, action: &str) -> Entry {
    Entry {
        start: at,
        end: at,
        tool: "act".into(),
        action: action.into(),
        steps: None,
        ok: true,
        verdict: "clicked".into(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeline_written_at_stop() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-tl1", OK_SCRIPT);
        let fake = FakeCdp::start(frames(40)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        let t = okay(h.start(&e, "tl").await);
        let video = started_path(&t);
        tokio::time::sleep(Duration::from_millis(600)).await;
        let log = h.timeline_log().expect("log while active");
        let now = Instant::now();
        for (back, action) in [(400u64, "click"), (200, "type"), (0, "press")] {
            log.push(entry_at(now - Duration::from_millis(back), action));
        }
        let r = okay(h.stop_marked(&e, MarkerIn::Note("boom".into())).await);
        let tl = timeline_of(&video);
        assert!(
            r.contains(&format!(". Timeline: {} (3 actions)", tl.display())),
            "{r}"
        );
        assert_eq!(mode(&tl), 0o600);
        let v = read_json(&tl);
        assert_eq!(v["version"], 1, "{v}");
        assert_eq!(v["dropped"], 0, "{v}");
        let entries = v["entries"].as_array().expect("entries");
        assert_eq!(entries.len(), 4, "{v}");
        assert_eq!(entries[3]["kind"], "marker", "marker sorts last: {v}");
        assert_eq!(entries[3]["note"], "boom", "{v}");
        let dur = v["duration_s"].as_f64().expect("duration_s");
        let ats: Vec<f64> = entries
            .iter()
            .map(|x| x["at_s"].as_f64().expect("at_s"))
            .collect();
        assert!(ats.windows(2).all(|w| w[0] <= w[1]), "{ats:?}");
        for x in entries {
            assert!(x["at_s"].as_f64().expect("at_s") <= dur + 1e-9, "{x}");
            if let Some(end) = x.get("end_s").and_then(Value::as_f64) {
                assert!(end <= dur + 1e-9, "{x}");
            }
        }
        let marker_t = number_after(&r, ". Marker at ");
        assert!(
            (ats[3] - marker_t).abs() <= 0.05,
            "timeline marker {} vs result {marker_t}: {r}",
            ats[3]
        );
        // The note is kept in the file (escaped JSON) but never echoed.
        assert!(!r.contains("boom"), "{r}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeline_with_no_actions() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-tl2", OK_SCRIPT);
        let fake = FakeCdp::start(frames(20)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        let t = okay(h.start(&e, "empty").await);
        let video = started_path(&t);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let r = okay(h.stop(&e).await);
        assert!(r.contains(" (0 actions)"), "{r}");
        let v = read_json(&timeline_of(&video));
        assert_eq!(v["entries"], serde_json::json!([]), "{v}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropped_entries_are_counted() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-tl3", OK_SCRIPT);
        let fake = FakeCdp::start(frames(20)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        let t = okay(h.start(&e, "many").await);
        let video = started_path(&t);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let log = h.timeline_log().expect("log");
        let now = Instant::now();
        for _ in 0..MAX_ENTRIES + 2 {
            log.push(entry_at(now, "click"));
        }
        let r = okay(h.stop(&e).await);
        assert!(r.contains("(1000 actions, 2 not kept)"), "{r}");
        let v = read_json(&timeline_of(&video));
        assert_eq!(v["dropped"], 2, "{v}");
        assert_eq!(v["entries"].as_array().map(Vec::len), Some(MAX_ENTRIES));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn timeline_offsets_match_the_segment_clock() {
    rig_guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        // B's capture timestamps start at the wall-clock switch (~1.0 s after
        // A's first frame), so the result's segment offset is comparable.
        let b = FakeCdp::start(fake(400, 1.0)).await;
        let mut r = Rig::new("tl-seg", &a, LONG).await;
        let path = r.start().await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let switched = Instant::now();
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(800)).await;
        r.hub
            .timeline_log()
            .expect("log")
            .push(entry_at(switched, "switch-tab"));
        let t = okay(r.hub.stop(&r.env).await);
        let seg_b: f64 = segments(&t)
            .split(", ")
            .find(|p| p.ends_with("tab B"))
            .and_then(|p| p.split(' ').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("segment B in {t}"));
        let v = read_json(&timeline_of(&path));
        let at = v["entries"][0]["at_s"].as_f64().expect("at_s");
        assert!(
            (at - seg_b).abs() <= 0.35,
            "entry {at} vs segment B {seg_b}: {t}"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn timeline_on_not_clean_stop_still_written() {
    rig_guard(async {
        let a1 = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(FakeOpts::default()).await; // never sends a frame
        let a2 = FakeCdp::start(fake(400, 8.0)).await;
        let mut r = Rig::new("tl-zero", &a1, Duration::from_secs(1)).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        r.set(vec![view("B", &b.url), view("A", &a1.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        r.set(vec![view("A", &a2.url), view("B", &b.url)]);
        until("A2 started", || started(&a2)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let t = err(r.hub.stop(&r.env).await);
        assert!(t.contains("tab B: no frames for "), "{t}");
        assert!(t.contains(". Timeline: "), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn per_tab_timeline_lists_files_with_start() {
    rig_guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = Rig::with("tl-tab", &a, OK_SCRIPT, Tabs::PerTab, LONG).await;
        // The start result gives the canonical path (the scratch dir may be a
        // symlink on macOS, so it is never rebuilt from the scratch root).
        let first = r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.hub
            .timeline_log()
            .expect("log")
            .push(entry_at(Instant::now(), "click"));
        let t = okay(r.hub.stop(&r.env).await);
        let tab1 = first
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        assert!(tab1.contains("-tab1-") && tab1.ends_with(".webm"), "{tab1}");
        let tl = timeline_of(&first);
        assert!(
            t.contains(&format!(". Timeline: {} (1 action)", tl.display())),
            "{t}"
        );
        let v = read_json(&tl);
        assert_eq!(v["video"].as_str(), Some(tab1.as_str()), "{v}");
        let files = v["files"].as_array().expect("files");
        assert_eq!(files.len(), 2, "{v}");
        assert_eq!(files[0]["file"].as_str(), Some(tab1.as_str()), "{v}");
        assert_eq!(files[0]["start_s"].as_f64(), Some(0.0), "{v}");
        assert!(files[1]["start_s"].as_f64().expect("start") > 0.0, "{v}");
        assert!(v["entries"][0]["at_s"].as_f64().expect("at_s") > 0.0, "{v}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_timeline_on_orphan_paths() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-tl4", OK_SCRIPT);
        let dir = s.0.join("recordings");
        // Each exit path closes the log: an entry pushed afterwards is ignored.
        let closed = |log: &Log| {
            log.push(entry_at(Instant::now(), "late"));
            assert_eq!(log.len(), 0, "a closed log keeps no entries");
        };

        let fake = FakeCdp::start(frames(40)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "orphan").await);
        let log = h.timeline_log().expect("log");
        log.push(entry_at(Instant::now(), "before"));
        h.finish_orphan().await;
        closed(&log);
        assert!(!h.orphaned_text().unwrap_or_default().contains("Timeline:"));

        let fake = FakeCdp::start(frames(40)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "interrupt").await);
        let gen = h.active_gen().expect("gen");
        let log = h.timeline_log().expect("log");
        assert!(h.interrupt_if(gen, Interrupt::BrowserLost).await);
        closed(&log);
        assert!(!h.orphaned_text().unwrap_or_default().contains("Timeline:"));

        let fake = FakeCdp::start(frames(40)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        okay(h.start(&e, "limit").await);
        let gen = h.active_gen().expect("gen");
        let log = h.timeline_log().expect("log");
        assert!(h.stop_on_limit(gen, Limit::MaxDuration).await);
        closed(&log);
        assert!(!h.orphaned_text().unwrap_or_default().contains("Timeline:"));

        assert!(
            names_in(&dir)
                .iter()
                .all(|n| !n.ends_with(".timeline.json")),
            "{:?}",
            names_in(&dir)
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stale_begin_after_stop_is_dropped() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-tl5", OK_SCRIPT);
        let fake_a = FakeCdp::start(frames(40)).await;
        let e = env(&s, &script, Attach::Ws(fake_a.url.clone()));
        let mut h = hub();
        let t = okay(h.start(&e, "a").await);
        let video_a = started_path(&t);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let log_a = h.timeline_log().expect("log a");
        // A call that began while A records and ends after A stopped.
        let args = serde_json::json!({ "action": "click" });
        let pending = begin_with(true, Some(log_a), "act", &args).expect("begin");
        okay(h.stop(&e).await);
        let a_json = std::fs::read(timeline_of(&video_a)).expect("A timeline");

        let fake_b = FakeCdp::start(frames(40)).await;
        let e2 = env(&s, &script, Attach::Ws(fake_b.url.clone()));
        okay(h.start(&e2, "b").await);
        let log_b = h.timeline_log().expect("log b");
        let done: crate::error::Result<String> = Ok("clicked".into());
        end(Some(pending), &done);
        assert_eq!(log_b.len(), 0, "a stale entry reached the next recording");
        assert_eq!(
            std::fs::read(timeline_of(&video_a)).expect("A again"),
            a_json,
            "A's timeline changed after its stop"
        );
        okay(h.stop(&e2).await);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeline_log_tracks_the_active_recording() {
    guard(async {
        let s = Scratch::new();
        let script = s.script("fake-enc-tl6", OK_SCRIPT);
        let fake = FakeCdp::start(frames(20)).await;
        let e = env(&s, &script, Attach::Ws(fake.url.clone()));
        let mut h = hub();
        assert!(h.timeline_log().is_none(), "no log while idle");
        okay(h.start(&e, "track").await);
        let log = h.timeline_log().expect("log while active");
        assert!(log.is_empty());
        okay(h.stop(&e).await);
        assert!(h.timeline_log().is_none(), "no log after stop");
    })
    .await;
}

#[test]
fn timeline_text_counts_one_action_singular() {
    use super::evidence::timeline_text;
    let p = PathBuf::from("/r/v.timeline.json");
    let t = |n: usize, k: u64| timeline_text(&Ok((p.clone(), n, k)));
    assert_eq!(t(1, 0), ". Timeline: /r/v.timeline.json (1 action)");
    assert_eq!(t(0, 0), ". Timeline: /r/v.timeline.json (0 actions)");
    assert_eq!(t(2, 0), ". Timeline: /r/v.timeline.json (2 actions)");
    assert_eq!(
        t(1, 3),
        ". Timeline: /r/v.timeline.json (1 action, 3 not kept)"
    );
    assert_eq!(
        t(1000, 2),
        ". Timeline: /r/v.timeline.json (1000 actions, 2 not kept)"
    );
    assert_eq!(
        timeline_text(&Err("disk full".into())),
        ". Timeline: not written (disk full)"
    );
}

#[test]
fn join_note_never_doubles_the_period() {
    let note = ". Marker not placed: no active recording";
    assert_eq!(
        msg::join_note("first.", note),
        "first. Marker not placed: no active recording"
    );
    assert_eq!(
        msg::join_note(&msg::none(), note),
        "no active recording; record-start <name> first. Marker not placed: no active recording"
    );
    assert_eq!(
        msg::join_note("saved: /p (bytes 1)", ". Marker at 1.0 s"),
        "saved: /p (bytes 1). Marker at 1.0 s"
    );
    assert_eq!(msg::join_note("first.", ""), "first.");
    assert_eq!(msg::join_note("first.", "x"), "first.x");
}
