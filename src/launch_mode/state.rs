//! Process state for the launch mode: the only impure part of the module.
//! These wrappers wire tested pure pieces and carry no unit tests of their own
//! (a runtime set/get test would race alerts' one-shot latch test).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use super::display::launch_note;
use super::mode::{resolve_mode, LaunchMode};

static HEADLESS_BY_REQUEST: AtomicBool = AtomicBool::new(false);
static NOTE_PRINTED: AtomicBool = AtomicBool::new(false);
static PINNED: OnceLock<LaunchMode> = OnceLock::new();

pub fn set_headless_by_request(b: bool) {
    HEADLESS_BY_REQUEST.store(b, Ordering::SeqCst);
}

/// True when headless was asked for (so "Xvfb missing" advisories would lie).
pub fn headless_by_request() -> bool {
    HEADLESS_BY_REQUEST.load(Ordering::SeqCst)
}

/// Uncached resolution from the real environment. 04.1-02 adds the project
/// and user layers here.
pub fn resolve_now() -> Result<LaunchMode, String> {
    resolve_mode(&|k| std::env::var(k).ok(), None, None)
}

/// Resolved once per process: a GL-ladder or crash relaunch never flips mode.
/// Errors are not cached so a fixed value is picked up on the next launch.
pub fn effective() -> Result<LaunchMode, String> {
    if let Some(m) = PINNED.get() {
        return Ok(m.clone());
    }
    let m = resolve_now()?;
    Ok(PINNED.get_or_init(|| m).clone())
}

/// Record the decision and print the launch note once per process.
pub fn note_launch(m: &LaunchMode, headful: bool) {
    set_headless_by_request(m.forced_headless());
    if !NOTE_PRINTED.swap(true, Ordering::SeqCst) {
        eprintln!(
            "{}",
            launch_note(
                m,
                headful,
                cfg!(any(target_os = "linux", target_os = "android"))
            )
        );
    }
}
