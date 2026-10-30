//! Popup rules (CAPT-06): pure and table-tested; `run.rs` only feeds them the
//! tab list and acts on the answers.
//!
//! A tab opened by a tab of the recording (the record-start tab, every
//! followed tab, and their popups, transitively) is followed once it has
//! stayed in front for `POPUP_SETTLE`; a tab nobody opened (open-tab) keeps the
//! ordinary two-sample confirm; a tab opened from outside the recording is
//! ignored. When Chrome cannot say who opened a tab (`Unknown`), it is treated
//! as a popup (settle, which is stricter than the fresh-tab confirm) but is
//! never counted into the family or the unfollowed note, so its own children
//! stay foreign.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::probe::{OpenerFeed, TabView};
use crate::record::hub::msg;
use crate::record::source::Source;

/// What the cache knows about one tab's opener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opener {
    /// Chrome says nobody opened it (open-tab, createTarget, user).
    None,
    Is(String),
    /// The lookup failed or Chrome did not list the tab.
    Unknown,
}

/// The follower's action for a new front tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Stay,
    Follow,
    /// Not a popup of the recording: the ordinary two-sample confirm.
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Popup,
    Fresh,
    Foreign,
}

/// Roots plus every cached tab whose opener is already in the family,
/// re-evaluated on each call (a re-parented grandchild still counts).
pub fn family(roots: &[String], cache: &HashMap<String, Opener>) -> HashSet<String> {
    let mut fam: HashSet<String> = roots.iter().cloned().collect();
    loop {
        let before = fam.len();
        for (id, o) in cache {
            if let Opener::Is(op) = o {
                if fam.contains(op) {
                    fam.insert(id.clone());
                }
            }
        }
        if fam.len() == before {
            return fam;
        }
    }
}

pub fn classify(id: &str, cache: &HashMap<String, Opener>, fam: &HashSet<String>) -> Kind {
    match cache.get(id) {
        Some(Opener::None) => Kind::Fresh,
        Some(Opener::Is(o)) if fam.contains(o) => Kind::Popup,
        Some(Opener::Is(_)) => Kind::Foreign,
        Some(Opener::Unknown) | Option::None => Kind::Popup,
    }
}

/// Continuous time in front for one candidate; `now` is any monotonic clock
/// as a `Duration` (tests pass literals).
#[derive(Debug, Default)]
pub struct Settle {
    pending: Option<(String, Duration)>,
}

impl Settle {
    /// True when `cand` has been the front tab for `settle`. A different
    /// candidate restarts the clock (burst: only the last one is followed);
    /// the current tab in front clears it.
    pub fn feed(&mut self, cand: &str, current: &str, now: Duration, settle: Duration) -> bool {
        if cand == current {
            self.pending = None;
            return false;
        }
        match &self.pending {
            Some((id, t0)) if id == cand => {
                if now.saturating_sub(*t0) >= settle {
                    self.pending = None;
                    true
                } else {
                    false
                }
            }
            _ => {
                self.pending = Some((cand.to_string(), now));
                if settle.is_zero() {
                    self.pending = None;
                    return true;
                }
                false
            }
        }
    }

    #[cfg(test)]
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

/// Where the video goes when the followed tab `gone` disappears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Return {
    pub to: String,
    /// `to` is the gone tab's opener (the note names it).
    pub via_opener: bool,
}

/// Front tab if it is in the family (or nobody opened it); else the gone tab's opener if listed;
/// else the newest previous segment tab still listed; else the front tab.
pub fn return_to(
    gone: &str,
    list: &[TabView],
    fam: &HashSet<String>,
    cache: &HashMap<String, Opener>,
    prev_tabs: &[String],
) -> Option<Return> {
    let listed = |id: &str| list.iter().any(|t| t.id == id);
    let opener = match cache.get(gone) {
        Some(Opener::Is(o)) if listed(o) => Some(o.clone()),
        _ => None,
    };
    let pick = |to: String| Return {
        via_opener: opener.as_deref() == Some(to.as_str()),
        to,
    };
    let front = list.first().map(|t| t.id.clone());
    // A front tab nobody opened (the agent's own new tab) is as good as one of
    // the family; only a tab opened from outside the recording is passed over.
    let ours = |f: &String| fam.contains(f) || matches!(cache.get(f), Some(Opener::None));
    if let Some(f) = front.clone().filter(ours) {
        return Some(pick(f));
    }
    if let Some(o) = opener.clone() {
        return Some(pick(o));
    }
    if let Some(p) = prev_tabs.iter().rev().find(|p| *p != gone && listed(p)) {
        return Some(pick(p.clone()));
    }
    front.map(pick)
}

/// The follower's opener memory: cache, family roots, settle clock and the
/// popups that were never followed.
#[derive(Debug)]
pub struct OpenerState {
    cache: HashMap<String, Opener>,
    roots: Vec<String>,
    seen_popups: Vec<String>,
    pub settle: Settle,
}

impl OpenerState {
    pub fn new(start_tab: &str) -> Self {
        Self {
            cache: HashMap::new(),
            roots: vec![start_tab.to_string()],
            seen_popups: Vec::new(),
            settle: Settle::default(),
        }
    }

    /// Tabs already open at record-start are never popups of this recording.
    pub fn with_known(mut self, known: &[String]) -> Self {
        for id in known {
            self.cache.entry(id.clone()).or_insert(Opener::None);
        }
        self
    }

    #[cfg(test)]
    pub fn cache(&self) -> &HashMap<String, Opener> {
        &self.cache
    }

    pub fn family(&self) -> HashSet<String> {
        family(&self.roots, &self.cache)
    }

    pub fn classify(&self, id: &str) -> Kind {
        classify(id, &self.cache, &self.family())
    }

    pub fn note_followed(&mut self, id: &str) {
        if !self.roots.iter().any(|r| r == id) {
            self.roots.push(id.to_string());
        }
    }

    /// Ids of `list` with no cached opener (steady state: empty).
    pub fn missing(&self, list: &[TabView]) -> Vec<String> {
        list.iter()
            .filter(|t| !self.cache.contains_key(&t.id))
            .map(|t| t.id.clone())
            .collect()
    }

    /// Ask the feed only when the list holds an uncached id. A failed lookup
    /// caches those ids as `Unknown` (never asked again for them).
    pub async fn refresh(&mut self, list: &[TabView], feed: &OpenerFeed, src: Option<&Source>) {
        let missing = self.missing(list);
        if missing.is_empty() || (feed.needs_source() && src.is_none()) {
            self.observe(list);
            return;
        }
        match feed.rows(src, Duration::from_secs(1)).await {
            Ok(rows) => {
                for id in &missing {
                    let o = match rows.iter().find(|r| &r.id == id) {
                        Some(r) => r.opener.clone().map_or(Opener::None, Opener::Is),
                        Option::None => Opener::Unknown,
                    };
                    self.cache.insert(id.clone(), o);
                }
            }
            Err(_) => {
                for id in missing {
                    self.cache.insert(id, Opener::Unknown);
                }
            }
        }
        self.observe(list);
    }

    /// Remember family popups seen in this list (for the stop note).
    pub fn observe(&mut self, list: &[TabView]) {
        let fam = self.family();
        for t in list {
            let is_popup = matches!(self.cache.get(&t.id), Some(Opener::Is(o)) if fam.contains(o));
            if is_popup && !self.roots.contains(&t.id) && !self.seen_popups.contains(&t.id) {
                self.seen_popups.push(t.id.clone());
            }
        }
    }

    /// Family popups never followed, in first-seen order.
    pub fn unfollowed(&self) -> Vec<String> {
        self.seen_popups
            .iter()
            .filter(|p| !self.roots.contains(p))
            .cloned()
            .collect()
    }

    /// What the follower does with front tab `cand` that is not the bound tab.
    /// `now` is any monotonic clock.
    pub fn step(&mut self, cand: &str, cur: &str, now: Duration, settle: Duration) -> Step {
        match self.classify(cand) {
            Kind::Popup => {
                if self.settle.feed(cand, cur, now, settle) {
                    Step::Follow
                } else {
                    Step::Stay
                }
            }
            Kind::Fresh => {
                self.settle.feed(cur, cur, now, settle);
                Step::Confirm
            }
            Kind::Foreign => {
                self.settle.feed(cur, cur, now, settle);
                Step::Stay
            }
        }
    }

    /// Where the video goes when `cur` is gone, and the note for it.
    pub fn gone(
        &self,
        cur: &str,
        list: &[TabView],
        prev: &[String],
        at_s: f64,
        cand: &str,
    ) -> (String, String) {
        match self.return_to(cur, list, prev) {
            Some(r) if r.via_opener => {
                let note = msg::popup_closed(cur, at_s, &r.to);
                (r.to, note)
            }
            Some(r) => {
                let note = msg::tab_gone(cur, at_s, &r.to);
                (r.to, note)
            }
            None => (cand.to_string(), msg::tab_gone(cur, at_s, cand)),
        }
    }

    /// The stop note for family popups that never stayed in front.
    pub fn note(&self) -> Option<String> {
        let ids = self.unfollowed();
        (!ids.is_empty()).then(|| msg::popups_unfollowed(ids.len(), &ids))
    }

    pub fn return_to(&self, gone: &str, list: &[TabView], prev_tabs: &[String]) -> Option<Return> {
        return_to(gone, list, &self.family(), &self.cache, prev_tabs)
    }
}
