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
    compose_follow, first_view, Files, FollowShared, Outcome, Policy, SpawnArgs, Tracker, CONFIRM,
    NEW_TAB_CONFIRM, POLL,
};
use super::follow_report::compose_per_tab;
use super::msg::{self, scrub};
use crate::record::{
    create_recording_file, tab_stem, Encoder, EncoderConfig, Endpoint, FrameSink, FrameSource,
    SourceConfig, Tabs,
};

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
    /// How long a brand-new tab must stay in front before it is followed.
    pub new_confirm: Option<Duration>,
    /// A zero-frame segment this long is a failure (default: the 5 s watchdog).
    pub no_frames_after: Option<Duration>,
}

struct Active {
    path: PathBuf,
    /// The one encoder (follow mode); per-tab files are owned by the follower.
    enc: Option<Arc<Encoder>>,
    rec: Tracker,
    started: Instant,
    fps: u32,
}

#[derive(Default)]
pub struct Hub {
    active: Option<Active>,
    /// Result of a recording finished by the browser/surface going away,
    /// returned by the next stop and prepended to the next start (D7-7).
    orphaned: Option<(bool, String)>,
    knobs: Knobs,
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

    pub fn is_active(&self) -> bool {
        self.active.is_some()
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
        let ffmpeg = env.ffmpeg().await.map_err(|e| failed(e.to_string()))?;
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
            new_confirm: self.knobs.new_confirm.unwrap_or(NEW_TAB_CONFIRM),
            scfg,
            poll: self.knobs.poll.unwrap_or(POLL),
            confirm: self.knobs.confirm.unwrap_or(CONFIRM),
        });
        self.active = Some(Active {
            path,
            enc,
            rec,
            started: Instant::now(),
            fps: cfg.fps,
        });
        let text = match self.orphaned.take() {
            Some((_, t)) => format!("{}{t}\n{text}", msg::orphan_prefix()),
            None => text,
        };
        Ok(scrub(&text))
    }

    /// Stop the active recording and report it honestly (D7-4/5).
    pub async fn stop(&mut self, env: &impl HubEnv) -> Result<String, HubError> {
        let Some(a) = self.active.take() else {
            return match self.orphaned.take() {
                Some((ok, t)) => {
                    let t = scrub(&format!("{}{t}", msg::orphan_prefix()));
                    if ok {
                        Ok(t)
                    } else {
                        Err(HubError::Failed(t))
                    }
                }
                None => Err(failed(msg::none())),
            };
        };
        // The agent's current tab, for the follower's catch-up of a switch
        // younger than one poll. A failed lookup only skips the catch-up.
        let hint = env.current_tab().await.ok();
        self.finish_active(a, hint).await
    }

    /// The browser or surface is going away: stop like `stop` and keep the
    /// result for the next call (D7-7). Never fails.
    pub async fn finish_orphan(&mut self) {
        let Some(a) = self.active.take() else {
            return;
        };
        self.orphaned = Some(match self.finish_active(a, None).await {
            Ok(t) => (true, t),
            Err(HubError::Failed(t)) | Err(HubError::Usage(t)) => (false, t),
        });
    }

    async fn finish_active(&self, a: Active, hint: Option<String>) -> Result<String, HubError> {
        let Active { enc, rec, fps, .. } = a;
        // Follower first: it ends the video, drains every source's acks and
        // releases every sink so the encoder can be taken back.
        let (ok, text) = match rec.stop(hint).await {
            Outcome::PerTab(r) => compose_per_tab(&r, fps),
            Outcome::One(fr) => {
                let Some(enc) = enc else {
                    return Err(failed(msg::follower_stuck()));
                };
                let enc = match take_encoder(enc, 20, Duration::from_millis(50)).await {
                    Ok(e) => e,
                    Err(m) => return Err(failed(m)),
                };
                let er = enc.finish().await;
                compose_follow(&fr, &er, fps)
            }
        };
        if ok {
            Ok(text)
        } else {
            Err(HubError::Failed(text))
        }
    }
}
