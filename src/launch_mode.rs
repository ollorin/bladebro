//! Managed-browser launch mode (BLADE_HEADLESS and, later, config layers).
//!
//! Module map:
//! - `mode`: env parsing + the layered resolver (env > project > user > default), `resolve_in`.
//! - `cli`: `bladebro headless on|off|status`, help text, doctor line.
//! - `find`: pure walk-up discovery of `.bladebro.json`.
//! - `project`: project file loading, trust rules, `record.dir` vetting.
//! - `user`: `<data root>/launch.json` load/write.
//! - `daemon`: request tag + mode-mismatch refusal for the CLI daemon.
//! - `display`: the single display decision (`plan_display`) and launch-note wording.
//! - `registry`: option registry (env vars, config keys) + the DOC-02 docs drift tests.
//! - `state`: pinned per-process mode, launch-note latch, headless-by-request flag.
//! - `wiring`: cfg(test) checks that every upstream hook is in place.
//!
//! Fork-only module (SYNC-01): upstream files are touched only by the hook
//! lines listed in .planning/HOOKS.md.

pub mod cli;
pub mod daemon;
pub mod display;
pub mod find;
pub mod mode;
pub mod project;
pub mod registry;
pub mod state;
pub mod user;
#[cfg(test)]
mod wiring;

pub use cli::{
    doctor_check, doctor_line, help_json, run_cli, status_text, toggle_in, ModeCheck, CHECK_NAME,
    HELP_TEXT,
};
pub use daemon::{
    daemon_mismatch, mismatch_message, parse_request, request_fields, request_fields_for,
    request_tag,
};
pub use display::{describe, launch_note, plan_display, DisplayPlan};
pub use find::{find_project_file, PROJECT_FILE};
pub use mode::{
    parse_headless_env, resolve_in, resolve_mode, LaunchMode, Layer, Source, ENV_HEADLESS,
};
pub use project::{
    load_project, project_in, vet_project_dir, ProjectFile, ProjectLoaded, ProjectOutcome,
    MAX_FILE_BYTES,
};
pub use state::{
    effective, headless_by_request, note_launch, resolve_now, set_headless_by_request,
};
pub use user::{load_user_launch, write_user_launch, UserLaunchFile, USER_LAUNCH_FILE};
