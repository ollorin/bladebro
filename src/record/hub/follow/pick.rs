//! Which tab is the agent on? Pure decisions, table-tested.
//!
//! Chrome gives the follower no CDP-free "current tab" signal. Live discovery
//! (08-01-discovery.txt) showed `/json` carries no `attached` key at all, so
//! `RULE` is `ListOrder`: the first page of Chrome's list (most recently
//! activated first). `Attached` stays as the documented alternative.

use super::probe::TabView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// A page that is `attached` and not one of ours; else the bound tab.
    Attached,
    /// The first real page in Chrome's order.
    ListOrder,
}

fn pages(list: &[TabView]) -> impl Iterator<Item = &TabView> {
    list.iter().filter(|t| !t.url.starts_with("devtools://"))
}

/// The tab the agent is on, or None when no page exists. `bound` holds every
/// tab id the follower has a socket to; its LAST element is the current one.
pub fn agent_tab(list: &[TabView], bound: &[String], rule: Rule) -> Option<String> {
    match rule {
        Rule::ListOrder => pages(list).next().map(|t| t.id.clone()),
        Rule::Attached => {
            let foreign = pages(list).find(|t| t.attached && !bound.contains(&t.id));
            if let Some(f) = foreign {
                return Some(f.id.clone());
            }
            if let Some(cur) = bound.last() {
                if pages(list).any(|t| &t.id == cur) {
                    return Some(cur.clone());
                }
            }
            pages(list).next().map(|t| t.id.clone())
        }
    }
}

/// Debounce: a switch needs the same candidate on two samples. A tab that is
/// only transiently first (a popup flashing past, the old agent tab still
/// attached for a few ms) never produces a segment.
#[derive(Debug, Default)]
pub struct Confirm {
    pending: Option<String>,
}

impl Confirm {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when `cand` (different from `current`) was seen on this and the
    /// previous sample: switch now.
    pub fn feed(&mut self, cand: &str, current: &str) -> bool {
        if cand == current {
            self.pending = None;
            return false;
        }
        if self.pending.as_deref() == Some(cand) {
            self.pending = None;
            return true;
        }
        self.pending = Some(cand.to_string());
        false
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn reset(&mut self) {
        self.pending = None;
    }
}

/// A lingering window popup can sit first in Chrome's list while the agent is
/// on another tab (found live). A head that already differed from the bound
/// tab at record-start is therefore ignored until the head changes once.
#[derive(Debug, Default)]
pub struct Baseline {
    stale: Option<String>,
}

impl Baseline {
    pub fn new(head: Option<&str>, bound: &str) -> Self {
        Self {
            stale: head.filter(|h| *h != bound).map(String::from),
        }
    }

    /// The candidate to act on: the bound tab while the head is the stale one.
    pub fn filter(&mut self, cand: Option<String>, current: &str) -> Option<String> {
        match (&self.stale, &cand) {
            (Some(s), Some(c)) if s == c => Some(current.to_string()),
            (Some(_), _) => {
                self.stale = None;
                cand
            }
            _ => cand,
        }
    }
}
