//! Pure composer of the per-tab stop result (CAPT-04): one compact line that
//! lists every file. Reuses `report::fields_line` for each file (its own
//! segments, offsets from that file's first frame) and leaves `report::compose`
//! (the one-file path) untouched.
//!
//! Clean means: every file finalized, every file's capture captured frames,
//! no run-level failure. A cap note and a "closed at" note are information,
//! not failures (the cap is a documented operator limit).

use std::path::PathBuf;

use super::msg::{self, scrub};
use super::report::{fields_line, Segment};
use crate::record::encoder::EncoderReport;
use crate::record::source::SourceReport;

/// One tab's finished file.
#[derive(Debug, Clone)]
pub struct FileOutcome {
    pub tab: String,
    /// First-visit order from 1 (the `-tab<N>-` of its name).
    pub n: u32,
    pub path: PathBuf,
    /// This file's visits merged into one report.
    pub src: SourceReport,
    /// `Err` = the encoder could not be taken back or finished in time.
    pub enc: Result<EncoderReport, String>,
    pub segments: Vec<Segment>,
    /// Capture failures of this file (already named by the merge).
    pub failures: Vec<String>,
    /// Seconds into the recording at which the tab closed, if it did.
    pub closed_at: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct PerTabResult {
    pub files: Vec<FileOutcome>,
    /// Run-level failures (tab not recorded, browser lost, no tab left, ...).
    pub failures: Vec<String>,
    /// Information that does not make the result not clean.
    pub notes: Vec<String>,
}

fn line(f: &FileOutcome, fps: u32) -> String {
    match &f.enc {
        Ok(e) => fields_line(&f.src, e, fps, &f.segments),
        Err(_) => format!("{} (not finalized)", f.path.display()),
    }
}

/// Every reason the result is not clean, files in order, then run-level.
fn verdicts(r: &PerTabResult) -> Vec<String> {
    let mut v = Vec::new();
    for f in &r.files {
        let named = |t: String| {
            if t.starts_with("tab ") {
                t
            } else {
                format!("tab {}: {t}", f.tab)
            }
        };
        match &f.enc {
            Err(m) => v.push(named(m.clone())),
            Ok(e) if !e.ok() => v.push(named(e.verdict())),
            Ok(_) => {}
        }
        v.extend(f.failures.iter().cloned().map(named));
        if f.failures.is_empty() && f.enc.as_ref().is_ok_and(|e| e.ok()) && !f.src.ok() {
            v.push(named(f.src.verdict()));
        }
    }
    v.extend(r.failures.iter().cloned());
    v
}

pub fn compose_per_tab(r: &PerTabResult, fps: u32) -> (bool, String) {
    let n = r.files.len();
    let head = format!("(per-tab, {n} file{})", if n == 1 { "" } else { "s" });
    let lines: Vec<String> = r.files.iter().map(|f| line(f, fps)).collect();
    let mut bad = verdicts(r);
    let ok = bad.is_empty();
    let mut notes: Vec<String> = Vec::new();
    let mut text = if ok {
        format!("recording saved {head}: {}", lines.join("; "))
    } else {
        let first = bad.remove(0);
        // Later reasons stay visible, in the note slot.
        notes.append(&mut bad);
        let rest = if lines.is_empty() {
            String::new()
        } else {
            format!("; {}", lines.join("; "))
        };
        format!("recording not clean {head}: {first}{rest}")
    };
    notes.extend(r.notes.iter().cloned());
    for f in &r.files {
        if let Some(at) = f.closed_at {
            notes.push(msg::tab_closed(&f.tab, at));
        }
    }
    if !notes.is_empty() {
        text.push_str(&format!(". Note: {}", notes.join("; ")));
    }
    (ok, scrub(&text))
}
