//! The hub state machine: Idle <-> Active over an injected `HubEnv`.
//!
//! All logic lives on `Hub` instances so tests never touch the process-global
//! (see `hub.rs`). The async mutex around the global is held across a whole
//! op, so two concurrent starts cannot both pass the Idle check (D7-9 b).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::env::{Attach, HubEnv, Lane};
use super::msg::{self, scrub};
use super::report::{compose, Segment};
use crate::record::{
    create_recording_file, Encoder, EncoderConfig, Endpoint, FrameSink, FrameSource, SourceConfig,
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
}

struct Active {
    path: PathBuf,
    enc: Arc<Encoder>,
    src: FrameSource,
    tab: String,
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
            return Err(failed(msg::active(
                &a.path.display().to_string(),
                a.started.elapsed().as_millis() as u64,
                &a.src.health(),
            )));
        }
        let cfg = env.config(name).map_err(failed)?;
        if !cfg.enabled {
            return Err(failed(msg::off()));
        }
        let ffmpeg = env.ffmpeg().await.map_err(|e| failed(e.to_string()))?;
        let tab = env.current_tab().await.map_err(failed)?;

        let (file, path) = create_recording_file(&cfg).map_err(failed)?;
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
        let sink: Arc<dyn FrameSink> = enc.clone();
        let started = match env.attach() {
            Attach::Port(base) => {
                FrameSource::start(&Endpoint::Port { base }, &tab, scfg, sink).await
            }
            Attach::Ws(url) => FrameSource::start_ws(&url, &tab, scfg, sink).await,
        };
        let src = match started {
            Ok(s) => s,
            Err(e) => {
                // Half-applied start: ffmpeg is up but nothing feeds it. The
                // sink handed to the source is gone with the failed start, so
                // the encoder can be finished (it exits on EOF) BEFORE its
                // empty file is removed, leaving no process and no stray file.
                if let Ok(enc) = take_encoder(enc, 20, Duration::from_millis(50)).await {
                    let _ = enc.finish().await;
                }
                let _ = std::fs::remove_file(&path);
                return Err(failed(e.to_string()));
            }
        };
        // Active only now: both the encoder and the source are running.
        let text = msg::started(&path.display().to_string(), &tab);
        self.active = Some(Active {
            path,
            enc,
            src,
            tab,
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
        // A failed lookup just means no note; it never blocks the stop.
        let note = match env.current_tab().await {
            Ok(now) if now != a.tab => Some(msg::tab_moved(&a.tab, &now)),
            _ => None,
        };
        self.finish_active(a, note).await
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

    async fn finish_active(&self, a: Active, note: Option<String>) -> Result<String, HubError> {
        let Active {
            enc, src, tab, fps, ..
        } = a;
        // Source first: it marks the end of the video, drains acks and
        // releases the sink so the encoder can be taken back.
        let sr = src.stop().await;
        let enc = match take_encoder(enc, 20, Duration::from_millis(50)).await {
            Ok(e) => e,
            Err(m) => return Err(failed(m)),
        };
        let er = enc.finish().await;
        let segments = [Segment { at_s: 0.0, tab }];
        let (ok, text) = compose(&sr, &er, fps, &segments, note.as_deref());
        if ok {
            Ok(text)
        } else {
            Err(HubError::Failed(text))
        }
    }
}
