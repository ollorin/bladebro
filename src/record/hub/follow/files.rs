//! Sink policy of the follower: where a tab's frames go.
//!
//! `One` (default): every source writes into the one encoder through a
//! generation-filtered `SegmentSink` (see `sink.rs`). `PerTab` (CAPT-04): each
//! tab gets its own file and encoder, at most [`MAX_TAB_FILES`] open at once.
//!
//! Per-tab lifecycle, in one place:
//! - enter a tab: it has an open file -> a new source writes into that file
//!   (return to a tab continues it); else a free slot -> create the file via
//!   `create_recording_file` (Phase 3 dir/permission/collision rules) and an
//!   `Encoder`; else the cap note (once per tab) and no source.
//! - a file whose tab is absent from two consecutive tab lists is finalized in
//!   the background (`mark_end`, wait for the sources' sinks, `finish`) and
//!   frees its slot.
//! - stop: `mark_end` everything, finalize every open file concurrently, each
//!   bounded; a file that cannot be finished in time is reported
//!   `not finalized` and its encoder is dropped (`kill_on_drop`).
//!
//! Ownership of an `Arc<Encoder>`: the `TabFile` (until its finalize task takes
//! it) plus one transient clone inside each live source's `SegmentSink`. The
//! follower's state is never locked across an await.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;

use super::merge::{merge, MergeIn};
use super::probe::TabView;
use super::run::{seg_in, SegRec};
use super::sink::FollowShared;
use crate::record::encoder::EncoderReport;
use crate::record::hub::follow_report::{FileOutcome, PerTabResult};
use crate::record::hub::machine::take_encoder;
use crate::record::hub::msg;
use crate::record::{
    create_recording_file, tab_stem, Encoder, EncoderConfig, FrameSink, RecordConfig,
};

/// Most per-tab files (encoders, ffmpeg processes) open at once.
pub const MAX_TAB_FILES: usize = 8;
/// A tab must be missing from this many consecutive lists to count as closed
/// (one glitchy list must not end a file).
const ABSENT_TICKS: u32 = 2;

pub enum Policy {
    One {
        shared: Arc<FollowShared>,
        inner: Arc<dyn FrameSink>,
    },
    PerTab(Box<Files>),
}

/// A sink for the new source, the state it belongs to, and the generation to
/// restore when the source cannot start.
pub struct Entered {
    pub sink: Arc<dyn FrameSink>,
    pub shared: Arc<FollowShared>,
    pub prev: u64,
}

/// Why a tab got no source.
pub enum Skip {
    /// Not clean: the text names the tab and the cause.
    Failed(String),
    /// Operator limit (cap), information only.
    Capped(String),
    /// Already reported for this tab.
    Silent,
}

impl Policy {
    pub fn enter(&mut self, v: &TabView, gen: u64, at_s: f64) -> Result<Entered, Skip> {
        match self {
            Policy::One { shared, inner } => {
                let prev = shared.current_gen();
                shared.set_gen(gen);
                Ok(Entered {
                    sink: shared.sink(inner.clone(), gen),
                    shared: shared.clone(),
                    prev,
                })
            }
            Policy::PerTab(f) => f.enter(v, gen, at_s),
        }
    }

    /// The video clock origin (first accepted frame).
    pub fn clock(&self) -> Option<Instant> {
        match self {
            Policy::One { shared, .. } => shared.t0_at(),
            Policy::PerTab(f) => f.map.values().filter_map(|t| t.shared.t0_at()).min(),
        }
    }

    /// Timestamp of the first frame accepted for visit `gen` of `tab`.
    pub fn first_ts(&self, tab: &str, gen: u64) -> Option<u64> {
        match self {
            Policy::One { shared, .. } => shared.first_ts(gen),
            Policy::PerTab(f) => f.shared_of(tab).and_then(|s| s.first_ts(gen)),
        }
    }

    /// End every open video at the stop request.
    pub fn mark_end(&self) {
        match self {
            Policy::One { inner, .. } => inner.mark_end(),
            Policy::PerTab(f) => f
                .map
                .values()
                .filter_map(|t| t.enc.as_ref())
                .for_each(|e| e.mark_end()),
        }
    }

    pub fn close_missing(&mut self, present: &[String], keep: Option<&str>, at_s: f64) {
        if let Policy::PerTab(f) = self {
            f.close_missing(present, keep, at_s);
        }
    }

    pub fn files(&self) -> Vec<PathBuf> {
        match self {
            Policy::One { .. } => Vec::new(),
            Policy::PerTab(f) => f.paths(),
        }
    }
}

struct TabFile {
    n: u32,
    path: PathBuf,
    enc: Option<Arc<Encoder>>,
    shared: Arc<FollowShared>,
    created: Instant,
    closed_at: Option<f64>,
    absent: u32,
    fin: Option<JoinHandle<Result<EncoderReport, String>>>,
}

pub struct Files {
    cfg: RecordConfig,
    ecfg: EncoderConfig,
    map: HashMap<String, TabFile>,
    next_n: u32,
    open: usize,
    /// Tabs whose cap note was already given.
    capped: HashSet<String>,
    /// How long a finalize waits for the sources' sinks to be released.
    tries: u32,
    /// Bound on the whole stop (one stop grace + the finish timeout + slack).
    stop_cap: Duration,
}

/// Finalize one file: end its video, wait for every sink to be dropped, finish.
async fn finalize(enc: Arc<Encoder>, tries: u32) -> Result<EncoderReport, String> {
    enc.mark_end();
    let e = take_encoder(enc, tries, Duration::from_millis(50)).await?;
    Ok(e.finish().await)
}

impl Files {
    /// Seed with the start tab's already-running file (tab 1).
    pub fn new(
        cfg: RecordConfig,
        ecfg: EncoderConfig,
        grace: Duration,
        first: (String, PathBuf, Arc<Encoder>, Arc<FollowShared>),
    ) -> Files {
        let stop_cap = Self::stop_cap(&ecfg, grace);
        let tries = ((grace * 2 + Duration::from_secs(4)).as_millis() / 50) as u32;
        let (tab, path, enc, shared) = first;
        let mut map = HashMap::new();
        map.insert(tab, TabFile::new(1, path, enc, shared));
        Files {
            cfg,
            ecfg,
            map,
            next_n: 2,
            open: 1,
            capped: HashSet::new(),
            tries,
            stop_cap,
        }
    }

    pub fn stop_cap(ecfg: &EncoderConfig, grace: Duration) -> Duration {
        ecfg.finish_timeout + grace * 2 + Duration::from_secs(5)
    }

    pub fn cap(&self) -> Duration {
        self.stop_cap
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        let mut v: Vec<_> = self.map.values().map(|t| (t.n, t.path.clone())).collect();
        v.sort();
        v.into_iter().map(|(_, p)| p).collect()
    }

    pub fn shared_of(&self, tab: &str) -> Option<Arc<FollowShared>> {
        self.map.get(tab).map(|t| t.shared.clone())
    }

    fn enter(&mut self, v: &TabView, gen: u64, at_s: f64) -> Result<Entered, Skip> {
        if !self.map.contains_key(&v.id) {
            self.create(v, at_s)?;
        }
        let Some(f) = self.map.get(&v.id) else {
            return Err(Skip::Silent);
        };
        let Some(enc) = &f.enc else {
            // Closed (or finished) tabs do not come back; a list glitch must
            // not recreate a second file for the same tab.
            return Err(Skip::Failed(msg::tab_file_done(&v.id, at_s)));
        };
        let prev = f.shared.current_gen();
        f.shared.set_gen(gen);
        let inner: Arc<dyn FrameSink> = enc.clone();
        Ok(Entered {
            sink: f.shared.sink(inner, gen),
            shared: f.shared.clone(),
            prev,
        })
    }

    fn create(&mut self, v: &TabView, at_s: f64) -> Result<(), Skip> {
        if self.open >= MAX_TAB_FILES {
            return Err(if self.capped.insert(v.id.clone()) {
                Skip::Capped(msg::tab_capped(&v.id, at_s, MAX_TAB_FILES))
            } else {
                Skip::Silent
            });
        }
        let n = self.next_n;
        let mut cfg = self.cfg.clone();
        cfg.name = Some(tab_stem(self.cfg.name.as_deref().unwrap_or(""), n));
        let fail = |e: String| Skip::Failed(msg::rebind_failed(&v.id, at_s, &e));
        let (file, path) = create_recording_file(&cfg).map_err(fail)?;
        // ffmpeg opens the path itself (`-y`); only the creation was needed.
        drop(file);
        let enc = match Encoder::start(&self.ecfg, &path) {
            Ok(e) => Arc::new(e),
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return Err(fail(e.to_string()));
            }
        };
        self.next_n += 1;
        self.open += 1;
        self.map.insert(
            v.id.clone(),
            TabFile::new(n, path, enc, FollowShared::new()),
        );
        Ok(())
    }

    /// Finalize the files of tabs that are gone. `keep` is the tab the follower
    /// still has a source on (its file is finished once the source is retired).
    fn close_missing(&mut self, present: &[String], keep: Option<&str>, at_s: f64) {
        let tries = self.tries;
        for (id, f) in self.map.iter_mut() {
            if f.enc.is_none() {
                continue;
            }
            if present.iter().any(|p| p == id) || keep == Some(id.as_str()) {
                f.absent = 0;
                continue;
            }
            f.absent += 1;
            if f.absent < ABSENT_TICKS {
                continue;
            }
            if let Some(enc) = f.enc.take() {
                // The video ends at the close, not at the later stop.
                enc.mark_end();
                f.closed_at = Some(at_s);
                f.fin = Some(tokio::spawn(finalize(enc, tries)));
                self.open -= 1;
            }
        }
    }

    /// Finalize everything still open and describe every file. Never panics;
    /// a file that cannot be finished in time is reported `not finalized`.
    pub async fn finish(
        &mut self,
        segs: &[SegRec],
        no_frames_after: Duration,
        failures: Vec<String>,
        notes: Vec<String>,
    ) -> PerTabResult {
        let tries = self.tries;
        for f in self.map.values_mut() {
            if let Some(enc) = f.enc.take() {
                f.fin = Some(tokio::spawn(finalize(enc, tries)));
            }
        }
        let deadline = Instant::now() + self.stop_cap;
        let mut ids: Vec<String> = self.map.keys().cloned().collect();
        ids.sort_by_key(|k| self.map[k].n);
        let now = Instant::now();
        let mut files = Vec::with_capacity(ids.len());
        for id in ids {
            let Some(f) = self.map.get_mut(&id) else {
                continue;
            };
            let enc = match f.fin.take() {
                Some(mut h) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    match tokio::time::timeout(left, &mut h).await {
                        Ok(Ok(r)) => r,
                        Ok(Err(_)) => Err(msg::not_finalized(&f.path.display().to_string())),
                        Err(_) => {
                            // Dropping the task drops the encoder: kill_on_drop.
                            h.abort();
                            Err(msg::not_finalized(&f.path.display().to_string()))
                        }
                    }
                }
                None => Err(msg::not_finalized(&f.path.display().to_string())),
            };
            let t0 = f.shared.t0_at().unwrap_or(f.created);
            let mine: Vec<_> = segs
                .iter()
                .filter(|s| s.tab == id)
                .map(|s| seg_in(s, &f.shared, t0, now))
                .collect();
            let m = merge(MergeIn {
                segs: mine,
                t0_ts_ms: f.shared.t0_ts(),
                failures: Vec::new(),
                notes: Vec::new(),
                no_frames_after_ms: no_frames_after.as_millis() as u64,
            });
            files.push(FileOutcome {
                tab: id,
                n: f.n,
                path: f.path.clone(),
                src: m.report,
                enc,
                segments: m.segments,
                failures: m.failures,
                closed_at: f.closed_at,
            });
        }
        PerTabResult {
            files,
            failures,
            notes,
        }
    }
}

impl TabFile {
    fn new(n: u32, path: PathBuf, enc: Arc<Encoder>, shared: Arc<FollowShared>) -> TabFile {
        TabFile {
            n,
            path,
            enc: Some(enc),
            shared,
            created: Instant::now(),
            closed_at: None,
            absent: 0,
            fin: None,
        }
    }
}
