//! How the follower sees Chrome's tabs: plain HTTP `GET /json` on the debugging
//! port (no CDP method, nothing page-visible). An enum, not a trait object,
//! so tests script the list without a new dependency.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::cdp::discovery::list_page_targets;
use crate::record::source::{Source, TargetRow};

/// One page-type target from Chrome's list, in Chrome's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabView {
    pub id: String,
    pub url: String,
    /// Chrome's `attached` flag. Live discovery (08-01-discovery.txt) found the
    /// key absent from `/json` entirely, so this is always false there; kept
    /// for `Rule::Attached` and tests.
    pub attached: bool,
    pub ws_url: String,
}

#[derive(Debug, Clone)]
pub enum TabProbe {
    /// The debugging port (`host:port`) of the browser.
    Http { base: String },
    /// Phase 7 behavior: the list is only the bound tab and the follower never
    /// rebinds (an explicit page WebSocket was given, e.g. the tests' FakeCdp).
    Fixed,
    /// Tests: the list lives behind a mutex the test mutates; `Err` scripts a
    /// browser that stopped answering.
    Scripted(Arc<Mutex<Scripted>>),
}

#[derive(Debug, Default)]
pub struct Scripted {
    pub tabs: Vec<TabView>,
    pub fail: bool,
    /// Number of `list` calls (tests prove the follower polls at all).
    pub calls: u64,
    /// child tab id -> the tab that opened it (what `Target.getTargets` says).
    pub openers: std::collections::HashMap<String, String>,
    /// Number of opener lookups (tests prove a steady state asks nothing).
    pub target_calls: u64,
    /// Script a `Target.getTargets` failure.
    pub targets_fail: bool,
}

/// Where opener facts come from: the current source's own connection, or a
/// scripted table in tests.
#[derive(Debug, Clone)]
pub enum OpenerFeed {
    Source,
    Scripted(Arc<Mutex<Scripted>>),
}

impl OpenerFeed {
    pub fn for_probe(p: &TabProbe) -> OpenerFeed {
        match p {
            TabProbe::Scripted(s) => OpenerFeed::Scripted(s.clone()),
            _ => OpenerFeed::Source,
        }
    }

    /// The real feed needs a live source to ask; the scripted one never does.
    pub fn needs_source(&self) -> bool {
        matches!(self, OpenerFeed::Source)
    }

    /// Rows for every page Chrome knows. `src` is the current source (None in
    /// tests and for a retiring one: never asked).
    pub async fn rows(
        &self,
        src: Option<&Source>,
        deadline: Duration,
    ) -> Result<Vec<TargetRow>, String> {
        match self {
            OpenerFeed::Source => match src {
                Some(s) => s.targets(deadline).await.map_err(|e| e.to_string()),
                None => Err("no live source to ask".to_string()),
            },
            OpenerFeed::Scripted(s) => {
                let mut g = s.lock().unwrap_or_else(|e| e.into_inner());
                g.target_calls += 1;
                if g.targets_fail {
                    return Err("getTargets failed".to_string());
                }
                Ok(g.tabs
                    .iter()
                    .map(|t| TargetRow {
                        id: t.id.clone(),
                        opener: g.openers.get(&t.id).cloned(),
                        kind_page: true,
                        url: t.url.clone(),
                    })
                    .collect())
            }
        }
    }
}

impl TabProbe {
    pub fn scripted(tabs: Vec<TabView>) -> (TabProbe, Arc<Mutex<Scripted>>) {
        let s = Arc::new(Mutex::new(Scripted {
            tabs,
            ..Default::default()
        }));
        (TabProbe::Scripted(s.clone()), s)
    }

    /// True when the follower must not poll or rebind.
    pub fn is_fixed(&self) -> bool {
        matches!(self, TabProbe::Fixed)
    }

    /// The page targets, Chrome order, devtools pages and targets without a
    /// page socket removed. `bound` is only used by `Fixed`.
    pub async fn list(&self, bound: &str) -> Result<Vec<TabView>, String> {
        match self {
            TabProbe::Http { base } => {
                let all = list_page_targets(base)
                    .await
                    .map_err(|e| format!("cannot list tabs at {base}: {e}"))?;
                Ok(all
                    .into_iter()
                    .filter(|t| !t.url.starts_with("devtools://"))
                    .filter_map(|t| {
                        let ws_url = t.web_socket_debugger_url.clone()?;
                        Some(TabView {
                            id: t.id,
                            url: t.url,
                            attached: t.attached,
                            ws_url,
                        })
                    })
                    .collect())
            }
            TabProbe::Fixed => Ok(vec![TabView {
                id: bound.to_string(),
                url: String::new(),
                attached: true,
                ws_url: String::new(),
            }]),
            TabProbe::Scripted(s) => {
                let mut g = s.lock().unwrap_or_else(|e| e.into_inner());
                g.calls += 1;
                if g.fail {
                    Err("connection refused".to_string())
                } else {
                    Ok(g.tabs.clone())
                }
            }
        }
    }
}
