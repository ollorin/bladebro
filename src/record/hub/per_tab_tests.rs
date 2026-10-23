//! Per-tab recording (CAPT-04) over the fake ffmpeg script (`cat`), one FakeCdp
//! per tab connection and a scripted tab list. Reuses the rig of
//! `follow_tests`; only the tab mode and the fake ffmpeg bodies differ.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use super::follow::MAX_TAB_FILES;
use super::follow_tests::{
    err, fake, guard, num_after, okay, segments, started, until, view, Rig, FRAME_BYTES, LONG,
    OK_SCRIPT,
};
use super::machine::{Hub, Knobs};
use crate::record::source::fake_cdp::FakeCdp;
use crate::record::Tabs;

/// Tab 2's fake ffmpeg drains its input and exits 1 (a failing encoder).
const FAIL_TAB2: &str = r#"for a; do out="$a"; done
case "$out" in *-tab2-*) cat > /dev/null; exit 1;; esac
cat > "$out""#;

/// Tab 2's fake ffmpeg ignores EOF and never exits on its own.
const HANG_TAB2: &str = r#"for a; do out="$a"; done
case "$out" in *-tab2-*) exec sleep 4242.7;; esac
cat > "$out""#;

async fn rig(name: &str, first: &FakeCdp, body: &str) -> Rig {
    Rig::with(name, first, body, Tabs::PerTab, LONG).await
}

/// The result's file lines: `<path> (bytes ... segments [...]...)`. A verdict
/// before them may itself contain "; ", so they are found by their `(bytes`.
fn lines(t: &str) -> Vec<String> {
    t.match_indices(" (bytes ")
        .map(|(i, _)| {
            let pre = &t[..i];
            let start = [
                pre.rfind("; ").map(|x| x + 2),
                pre.rfind("): ").map(|x| x + 3),
            ]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(0);
            let end = i + t[i..].find(')').map_or(0, |c| c + 1);
            t[start..end].to_string()
        })
        .collect()
}

fn path_of(line: &str) -> PathBuf {
    PathBuf::from(line.split(" (bytes").next().expect("path"))
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

fn count_alive(needle: &str) -> usize {
    let o = Command::new("pgrep")
        .args(["-f", needle])
        .output()
        .expect("pgrep");
    String::from_utf8_lossy(&o.stdout).lines().count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_b_c_a_makes_three_files_and_the_return_reuses_the_first() {
    guard(async {
        let a1 = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 4.2)).await;
        let c = FakeCdp::start(fake(400, 6.0)).await;
        let a2 = FakeCdp::start(fake(400, 9.8)).await;
        let mut r = rig("abca", &a1, OK_SCRIPT).await;
        let first = r.start().await;
        assert!(first.display().to_string().contains("-tab1-"), "{first:?}");
        tokio::time::sleep(Duration::from_millis(400)).await;
        r.set(vec![view("B", &b.url), view("A", &a1.url)]);
        until("B started", || started(&b)).await;
        assert_eq!(r.encoders_alive(), 2, "one ffmpeg per tab file");
        tokio::time::sleep(Duration::from_millis(400)).await;
        r.set(vec![
            view("C", &c.url),
            view("B", &b.url),
            view("A", &a1.url),
        ]);
        until("C started", || started(&c)).await;
        assert_eq!(r.encoders_alive(), 3, "three distinct ffmpeg processes");
        tokio::time::sleep(Duration::from_millis(400)).await;
        r.set(vec![
            view("A", &a2.url),
            view("C", &c.url),
            view("B", &b.url),
        ]);
        until("A2 started", || started(&a2)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(r.encoders_alive(), 3, "the return opened no fourth file");
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved (per-tab, 3 files): "), "{t}");
        let l = lines(&t);
        assert_eq!(l.len(), 3, "{t}");
        for (i, (line, tab)) in l.iter().zip(["A", "B", "C"]).enumerate() {
            let p = path_of(line);
            assert!(
                p.display().to_string().contains(&format!("-tab{}-", i + 1)),
                "{line}"
            );
            // Every file is exactly the frames its encoder wrote.
            let frames = num_after(line, "frames ") as u64;
            assert_eq!(
                num_after(line, "bytes ") as u64,
                frames * FRAME_BYTES,
                "{line}"
            );
            assert_eq!(
                std::fs::metadata(&p).expect("file").len(),
                frames * FRAME_BYTES
            );
            assert!(
                segments(line).contains(&format!("0.0 s tab {tab}")),
                "{line}"
            );
        }
        assert_eq!(segments(&l[0]), "0.0 s tab A, 9.8 s tab A", "{t}");
        assert_eq!(segments(&l[1]), "0.0 s tab B", "{t}");
        assert_eq!(files_in(&r).len(), 3, "exactly three files on disk");
        assert_eq!(r.encoders_alive(), 0, "no ffmpeg left");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_a_tab_finalizes_its_file_before_the_stop() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let c = FakeCdp::start(fake(400, 6.0)).await;
        let mut r = rig("close", &a, OK_SCRIPT).await;
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
        // B is closed while the agent is on C.
        r.set(vec![view("C", &c.url), view("A", &a.url)]);
        until("B's ffmpeg finished", || r.encoders_alive() == 2).await;
        let b_file = files_in(&r)
            .into_iter()
            .find(|f| f.contains("-tab2-"))
            .expect("tab2 file");
        let size = std::fs::metadata(r.s.0.join("recordings").join(&b_file))
            .expect("meta")
            .len();
        assert!(size > 0 && size % FRAME_BYTES == 0, "{size}");
        tokio::time::sleep(Duration::from_millis(300)).await;
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved (per-tab, 3 files): "), "{t}");
        assert!(t.contains(". Note: tab B closed at "), "{t}");
        assert_eq!(r.encoders_alive(), 0);
        // B's bytes in the result are what was on disk before the stop.
        assert!(t.contains(&format!("bytes {size},")), "{t}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_ninth_open_tab_is_not_recorded_and_a_freed_slot_is_reused() {
    guard(async {
        assert_eq!(MAX_TAB_FILES, 8);
        let mut fakes = Vec::new();
        for i in 0..11 {
            fakes.push(FakeCdp::start(fake(600, i as f64 * 1.5)).await);
        }
        let id = |i: usize| format!("T{}", i + 1);
        let mut r = rig("cap", &fakes[0], OK_SCRIPT).await;
        r.start().await;
        let mut tabs = vec![view("A", &fakes[0].url)];
        #[allow(clippy::needless_range_loop)] // `i` also names the tab
        for i in 1..9 {
            tokio::time::sleep(Duration::from_millis(150)).await;
            tabs.insert(0, view(&id(i), &fakes[i].url));
            r.set(tabs.clone());
            if i < 8 {
                until(&format!("T{} started", i + 1), || started(&fakes[i])).await;
            } else {
                // The 9th has no file: nothing connects; give it time to settle.
                tokio::time::sleep(Duration::from_millis(700)).await;
                assert!(!started(&fakes[i]), "the 9th tab must not be recorded");
            }
        }
        assert_eq!(r.encoders_alive(), 8, "never more than 8 open files");
        // Close T3 (dormant): its slot frees; the agent opens a 10th tab.
        tabs.retain(|v| v.id != "T3");
        r.set(tabs.clone());
        until("T3's ffmpeg finished", || r.encoders_alive() == 7).await;
        tabs.insert(0, view("T10", &fakes[9].url));
        r.set(tabs.clone());
        until("T10 started", || started(&fakes[9])).await;
        assert_eq!(r.encoders_alive(), 8);
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved (per-tab, 9 files): "), "{t}");
        assert!(
            t.contains("tab T9 not recorded from ")
                && t.contains("per-tab cap of 8 open files reached (close a tab to free one)"),
            "{t}"
        );
        assert!(t.contains("tab T3 closed at "), "{t}");
        assert!(
            files_in(&r).iter().any(|f| f.contains("-tab9-")),
            "T10 is file 9"
        );
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_failing_encoder_marks_only_its_file_not_clean() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let c = FakeCdp::start(fake(400, 6.0)).await;
        let mut r = rig("fail", &a, FAIL_TAB2).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("C", &c.url), view("B", &b.url), view("A", &a.url)]);
        until("C started", || started(&c)).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let t = err(r.hub.stop(&r.env).await);
        assert!(
            t.starts_with("recording not clean (per-tab, 3 files): tab B: recording failed: ffmpeg exited with exit code 1"),
            "{t}"
        );
        assert!(!t.contains("recording saved"), "{t}");
        let l = lines(&t);
        // Files 1 and 3 are intact and listed with their own facts.
        assert_eq!(l.len(), 3, "{t}");
        for line in [&l[0], &l[2]] {
            let frames = num_after(line, "frames ") as u64;
            assert_eq!(
                std::fs::metadata(path_of(line)).expect("file").len(),
                frames * FRAME_BYTES,
                "{line}"
            );
        }
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_file_that_cannot_be_created_is_named_and_the_first_file_survives() {
    use std::os::unix::fs::PermissionsExt;
    struct Restore(PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("nocreate", &a, OK_SCRIPT).await;
        r.start().await;
        let dir = r.s.0.join("recordings");
        let _restore = Restore(dir.clone());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        if std::fs::File::create(dir.join("probe")).is_ok() {
            return; // permissions are not enforced here (root): nothing to prove
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        tokio::time::sleep(Duration::from_millis(900)).await;
        assert!(!started(&b), "no file, no source");
        let t = err(r.hub.stop(&r.env).await);
        assert!(
            t.starts_with("recording not clean (per-tab, 1 file): tab B not recorded from "),
            "{t}"
        );
        assert!(t.contains("group/other-accessible"), "{t}");
        let l = lines(&t);
        let line = l.last().expect("file line");
        let frames = num_after(line, "frames ") as u64;
        assert!(frames > 0, "{line}");
        assert_eq!(
            std::fs::metadata(path_of(line)).expect("file").len(),
            frames * FRAME_BYTES
        );
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stop_is_bounded_when_one_encoder_ignores_eof() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("hang", &a, HANG_TAB2).await;
        r.hub = Hub::with_knobs(Knobs {
            stop_grace: Some(Duration::from_millis(100)),
            finish_timeout: Some(Duration::from_secs(1)),
            poll: Some(Duration::from_millis(30)),
            confirm: Some(Duration::from_millis(10)),
            new_confirm: Some(Duration::from_millis(150)),
            no_frames_after: Some(LONG),
        });
        r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        // The fake ffmpeg exec()s into `sleep` asynchronously: poll, don't guess.
        until("the hung encoder to exec sleep", || {
            count_alive("sleep 4242.7") == 1
        })
        .await;
        assert_eq!(count_alive("sleep 4242.7"), 1);
        let t0 = Instant::now();
        let t = err(r.hub.stop(&r.env).await);
        // finish_timeout (1 s) + 2 * grace + 5 s is the documented bound.
        assert!(
            t0.elapsed() < Duration::from_millis(6200),
            "{:?}",
            t0.elapsed()
        );
        assert!(
            t.starts_with("recording not clean (per-tab, 2 files): tab B: "),
            "{t}"
        );
        assert!(
            t.contains("unfinalized") || t.contains("truncated") || t.contains("not finalized"),
            "{t}"
        );
        let l = lines(&t);
        assert!(
            l[0].contains("segments [0.0 s tab A]"),
            "file 1 is fine: {l:?}"
        );
        assert_eq!(count_alive("sleep 4242.7"), 0, "the hung ffmpeg was killed");
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn finish_orphan_finalizes_every_file_and_lists_them() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("orphan", &a, OK_SCRIPT).await;
        r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.hub.finish_orphan().await;
        assert!(!r.hub.is_active());
        let t = r.hub.orphaned_text().expect("orphan text").to_string();
        assert!(t.starts_with("recording saved (per-tab, 2 files): "), "{t}");
        assert_eq!(lines(&t).len(), 2, "{t}");
        assert_eq!(r.encoders_alive(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_second_start_names_the_first_file_and_the_file_count() {
    guard(async {
        let a = FakeCdp::start(fake(400, 0.0)).await;
        let b = FakeCdp::start(fake(400, 3.0)).await;
        let mut r = rig("double", &a, OK_SCRIPT).await;
        let first = r.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.set(vec![view("B", &b.url), view("A", &a.url)]);
        until("B started", || started(&b)).await;
        // The file count is the follower's per-tick snapshot: allow one tick.
        let mut e = err(r.hub.start(&r.env, "again").await);
        for _ in 0..100 {
            if e.contains("per-tab, 2 files") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            e = err(r.hub.start(&r.env, "again").await);
        }
        assert!(e.contains(&first.display().to_string()), "{e}");
        assert!(e.contains("per-tab, 2 files"), "{e}");
        assert_eq!(files_in(&r).len(), 2, "the refused start created nothing");
        let t = okay(r.hub.stop(&r.env).await);
        assert!(t.starts_with("recording saved (per-tab, 2 files): "), "{t}");
    })
    .await;
}
