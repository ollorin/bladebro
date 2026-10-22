//! Live `HubEnv` over a `Page`: the tab and transport the `state` call runs on.

use super::env::{Attach, HubEnv, Lane};
use crate::page::Page;
use crate::record::{EncoderError, RecordConfig, RecordOverrides, ResolvedFfmpeg};

pub struct PageEnv<'a> {
    pub page: &'a Page,
}

/// Pure lane choice: the real-browser lane wins (never record the user's own
/// browser), then one-shot, then the pipe transport (no port to dial).
pub fn lane_for(real: bool, ephemeral: bool, pipe: bool, base: &str) -> Lane {
    if real {
        Lane::Real
    } else if ephemeral {
        Lane::Ephemeral
    } else if pipe || base == "pipe" {
        Lane::Pipe
    } else {
        Lane::Ws
    }
}

impl HubEnv for PageEnv<'_> {
    fn config(&self, name: &str) -> Result<RecordConfig, String> {
        crate::record::resolve_from_env(&RecordOverrides {
            name: Some(name.to_string()),
            ..Default::default()
        })
    }

    async fn ffmpeg(&self) -> Result<ResolvedFfmpeg, EncoderError> {
        crate::record::resolve_ffmpeg().await
    }

    fn lane(&self) -> Lane {
        lane_for(
            crate::realbrowser::real_lane(),
            super::is_ephemeral(),
            crate::browser::launched_pipe(),
            self.page.base(),
        )
    }

    async fn current_tab(&self) -> Result<String, String> {
        let info = self
            .page
            .cdp_ref()
            .send("Target.getTargetInfo", None)
            .await
            .map_err(|e| super::msg::scrub(&format!("cannot identify the active tab: {e}")))?;
        info["targetInfo"]["targetId"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| "cannot identify the active tab (no targetId)".to_string())
    }

    fn attach(&self) -> Attach {
        Attach::Port(self.page.base().to_string())
    }
}
