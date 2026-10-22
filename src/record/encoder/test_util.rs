//! Test-only helpers: scratch dirs, fake ffmpeg descriptors and (unix) fake
//! encoder scripts. Scripts are named `fake-enc-*` so a leftover process is
//! easy to find with `pgrep -fl fake-enc`, and must `exec` anything long
//! running so killing the pid leaves nothing behind.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::record::ffmpeg::{parse_encoders, FfmpegVersion};
use crate::record::values::{Format, FrameSize};

use super::args::EncodeSpec;
use super::process::EncoderConfig;
use super::resolve::ResolvedFfmpeg;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// Unique scratch dir under the temp dir, removed on drop.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "bladebro-enc-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::create_dir_all(&p);
        Self(p)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A `ResolvedFfmpeg` for `path` listing exactly `listed` video encoders.
pub fn fake_ffmpeg(path: &Path, listed: &[&str]) -> ResolvedFfmpeg {
    let mut text = String::from("Encoders:\n ------\n");
    for n in listed {
        text.push_str(&format!(" V....D {n} fake\n"));
    }
    ResolvedFfmpeg {
        path: path.to_path_buf(),
        version: FfmpegVersion::Release {
            display: "n9.0.2".into(),
            major: 9,
        },
        encoders: parse_encoders(&text),
    }
}

pub fn cfg_for(
    ffmpeg: ResolvedFfmpeg,
    format: Format,
    fps: u32,
    queue_len: usize,
    finish_timeout: Duration,
) -> EncoderConfig {
    EncoderConfig {
        ffmpeg,
        spec: EncodeSpec {
            format,
            fps,
            size: FrameSize {
                width: 1280,
                height: 720,
            },
        },
        queue_len,
        finish_timeout,
    }
}

#[cfg(unix)]
impl Scratch {
    /// Write an executable `#!/bin/sh` script.
    pub fn script(&self, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = self.0.join(name);
        let _ = std::fs::write(&p, format!("#!/bin/sh\n{body}\n"));
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755));
        p
    }
}

/// Retry `Encoder::start` on ETXTBSY (Linux: a concurrent test's fork can
/// briefly hold the just-written script open for writing).
#[cfg(unix)]
pub fn start_retry(
    cfg: &EncoderConfig,
    out: &Path,
) -> Result<super::process::Encoder, super::report::EncoderError> {
    let mut last = super::process::Encoder::start(cfg, out);
    for _ in 0..5 {
        match &last {
            Err(e) if e.to_string().contains("Text file busy") => {
                std::thread::sleep(Duration::from_millis(50));
                last = super::process::Encoder::start(cfg, out);
            }
            _ => break,
        }
    }
    last
}

/// True when `pid` no longer exists. A zombie still answers `kill(pid, 0)`,
/// so ESRCH proves the child was reaped.
#[cfg(unix)]
pub fn pid_gone(pid: u32) -> bool {
    // SAFETY: signal 0 only checks existence; no signal is delivered.
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    r == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

#[cfg(windows)]
impl Scratch {
    /// Write a `.cmd` batch fake (CRLF). WRITTEN, NOT COMPILED OR RUN HERE.
    pub fn cmd(&self, name: &str, body: &str) -> PathBuf {
        let p = self.0.join(format!("{name}.cmd"));
        let text = format!("@echo off\r\n{}\r\n", body.replace('\n', "\r\n"));
        let _ = std::fs::write(&p, text);
        p
    }
}
