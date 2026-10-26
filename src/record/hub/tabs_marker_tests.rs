//! `state tabs` markers while recording (CAPT-06 SC3): pure table tests, plus
//! the off-mode byte-identity against the pre-phase row format.

use super::{tab_marker, tabs_footer, TabsView};

fn own(agent: &str, video: Option<&str>) -> TabsView {
    TabsView {
        agent: agent.into(),
        video: video.map(String::from),
    }
}

#[test]
fn off_marker_is_the_attached_flag() {
    assert_eq!(tab_marker(true, "A", None), "*");
    assert_eq!(tab_marker(false, "A", None), " ");
    assert_eq!(tabs_footer(None), "");
}

#[test]
fn agent_and_video_on_one_tab() {
    let o = own("A", Some("A"));
    assert_eq!(tab_marker(true, "A", Some(&o)), "*R");
    assert_eq!(tab_marker(false, "B", Some(&o)), "  ");
    assert_eq!(tabs_footer(Some(&o)), "");
}

#[test]
fn agent_and_video_differ() {
    let o = own("A", Some("P"));
    assert_eq!(tab_marker(true, "A", Some(&o)), "* ");
    assert_eq!(tab_marker(true, "P", Some(&o)), " R");
    assert_eq!(tab_marker(false, "C", Some(&o)), "  ");
    assert_eq!(
        tabs_footer(Some(&o)),
        "(* agent, R video: differ; switch-tab to move the agent)\n"
    );
}

#[test]
fn parked_video_marks_only_the_agent() {
    let o = own("A", None);
    assert_eq!(tab_marker(true, "A", Some(&o)), "* ");
    assert_eq!(tab_marker(true, "B", Some(&o)), "  ");
    assert_eq!(tabs_footer(Some(&o)), "");
}

#[test]
fn off_rows_are_byte_identical_to_the_pre_phase_format() {
    // Row format copied from the a92ad77 `ListTabs` arm.
    let rows = [(true, "T1", "Home"), (false, "T2", ""), (false, "T3", "x")];
    let mut now = String::new();
    let mut old = String::new();
    for (att, id, title) in rows {
        let m = tab_marker(att, id, None);
        now.push_str(&format!("{} {}  {}\n", m, id, title));
        let m = if att { "*" } else { " " };
        old.push_str(&format!("{} {}  {}\n", m, id, title));
    }
    now.push_str(tabs_footer(None));
    assert_eq!(now, old);
    assert_eq!(now, "* T1  Home\n  T2  \n  T3  x\n");
}

#[test]
fn state_hook_is_small_and_keeps_upstream_lines() {
    let src = include_str!("../../state.rs");
    assert_eq!(src.matches("record::hub::tab_marker(").count(), 1);
    assert_eq!(src.matches("record::hub::tabs_footer(").count(), 1);
    assert!(!src.contains("is_current"));
}
