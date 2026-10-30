//! The encoder runs in its own process group (unix). Executed on macOS only;
//! Linux is reasoned (same `setpgid` semantics), Windows has no such group.

use std::time::Duration;

use super::report::Outcome;
use super::test_util::{cfg_for, fake_ffmpeg, pid_gone, start_retry, Scratch};
use crate::record::values::Format;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_encoder_has_its_own_process_group_and_still_finalizes() {
    let s = Scratch::new();
    let script = s.script("fake-enc-pgid", r#"for a; do out="$a"; done; cat > "$out""#);
    let out = s.0.join("rec.webm");
    let cfg = cfg_for(
        fake_ffmpeg(&script, &["libx264", "libvpx-vp9"]),
        Format::WebM,
        10,
        16,
        Duration::from_secs(15),
    );
    let enc = start_retry(&cfg, &out).expect("start");
    let pid = enc.pid().expect("pid");
    // SAFETY: getpgid only reads the group of an existing pid / this process.
    let (child_pg, own_pg) = unsafe {
        (
            libc::getpgid(pid as libc::pid_t),
            libc::getpgid(0 as libc::pid_t),
        )
    };
    assert_eq!(child_pg, pid as libc::pid_t, "child leads its own group");
    assert_ne!(child_pg, own_pg, "a terminal Ctrl-C to our group misses it");
    enc.send_frame(b"JPEGDATA01".to_vec(), 0);
    // Closing stdin (the finish path) still finalizes it: no signal needed.
    let r = enc.finish().await;
    assert_eq!(r.outcome, Outcome::Finalized, "{}", r.verdict());
    assert!(pid_gone(pid));
}
