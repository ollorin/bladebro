//! What the hub needs from its surroundings, injected so tests drive a Hub
//! with a fake ffmpeg script and the in-process FakeCdp endpoint (no Chrome,
//! no env reads, no global). The live implementation arrives with the state
//! hook (07-02).

use super::follow::TabProbe;
use crate::record::{EncoderError, RecordConfig, ResolvedFfmpeg};

/// Which surface/transport the hub runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// Long-lived session on the WebSocket transport: can record.
    Ws,
    /// Pipe transport has no port for the recorder's own connection.
    Pipe,
    /// One-shot CLI: the process exits after the call.
    Ephemeral,
    /// Real-browser lane: would capture the user's own browser.
    Real,
}

/// How the recorder reaches the tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attach {
    /// `host:port` of the debugging port; the page socket is looked up by id.
    Port(String),
    /// An explicit page WebSocket URL (tests: FakeCdp).
    Ws(String),
}

// Crate-internal trait with static dispatch (`&impl HubEnv`), so the
// auto-trait caveat the lint warns about does not apply.
#[allow(async_fn_in_trait)]
pub trait HubEnv {
    /// Resolved settings for a recording called `name` (live: the layered
    /// env/project/user resolution; per-call input is the name only).
    fn config(&self, name: &str) -> Result<RecordConfig, String>;
    async fn ffmpeg(&self) -> Result<ResolvedFfmpeg, EncoderError>;
    fn lane(&self) -> Lane;
    /// Target id of the tab the agent is on right now.
    async fn current_tab(&self) -> Result<String, String>;
    fn attach(&self) -> Attach;
    /// How the follower reads Chrome's tab list. Default: the debugging port
    /// over HTTP, or `Fixed` (no following, Phase 7 behavior) for an explicit
    /// page WebSocket. Tests override it with a scripted list.
    fn probe(&self) -> TabProbe {
        match self.attach() {
            Attach::Port(base) => TabProbe::Http { base },
            Attach::Ws(_) => TabProbe::Fixed,
        }
    }
}
