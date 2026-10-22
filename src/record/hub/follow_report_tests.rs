//! Pure tables for the per-tab stop result.

use std::path::PathBuf;

use super::follow_report::{compose_per_tab, FileOutcome, PerTabResult};
use super::msg;
use super::report::Segment;
use crate::record::encoder::{EncoderReport, Outcome};
use crate::record::source::{SourceOutcome, SourceReport, SourceStats};

fn src(frames: u64) -> SourceReport {
    SourceReport {
        outcome: SourceOutcome::Captured,
        stats: SourceStats {
            frames_received: frames,
            forwarded: frames,
            ..Default::default()
        },
        target_id: "x".into(),
        elapsed_ms: 1000,
        last_frame_age_ms: None,
        quiet_tail_ms: None,
        hidden: false,
    }
}

fn enc(path: &str, outcome: Outcome) -> EncoderReport {
    EncoderReport {
        outcome,
        path: PathBuf::from(path),
        bytes: 10,
        frames_queued: 20,
        frames_dropped: 0,
        frames_written: 20,
        frames_unwritten: 0,
        gaps_clamped: 0,
        pid: Some(1),
        finalize_ms: 5,
        stderr_tail: "boom".into(),
    }
}

fn seg(at: f64, tab: &str) -> Segment {
    Segment {
        at_s: at,
        tab: tab.into(),
    }
}

fn file(n: u32, tab: &str, outcome: Outcome, segs: Vec<Segment>) -> FileOutcome {
    let path = format!("/p{n}");
    FileOutcome {
        tab: tab.into(),
        n,
        path: PathBuf::from(&path),
        src: src(20),
        enc: Ok(enc(&path, outcome)),
        segments: segs,
        failures: Vec::new(),
        closed_at: None,
    }
}

fn two_clean() -> PerTabResult {
    PerTabResult {
        files: vec![
            file(
                1,
                "A",
                Outcome::Finalized,
                vec![seg(0.0, "A"), seg(6.1, "A")],
            ),
            file(2, "B", Outcome::Finalized, vec![seg(0.0, "B")]),
        ],
        ..Default::default()
    }
}

#[test]
fn two_clean_files_are_listed_in_one_saved_line() {
    let (ok, t) = compose_per_tab(&two_clean(), 10);
    assert!(ok, "{t}");
    assert!(
        t.starts_with("recording saved (per-tab, 2 files): /p1 (bytes 10, duration 2.0 s, frames 20, dropped 0, segments [0.0 s tab A, 6.1 s tab A]); /p2 ("),
        "{t}"
    );
    assert!(t.contains("segments [0.0 s tab B]"), "{t}");
    assert!(!t.contains("Note"), "{t}");
}

#[test]
fn a_failed_encoder_makes_only_its_file_not_clean_and_all_files_are_listed() {
    let mut r = two_clean();
    r.files[1] = file(
        2,
        "B",
        Outcome::ExitFailed {
            status: "exit code 1".into(),
        },
        vec![seg(0.0, "B")],
    );
    let (ok, t) = compose_per_tab(&r, 10);
    assert!(!ok);
    assert!(
        t.starts_with("recording not clean (per-tab, 2 files): tab B: recording failed: ffmpeg exited with exit code 1"),
        "{t}"
    );
    assert!(t.contains("/p1 (") && t.contains("/p2 ("), "{t}");
    assert!(!t.contains("recording saved"), "{t}");
}

#[test]
fn the_cap_note_keeps_a_clean_result_clean() {
    let mut r = two_clean();
    r.notes.push(msg::tab_capped("I", 12.3, 8));
    let (ok, t) = compose_per_tab(&r, 10);
    assert!(ok, "{t}");
    assert!(
        t.ends_with(". Note: tab I not recorded from 12.3 s: per-tab cap of 8 open files reached (close a tab to free one)"),
        "{t}"
    );
}

#[test]
fn a_create_failure_is_not_clean_and_names_the_tab_and_error() {
    let mut r = two_clean();
    r.failures.push(msg::rebind_failed(
        "C",
        4.0,
        "cannot create recording file /x: denied",
    ));
    let (ok, t) = compose_per_tab(&r, 10);
    assert!(!ok);
    assert!(
        t.starts_with("recording not clean (per-tab, 2 files): tab C not recorded from 4.0 s: cannot create recording file /x: denied; /p1"),
        "{t}"
    );
}

#[test]
fn one_file_says_file_not_files_and_a_closed_tab_is_noted() {
    let mut r = PerTabResult {
        files: vec![file(1, "A", Outcome::Finalized, vec![seg(0.0, "A")])],
        ..Default::default()
    };
    r.files[0].closed_at = Some(7.5);
    let (ok, t) = compose_per_tab(&r, 10);
    assert!(
        ok && t.starts_with("recording saved (per-tab, 1 file): "),
        "{t}"
    );
    assert!(t.ends_with(". Note: tab A closed at 7.5 s"), "{t}");
}

#[test]
fn an_unfinalized_file_is_named_without_a_report_and_later_reasons_stay_visible() {
    let mut r = two_clean();
    r.files[0].enc = Err(msg::not_finalized("/p1"));
    r.files[1].failures.push(msg::zero_frames("B", 6.0));
    let (ok, t) = compose_per_tab(&r, 10);
    assert!(!ok);
    assert!(
        t.starts_with("recording not clean (per-tab, 2 files): tab A: recording not finalized"),
        "{t}"
    );
    assert!(t.contains("/p1 (not finalized)"), "{t}");
    assert!(t.contains(". Note: tab B: no frames for 6.0 s"), "{t}");
}

#[test]
fn nothing_scrubbable_survives() {
    let mut r = two_clean();
    r.failures
        .push("tab C: Target closed while Session closed".into());
    let (_, t) = compose_per_tab(&r, 10);
    for d in [
        "Target closed",
        "No target with given id",
        "Session closed",
        "Target.detachedFromTarget",
    ] {
        assert!(!t.contains(d), "{d} in {t}");
    }
}
