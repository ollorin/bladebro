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
    /// Read-only: which tab opened which (`openerId`); sent only on the
    /// recorder's own page connection when a new tab appears (CAPT-06).
    TargetGetTargets,
}

impl RecMethod {
    pub const ALL: [RecMethod; 5] = [
        RecMethod::PageEnable,
        RecMethod::StartScreencast,
        RecMethod::ScreencastFrameAck,
        RecMethod::StopScreencast,
        RecMethod::TargetGetTargets,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RecMethod::PageEnable => "Page.enable",
            RecMethod::StartScreencast => "Page.startScreencast",
            RecMethod::ScreencastFrameAck => "Page.screencastFrameAck",
            RecMethod::StopScreencast => "Page.stopScreencast",
            RecMethod::TargetGetTargets => "Target.getTargets",
        }
    }

    fn idx(self) -> usize {
        self as usize
    }
}

/// The recorder's own connection plus a per-method send counter.
pub(super) struct Wire {
    session: CdpSession,
    counts: Mutex<[u64; 5]>,
}

impl Wire {
    pub fn new(session: CdpSession) -> Self {
        Self {
            session,
            counts: Mutex::new([0; 5]),
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
        let c = self.counts.lock().map(|c| *c).unwrap_or([0; 5]);
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
                "Page.stopScreencast",
                "Target.getTargets"
            ]
        );
        for (i, m) in RecMethod::ALL.iter().enumerate() {
            assert_eq!(m.idx(), i);
        }
    }

    /// SAFE-01: no page-visible or input-driving method exists anywhere in
    /// the recorder (string literals of non-test code).
    #[test]
    fn no_forbidden_method_in_src_record() {
        let forbidden = [
            "Runtime.",
            "Emulation.",
            "Input.",
            "Page.bringToFront",
            "Target.activateTarget",
            "Target.setDiscoverTargets",
        ];
        let mut stack =
            vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/record")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).expect("dir").flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.ends_with(".rs") || name.contains("test") || name == "fake_cdp.rs" {
                    continue;
                }
                let text = std::fs::read_to_string(&p).expect("read");
                // Drop inline `#[cfg(test)] mod tests` (this file's own list).
                let body = text.split("#[cfg(test)]").next().unwrap_or("");
                for line in body.lines().filter(|l| !l.trim_start().starts_with("//")) {
                    for f in forbidden {
                        assert!(
                            !line.contains(&format!("\"{f}")),
                            "{} uses forbidden method {f}: {line}",
                            p.display()
                        );
                    }
                }
            }
        }
    }
}
