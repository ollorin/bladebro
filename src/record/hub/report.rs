//! Pure mapping of (source report, encoder report) to one honest stop result.
//!
//! Ok only for Captured + Finalized. Everything else is Err so the MCP
//! `isError` flag and the CLI exit code are truthful; the Err text still
//! carries the full report line (path, bytes, frames) because a kept file may
//! be playable. "saved" never appears in an Err.

use crate::record::encoder::EncoderReport;
use crate::record::source::SourceReport;

use super::msg::scrub;

/// One recorded stretch of one tab. Phase 8 appends; Phase 7 has exactly one.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub at_s: f64,
    pub tab: String,
}

/// Video length. The writer stamps frame i at i/fps (constant rate), so this
/// is the length of the file's content without shelling out to ffprobe.
fn duration_s(frames_written: u64, fps: u32) -> f64 {
    if fps == 0 {
        return 0.0;
    }
    frames_written as f64 / fps as f64
}

fn segments_text(segments: &[Segment]) -> String {
    let parts: Vec<String> = segments
        .iter()
        .map(|s| format!("{:.1} s tab {}", s.at_s, s.tab))
        .collect();
    format!("[{}]", parts.join(", "))
}

/// `<abs path> (bytes N, duration D.D s, frames N, dropped N, segments [...])`.
pub fn fields_line(
    src: &SourceReport,
    enc: &EncoderReport,
    fps: u32,
    segments: &[Segment],
) -> String {
    // Queue-full drops are already counted once by the encoder (the source's
    // queue_dropped is the same events), so only bus-lag losses are added.
    let dropped = enc.frames_dropped + src.stats.lost;
    let mut s = format!(
        "{} (bytes {}, duration {:.1} s, frames {}, dropped {}, segments {}",
        enc.path.display(),
        enc.bytes,
        duration_s(enc.frames_written, fps),
        enc.frames_written,
        dropped,
        segments_text(segments)
    );
    // Held-frame repeats make frames_written differ from what Chrome sent.
    if src.stats.forwarded != enc.frames_written {
        s.push_str(&format!(", captured {}", src.stats.forwarded));
    }
    s.push(')');
    s
}

/// Compose the stop result: `(true, "recording saved: ...")` or
/// `(false, "recording not clean: ...")`.
pub fn compose(
    src: &SourceReport,
    enc: &EncoderReport,
    fps: u32,
    segments: &[Segment],
    tab_note: Option<&str>,
) -> (bool, String) {
    let fields = fields_line(src, enc, fps, segments);
    let ok = src.ok() && enc.ok();
    let mut text = if ok {
        let mut t = format!("recording saved: {fields}");
        if let Some(q) = src.quiet_tail_ms {
            t.push_str(&format!(
                ". Note: no new frames for {:.1} s (normal for a static page; a background or stalled tab looks the same, so the video holds the last frame)",
                q as f64 / 1000.0
            ));
        }
        t
    } else {
        // The encoder verdict wins when the encoder is not clean: it names the
        // file state; otherwise the source verdict names the capture problem.
        let first = if enc.ok() {
            src.verdict()
        } else {
            enc.verdict()
        };
        format!("recording not clean: {first}; {fields}")
    };
    if let Some(n) = tab_note {
        text.push_str(&format!(". Note: {n}"));
    }
    (ok, scrub(&text))
}
