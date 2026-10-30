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

/// The per-call layer the `state` tool can supply: the recording name only.
/// `tabs` stays None on purpose (no schema parameter; BLADE_RECORD_TABS and
/// the config files select the mode).
fn overrides_for(name: &str) -> RecordOverrides {
    RecordOverrides {
        name: Some(name.to_string()),
        ..Default::default()
    }
}

impl HubEnv for PageEnv<'_> {
    fn config(&self, name: &str) -> Result<RecordConfig, String> {
        crate::record::resolve_from_env(&overrides_for(name))
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

#[cfg(test)]
mod tests {
    #[test]
    fn the_state_tool_supplies_only_a_name() {
        let o = super::overrides_for("n");
        assert_eq!(o.name.as_deref(), Some("n"));
        assert!(o.tabs.is_none() && o.format.is_none() && o.fps.is_none() && o.size.is_none());
    }
}
