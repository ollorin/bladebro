//! Screencast frame source (CAPT-01/02, SAFE-01).
//!
//! Opens the recorder's OWN CDP connection to a tab (never `Page`'s session
//! or event bus), enables `Page.screencast*`, acks every frame on receipt,
//! applies an fps cap, and hands frames to the encoder through a bounded,
//! non-blocking queue with drop/lag counters. Only a closed set of Page
//! screencast methods is ever sent.
//!
//! Module map:
//! - `capture`: public API (Source, SourceConfig, Endpoint), connection, ack-first loop, start/stop.
//! - `wire`: RecMethod closed method set + the single send path (SAFE-01).
//! - `sink`: FrameSink bounded hand-off, impl for Encoder.
//! - `handler`: per-event logic and counters (sync, unit-testable).
//! - `frame`: event parsing, base64, timestamps.
//! - `gate`: fps cap (newest wins, trailing flush).
//! - `report`: stats, outcomes, errors, verdicts.
//! - `watch`: no-frames watchdog state (Liveness, Health, NO_FRAMES_AFTER).
//! - tests only: `fake_cdp` (in-process fake CDP endpoint), `hook_tests`,
//!   `capture_tests`.
//!
//! Fork-only module (SYNC-01). The one upstream hook is the screencast
//! exemption in `cdp::client::route_cdp_text` (see .planning/HOOKS.md).

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
pub(crate) mod fake_cdp;
#[cfg(test)]
mod hook_tests;
#[cfg(test)]
mod watch_tests;

mod capture;
mod frame;
mod gate;
mod handler;
mod report;
mod sink;
mod watch;
mod wire;

pub use capture::{Endpoint, Source, SourceConfig, ACK_TIMEOUT, STOP_GRACE};
pub use frame::{decode_jpeg, parse_frame, parse_frame_owned, ts_ms_from_metadata, FrameEvent};
pub use gate::{FpsGate, GateStep};
pub use report::{SourceError, SourceOutcome, SourceReport, SourceStats};
pub use sink::FrameSink;
pub use watch::{Health, Liveness, NO_FRAMES_AFTER};
pub use wire::RecMethod;
