//! Stop extras of an agent `record-stop` (OUT-06, D10-1/2/5): the poster PNG of
//! the finished video and the marker offset on the video clock, appended to the
//! stop result. Everything here is pure except `finish_extras`, which runs the
//! one bounded poster ffmpeg (`record::poster::make_poster`).
//!
//! Clock (D10-4/5): 0 s = the earliest first accepted frame of the recording's
//! files; with no frame at all, the record-start instant (the result names it
//! wherever a clock is quoted). Offsets are saturating and clamped to the
//! recording's length on that clock.

use std::path::PathBuf;
use std::time::Instant;

use super::follow::FollowResult;
use super::follow_report::PerTabResult;
use crate::record::encoder::{EncoderReport, ResolvedFfmpeg};
use crate::record::marker::MarkerIn;
use crate::record::poster::make_poster;
use crate::record::sidecar::write_sidecar;
use crate::record::timeline::{
    place, to_json, Entry, FileClock, Placed, CLOCK_FIRST_FRAME, CLOCK_START,
};

/// Suffix of the timeline sidecar, named after the first video (D10-3).
pub const TIMELINE_SUFFIX: &str = ".timeline.json";

/// One video of the recording, as the stop saw it.
#[derive(Debug, Clone)]
pub struct VideoFile {
    pub path: PathBuf,
    /// Arrival of its first accepted frame (None: it got no frame).
    pub t0_at: Option<Instant>,
    pub duration_s: f64,
    /// True only for a proven, finalized file (the encoder said so).
    pub finalized: bool,
}

/// What `finish_extras` needs from a finished recording.
#[derive(Debug, Clone)]
pub struct Stopped {
    /// Videos in tab order (one entry in follow mode).
    pub videos: Vec<VideoFile>,
    /// Index of the file the video followed at stop; the poster prefers it.
    pub poster_from: Option<usize>,
    /// Record-start instant: the clock when no file got a frame.
    pub fallback_clock: Instant,
    /// When the agent's record-stop arrived (the marker's instant).
    pub stop_at: Instant,
}

/// The recording clock: the earliest first frame, else the fallback.
pub fn clock(s: &Stopped) -> Instant {
    s.videos
        .iter()
        .filter_map(|v| v.t0_at)
        .min()
        .unwrap_or(s.fallback_clock)
}

/// Seconds from `clock` to `at`: 0.0 before it, at most `max_s`, 2 decimals.
pub fn offset_s(at: Instant, clock: Instant, max_s: f64) -> f64 {
    let raw = at
        .checked_duration_since(clock)
        .map_or(0.0, |d| d.as_secs_f64());
    let clamped = raw.clamp(0.0, max_s.max(0.0));
    (clamped * 100.0).round() / 100.0
}

/// Length of the recording on the video clock: the latest file end.
fn span_s(v: &[VideoFile], clock: Instant) -> f64 {
    v.iter()
        .map(|f| {
            let start = f
                .t0_at
                .and_then(|t| t.checked_duration_since(clock))
                .map_or(0.0, |d| d.as_secs_f64());
            start + f.duration_s
        })
        .fold(0.0, f64::max)
}

/// The poster's source: the followed file when it is finalized, else the
/// first finalized file in tab order (None: nothing finalized).
pub fn pick_poster(v: &[VideoFile], last: Option<usize>) -> Option<usize> {
    if let Some(i) = last.filter(|&i| v.get(i).is_some_and(|f| f.finalized)) {
        return Some(i);
    }
    v.iter().position(|f| f.finalized)
}

/// The marker's result text. The note itself is never echoed (tokens, D10-2).
pub fn marker_text(m: &MarkerIn, at_s: f64) -> String {
    match m {
        MarkerIn::None => String::new(),
        MarkerIn::Note(_) => format!(". Marker at {at_s:.1} s"),
        MarkerIn::Bad(why) => format!(". Marker not placed: {why}"),
    }
}

/// The poster's result text; a failed poster never changes ok/err.
pub fn poster_text(r: &Result<PathBuf, String>) -> String {
    match r {
        Ok(p) => format!(". Poster: {}", p.display()),
        Err(why) => format!(". Poster: not made ({why})"),
    }
}

fn frames_s(frames: u64, fps: u32) -> f64 {
    if fps == 0 {
        return 0.0;
    }
    frames as f64 / fps as f64
}

/// Follow mode: the one merged file, its first frame from the shared sink.
pub fn videos_one(m: &FollowResult, er: &EncoderReport, fps: u32) -> Vec<VideoFile> {
    vec![VideoFile {
        path: er.path.clone(),
        t0_at: m.t0_at,
        duration_s: frames_s(er.frames_written, fps),
        finalized: er.ok(),
    }]
}

/// Per-tab mode: one entry per file, in tab order. A file whose encoder could
/// not be taken back has no length and is not finalized.
pub fn videos_per_tab(r: &PerTabResult, fps: u32) -> Vec<VideoFile> {
    r.files
        .iter()
        .map(|f| {
            let (duration_s, finalized) = match &f.enc {
                Ok(e) => (frames_s(e.frames_written, fps), e.ok()),
                Err(_) => (0.0, false),
            };
            VideoFile {
                path: f.path.clone(),
                t0_at: f.t0_at,
                duration_s,
                finalized,
            }
        })
        .collect()
}

/// The timeline's result text (D10-3): the path and the kept action count, or
/// why it was not written. Never changes the stop's ok/err.
pub fn timeline_text(r: &Result<(PathBuf, usize, u64), String>) -> String {
    match r {
        Ok((p, n, 0)) => format!(". Timeline: {} ({n} {})", p.display(), actions_word(*n)),
        Ok((p, n, k)) => format!(
            ". Timeline: {} ({n} {}, {k} not kept)",
            p.display(),
            actions_word(*n)
        ),
        Err(why) => format!(". Timeline: not written ({why})"),
    }
}

/// "1 action", "0 actions", "2 actions": the stop text is read by agents.
fn actions_word(n: usize) -> &'static str {
    if n == 1 {
        "action"
    } else {
        "actions"
    }
}

/// The timeline sidecar (OUT-07, D10-3/5): the action entries and the marker on
/// the recording clock, written beside the first video (the -tab1- file in
/// per-tab mode). `files[]` gives each video's start on that clock.
fn write_timeline(
    s: &Stopped,
    clock: Instant,
    span: f64,
    entries: &[Entry],
    dropped: u64,
    marker: &MarkerIn,
    marker_s: f64,
) -> String {
    let Some(primary) = s.videos.first() else {
        return timeline_text(&Err("no video file".to_string()));
    };
    let mut placed = place(entries, clock, span);
    if let MarkerIn::Note(note) = marker {
        placed.push(Placed::Marker {
            at_s: marker_s,
            note: note.clone(),
        });
    }
    let actions = placed
        .iter()
        .filter(|p| matches!(p, Placed::Action { .. }))
        .count();
    let files: Vec<FileClock> = s
        .videos
        .iter()
        .map(|v| FileClock {
            file: v.path.display().to_string(),
            start_s: v.t0_at.map(|t| offset_s(t, clock, span)),
            duration_s: v.duration_s,
        })
        .collect();
    let clock_text = if s.videos.iter().any(|v| v.t0_at.is_some()) {
        CLOCK_FIRST_FRAME
    } else {
        CLOCK_START
    };
    let json = to_json(
        &primary.path.display().to_string(),
        span,
        &files,
        &placed,
        dropped,
        clock_text,
    );
    let bytes = match serde_json::to_vec_pretty(&json) {
        Ok(b) => b,
        Err(e) => return timeline_text(&Err(format!("cannot encode: {e}"))),
    };
    match write_sidecar(&primary.path, TIMELINE_SUFFIX, &bytes) {
        Ok(p) => timeline_text(&Ok((p, actions, dropped))),
        Err(why) => timeline_text(&Err(why)),
    }
}

/// The poster (one bounded ffmpeg run on the picked file), the timeline
/// sidecar and the marker offset, as result text, in that order. Runs after the
/// encoders are finished, so the files are final. Never fails: every problem is
/// a `not made` / `not written` / `not placed` note.
pub async fn finish_extras(
    s: &Stopped,
    ff: &ResolvedFfmpeg,
    marker: &MarkerIn,
    entries: Vec<Entry>,
    dropped: u64,
) -> String {
    let poster = match pick_poster(&s.videos, s.poster_from) {
        Some(i) => make_poster(ff, &s.videos[i].path).await,
        None => Err("no finalized file".to_string()),
    };
    let clock = clock(s);
    let span = span_s(&s.videos, clock);
    let at_s = offset_s(s.stop_at, clock, span);
    let timeline = write_timeline(s, clock, span, &entries, dropped, marker, at_s);
    format!(
        "{}{}{}",
        poster_text(&poster),
        timeline,
        marker_text(marker, at_s)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn vf(t0: Option<Instant>, finalized: bool) -> VideoFile {
        VideoFile {
            path: PathBuf::from("/d/x.webm"),
            t0_at: t0,
            duration_s: 2.0,
            finalized,
        }
    }

    #[test]
    fn offset_is_saturating_and_clamped() {
        let c = Instant::now();
        assert_eq!(offset_s(c, c + Duration::from_secs(1), 9.0), 0.0);
        assert_eq!(offset_s(c + Duration::from_millis(1234), c, 9.0), 1.23);
        assert_eq!(offset_s(c + Duration::from_secs(5), c, 2.0), 2.0);
    }

    #[test]
    fn clock_is_min_first_frame_or_fallback() {
        let c = Instant::now();
        let fb = c + Duration::from_secs(9);
        let s = |v: Vec<VideoFile>| Stopped {
            videos: v,
            poster_from: None,
            fallback_clock: fb,
            stop_at: c,
        };
        let got = clock(&s(vec![
            vf(Some(c + Duration::from_secs(2)), true),
            vf(Some(c + Duration::from_secs(1)), true),
            vf(None, true),
        ]));
        assert_eq!(got, c + Duration::from_secs(1));
        assert_eq!(clock(&s(vec![vf(None, true)])), fb);
    }

    #[test]
    fn pick_poster_prefers_last_tab_then_first_finalized() {
        let v = vec![vf(None, true), vf(None, false), vf(None, true)];
        assert_eq!(pick_poster(&v, Some(2)), Some(2));
        assert_eq!(pick_poster(&v, Some(1)), Some(0));
        assert_eq!(pick_poster(&v, None), Some(0));
        assert_eq!(pick_poster(&[vf(None, false)], Some(0)), None);
        assert_eq!(pick_poster(&[], None), None);
    }

    #[test]
    fn marker_text_rows() {
        assert_eq!(
            marker_text(&MarkerIn::Note("x".into()), 3.21),
            ". Marker at 3.2 s"
        );
        assert_eq!(
            marker_text(&MarkerIn::Bad("empty note"), 0.0),
            ". Marker not placed: empty note"
        );
        assert_eq!(marker_text(&MarkerIn::None, 1.0), "");
    }

    #[test]
    fn poster_text_rows() {
        assert_eq!(
            poster_text(&Ok(PathBuf::from("/d/a.poster.png"))),
            ". Poster: /d/a.poster.png"
        );
        assert_eq!(
            poster_text(&Err("timed out after 3 s".into())),
            ". Poster: not made (timed out after 3 s)"
        );
    }
}
