//! Managed-browser launch mode (BLADE_HEADLESS and, later, config layers).
//!
//! Module map:
//! - `mode`: env parsing + the layered resolver (env > project > user > default).
//! - `display`: the single display decision (`plan_display`) and launch-note wording.
//! - `state`: pinned per-process mode, launch-note latch, headless-by-request flag.
//! - `wiring`: cfg(test) checks that every upstream hook is in place.
//!
//! Fork-only module (SYNC-01): upstream files are touched only by the hook
//! lines listed in .planning/HOOKS.md.

pub mod display;
pub mod mode;
pub mod state;
#[cfg(test)]
mod wiring;

pub use display::{describe, launch_note, plan_display, DisplayPlan};
pub use mode::{parse_headless_env, resolve_mode, LaunchMode, Layer, Source, ENV_HEADLESS};
pub use state::{
    effective, headless_by_request, note_launch, resolve_now, set_headless_by_request,
};
