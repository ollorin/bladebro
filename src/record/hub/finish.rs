//! Hub stop/finish paths, split from machine.rs to keep it under 500 lines.
//! A child of `machine` (declared there with `#[path]`), so it sees the private
//! `Active` fields and `failed()` exactly as the moved code did.
//!
//! Module-map note: `Hub::stop`, `Hub::finish_orphan` and the shared
//! `finish_active` live here; the public paths (`Hub::stop`, ...) are unchanged.

use std::time::Duration;

use super::super::env::HubEnv;
use super::super::follow::{compose_follow, Outcome};
use super::super::follow_report::compose_per_tab;
use super::super::msg::{self, scrub};
use super::{failed, take_encoder, Active, Hub, HubError};

impl Hub {
    /// Stop the active recording and report it honestly (D7-4/5).
    pub async fn stop(&mut self, env: &impl HubEnv) -> Result<String, HubError> {
        let Some(a) = self.active.take() else {
            return match self.orphaned.take() {
                Some((ok, t)) => {
                    let t = scrub(&msg::orphan_text(&t));
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

    pub(super) async fn finish_active(
        &self,
        a: Active,
        hint: Option<String>,
    ) -> Result<String, HubError> {
        let Active { enc, rec, fps, .. } = a;
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
                compose_per_tab(&r, fps)
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
