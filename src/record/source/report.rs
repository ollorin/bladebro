//! Source counters, outcomes, errors and the honest verdict strings.

use std::fmt;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceStats {
    pub frames_received: u64,
    pub acks_sent: u64,
    pub acks_ok: u64,
    pub ack_errors: u64,
    pub acked_with_cached_id: u64,
    pub unacked: u64,
    pub forwarded: u64,
    pub queue_dropped: u64,
    pub capped: u64,
    pub lost: u64,
    pub bad_frames: u64,
    pub ts_fallback: u64,
    /// ms since start.
    pub first_frame_ms: Option<u64>,
    pub last_frame_ms: Option<u64>,
    pub longest_gap_ms: u64,
    pub methods_sent: Vec<(&'static str, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceOutcome {
    Captured,
    ConnectionLost {
        after_ms: u64,
    },
    /// Zero frames at stop (CAPT-05): a blank video, never a success.
    NoFrames {
        waited_ms: u64,
        hidden: bool,
    },
}

#[derive(Debug, Clone)]
pub struct SourceReport {
    pub outcome: SourceOutcome,
    pub stats: SourceStats,
    pub target_id: String,
    pub elapsed_ms: u64,
    /// Age of the newest frame at stop (None: no frame ever arrived).
    pub last_frame_age_ms: Option<u64>,
    /// Set when the tail was quiet for at least the no-frames period.
    pub quiet_tail_ms: Option<u64>,
    /// Chrome's latest `screencastVisibilityChanged` said hidden.
    pub hidden: bool,
}

impl SourceReport {
    pub fn ok(&self) -> bool {
        self.outcome == SourceOutcome::Captured
    }

    pub fn verdict(&self) -> String {
        let s = &self.stats;
        match &self.outcome {
            SourceOutcome::Captured => {
                let mut v = format!(
                    "capture ok: {} frames received, {} acked, {} to the encoder, {} skipped by the fps cap, {} dropped at the queue, {} lost",
                    s.frames_received, s.acks_sent, s.forwarded, s.capped, s.queue_dropped, s.lost
                );
                if s.bad_frames > 0 {
                    v.push_str(&format!(", {} unreadable", s.bad_frames));
                }
                if let Some(age) = self.last_frame_age_ms {
                    v.push_str(&format!("; last frame {:.1} s before stop", secs(age)));
                }
                if let Some(q) = self.quiet_tail_ms {
                    v.push_str(&format!(
                        " (no new frames for {:.1} s: normal for a static page; a background, minimized or stalled tab looks the same, so the video holds the last frame)",
                        secs(q)
                    ));
                }
                v
            }
            SourceOutcome::NoFrames { waited_ms, hidden } => format!(
                "capture failed: no frames for {:.1} s since recording start, so the video would be blank (not a success). The tab may be in the background, minimized or covered{}, or the screencast stalled; bring the tab to the front and record again.",
                secs(*waited_ms),
                if *hidden { ", and Chrome reported it hidden" } else { "" }
            ),
            SourceOutcome::ConnectionLost { after_ms } => format!(
                "capture interrupted: the recorder's CDP connection closed after {} s (tab closed or browser gone); {} frames received before that",
                after_ms / 1000,
                s.frames_received
            ),
        }
    }
}

fn secs(ms: u64) -> f64 {
    ms as f64 / 1000.0
}

/// Why a source could not start. Every message ends "Browsing is unaffected."
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// Pipe transport has no debugging port for a second connection (D6-2):
    /// no flat-session hack, an honest limitation instead.
    PipeTransport,
    TargetNotFound {
        target_id: String,
        base: String,
    },
    Connect {
        url: String,
        error: String,
    },
    Cdp {
        method: &'static str,
        error: String,
    },
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::PipeTransport => f.write_str(
                "recording unavailable: BLADE_TRANSPORT=pipe has no debugging port for the recorder's own connection. Unset BLADE_TRANSPORT (the default WebSocket transport records), then run bladebro stop so the next launch uses it. Browsing is unaffected.",
            ),
            SourceError::TargetNotFound { target_id, base } => write!(
                f,
                "recording not started: tab {target_id} is not in the browser's target list at {base}; it may have closed. Browsing is unaffected."
            ),
            SourceError::Connect { url, error } => write!(
                f,
                "recording not started: cannot open the recorder's CDP connection to {url}: {error}. Browsing is unaffected."
            ),
            SourceError::Cdp { method, error } => write!(
                f,
                "recording not started: {method} failed: {error}. Browsing is unaffected."
            ),
        }
    }
}

impl std::error::Error for SourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(outcome: SourceOutcome, stats: SourceStats) -> SourceReport {
        SourceReport {
            outcome,
            stats,
            target_id: "T".into(),
            elapsed_ms: 0,
            last_frame_age_ms: None,
            quiet_tail_ms: None,
            hidden: false,
        }
    }

    #[test]
    fn no_frames_is_never_ok_and_names_the_wait() {
        let r = report(
            SourceOutcome::NoFrames {
                waited_ms: 300,
                hidden: false,
            },
            SourceStats::default(),
        );
        assert!(!r.ok());
        let v = r.verdict();
        assert!(v.starts_with("capture failed: no frames for 0.3 s"), "{v}");
        assert!(!v.contains("hidden"));
        let h = report(
            SourceOutcome::NoFrames {
                waited_ms: 6000,
                hidden: true,
            },
            SourceStats::default(),
        );
        assert!(h.verdict().contains("Chrome reported it hidden"));
    }

    #[test]
    fn captured_verdict_quiet_note_only_when_quiet() {
        let mut r = report(SourceOutcome::Captured, SourceStats::default());
        r.last_frame_age_ms = Some(7000);
        r.quiet_tail_ms = Some(7000);
        let v = r.verdict();
        assert!(r.ok());
        assert!(v.contains("last frame 7.0 s before stop"), "{v}");
        assert!(v.contains("no new frames for 7.0 s"), "{v}");
        r.last_frame_age_ms = Some(200);
        r.quiet_tail_ms = None;
        let v = r.verdict();
        assert!(v.contains("last frame 0.2 s"), "{v}");
        assert!(!v.contains("no new frames"), "{v}");
    }

    #[test]
    fn connection_lost_with_zero_frames_stays_interrupted() {
        let r = report(
            SourceOutcome::ConnectionLost { after_ms: 1000 },
            SourceStats::default(),
        );
        assert!(!r.ok());
        assert!(r.verdict().starts_with("capture interrupted"));
    }

    #[test]
    fn captured_verdict_prints_zeros() {
        let r = report(SourceOutcome::Captured, SourceStats::default());
        assert!(r.ok());
        assert_eq!(
            r.verdict(),
            "capture ok: 0 frames received, 0 acked, 0 to the encoder, 0 skipped by the fps cap, 0 dropped at the queue, 0 lost"
        );
    }

    #[test]
    fn captured_verdict_unreadable_only_when_positive() {
        let s = SourceStats {
            frames_received: 5,
            acks_sent: 5,
            forwarded: 3,
            capped: 1,
            queue_dropped: 0,
            bad_frames: 1,
            ..Default::default()
        };
        let v = report(SourceOutcome::Captured, s).verdict();
        assert!(v.ends_with(", 1 unreadable"), "{v}");
        assert!(v.contains("5 frames received, 5 acked, 3 to the encoder"));
    }

    #[test]
    fn connection_lost_verdict() {
        let s = SourceStats {
            frames_received: 42,
            ..Default::default()
        };
        let r = report(SourceOutcome::ConnectionLost { after_ms: 12_999 }, s);
        assert!(!r.ok());
        let v = r.verdict();
        assert_eq!(
            v,
            "capture interrupted: the recorder's CDP connection closed after 12 s (tab closed or browser gone); 42 frames received before that"
        );
        assert!(!v.starts_with("capture ok"));
    }

    #[test]
    fn error_verdicts() {
        let pipe = SourceError::PipeTransport.to_string();
        assert!(pipe.contains("BLADE_TRANSPORT") && pipe.contains("bladebro stop"));
        let rows = [
            (
                SourceError::TargetNotFound {
                    target_id: "A1".into(),
                    base: "127.0.0.1:9222".into(),
                },
                "recording not started: tab A1 is not in the browser's target list at 127.0.0.1:9222; it may have closed. Browsing is unaffected.",
            ),
            (
                SourceError::Connect {
                    url: "ws://x".into(),
                    error: "refused".into(),
                },
                "recording not started: cannot open the recorder's CDP connection to ws://x: refused. Browsing is unaffected.",
            ),
            (
                SourceError::Cdp {
                    method: "Page.enable",
                    error: "boom".into(),
                },
                "recording not started: Page.enable failed: boom. Browsing is unaffected.",
            ),
        ];
        for (e, want) in rows {
            assert_eq!(e.to_string(), want);
        }
        assert!(pipe.ends_with("Browsing is unaffected."));
        assert!(!pipe.starts_with("capture ok"));
    }
}
