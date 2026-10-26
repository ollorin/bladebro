//! Stop extras over the fake ffmpeg (OUT-06): the poster PNG and the marker
//! offset appear in the record-stop result; a failing extra never changes the
//! stop's ok/err and leaves no file behind. Rigs come from machine_tests and
//! follow_tests (fake CDP, no Chrome).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::env::Attach;
use super::follow_tests::{fake, guard as rig_guard, started, until, view, Rig, LONG};
use super::machine::{Interrupt, Limit};
use super::machine_tests::{env, err, frames, guard, hub, okay, started_path, TestEnv, OK_SCRIPT};
use super::msg;
use crate::record::encoder::test_util::{fake_ffmpeg, Scratch};
use crate::record::marker::MarkerIn;
use crate::record::poster::is_png;
use crate::record::source::fake_cdp::FakeCdp;
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
            format!("{}. Marker not placed: no active recording", msg::none())
        );
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
