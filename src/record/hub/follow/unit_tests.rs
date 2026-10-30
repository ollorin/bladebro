//! Pure tables for the follow building blocks: agent-tab rule, debounce,
//! segment sink, merge, messages.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::merge::{compose_follow, merge, MergeIn, SegIn};
use super::pick::{agent_tab, Baseline, Confirm, Rule};
use super::probe::TabView;
use super::sink::FollowShared;
use crate::record::encoder::{EncoderReport, Outcome};
use crate::record::hub::msg;
use crate::record::source::{FrameSink, SourceOutcome, SourceReport, SourceStats};

fn v(id: &str, attached: bool) -> TabView {
    TabView {
        id: id.into(),
        url: format!("http://x/{id}"),
        attached,
        ws_url: format!("ws://x/{id}"),
    }
}

fn ids(x: &[&str]) -> Vec<String> {
    x.iter().map(|s| s.to_string()).collect()
}

#[test]
fn agent_tab_table() {
    let dev = TabView {
        id: "dev".into(),
        url: "devtools://devtools/x".into(),
        attached: true,
        ws_url: "ws://x/dev".into(),
    };
    type Row<'a> = (&'a str, Vec<TabView>, Vec<String>, Rule, Option<&'a str>);
    let rows: Vec<Row> = vec![
        (
            "agent on bound",
            vec![v("A", true), v("B", false)],
            ids(&["A"]),
            Rule::Attached,
            Some("A"),
        ),
        (
            "agent switched",
            vec![v("A", true), v("B", true)],
            ids(&["A"]),
            Rule::Attached,
            Some("B"),
        ),
        (
            "switched, old still retiring",
            vec![v("A", true), v("B", true), v("C", true)],
            ids(&["A", "B"]),
            Rule::Attached,
            Some("C"),
        ),
        (
            "popup unattached",
            vec![v("P", false), v("A", true)],
            ids(&["A"]),
            Rule::Attached,
            Some("A"),
        ),
        (
            "devtools page ignored",
            vec![dev.clone(), v("A", true)],
            ids(&["A"]),
            Rule::Attached,
            Some("A"),
        ),
        (
            "bound closed, other attached",
            vec![v("B", true)],
            ids(&["A"]),
            Rule::Attached,
            Some("B"),
        ),
        (
            "bound closed, none attached",
            vec![v("C", false), v("D", false)],
            ids(&["A"]),
            Rule::Attached,
            Some("C"),
        ),
        ("empty list", vec![], ids(&["A"]), Rule::Attached, None),
        (
            "list order",
            vec![v("B", false), v("A", true)],
            ids(&["A"]),
            Rule::ListOrder,
            Some("B"),
        ),
        (
            "list order skips devtools",
            vec![dev, v("A", false)],
            ids(&["A"]),
            Rule::ListOrder,
            Some("A"),
        ),
        (
            "list order empty",
            vec![],
            ids(&["A"]),
            Rule::ListOrder,
            None,
        ),
    ];
    for (name, list, bound, rule, want) in rows {
        assert_eq!(agent_tab(&list, &bound, rule).as_deref(), want, "{name}");
    }
}

#[test]
fn confirm_needs_the_same_candidate_twice() {
    let mut c = Confirm::new();
    // Seen once, then reverted: no switch.
    assert!(!c.feed("B", "A"));
    assert!(c.is_pending());
    assert!(!c.feed("A", "A"));
    assert!(!c.is_pending());
    // Seen twice: switch.
    assert!(!c.feed("B", "A"));
    assert!(c.feed("B", "A"));
    // Three different candidates in a row: nothing confirmed until one repeats.
    let mut c = Confirm::new();
    assert!(!c.feed("X", "A"));
    assert!(!c.feed("Y", "A"));
    assert!(!c.feed("Z", "A"));
    assert!(c.feed("Z", "A"));
}

#[test]
fn baseline_ignores_a_lingering_head_until_it_changes() {
    // Popup P was already first at record-start while the agent is on A.
    let mut b = Baseline::new(Some("P"), "A");
    assert_eq!(b.filter(Some("P".into()), "A").as_deref(), Some("A"));
    assert_eq!(b.filter(Some("P".into()), "A").as_deref(), Some("A"));
    // The head changes once (agent switched to B): acted on, baseline gone.
    assert_eq!(b.filter(Some("B".into()), "A").as_deref(), Some("B"));
    assert_eq!(b.filter(Some("P".into()), "B").as_deref(), Some("P"));
    // A head equal to the bound tab never sets a baseline.
    let mut b = Baseline::new(Some("A"), "A");
    assert_eq!(b.filter(Some("P".into()), "A").as_deref(), Some("P"));
}

#[derive(Default)]
struct Count {
    frames: Mutex<Vec<u64>>,
    ends: AtomicU64,
    refuse: bool,
}

impl FrameSink for Count {
    fn send_frame(&self, _j: Vec<u8>, ts: u64) -> bool {
        if self.refuse {
            return false;
        }
        self.frames.lock().expect("frames").push(ts);
        true
    }
    fn mark_end(&self) {
        self.ends.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn segment_sink_filters_generation_and_order() {
    let inner = Arc::new(Count::default());
    let shared = FollowShared::new();
    let g1 = shared.sink(inner.clone(), 1);
    let g2 = shared.sink(inner.clone(), 2);
    assert!(g1.send_frame(vec![1], 1000));
    assert!(g1.send_frame(vec![1], 1100));
    shared.set_gen(2);
    // A retired generation still "succeeds" (no queue-drop count) but goes nowhere.
    assert!(g1.send_frame(vec![1], 1200));
    assert_eq!(shared.stale(), 1);
    // Out of order for the current generation: discarded.
    assert!(g2.send_frame(vec![1], 1050));
    assert_eq!(shared.stale(), 2);
    assert!(g2.send_frame(vec![1], 1300));
    assert_eq!(
        *inner.frames.lock().expect("frames"),
        vec![1000, 1100, 1300]
    );
    assert_eq!(shared.first_ts(1), Some(1000));
    assert_eq!(shared.first_ts(2), Some(1300));
    assert_eq!(shared.t0_ts(), Some(1000));
    assert!(shared.t0_at().is_some());
    // The wrapper never ends the shared video.
    g1.mark_end();
    g2.mark_end();
    assert_eq!(inner.ends.load(Ordering::SeqCst), 0);
}

#[test]
fn segment_sink_reports_a_refusing_inner_and_records_nothing() {
    let inner = Arc::new(Count {
        refuse: true,
        ..Default::default()
    });
    let shared = FollowShared::new();
    let g1 = shared.sink(inner, 1);
    assert!(!g1.send_frame(vec![1], 1000), "queue-full must be visible");
    assert_eq!(shared.first_ts(1), None);
    assert_eq!(shared.t0_ts(), None);
}

fn rep(frames: u64, outcome: SourceOutcome, quiet: Option<u64>) -> SourceReport {
    SourceReport {
        outcome,
        stats: SourceStats {
            frames_received: frames,
            forwarded: frames,
            ..Default::default()
        },
        target_id: "x".into(),
        elapsed_ms: 1000,
        last_frame_age_ms: None,
        quiet_tail_ms: quiet,
        hidden: false,
    }
}

fn seg(
    tab: &str,
    first: Option<u64>,
    wall: u64,
    dur: u64,
    r: SourceReport,
    rebound: bool,
) -> SegIn {
    SegIn {
        tab: tab.into(),
        first_ts_ms: first,
        wall_ms: wall,
        dur_ms: dur,
        report: r,
        rebound,
    }
}

fn input(segs: Vec<SegIn>, t0: Option<u64>) -> MergeIn {
    MergeIn {
        segs,
        t0_ts_ms: t0,
        failures: vec![],
        notes: vec![],
        no_frames_after_ms: 5000,
    }
}

fn texts(m: &super::merge::Merged) -> Vec<String> {
    m.segments
        .iter()
        .map(|s| format!("{:.1} {}", s.at_s, s.tab))
        .collect()
}

#[test]
fn merge_table() {
    let cap = SourceOutcome::Captured;
    // (a) two segments with frames: clean, stats summed, offsets from first_ts.
    let m = merge(input(
        vec![
            seg("A", Some(10_000), 0, 4200, rep(10, cap.clone(), None), true),
            seg(
                "B",
                Some(14_200),
                4200,
                5000,
                rep(7, cap.clone(), None),
                false,
            ),
        ],
        Some(10_000),
    ));
    assert!(m.report.ok() && m.failures.is_empty());
    assert_eq!(m.report.stats.frames_received, 17);
    assert_eq!(texts(&m), ["0.0 A", "4.2 B"]);

    // (b) no frame anywhere: NoFrames.
    let m = merge(input(
        vec![seg("A", None, 0, 800, rep(0, cap.clone(), None), false)],
        None,
    ));
    assert!(matches!(m.report.outcome, SourceOutcome::NoFrames { .. }));

    // (c) middle segment silent for 6 s with a 5 s threshold: not clean, names the tab.
    let m = merge(input(
        vec![
            seg("A", Some(1000), 0, 2000, rep(5, cap.clone(), None), true),
            seg("B", None, 2000, 6000, rep(0, cap.clone(), None), true),
            seg(
                "A",
                Some(9000),
                8000,
                2000,
                rep(5, cap.clone(), None),
                false,
            ),
        ],
        Some(1000),
    ));
    assert_eq!(m.failures.len(), 1, "{:?}", m.failures);
    assert!(
        m.failures[0].contains("tab B: no frames for 6.0 s"),
        "{:?}",
        m.failures
    );
    assert!(m.report.ok());

    // (d) silent for 1 s: clean, labelled.
    let m = merge(input(
        vec![
            seg("A", Some(1000), 0, 2000, rep(5, cap.clone(), None), true),
            seg("B", None, 2000, 1000, rep(0, cap.clone(), None), true),
            seg(
                "A",
                Some(4000),
                3000,
                2000,
                rep(5, cap.clone(), None),
                false,
            ),
        ],
        Some(1000),
    ));
    assert!(m.failures.is_empty());
    assert_eq!(texts(&m), ["0.0 A", "2.0 B (no frames)", "3.0 A"]);

    // (e) ConnectionLost on a rebound segment is handled, not a failure.
    let lost = SourceOutcome::ConnectionLost { after_ms: 3000 };
    let m = merge(input(
        vec![
            seg("A", Some(1000), 0, 3000, rep(5, lost.clone(), None), true),
            seg(
                "B",
                Some(4000),
                3000,
                2000,
                rep(5, cap.clone(), None),
                false,
            ),
        ],
        Some(1000),
    ));
    assert!(m.failures.is_empty() && m.report.ok());

    // (f) ConnectionLost on the last segment with no rebind: not clean.
    let m = merge(input(
        vec![seg("A", Some(1000), 0, 3000, rep(5, lost, None), false)],
        Some(1000),
    ));
    assert_eq!(m.failures.len(), 1);
    assert!(
        m.failures[0].contains("capture interrupted"),
        "{:?}",
        m.failures
    );

    // (g) quiet tail from the last segment only.
    let m = merge(input(
        vec![
            seg(
                "A",
                Some(1000),
                0,
                3000,
                rep(5, cap.clone(), Some(9000)),
                true,
            ),
            seg(
                "B",
                Some(4000),
                3000,
                2000,
                rep(5, cap.clone(), None),
                false,
            ),
        ],
        Some(1000),
    ));
    assert_eq!(m.report.quiet_tail_ms, None);
    let m = merge(input(
        vec![
            seg("A", Some(1000), 0, 3000, rep(5, cap.clone(), None), true),
            seg(
                "B",
                Some(4000),
                3000,
                2000,
                rep(5, cap.clone(), Some(7000)),
                false,
            ),
        ],
        Some(1000),
    ));
    assert_eq!(m.report.quiet_tail_ms, Some(7000));

    // (h) a segment with no frame uses its wall-clock offset.
    let m = merge(input(
        vec![
            seg("A", Some(1000), 0, 3000, rep(5, cap.clone(), None), true),
            seg("B", None, 3000, 500, rep(0, cap, None), false),
        ],
        Some(1000),
    ));
    assert_eq!(texts(&m), ["0.0 A", "3.0 B (no frames)"]);
}

fn enc_ok() -> EncoderReport {
    EncoderReport {
        outcome: Outcome::Finalized,
        path: PathBuf::from("/abs/a.webm"),
        bytes: 10,
        frames_queued: 20,
        frames_dropped: 0,
        frames_written: 20,
        frames_unwritten: 0,
        gaps_clamped: 0,
        pid: Some(1),
        finalize_ms: 5,
        stderr_tail: String::new(),
    }
}

#[test]
fn compose_follow_names_failures_and_never_says_saved() {
    let cap = SourceOutcome::Captured;
    let mut i = input(
        vec![seg("A", Some(1000), 0, 3000, rep(20, cap, None), false)],
        Some(1000),
    );
    i.failures.push(msg::no_tab(3.0));
    i.notes.push(msg::tab_gone("A", 1.0, "B"));
    let m = merge(i);
    let (ok, t) = compose_follow(&m, &enc_ok(), 10);
    assert!(!ok, "{t}");
    assert!(
        t.starts_with("recording not clean: no tab left at 3.0 s; /abs/a.webm"),
        "{t}"
    );
    assert!(
        t.contains("Note: tab A gone at 1.0 s; following tab B"),
        "{t}"
    );
    assert!(!t.contains("recording saved"), "{t}");
    // No failure: exactly the compose text.
    let m = merge(input(
        vec![seg(
            "A",
            Some(1000),
            0,
            3000,
            rep(20, SourceOutcome::Captured, None),
            false,
        )],
        Some(1000),
    ));
    let (ok, t) = compose_follow(&m, &enc_ok(), 10);
    assert!(ok && t.starts_with("recording saved: "), "{t}");
}

#[test]
fn follow_messages_never_trigger_the_dead_tab_loops() {
    for t in [
        msg::tab_gone("A", 1.0, "B"),
        msg::tab_reconnected("A", 1.0),
        msg::no_tab(2.0),
        msg::browser_lost(2.0),
        msg::rebind_failed("B", 2.0, "x"),
        msg::zero_frames("B", 6.0),
        msg::follower_stuck(),
    ] {
        assert_eq!(msg::scrub(&t), t, "{t}");
    }
    assert_eq!(
        msg::tab_gone("A", 4.2, "B"),
        "tab A gone at 4.2 s; following tab B"
    );
    assert_eq!(msg::no_tab(9.8), "no tab left at 9.8 s");
    assert_eq!(msg::browser_lost(9.8), "browser connection lost at 9.8 s");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_is_finished_after_the_connection_closes() {
    use crate::record::source::fake_cdp::{FakeCdp, FakeOpts};
    use crate::record::source::{Source, SourceConfig};
    use std::time::Duration;

    let fake = FakeCdp::start(FakeOpts::default()).await;
    let cfg = SourceConfig {
        fps: 10,
        quality: 70,
        max_width: 1280,
        max_height: 720,
        ack_timeout: Duration::from_secs(2),
        stop_grace: Duration::from_millis(100),
        no_frames_after: Duration::from_secs(5),
    };
    let sink: Arc<dyn FrameSink> = Arc::new(Count::default());
    let src = Source::start_ws(&fake.url, "T", cfg, sink)
        .await
        .expect("start");
    assert!(!src.is_finished());
    fake.close_now();
    let mut done = false;
    for _ in 0..40 {
        if src.is_finished() {
            done = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(done, "closed connection must be noticed within 1 s");
}

#[test]
fn merge_leaves_t0_at_unset() {
    // The video clock Instant is set by the caller (run.rs) after the merge:
    // merge() only sees timestamps, so it must start with None.
    let cap = SourceOutcome::Captured;
    let m = merge(input(
        vec![seg("A", Some(10_000), 0, 4200, rep(10, cap, None), true)],
        Some(10_000),
    ));
    assert!(m.t0_at.is_none());
}
