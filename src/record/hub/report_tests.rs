//! Outcome table for `compose` and guards on every hub message.

use std::path::PathBuf;

use super::msg::{self, scrub};
use super::report::{compose, Segment};
use crate::record::encoder::{EncoderReport, Outcome};
use crate::record::source::{Health, SourceOutcome, SourceReport, SourceStats};

const DEAD: [&str; 4] = [
    "Target closed",
    "No target with given id",
    "Session closed",
    "Target.detachedFromTarget",
];

fn assert_clean(s: &str) {
    for d in DEAD {
        assert!(!s.contains(d), "{d} in {s}");
    }
}

fn src(outcome: SourceOutcome, forwarded: u64, lost: u64, quiet: Option<u64>) -> SourceReport {
    SourceReport {
        outcome,
        stats: SourceStats {
            frames_received: forwarded + lost,
            forwarded,
            lost,
            ..Default::default()
        },
        target_id: "T1".into(),
        elapsed_ms: 8000,
        last_frame_age_ms: None,
        quiet_tail_ms: quiet,
        hidden: false,
    }
}

fn enc(outcome: Outcome, written: u64, dropped: u64) -> EncoderReport {
    EncoderReport {
        outcome,
        path: PathBuf::from("/abs/a.webm"),
        bytes: 123456,
        frames_queued: written,
        frames_dropped: dropped,
        frames_written: written,
        frames_unwritten: 0,
        gaps_clamped: 0,
        pid: Some(1),
        finalize_ms: 5,
        stderr_tail: String::new(),
    }
}

fn segs() -> Vec<Segment> {
    vec![Segment {
        at_s: 0.0,
        tab: "T1".into(),
    }]
}

#[test]
fn clean_capture_is_ok_with_all_fields() {
    let (ok, t) = compose(
        &src(SourceOutcome::Captured, 81, 0, None),
        &enc(Outcome::Finalized, 81, 0),
        10,
        &segs(),
        None,
    );
    assert!(ok);
    assert!(t.starts_with("recording saved: /abs/a.webm"), "{t}");
    for want in [
        "bytes 123456",
        "duration 8.1 s",
        "frames 81",
        "dropped 0",
        "segments [0.0 s tab T1]",
    ] {
        assert!(t.contains(want), "{want} in {t}");
    }
    assert!(!t.contains("captured"), "{t}");
    assert_clean(&t);
}

#[test]
fn drops_add_up_and_captured_shows_when_different() {
    let (ok, t) = compose(
        &src(SourceOutcome::Captured, 70, 2, None),
        &enc(Outcome::Finalized, 81, 3),
        10,
        &segs(),
        None,
    );
    assert!(ok);
    assert!(t.contains("dropped 5"), "{t}");
    assert!(t.contains("captured 70"), "{t}");
}

#[test]
fn quiet_tail_is_noted_but_still_ok() {
    let (ok, t) = compose(
        &src(SourceOutcome::Captured, 50, 0, Some(7000)),
        &enc(Outcome::Finalized, 50, 0),
        10,
        &segs(),
        None,
    );
    assert!(ok);
    assert!(t.contains("no new frames for 7.0 s"), "{t}");
}

#[test]
fn no_frames_is_an_error_with_the_encoder_verdict() {
    let (ok, t) = compose(
        &src(
            SourceOutcome::NoFrames {
                waited_ms: 6000,
                hidden: false,
            },
            0,
            0,
            None,
        ),
        &enc(Outcome::NoFrames, 0, 0),
        10,
        &segs(),
        None,
    );
    assert!(!ok);
    assert!(t.starts_with("recording not clean: "), "{t}");
    assert!(t.contains("no frames reached the encoder"), "{t}");
    assert!(t.contains("frames 0"), "{t}");
    assert!(!t.contains("recording saved"), "{t}");
}

#[test]
fn connection_lost_keeps_the_file_facts() {
    let (ok, t) = compose(
        &src(
            SourceOutcome::ConnectionLost { after_ms: 4000 },
            40,
            0,
            None,
        ),
        &enc(Outcome::Finalized, 40, 0),
        10,
        &segs(),
        None,
    );
    assert!(!ok);
    assert!(t.contains("capture interrupted"), "{t}");
    assert!(t.contains("duration 4.0 s"), "{t}");
    assert!(t.contains("/abs/a.webm"), "{t}");
    assert!(t.contains("bytes 123456"), "{t}");
    assert!(!t.contains("recording saved"), "{t}");
}

#[test]
fn every_bad_encoder_outcome_is_an_error() {
    let rows: Vec<(Outcome, &str)> = vec![
        (Outcome::Truncated { unwritten: 4 }, "recording truncated"),
        (
            Outcome::Unfinalized {
                waited_ms: 10_000,
                reaped: true,
            },
            "recording unfinalized",
        ),
        (
            Outcome::ExitFailed {
                status: "exit code 1".into(),
            },
            "ffmpeg exited with exit code 1",
        ),
        (
            Outcome::StoppedReading {
                error: "broken pipe".into(),
            },
            "stopped reading frames early",
        ),
        (Outcome::EmptyOutput, "missing or empty"),
    ];
    for (o, want) in rows {
        let (ok, t) = compose(
            &src(SourceOutcome::Captured, 30, 0, None),
            &enc(o, 30, 0),
            10,
            &segs(),
            None,
        );
        assert!(!ok, "{t}");
        assert!(t.contains(want), "{want} in {t}");
        assert!(!t.contains("recording saved"), "{t}");
        assert_clean(&t);
    }
}

#[test]
fn zero_fps_never_divides() {
    let (_, t) = compose(
        &src(SourceOutcome::Captured, 10, 0, None),
        &enc(Outcome::Finalized, 10, 0),
        0,
        &segs(),
        None,
    );
    assert!(t.contains("duration 0.0 s"), "{t}");
    assert!(!t.contains("NaN") && !t.contains("inf"), "{t}");
}

#[test]
fn tab_note_is_appended() {
    let n = msg::tab_gone("T1", 4.2, "T2");
    let (_, t) = compose(
        &src(SourceOutcome::Captured, 10, 0, None),
        &enc(Outcome::Finalized, 10, 0),
        10,
        &segs(),
        Some(&n),
    );
    assert!(
        t.contains(". Note: tab T1 gone at 4.2 s; following tab T2"),
        "{t}"
    );
}

#[test]
fn messages_are_clean_and_actionable() {
    assert_eq!(
        scrub("x No target with given id y"),
        "x tab gone y",
        "scrub replaces"
    );
    assert_eq!(scrub("Target closed, Session closed"), "tab gone, tab gone");
    let q = Health::Quiet {
        last_age_ms: 6500,
        hidden: false,
    };
    let a = msg::active("/abs/a.webm", 12_300, &q);
    assert!(
        a.contains("/abs/a.webm") && a.contains("record-stop"),
        "{a}"
    );
    assert!(
        a.contains("started 12.3 s ago") && a.contains("quiet 6.5 s"),
        "{a}"
    );
    let off = msg::off();
    assert!(off.contains("BLADE_RECORD") && off.contains("bladebro stop"));
    let all = [
        off,
        msg::ephemeral(),
        msg::pipe(),
        msg::real_lane(),
        msg::usage(),
        msg::none(),
        a,
        msg::started("/abs/a.webm", "T1"),
        msg::not_finalized("/abs/a.webm"),
        msg::tab_gone("T1", 4.2, "T2"),
        msg::tab_reconnected("T1", 4.2),
        msg::no_tab(9.0),
        msg::browser_lost(9.0),
        msg::rebind_failed("T2", 3.0, "cannot open the recorder's CDP connection"),
        msg::zero_frames("T2", 6.0),
        msg::follower_stuck(),
        msg::orphan_prefix().to_string(),
    ];
    for m in &all {
        assert_clean(m);
    }
}

#[test]
fn health_labels_cover_every_variant() {
    assert_eq!(
        msg::health_label(&Health::Waiting { for_ms: 10 }),
        "waiting for first frame"
    );
    assert_eq!(
        msg::health_label(&Health::NoFrames {
            for_ms: 6000,
            hidden: true
        }),
        "no frames yet (tab hidden or in background?)"
    );
    assert_eq!(
        msg::health_label(&Health::Receiving { last_age_ms: 5 }),
        "receiving"
    );
    assert_eq!(
        msg::health_label(&Health::Quiet {
            last_age_ms: 7000,
            hidden: false
        }),
        "quiet 7.0 s"
    );
}
