//! Disk and size guards (LIFE-04): free space of the recordings dir and the
//! fixed limits that keep one recording from growing without bound.
//!
//! Why constants and not options (D9-13): nothing here is something an agent
//! or a user needs to tune; a new env var or config key would be schema
//! surface read on every call. Someone who needs a longer take splits it with
//! record-stop / record-start. Values come from measured sizes (see
//! docs/recording-and-launch-config.md, "File size per minute"): ~1 MB/min at
//! the defaults, so 1 GiB is ~17 h of default video and 2 h is ~120 MB; the
//! duration limit therefore bites first at the defaults and the size limit at
//! very large frame sizes.
//!
//! Why `df`: the same approach as the updater's download space check (copied,
//! never imported: that file is upstream, SYNC-01). It is a safe subprocess,
//! no FFI. Windows has no `df`: `free_bytes` returns `None`, the disk guard is
//! off there (a `None` is never a stop), the duration and size limits still
//! apply.
//!
//! Module map: `free_bytes` (probe), `parse_df` (pure), consts.

use std::path::Path;
use std::time::Duration;

/// Recording is refused, and stopped while running, when strictly less than
/// this is free on the recordings dir's filesystem.
pub const DISK_LOW_MIB: u64 = 500;
/// One recording ends (finalized) after this long.
pub const MAX_DURATION: Duration = Duration::from_secs(2 * 3600);
/// Display form of `MAX_DURATION` in texts.
pub const MAX_DURATION_TEXT: &str = "2 h";
/// One file ends (finalized) when it reaches this many bytes.
pub const MAX_FILE_BYTES: u64 = 1 << 30;
/// Display form of `MAX_FILE_BYTES` in texts.
pub const MAX_FILE_TEXT: &str = "1 GiB";

/// Injectable free-space probe (`HubEnv::free_fn`): dir -> free bytes, or
/// `None` when unknown.
pub type FreeFn = std::sync::Arc<dyn Fn(&Path) -> Option<u64> + Send + Sync>;

/// Free bytes on the filesystem holding `dir` (nearest existing ancestor, so a
/// recordings dir that is not created yet is measured on its parent).
#[cfg(unix)]
pub fn free_bytes(dir: &Path) -> Option<u64> {
    let mut probe = dir;
    while !probe.exists() {
        probe = probe.parent()?;
    }
    let out = std::process::Command::new("df")
        .arg("-k")
        .arg(probe)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_df(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(not(unix))]
pub fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

/// `df -k` output -> available bytes. The data row is the last line; the
/// Available column is the field right before the first `NN%` field, which
/// keeps working when the filesystem name wrapped or holds spaces.
pub fn parse_df(stdout: &str) -> Option<u64> {
    let line = stdout.lines().rev().find(|l| !l.trim().is_empty())?;
    let fields: Vec<&str> = line.split_whitespace().collect();
    let pct = fields.iter().position(|f| f.ends_with('%'))?;
    let kb: u64 = fields.get(pct.checked_sub(1)?)?.parse().ok()?;
    kb.checked_mul(1024)
}

/// Whole MiB, rounded down (what the texts print).
pub fn mib(bytes: u64) -> u64 {
    bytes >> 20
}

/// `true` when `free` is known and strictly below the threshold.
pub fn is_low(free: Option<u64>) -> bool {
    free.is_some_and(|f| mib(f) < DISK_LOW_MIB)
}
