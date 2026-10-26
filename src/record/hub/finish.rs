//! Hub stop/finish paths, split from machine.rs to keep it under 500 lines.
//! A child of `machine` (declared there with `#[path]`), so it sees the private
//! `Active` fields and `failed()` exactly as the moved code did.
//!
//! Module-map note: `Hub::stop`, `Hub::stop_marked`, `Hub::finish_orphan` and the
//! shared `finish_active` live here; the public paths (`Hub::stop`, ...) are
//! unchanged. Stop extras (poster, marker, OUT-06) are added only on the agent's
//! stop: `stop_marked` passes `StopExtras`, the exit paths pass None.

use std::time::{Duration, Instant};

use super::super::env::HubEnv;
use super::super::evidence::{finish_extras, videos_one, videos_per_tab, Stopped};
use super::super::follow::{compose_follow, Outcome};
use super::super::follow_report::compose_per_tab;
use super::super::msg::{self, scrub};
use super::{failed, take_encoder, Active, Hub, HubError, StopExtras};
use crate::record::marker::MarkerIn;

/// The marker part of an idle stop's text: there is no video to place it on.
fn idle_marker_text(m: &MarkerIn) -> String {
    match m {
        MarkerIn::None => String::new(),
        MarkerIn::Note(_) => ". Marker not placed: no active recording".into(),
        MarkerIn::Bad(why) => format!(". Marker not placed: {why}"),
    }
}

impl Hub {
    /// Stop the active recording and report it honestly (D7-4/5).
    pub async fn stop(&mut self, env: &impl HubEnv) -> Result<String, HubError> {
        self.stop_marked(env, MarkerIn::None).await
    }

    /// `record-stop` with an optional marker note (D10-2). The marker and the
    /// poster never change the stop's ok/err; a bad marker is reported and the
    /// stop still happens.
    pub async fn stop_marked(
        &mut self,
        env: &impl HubEnv,
        marker: MarkerIn,
    ) -> Result<String, HubError> {
        let at = Instant::now();
        let Some(a) = self.active.take() else {
            let note = idle_marker_text(&marker);
            return match self.orphaned.take() {
                Some((ok, t)) => {
                    let t = scrub(&format!("{}{note}", msg::orphan_text(&t)));
                    if ok {
                        Ok(t)
                    } else {
                        Err(HubError::Failed(t))
                    }
                }
                None => Err(failed(format!("{}{note}", msg::none()))),
            };
        };
        // The agent's current tab, for the follower's catch-up of a switch
        // younger than one poll. A failed lookup only skips the catch-up.
        let hint = env.current_tab().await.ok();
        self.finish_active(a, hint, Some(StopExtras { at, marker }))
            .await
    }

    /// The browser or surface is going away: stop like `stop` and keep the
    /// result for the next call (D7-7). Never fails.
    pub async fn finish_orphan(&mut self) {
        let Some(a) = self.active.take() else {
            return;
        };
        self.orphaned = Some(match self.finish_active(a, None, None).await {
            Ok(t) => (true, t),
            Err(HubError::Failed(t)) | Err(HubError::Usage(t)) => (false, t),
        });
    }

    pub(super) async fn finish_active(
        &self,
        a: Active,
        hint: Option<String>,
        extras: Option<StopExtras>,
    ) -> Result<String, HubError> {
        let Active {
            enc,
            rec,
            fps,
            ffmpeg,
            started,
            log,
            ..
        } = a;
        // Close the action log first: an action that ends after the stop began
        // is not in this recording's timeline, and it never reaches a later one.
        // Exit paths (extras None) drop the entries here, as before.
        let (entries, dropped) = log.take();
        // Follower first: it ends the video, drains every source's acks and
        // releases every sink so the encoder can be taken back.
        let (ok, text) = match rec.stop(hint.clone()).await {
            Outcome::PerTab(mut r) => {
                let last = r.files.iter().flat_map(|f| &f.segments).max_by(|a, b| {
                    a.at_s
                        .partial_cmp(&b.at_s)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let last = last.map(|s| s.tab.clone());
                r.notes
                    .extend(msg::agent_vs_video(hint.as_deref(), last.as_deref()));
                let (ok, mut text) = compose_per_tab(&r, fps);
                if let Some(x) = &extras {
                    let last_idx = last
                        .as_deref()
                        .and_then(|t| r.files.iter().position(|f| f.tab == t));
                    let stopped = Stopped {
                        videos: videos_per_tab(&r, fps),
                        poster_from: last_idx,
                        fallback_clock: started,
                        stop_at: x.at,
                    };
                    text.push_str(
                        &finish_extras(&stopped, &ffmpeg, &x.marker, entries, dropped).await,
                    );
                }
                (ok, text)
            }
            Outcome::One(mut fr) => {
                let last = fr.segments.last().map(|s| s.tab.clone());
                fr.notes
                    .extend(msg::agent_vs_video(hint.as_deref(), last.as_deref()));
                let Some(enc) = enc else {
                    return Err(failed(msg::follower_stuck()));
                };
                let enc = match take_encoder(enc, 20, Duration::from_millis(50)).await {
                    Ok(e) => e,
                    Err(m) => return Err(failed(m)),
                };
                let er = enc.finish().await;
                let (ok, mut text) = compose_follow(&fr, &er, fps);
                if let Some(x) = &extras {
                    let stopped = Stopped {
                        videos: videos_one(&fr, &er, fps),
                        poster_from: Some(0),
                        fallback_clock: started,
                        stop_at: x.at,
                    };
                    text.push_str(
                        &finish_extras(&stopped, &ffmpeg, &x.marker, entries, dropped).await,
                    );
                }
                (ok, text)
            }
        };
        // Agent stops (extras Some) are scrubbed: the marker reason is the one
        // caller-supplied text here. Exit paths (extras None) keep their bytes.
        let text = if extras.is_some() { scrub(&text) } else { text };
        if ok {
            Ok(text)
        } else {
            Err(HubError::Failed(text))
        }
    }
}
