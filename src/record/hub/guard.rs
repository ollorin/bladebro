//! Recording guard (LIFE-02): one small task per recording that notices a dead
//! browser or a dead encoder and finalizes the recording EARLY, so the partial
//! file is playable up to the cut without waiting for an agent call.
//!
//! Rules:
//! - Every recording has a generation id (`Hub::active_gen`). The guard acts
//!   only while its own generation is the active one; after a stop, a
//!   shutdown-hook finish or a newer start it exits silently (stale guards are
//!   harmless).
//! - Each tick takes the hub lock, reads `Hub::verdict` (no I/O) and, only on a
//!   trigger, `interrupt_if`, which holds the lock for one finish (same bound
//!   as `finish_orphan`). A concurrent record-stop holds the lock instead; the
//!   guard then sees `Gone`.
//! - Triggers: the follower lost Chrome (3 failed tab-list reads, ~0.6 s) and
//!   ffmpeg dead (write error or exited). Per-tab mode has no single encoder
//!   handle here, so only the browser trigger applies to it.
//! - The guard never touches a page or a CDP session: browsing is unaffected.
//!   09-03 adds disk-low / duration / size checks to the same tick.

use std::ops::Deref;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::machine::{Hub, Verdict};

/// How often a recording is checked.
pub const GUARD_TICK: Duration = Duration::from_secs(2);

/// Start the guard of recording `gen`. `hub` is the process global
/// (`&'static Mutex<Hub>`) or a test's `Arc<Mutex<Hub>>`.
pub fn spawn<R>(hub: R, gen: u64, tick: Duration) -> JoinHandle<()>
where
    R: Deref<Target = Mutex<Hub>> + Send + Sync + 'static,
{
    tokio::spawn(async move {
        // A previous recorder may have died with its ffmpeg: sweep this
        // recording's own dir (the reaper only knows the default dirs).
        let dir = hub.lock().await.dir_of(gen);
        if let Some(d) = dir {
            crate::record::orphans::sweep_dir(d);
        }
        loop {
            tokio::time::sleep(tick).await;
            let mut h = hub.lock().await;
            match h.verdict(gen) {
                Verdict::Gone => return,
                Verdict::Healthy => {}
                Verdict::Interrupted(why) => {
                    if h.interrupt_if(gen, why).await {
                        if let Some(t) = h.orphaned_text() {
                            tracing::warn!(target: "bladebro.record", "{t}");
                        }
                    }
                    return;
                }
            }
        }
    })
}
