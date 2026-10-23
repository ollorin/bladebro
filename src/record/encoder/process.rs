//! ffmpeg process lifecycle: spawn by argv (never a shell), bounded frame
//! queue, stderr tail, finalize by closing stdin, kill only as a last resort.
//!
//! Lifecycle invariants:
//! - Stdin EOF is the only graceful stop (same path on every OS). The parent
//!   holds exactly one stdin handle, owned by the writer task.
//! - Every wait is bounded. Every path that kills also waits (reaps).
//! - `kill_on_drop(true)` is a backstop for an `Encoder` dropped without
//!   `finish` (Phase 9 owns that path); it is never the finalizer.
//! - Nothing here panics: all failures are `EncoderError` or an `Outcome`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Child;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{timeout, timeout_at, Instant};

use crate::record::config::RecordConfig;
use crate::record::values::Format;

use super::args::{build_args, EncodeSpec};
use super::report::{classify, EncoderError, EncoderReport, ExitInfo};
use super::resolve::ResolvedFfmpeg;
use super::writer::{self, Frame, Shared};

/// Total budget for `finish` (writer drain + ffmpeg exit). A const, not
/// env-tunable (D5-7); `EncoderConfig::finish_timeout` exists for tests.
pub const FINISH_TIMEOUT: Duration = Duration::from_secs(10);
/// Bytes of ffmpeg stderr kept (the last ones).
pub const STDERR_TAIL_BYTES: usize = 4096;
/// Budget for reaping after a kill.
pub const KILL_REAP_TIMEOUT: Duration = Duration::from_secs(5);
/// Grace for the stderr reader after ffmpeg exited.
pub const STDERR_DRAIN_GRACE: Duration = Duration::from_secs(1);
/// `child.wait()` always gets at least this long once the writer drained,
/// even if the writer ate the whole finish budget: ffmpeg needs real time to
/// write the container trailer after EOF, and killing it loses the video.
pub const CHILD_WAIT_FLOOR: Duration = Duration::from_secs(2);

/// Everything `Encoder::start` needs.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub ffmpeg: ResolvedFfmpeg,
    pub spec: EncodeSpec,
    pub queue_len: usize,
    pub finish_timeout: Duration,
}

impl EncoderConfig {
    pub fn new(ffmpeg: ResolvedFfmpeg, rc: &RecordConfig) -> Self {
        Self {
            ffmpeg,
            spec: EncodeSpec::from_config(rc),
            queue_len: rc.queue_len,
            finish_timeout: FINISH_TIMEOUT,
        }
    }
}

/// Keep the last `cap` bytes of `tail` after appending `chunk`.
pub fn push_tail(tail: &mut Vec<u8>, chunk: &[u8], cap: usize) {
    if chunk.len() >= cap {
        tail.clear();
        tail.extend_from_slice(&chunk[chunk.len() - cap..]);
        return;
    }
    tail.extend_from_slice(chunk);
    if tail.len() > cap {
        let excess = tail.len() - cap;
        tail.drain(..excess);
    }
}

/// A running ffmpeg encoder. Share behind an `Arc` for `send_frame`;
/// `finish` consumes it.
pub struct Encoder {
    path: PathBuf,
    pid: Option<u32>,
    child: Child,
    tx: mpsc::Sender<Frame>,
    writer: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
    stderr_tail: Arc<Mutex<Vec<u8>>>,
    shared: Arc<Shared>,
    queued: AtomicU64,
    dropped: AtomicU64,
    finish_timeout: Duration,
}

fn spawn_err(path: &Path, error: impl Into<String>) -> EncoderError {
    EncoderError::Spawn {
        path: path.display().to_string(),
        error: error.into(),
    }
}

impl Encoder {
    /// Check, build argv, spawn. Encoder presence, the output path and the
    /// runtime are all checked before any process exists (D5-10).
    pub fn start(cfg: &EncoderConfig, out: &Path) -> Result<Encoder, EncoderError> {
        let ff = &cfg.ffmpeg;
        let format = cfg.spec.format;
        if !ff.encoders.has(format.encoder()) {
            let other = match format {
                Format::WebM => Format::Mp4,
                Format::Mp4 => Format::WebM,
            };
            return Err(EncoderError::EncoderMissing {
                path: ff.path.display().to_string(),
                format,
                alt: ff.encoders.has(other.encoder()).then_some(other),
            });
        }
        let args = build_args(&cfg.spec, &ff.version, out)?;
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(spawn_err(
                &ff.path,
                "no async runtime: call from inside the daemon/MCP tokio runtime",
            ));
        }

        let mut cmd = tokio::process::Command::new(&ff.path);
        cmd.args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Own process group: a terminal Ctrl-C (SIGINT to the foreground group)
        // must reach bladebro only. bladebro then finalizes by closing stdin;
        // a SIGINT hitting ffmpeg too races that and ends it with exit 255.
        // A parent crash still finalizes: ffmpeg exits on stdin EOF.
        #[cfg(unix)]
        cmd.process_group(0);
        // Written, not compiled here; CREATE_NO_WINDOW keeps a console from
        // flashing when the daemon has none.
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| spawn_err(&ff.path, e.to_string()))?;
        // Read now: None once the child has been reaped.
        let pid = child.id();
        let (Some(stdin), Some(mut stderr)) = (child.stdin.take(), child.stderr.take()) else {
            let _ = child.start_kill();
            return Err(spawn_err(&ff.path, "stdio pipes unavailable"));
        };

        // The tail lives behind a mutex (never held across an await) so an
        // aborted reader still leaves what it had read.
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));
        let tail_w = Arc::clone(&stderr_tail);
        let stderr_task = tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut t = tail_w.lock().unwrap_or_else(|e| e.into_inner());
                        push_tail(&mut t, &buf[..n], STDERR_TAIL_BYTES);
                    }
                }
            }
        });

        let (tx, rx) = mpsc::channel(cfg.queue_len.max(1));
        let shared = Arc::new(Shared::default());
        let writer = tokio::spawn(writer::run(rx, stdin, cfg.spec.fps, Arc::clone(&shared)));
        Ok(Encoder {
            path: out.to_path_buf(),
            pid,
            child,
            tx,
            writer,
            stderr_task,
            stderr_tail,
            shared,
            queued: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            finish_timeout: cfg.finish_timeout,
        })
    }

    /// Queue a frame without blocking. False (and counted) when the queue is
    /// full or the writer has ended.
    pub fn send_frame(&self, jpeg: Vec<u8>, timestamp_ms: u64) -> bool {
        let frame = Frame {
            jpeg,
            ts_ms: timestamp_ms,
            arrived: std::time::Instant::now(),
        };
        match self.tx.try_send(frame) {
            Ok(()) => {
                self.queued.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(_) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// Fix the recording's end at this instant (first call wins; `finish`
    /// keeps an earlier mark). The last frame is held to here, so time spent
    /// between the stop request and `finish` (the source's ack grace) is not
    /// added to the video.
    pub fn mark_end(&self) {
        let _ = self.shared.end_at.set(std::time::Instant::now());
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// ffmpeg can no longer take frames: the writer hit a write error (broken
    /// pipe after a kill), or the process has exited. Non-blocking and does
    /// not reap (the child stays waitable for `finish`). Used by the recording
    /// guard to end an interrupted recording early.
    pub fn is_dead(&self) -> bool {
        self.shared.write_error.get().is_some() || self.pid.is_some_and(process_exited)
    }

    pub fn frames_dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Finalize: close the queue, let the writer drain and drop stdin (EOF),
    /// wait for ffmpeg to exit, all inside one deadline. Only on expiry is
    /// ffmpeg killed, and it is always waited for afterwards.
    ///
    /// Kill is not a finalizer: SIGKILL/TerminateProcess leaves no WebM cues
    /// and no MP4 moov, so the file is reported `Unfinalized`. Every exit
    /// path waits so no zombie or orphan is left behind.
    pub async fn finish(self) -> EncoderReport {
        let Encoder {
            path,
            pid,
            mut child,
            tx,
            mut writer,
            mut stderr_task,
            stderr_tail,
            shared,
            queued,
            dropped,
            finish_timeout,
        } = self;
        let started = Instant::now();
        let deadline = started + finish_timeout;

        let _ = shared.end_at.set(std::time::Instant::now());
        drop(tx);
        let mut writer_aborted = false;
        if timeout_at(deadline, &mut writer).await.is_err() {
            // Aborting drops ChildStdin. On unix that delivers EOF even when
            // the writer was stuck in a write; on Windows a blocked write may
            // keep the pipe open, and the kill fallback below bounds finish.
            writer_aborted = true;
            writer.abort();
            let _ = (&mut writer).await;
        }
        // Honesty: an abort discards the channel backlog and the rest of the
        // held frame's repeats; ffmpeg still sees a clean EOF and exits 0.
        let frames_unwritten = if writer_aborted {
            queued
                .load(Ordering::Relaxed)
                .saturating_sub(shared.frames_done.load(Ordering::Relaxed))
        } else {
            0
        };

        // Floor: a slow writer must not leave ffmpeg no time for its trailer.
        let wait_deadline = deadline.max(Instant::now() + CHILD_WAIT_FLOOR);
        let exit = match timeout_at(wait_deadline, child.wait()).await {
            Ok(Ok(status)) => match status.code() {
                Some(c) => ExitInfo::Code(c),
                None => ExitInfo::Signal(status.to_string()),
            },
            Ok(Err(e)) => ExitInfo::WaitFailed(e.to_string()),
            Err(_) => {
                let reaped = matches!(timeout(KILL_REAP_TIMEOUT, child.kill()).await, Ok(Ok(())));
                ExitInfo::Killed { reaped }
            }
        };
        let waited_ms = elapsed_ms(started);

        if timeout(STDERR_DRAIN_GRACE, &mut stderr_task).await.is_err() {
            stderr_task.abort();
            let _ = (&mut stderr_task).await;
        }
        let tail_bytes = stderr_tail
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let stderr_text = String::from_utf8_lossy(&tail_bytes).into_owned();

        let bytes = std::fs::metadata(&path)
            .ok()
            .filter(|m| m.is_file())
            .map(|m| m.len());
        let frames_written = shared.frames_written.load(Ordering::Relaxed);
        let outcome = classify(
            &exit,
            frames_written,
            shared.write_error.get().map(String::as_str),
            bytes,
            waited_ms,
            frames_unwritten,
        );
        EncoderReport {
            outcome,
            path,
            bytes: bytes.unwrap_or(0),
            frames_queued: queued.load(Ordering::Relaxed),
            frames_dropped: dropped.load(Ordering::Relaxed),
            frames_written,
            frames_unwritten,
            gaps_clamped: shared.gaps_clamped.load(Ordering::Relaxed),
            pid,
            finalize_ms: elapsed_ms(started),
            stderr_tail: stderr_text,
        }
    }
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// True once `pid` (our child) has exited, without reaping it: a zombie still
/// answers `kill(pid, 0)`, so ask `waitid` with WNOWAIT instead.
#[cfg(unix)]
fn process_exited(pid: u32) -> bool {
    // SAFETY: zeroed siginfo_t is a valid out-parameter for waitid; WNOHANG|WNOWAIT
    // only inspects the child's state.
    unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        let r = libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        );
        if r != 0 {
            // ECHILD: already reaped elsewhere, so it is gone.
            return std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD);
        }
        info.si_pid() != 0
    }
}

/// Windows: the writer's write error is the signal (a dead ffmpeg closes the
/// pipe); no cheap non-reaping exit check without more Win32 surface.
#[cfg(not(unix))]
fn process_exited(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{cfg_for, fake_ffmpeg, Scratch};
    use super::*;
    use crate::record::values::FrameSize;

    #[test]
    fn push_tail_keeps_last_bytes() {
        let all: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let mut tail = Vec::new();
        for c in [&all[..3000], &all[3000..7000], &all[7000..]] {
            push_tail(&mut tail, c, 4096);
        }
        assert_eq!(tail.len(), 4096);
        assert_eq!(tail, all[all.len() - 4096..]);

        let mut t = Vec::new();
        push_tail(&mut t, &all, 100);
        assert_eq!(t, all[all.len() - 100..]);

        let before = t.clone();
        push_tail(&mut t, &[], 100);
        assert_eq!(t, before);
    }

    #[test]
    fn config_new_carries_record_config() {
        let rc = RecordConfig {
            enabled: true,
            dir: PathBuf::from("/r"),
            format: Format::Mp4,
            fps: 15,
            size: FrameSize {
                width: 640,
                height: 360,
            },
            quality: 70,
            tabs: crate::record::Tabs::Follow,
            queue_len: 7,
            name: None,
            artifacts_dir: PathBuf::from("/a"),
        };
        let ff = fake_ffmpeg(Path::new("/x/ffmpeg"), &["libx264"]);
        let c = EncoderConfig::new(ff, &rc);
        assert_eq!(c.spec.format, Format::Mp4);
        assert_eq!(c.spec.fps, 15);
        assert_eq!(c.spec.size, rc.size);
        assert_eq!(c.queue_len, 7);
        assert_eq!(c.finish_timeout, FINISH_TIMEOUT);
    }

    #[test]
    fn start_without_runtime_is_an_error_not_a_panic() {
        let s = Scratch::new();
        let ff = fake_ffmpeg(&s.0.join("fake-enc-none"), &["libvpx-vp9"]);
        let cfg = cfg_for(ff, Format::WebM, 10, 4, Duration::from_secs(1));
        match Encoder::start(&cfg, &s.0.join("a.webm")) {
            Err(EncoderError::Spawn { error, .. }) => {
                assert!(error.contains("no async runtime"), "{error}")
            }
            Err(e) => panic!("wrong error: {e:?}"),
            Ok(_) => panic!("started without a runtime"),
        }
    }
}
