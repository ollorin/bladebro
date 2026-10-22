//! The recorder's closed set of CDP methods (SAFE-01, D6-3).
//!
//! `RecMethod` is the ONLY way a method reaches the wire from `source/`: there
//! is no `&str` method parameter anywhere, so a stray Runtime/Emulation/Input
//! call cannot be added without extending this enum (and its test).

use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use crate::cdp::CdpSession;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecMethod {
    PageEnable,
    StartScreencast,
    ScreencastFrameAck,
    StopScreencast,
}

impl RecMethod {
    pub const ALL: [RecMethod; 4] = [
        RecMethod::PageEnable,
        RecMethod::StartScreencast,
        RecMethod::ScreencastFrameAck,
        RecMethod::StopScreencast,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RecMethod::PageEnable => "Page.enable",
            RecMethod::StartScreencast => "Page.startScreencast",
            RecMethod::ScreencastFrameAck => "Page.screencastFrameAck",
            RecMethod::StopScreencast => "Page.stopScreencast",
        }
    }

    fn idx(self) -> usize {
        self as usize
    }
}

/// The recorder's own connection plus a per-method send counter.
pub(super) struct Wire {
    session: CdpSession,
    counts: Mutex<[u64; 4]>,
}

impl Wire {
    pub fn new(session: CdpSession) -> Self {
        Self {
            session,
            counts: Mutex::new([0; 4]),
        }
    }

    pub fn session(&self) -> &CdpSession {
        &self.session
    }

    /// The single send path. Counted at send time (a timed-out command was
    /// still sent).
    pub async fn send(
        &self,
        method: RecMethod,
        params: Option<Value>,
        deadline: Duration,
    ) -> Result<Value> {
        if let Ok(mut c) = self.counts.lock() {
            c[method.idx()] += 1;
        }
        self.session
            .send_with_timeout(method.as_str(), params, deadline)
            .await
    }

    /// Non-zero counts in `RecMethod::ALL` order.
    pub fn methods_sent(&self) -> Vec<(&'static str, u64)> {
        let c = self.counts.lock().map(|c| *c).unwrap_or([0; 4]);
        RecMethod::ALL
            .iter()
            .filter(|m| c[m.idx()] > 0)
            .map(|m| (m.as_str(), c[m.idx()]))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_set_names() {
        let names: Vec<_> = RecMethod::ALL.iter().map(|m| m.as_str()).collect();
        assert_eq!(
            names,
            [
                "Page.enable",
                "Page.startScreencast",
                "Page.screencastFrameAck",
                "Page.stopScreencast"
            ]
        );
        for (i, m) in RecMethod::ALL.iter().enumerate() {
            assert_eq!(m.idx(), i);
        }
    }
}
