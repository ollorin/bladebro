//! Tables for the opener rules (pure) and the opener cache.

use std::collections::HashMap;
use std::time::Duration;

use super::opener::*;
use super::probe::{OpenerFeed, TabProbe, TabView};
use crate::record::hub::msg;

fn tab(id: &str) -> TabView {
    TabView {
        id: id.into(),
        url: format!("http://x/{id}"),
        attached: false,
        ws_url: String::new(),
    }
}

fn tabs(ids: &[&str]) -> Vec<TabView> {
    ids.iter().map(|i| tab(i)).collect()
}

fn cache(rows: &[(&str, Opener)]) -> HashMap<String, Opener> {
    rows.iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn is(o: &str) -> Opener {
    Opener::Is(o.into())
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn family_is_transitive_and_excludes_foreign() {
    let c = cache(&[
        ("P", is("A")),
        ("Q", is("P")),
        ("X", is("Z")),
        ("Z", Opener::None),
    ]);
    let f = family(&ids(&["A"]), &c);
    assert!(f.contains("A") && f.contains("P") && f.contains("Q"));
    assert!(!f.contains("X") && !f.contains("Z"));
    // After P is followed it is a root; R(opener P) is in even with no chain.
    let c2 = cache(&[("R", is("P"))]);
    assert!(family(&ids(&["A", "P"]), &c2).contains("R"));
    // Re-parented Q (opener A) is still in.
    assert!(family(&ids(&["A"]), &cache(&[("Q", is("A"))])).contains("Q"));
}

#[test]
fn classify_table() {
    let c = cache(&[
        ("P", is("A")),
        ("N", Opener::None),
        ("X", is("Z")),
        ("U", Opener::Unknown),
    ]);
    let f = family(&ids(&["A"]), &c);
    assert_eq!(classify("P", &c, &f), Kind::Popup);
    assert_eq!(classify("N", &c, &f), Kind::Fresh);
    assert_eq!(classify("X", &c, &f), Kind::Foreign);
    assert_eq!(classify("U", &c, &f), Kind::Popup);
    // Never looked up: same as unknown, never a silent "fresh".
    assert_eq!(classify("W", &c, &f), Kind::Popup);
}

#[test]
fn unknown_opener_needs_the_settle_and_never_joins_the_family() {
    let c = cache(&[("U", Opener::Unknown), ("V", is("U"))]);
    let f = family(&ids(&["A"]), &c);
    assert!(!f.contains("U"));
    // Its own child has an opener outside the family: foreign.
    assert_eq!(classify("V", &c, &f), Kind::Foreign);
    let mut s = Settle::default();
    assert!(!s.feed("U", "A", ms(0), ms(300)), "no instant follow");
    assert!(
        !s.feed("U", "A", ms(50), ms(300)),
        "two samples are not enough"
    );
    assert!(s.feed("U", "A", ms(300), ms(300)));
}

#[test]
fn settle_waits_then_follows_and_restarts_on_a_new_front() {
    let mut s = Settle::default();
    assert!(!s.feed("P", "A", ms(1000), ms(300)));
    assert!(!s.feed("P", "A", ms(1299), ms(300)));
    assert!(s.feed("P", "A", ms(1300), ms(300)));
    let mut s = Settle::default();
    assert!(!s.feed("P1", "A", ms(0), ms(300)));
    assert!(
        !s.feed("P2", "A", ms(100), ms(300)),
        "front changed: restart"
    );
    assert!(!s.feed("P2", "A", ms(399), ms(300)));
    assert!(s.feed("P2", "A", ms(400), ms(300)));
    // Opener re-fronted in time: the clock is cleared.
    let mut s = Settle::default();
    s.feed("P", "A", ms(0), ms(300));
    assert!(!s.feed("A", "A", ms(100), ms(300)));
    assert!(!s.is_pending());
    assert!(
        !s.feed("P", "A", ms(350), ms(300)),
        "restarted, not carried over"
    );
}

#[test]
fn return_to_table() {
    let c = cache(&[("P", is("A"))]);
    let fam = family(&ids(&["A"]), &c);
    // front A (family): A, via opener.
    let r = return_to("P", &tabs(&["A", "B"]), &fam, &c, &ids(&["A", "P"])).unwrap();
    assert_eq!((r.to.as_str(), r.via_opener), ("A", true));
    // front Z foreign, A listed: the opener wins over the front.
    let r = return_to("P", &tabs(&["Z", "A"]), &fam, &c, &ids(&["A", "P"])).unwrap();
    assert_eq!((r.to.as_str(), r.via_opener), ("A", true));
    // opener gone too: newest previous segment tab still listed.
    let r = return_to("P", &tabs(&["Z", "B"]), &fam, &c, &ids(&["B", "P"])).unwrap();
    assert_eq!((r.to.as_str(), r.via_opener), ("B", false));
    // nothing in the family: the front tab.
    let r = return_to("P", &tabs(&["Z"]), &fam, &c, &ids(&["A", "P"])).unwrap();
    assert_eq!((r.to.as_str(), r.via_opener), ("Z", false));
    // no page left.
    assert!(return_to("P", &[], &fam, &c, &ids(&["A"])).is_none());
}

#[test]
fn notes() {
    assert_eq!(
        msg::popup_closed("P", 3.2, "A"),
        "popup P closed at 3.2 s; back on opener A"
    );
    assert_eq!(
        msg::popups_unfollowed(5, &ids(&["P1", "P2", "P3", "P4", "P5"])),
        "5 popup(s) not recorded (never in front): P1, P2, P3, +2 more; switch-tab to record one"
    );
    assert_eq!(
        msg::popups_unfollowed(1, &ids(&["P"])),
        "1 popup(s) not recorded (never in front): P; switch-tab to record one"
    );
}

fn scripted(
    list: &[&str],
    openers: &[(&str, &str)],
) -> (
    OpenerFeed,
    std::sync::Arc<std::sync::Mutex<super::probe::Scripted>>,
) {
    let (probe, s) = TabProbe::scripted(tabs(list));
    drop(probe);
    s.lock().unwrap().openers = openers
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    (OpenerFeed::Scripted(s.clone()), s)
}

#[tokio::test]
async fn feed_asks_only_for_new_ids() {
    let (feed, s) = scripted(&["P", "A"], &[("P", "A")]);
    let mut st = OpenerState::new("A");
    let list = tabs(&["P", "A"]);
    st.refresh(&list, &feed, None).await;
    st.refresh(&list, &feed, None).await;
    assert_eq!(s.lock().unwrap().target_calls, 1);
    assert_eq!(st.classify("P"), Kind::Popup);
    // a new id triggers exactly one more call
    s.lock().unwrap().tabs.insert(0, tab("Q"));
    st.refresh(&tabs(&["Q", "P", "A"]), &feed, None).await;
    assert_eq!(s.lock().unwrap().target_calls, 2);
    assert_eq!(st.classify("Q"), Kind::Fresh);
}

#[tokio::test]
async fn failed_lookup_is_unknown_and_not_retried() {
    let (feed, s) = scripted(&["P", "A"], &[("P", "A")]);
    s.lock().unwrap().targets_fail = true;
    let mut st = OpenerState::new("A");
    let list = tabs(&["P", "A"]);
    st.refresh(&list, &feed, None).await;
    st.refresh(&list, &feed, None).await;
    assert_eq!(s.lock().unwrap().target_calls, 1);
    assert_eq!(st.cache().get("P"), Some(&Opener::Unknown));
    assert_eq!(st.classify("P"), Kind::Popup);
    assert!(st.unfollowed().is_empty(), "unknown is never named");
}

#[tokio::test]
async fn unfollowed_lists_family_popups_in_order_and_drops_followed() {
    let (feed, _s) = scripted(
        &["P3", "P2", "P1", "A", "X"],
        &[("P1", "A"), ("P2", "A"), ("P3", "A"), ("X", "Z")],
    );
    let mut st = OpenerState::new("A");
    st.refresh(&tabs(&["P3", "P2", "P1", "A", "X"]), &feed, None)
        .await;
    assert_eq!(st.unfollowed(), ids(&["P3", "P2", "P1"]));
    st.note_followed("P3");
    assert_eq!(st.unfollowed(), ids(&["P2", "P1"]));
}
