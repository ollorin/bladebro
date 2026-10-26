//! Action timeline (OUT-07, D10-3): a bounded per-recording log of the
//! action / run / tab-changing state verdicts the tools already returned,
//! placed on the video clock when the agent stops the recording.
//!
//! Why inert when off: the two hooks (proto.rs, cli.rs) run on every tool
//! call of every user. With no recording, `begin` is one relaxed atomic load
//! and returns None, so the recording-off output and CDP traffic stay
//! byte-identical to a build without this module.
//!
//! Module map:
//! - `Log` / `Entry`: bounded, closable per-recording log (MAX_ENTRIES).
//! - `include` / `verdict_line`: which calls are logged and their one-line verdict.
//! - `begin` / `end`: the two hook calls (proto.rs, cli.rs). Inert when off.
//! - `place` / `Placed`: entries on the video clock.
//! - `to_json`: the timeline sidecar, version 1 (D10-3).
//!
//! Fork-only module (SYNC-01).

use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{json, Value};

use crate::error::Result as BladeResult;
use crate::record::marker::clean_text;

/// Most entries kept per recording; later calls are counted as `dropped`.
pub const MAX_ENTRIES: usize = 1000;
/// Longest verdict kept (chars). Verdicts are first-line summaries, not payloads.
pub const MAX_VERDICT_CHARS: usize = 200;
/// Longest action label kept (chars); action names are short tokens.
const MAX_ACTION_CHARS: usize = 32;

/// Clock text when the video has a first frame (D10-4).
pub const CLOCK_FIRST_FRAME: &str = "seconds from the first video frame";
/// Clock text when no frame ever arrived (fallback to record-start).
pub const CLOCK_START: &str = "seconds from record-start (no video frame)";

/// One finished tool call.
#[derive(Debug, Clone)]
pub struct Entry {
    pub start: Instant,
    pub end: Instant,
    pub tool: String,
    pub action: String,
    pub steps: Option<usize>,
    pub ok: bool,
    pub verdict: String,
}

#[derive(Default)]
struct LogInner {
    entries: Vec<Entry>,
    dropped: u64,
    closed: bool,
}

/// Bounded log owned by one recording. `take` closes it: later pushes are
/// ignored, so an action that ends after its recording stopped never lands in
/// the next recording's timeline.
#[derive(Default)]
pub struct Log {
    inner: Mutex<LogInner>,
}

impl Log {
    pub fn new() -> Arc<Log> {
        Arc::new(Log::default())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LogInner> {
        // A poisoned lock still holds consistent data (every mutation is one
        // push or one counter bump); recording must not panic the server.
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn push(&self, e: Entry) {
        let mut g = self.lock();
        if g.closed {
            return;
        }
        if g.entries.len() >= MAX_ENTRIES {
            g.dropped += 1;
        } else {
            g.entries.push(e);
        }
    }

    /// Entries and the dropped count; closes the log.
    pub fn take(&self) -> (Vec<Entry>, u64) {
        let mut g = self.lock();
        g.closed = true;
        let entries = std::mem::take(&mut g.entries);
        (entries, g.dropped)
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Log of the recording currently active in this process (set by the hub).
static CURRENT: Mutex<Option<Arc<Log>>> = Mutex::new(None);

/// Publish (or clear) the active recording's log. Called by the hub.
pub fn publish(log: Option<Arc<Log>>) {
    let mut g = CURRENT.lock().unwrap_or_else(|p| p.into_inner());
    *g = log;
}

fn current() -> Option<Arc<Log>> {
    CURRENT.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Which calls enter the timeline and their action label (D10-3): every
/// `act`, every `run` (with its top-level step count), and `state` only for
/// the tab-changing ops. Reads (`see`, `vision`) and recording control are not
/// listed.
pub fn include(tool: &str, args: &Value) -> Option<(String, Option<usize>)> {
    match tool {
        "act" => {
            let action = args.get("action").and_then(Value::as_str).unwrap_or("");
            Some((clean_text(action, MAX_ACTION_CHARS), None))
        }
        "run" => {
            let steps = args.get("steps").and_then(Value::as_array).map(Vec::len);
            Some(("run".to_string(), steps))
        }
        "state" => {
            let op = args.get("op").and_then(Value::as_str).unwrap_or("");
            match op {
                "open-tab" | "close-tab" | "switch-tab" => Some((op.to_string(), None)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// First non-empty line of a result, cleaned and bounded (D10-3). Lines whose
/// cleaned form is empty (only controls or format chars) are skipped.
pub fn verdict_line(text: &str) -> String {
    text.lines()
        .map(|l| clean_text(l, MAX_VERDICT_CHARS))
        .find(|l| !l.is_empty())
        .unwrap_or_default()
}

/// An in-flight call. Holds the log of the recording it started in.
pub struct Begin {
    log: Arc<Log>,
    tool: String,
    action: String,
    steps: Option<usize>,
    start: Instant,
}

/// Start a call's entry, given whether recording is active and the log.
/// `active` is checked first so the off path does no other work.
pub fn begin_with(active: bool, log: Option<Arc<Log>>, tool: &str, args: &Value) -> Option<Begin> {
    if !active {
        return None;
    }
    let log = log?;
    let (action, steps) = include(tool, args)?;
    Some(Begin {
        log,
        tool: tool.to_string(),
        action,
        steps,
        start: Instant::now(),
    })
}

/// Start a call's entry. One atomic load when recording is off.
pub fn begin(tool: &str, args: &Value) -> Option<Begin> {
    if !crate::record::hub::recording_active() {
        return None;
    }
    begin_with(true, current(), tool, args)
}

/// Finish a call's entry with the result it produced.
pub fn end(b: Option<Begin>, r: &BladeResult<String>) {
    let Some(b) = b else {
        return;
    };
    let (ok, verdict) = match r {
        Ok(text) => (true, verdict_line(text)),
        Err(e) => (false, verdict_line(&e.to_string())),
    };
    b.log.push(Entry {
        start: b.start,
        end: Instant::now(),
        tool: b.tool,
        action: b.action,
        steps: b.steps,
        ok,
        verdict,
    });
}

/// An entry placed on the video clock.
#[derive(Debug, Clone, PartialEq)]
pub enum Placed {
    Action {
        at_s: f64,
        end_s: f64,
        tool: String,
        action: String,
        steps: Option<usize>,
        ok: bool,
        verdict: String,
    },
    Marker {
        at_s: f64,
        note: String,
    },
}

impl Placed {
    fn at_s(&self) -> f64 {
        match self {
            Placed::Action { at_s, .. } | Placed::Marker { at_s, .. } => *at_s,
        }
    }
}

/// Place entries on the video clock (0 s = `clock`), clamped to `[0, max_s]`.
pub fn place(entries: &[Entry], clock: Instant, max_s: f64) -> Vec<Placed> {
    use crate::record::hub::evidence::offset_s;
    entries
        .iter()
        .map(|e| Placed::Action {
            at_s: offset_s(e.start, clock, max_s),
            end_s: offset_s(e.end, clock, max_s),
            tool: e.tool.clone(),
            action: e.action.clone(),
            steps: e.steps,
            ok: e.ok,
            verdict: e.verdict.clone(),
        })
        .collect()
}

/// One video file of the recording, on the recording clock.
#[derive(Debug, Clone)]
pub struct FileClock {
    pub file: String,
    pub start_s: Option<f64>,
    pub duration_s: f64,
}

/// Last path component, split on both separators so a Windows path never
/// leaks its directory into the sidecar.
fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// The timeline sidecar, version 1 (D10-3). Basenames only; entries sorted by
/// `at_s` (stable).
pub fn to_json(
    video: &str,
    duration_s: f64,
    files: &[FileClock],
    placed: &[Placed],
    dropped: u64,
    clock_text: &str,
) -> Value {
    let mut sorted: Vec<&Placed> = placed.iter().collect();
    sorted.sort_by(|a, b| a.at_s().total_cmp(&b.at_s()));
    let entries: Vec<Value> = sorted.into_iter().map(entry_json).collect();
    let files: Vec<Value> = files
        .iter()
        .map(|f| {
            json!({
                "file": basename(&f.file),
                "start_s": f.start_s.map(r2),
                "duration_s": r2(f.duration_s),
            })
        })
        .collect();
    json!({
        "version": 1,
        "clock": clock_text,
        "video": basename(video),
        "duration_s": r2(duration_s),
        "files": files,
        "entries": entries,
        "dropped": dropped,
    })
}

fn entry_json(p: &Placed) -> Value {
    match p {
        Placed::Action {
            at_s,
            end_s,
            tool,
            action,
            steps,
            ok,
            verdict,
        } => {
            let mut v = json!({
                "kind": "action",
                "at_s": r2(*at_s),
                "end_s": r2(*end_s),
                "tool": tool,
                "action": action,
                "ok": ok,
                "verdict": verdict,
            });
            if let (Some(n), Some(obj)) = (steps, v.as_object_mut()) {
                obj.insert("steps".into(), json!(n));
            }
            v
        }
        Placed::Marker { at_s, note } => json!({
            "kind": "marker",
            "at_s": r2(*at_s),
            "note": note,
        }),
    }
}
