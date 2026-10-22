//! Screencast frame source (CAPT-01/02, SAFE-01).
//!
//! Opens the recorder's OWN CDP connection to a tab (never `Page`'s session
//! or event bus), enables `Page.screencast*`, acks every frame on receipt,
//! applies an fps cap, and hands frames to the encoder through a bounded,
//! non-blocking queue with drop/lag counters. Only a closed set of Page
//! screencast methods is ever sent.
//!
//! Module map:
//! - `capture`: public API, connection, start/stop, closed method allowlist.
//! - `handler`: per-event logic and counters (sync, unit-testable).
//! - `frame`: event parsing, base64, timestamps.
//! - `gate`: fps cap (newest wins, trailing flush).
//! - `report`: stats, outcomes, errors, verdicts.
//! - `watch`: (06-02) no-frames watchdog.
//! - tests only: `fake_cdp` (in-process fake CDP endpoint), `hook_tests`,
//!   `capture_tests`.
//!
//! Fork-only module (SYNC-01). The one upstream hook is the screencast
//! exemption in `cdp::client::route_cdp_text` (see .planning/HOOKS.md).

#[cfg(test)]
mod fake_cdp;
#[cfg(test)]
mod hook_tests;
