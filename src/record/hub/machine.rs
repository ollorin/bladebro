//! The hub state machine: Idle <-> Active over an injected `HubEnv`.
//!
//! All logic lives on `Hub` instances so tests never touch the process-global
//! (see `hub.rs`). The async mutex around the global is held across a whole
//! op, so two concurrent starts cannot both pass the Idle check (D7-9 b).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::env::{Attach, HubEnv, Lane};
use super::follow::{
    first_view, Files, FollowShared, Policy, SpawnArgs, Tracker, CONFIRM, POLL, POPUP_SETTLE,
};
use super::msg::{self, scrub};
use crate::record::disk::{self, FreeFn};
use crate::record::marker::MarkerIn;
use crate::record::{
    create_recording_file, tab_stem, Encoder, EncoderConfig, Endpoint, FrameSink, FrameSource,
    ResolvedFfmpeg, SourceConfig, Tabs,
};

// Hub stop/finish paths live in a child module (keeps this file under 500 lines).
#[path = "finish.rs"]
mod finish;

/// How the hub reports a failure. Both map to an MCP/CLI error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubError {
    /// Bad call (the adapter maps it to a usage error).
    Usage(String),
    /// Valid call that did not (cleanly) succeed; the text is the honest report.
    Failed(String),
}

impl std::fmt::Display for HubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HubError::Usage(s) | HubError::Failed(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for HubError {}

fn failed(s: impl AsRef<str>) -> HubError {
    HubError::Failed(scrub(s.as_ref()))
}

/// Test-shortened timings; `None` keeps the production constants.
#[derive(Debug, Clone, Default)]
pub struct Knobs {
    pub stop_grace: Option<Duration>,
    pub finish_timeout: Option<Duration>,
    /// Tab-list poll interval and the gap between the two confirming samples.
    pub poll: Option<Duration>,
    pub confirm: Option<Duration>,
    /// How long a popup of the recording must stay in front before it is followed.
    pub popup_settle: Option<Duration>,
    /// A zero-frame segment this long is a failure (default: the 5 s watchdog).
    pub no_frames_after: Option<Duration>,
    /// Test-shortened LIFE-04 limits (default: `record::disk` consts).
    pub max_duration: Option<Duration>,
    pub max_file_bytes: Option<u64>,
}

/// Why the guard ended a recording early (LIFE-02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Interrupt {
    /// The follower lost Chrome (browser killed or its port gone).
    BrowserLost,
    /// ffmpeg is dead or its pipe is broken.
    EncoderDead,
}

/// Why the guard STOPPED a healthy recording on a limit (LIFE-04).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Limit {
    /// Free space fell below the threshold (MiB free).
    DiskLow(u64),
    MaxDuration,
    /// Per-tab mode names the file that reached the limit.
    MaxSize(Option<String>),
}

/// What one guard tick learned about its recording.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Not the active recording any more (stopped, replaced): the guard exits.
    Gone,
    Healthy,
    Interrupted(Interrupt),
}

struct Active {
    /// Generation of this recording; a guard only acts on its own (D9-7).
    gen: u64,
    path: PathBuf,
    /// The one encoder (follow mode); per-tab files are owned by the follower.
    enc: Option<Arc<Encoder>>,
    rec: Tracker,
    started: Instant,
    fps: u32,
    /// Free-space probe captured at start (injectable, LIFE-04).
    free: FreeFn,
    /// The ffmpeg that wrote the video; the stop's poster run uses it (D10-1).
    ffmpeg: ResolvedFfmpeg,
}

/// Extras of an agent `record-stop` (OUT-06): the stop instant and the marker.
/// Only `Hub::stop_marked` passes Some. The exit paths (finish_orphan,
/// interrupt_if, stop_on_limit) pass None: no ffmpeg spawn while a process is
/// exiting, and their results and files stay exactly as before.
pub struct StopExtras {
    pub at: Instant,
    pub marker: MarkerIn,
}

#[derive(Default)]
pub struct Hub {
    active: Option<Active>,
    /// Result of a recording finished by the browser/surface going away,
    /// returned by the next stop and prepended to the next start (D7-7).
    orphaned: Option<(bool, String)>,
    knobs: Knobs,
    /// Last generation handed out.
    last_gen: u64,
}

/// Get the encoder back from the shared handle once the source released its
/// sink. The source's stop can time out with its loop still holding the sink,
/// so retry briefly and then report honestly instead of `expect` (a panic here
/// would wedge the hub with a live ffmpeg).
pub async fn take_encoder(
    mut enc: Arc<Encoder>,
    tries: u32,
    wait: Duration,
) -> Result<Encoder, String> {
    for attempt in 0..tries.max(1) {
        match Arc::try_unwrap(enc) {
            Ok(e) => return Ok(e),
            Err(back) => enc = back,
        }
        if attempt + 1 < tries {
            tokio::time::sleep(wait).await;
        }
    }
    Err(msg::not_finalized(&enc.path().display().to_string()))
}

impl Hub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_knobs(knobs: Knobs) -> Self {
        Self {
            knobs,
            ..Self::default()
        }
    }

    /// The tab the video follows right now (None: no recording, or no file for it).
    pub fn video_tab(&self) -> Option<String> {
        let t = self.active.as_ref()?.rec.current_tab();
        (!t.is_empty()).then_some(t)
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Generation of the active recording (None when idle).
    pub fn active_gen(&self) -> Option<u64> {
        self.active.as_ref().map(|a| a.gen)
    }

    /// Recordings dir of generation `gen` (where its files are written).
    pub fn dir_of(&self, gen: u64) -> Option<PathBuf> {
        let a = self.active.as_ref().filter(|a| a.gen == gen)?;
        a.path.parent().map(PathBuf::from)
    }

    /// The free-space probe and dir of generation `gen`, so the guard can run
    /// the (blocking) probe outside the hub lock.
    pub fn disk_probe(&self, gen: u64) -> Option<(FreeFn, PathBuf)> {
        let a = self.active.as_ref().filter(|a| a.gen == gen)?;
        Some((a.free.clone(), a.path.parent().map(PathBuf::from)?))
    }

    /// LIFE-04 limits of a healthy recording: disk low (`free` from the probe,
    /// `None` = unknown, never a stop), maximum duration, per-file size.
    pub fn limit_hit(&self, gen: u64, free: Option<u64>) -> Option<Limit> {
        let a = self.active.as_ref().filter(|a| a.gen == gen)?;
        if disk::is_low(free) {
            return Some(Limit::DiskLow(disk::mib(free.unwrap_or(0))));
        }
        if a.started.elapsed() >= self.knobs.max_duration.unwrap_or(disk::MAX_DURATION) {
            return Some(Limit::MaxDuration);
        }
        let max = self.knobs.max_file_bytes.unwrap_or(disk::MAX_FILE_BYTES);
        let files = match a.enc {
            Some(_) => vec![a.path.clone()],
            None => a.rec.files(),
        };
        let big = files
            .iter()
            .find(|f| std::fs::metadata(f).map(|m| m.len()).unwrap_or(0) >= max)?;
        let name = a.enc.is_none().then(|| {
            big.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        Some(Limit::MaxSize(name))
    }

    /// Stop and FINALIZE a recording that hit a limit; park the honest
    /// `stopped` outcome (ok = the finalize verdict). Stale generations do
    /// nothing, like `interrupt_if`.
    pub async fn stop_on_limit(&mut self, gen: u64, why: Limit) -> bool {
        if self.active.as_ref().map(|a| a.gen) != Some(gen) {
            return false;
        }
        let Some(a) = self.active.take() else {
            return false;
        };
        let path = a.path.display().to_string();
        let (ok, finish) = match self.finish_active(a, None, None).await {
            Ok(t) => (true, t),
            Err(HubError::Failed(t)) | Err(HubError::Usage(t)) => (false, t),
        };
        let reason = match why {
            Limit::DiskLow(m) => msg::reason_disk_low(m),
            Limit::MaxDuration => msg::reason_max_duration(),
            Limit::MaxSize(f) => msg::reason_max_size(f.as_deref()),
        };
        self.orphaned = Some((ok, scrub(&msg::stopped(&reason, &path, &finish))));
        true
    }

    /// One guard tick: cheap, non-async, no I/O.
    pub fn verdict(&self, gen: u64) -> Verdict {
        let Some(a) = self.active.as_ref().filter(|a| a.gen == gen) else {
            return Verdict::Gone;
        };
        if a.rec.browser_lost() {
            return Verdict::Interrupted(Interrupt::BrowserLost);
        }
        if a.enc.as_ref().is_some_and(|e| e.is_dead()) {
            return Verdict::Interrupted(Interrupt::EncoderDead);
        }
        Verdict::Healthy
    }

    /// Finalize the recording early (stdin closed so the partial file plays to
    /// the cut) and park an honest `interrupted` result, but only if `gen` is
    /// still the active recording: a stale guard never stops a newer one.
    pub async fn interrupt_if(&mut self, gen: u64, why: Interrupt) -> bool {
        if self.active.as_ref().map(|a| a.gen) != Some(gen) {
            return false;
        }
        let Some(a) = self.active.take() else {
            return false;
        };
        let (at_s, path) = (a.started.elapsed().as_secs_f64(), a.path.clone());
        let one_file = a.enc.is_some();
        let finish = match self.finish_active(a, None, None).await {
            Ok(t) | Err(HubError::Failed(t)) | Err(HubError::Usage(t)) => t,
        };
        // Judged after the finalize (ffmpeg writes its output on EOF). Follow
        // mode has one file; per-tab files are judged by their own report.
        let empty = one_file && std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) == 0;
        let reason = match why {
            Interrupt::BrowserLost => msg::reason_browser_lost(),
            Interrupt::EncoderDead => msg::reason_encoder_dead(),
        };
        self.orphaned = Some((
            false,
            scrub(&msg::interrupted(
                reason,
                at_s,
                &path.display().to_string(),
                empty,
                &finish,
            )),
        ));
        true
    }

    /// Text of the stored orphan result, if any (best-effort logging).
    pub fn orphaned_text(&self) -> Option<&str> {
        self.orphaned.as_ref().map(|(_, t)| t.as_str())
    }

    /// Start one recording. Cheap refusals come first so a refused call spawns
    /// nothing and creates no file or directory.
    pub async fn start(&mut self, env: &impl HubEnv, name: &str) -> Result<String, HubError> {
        if name.trim().is_empty() {
            return Err(HubError::Usage(msg::usage()));
        }
        match env.lane() {
            Lane::Real => return Err(failed(msg::real_lane())),
            Lane::Ephemeral => return Err(failed(msg::ephemeral())),
            Lane::Pipe => return Err(failed(msg::pipe())),
            Lane::Ws => {}
        }
        if let Some(a) = &self.active {
            let (p, age, h) = (
                a.path.display().to_string(),
                a.started.elapsed().as_millis() as u64,
                a.rec.health(),
            );
            return Err(failed(match a.enc {
                Some(_) => msg::active(&p, age, &h),
                None => msg::active_per_tab(&p, a.rec.files().len(), age, &h),
            }));
        }
        let cfg = env.config(name).map_err(failed)?;
        if !cfg.enabled {
            return Err(failed(msg::off()));
        }
        // LIFE-04: refuse below the threshold before anything is created.
        let free = env.free_fn();
        if let Some(f) = free(&cfg.dir).filter(|f| disk::is_low(Some(*f))) {
            return Err(failed(msg::disk_low_refusal(
                disk::mib(f),
                &cfg.dir.display().to_string(),
            )));
        }
        let ffmpeg = env.ffmpeg().await.map_err(|e| failed(e.to_string()))?;
        let poster_ff = ffmpeg.clone();
        let tab = env.current_tab().await.map_err(failed)?;

        // Per-tab mode names every file `<stem>-tab<N>-...`; the start tab is tab 1.
        let per_tab = cfg.tabs == Tabs::PerTab;
        let mut first_cfg = cfg.clone();
        if per_tab {
            first_cfg.name = Some(tab_stem(cfg.name.as_deref().unwrap_or(""), 1));
        }
        let (file, path) = create_recording_file(&first_cfg).map_err(failed)?;
        // ffmpeg opens the path itself (`-y`); only the creation was needed.
        drop(file);
        let mut ecfg = EncoderConfig::new(ffmpeg, &cfg);
        if let Some(t) = self.knobs.finish_timeout {
            ecfg.finish_timeout = t;
        }
        let enc = match Encoder::start(&ecfg, &path) {
            Ok(e) => Arc::new(e),
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return Err(failed(e.to_string()));
            }
        };
        let mut scfg = SourceConfig::from_record(&cfg);
        if let Some(g) = self.knobs.stop_grace {
            scfg.stop_grace = g;
        }
        if let Some(n) = self.knobs.no_frames_after {
            scfg.no_frames_after = n;
        }
        // Every source writes through a SegmentSink over the one encoder, so
        // the follower can rebind to another tab without ending the video.
        let probe = env.probe();
        let initial = first_view(&probe, &tab).await;
        let shared = FollowShared::new();
        let inner: Arc<dyn FrameSink> = enc.clone();
        let sink: Arc<dyn FrameSink> = shared.sink(inner.clone(), 1);
        let started = match env.attach() {
            Attach::Port(base) => {
                FrameSource::start(&Endpoint::Port { base }, &tab, scfg.clone(), sink).await
            }
            Attach::Ws(url) => FrameSource::start_ws(&url, &tab, scfg.clone(), sink).await,
        };
        let src = match started {
            Ok(s) => s,
            Err(e) => {
                // Half-applied start: ffmpeg is up but nothing feeds it. The
                // sink handed to the source is gone with the failed start, so
                // the encoder can be finished (it exits on EOF) BEFORE its
                // empty file is removed, leaving no process and no stray file.
                drop(inner);
                if let Ok(enc) = take_encoder(enc, 20, Duration::from_millis(50)).await {
                    let _ = enc.finish().await;
                }
                let _ = std::fs::remove_file(&path);
                return Err(failed(e.to_string()));
            }
        };
        // Active only now: both the encoder and the source are running.
        let p = path.display().to_string();
        let text = if per_tab {
            msg::started_per_tab(&p, &tab)
        } else {
            msg::started(&p, &tab)
        };
        let (policy, enc) = if per_tab {
            let files = Files::new(
                cfg.clone(),
                ecfg,
                scfg.stop_grace,
                (tab.clone(), path.clone(), enc, shared),
            );
            drop(inner);
            (Policy::PerTab(Box::new(files)), None)
        } else {
            (Policy::One { shared, inner }, Some(enc))
        };
        let rec = Tracker::spawn(SpawnArgs {
            first: src,
            tab,
            policy,
            probe,
            initial,
            popup_settle: self.knobs.popup_settle.unwrap_or(POPUP_SETTLE),
            scfg,
            poll: self.knobs.poll.unwrap_or(POLL),
            confirm: self.knobs.confirm.unwrap_or(CONFIRM),
        });
        self.last_gen += 1;
        self.active = Some(Active {
            gen: self.last_gen,
            path,
            enc,
            rec,
            started: Instant::now(),
            fps: cfg.fps,
            free,
            ffmpeg: poster_ff,
        });
        let text = match self.orphaned.take() {
            Some((_, t)) => format!("{}\n{text}", msg::orphan_text(&t)),
            None => text,
        };
        Ok(scrub(&text))
    }
}
