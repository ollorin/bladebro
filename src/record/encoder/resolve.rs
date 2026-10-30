//! ffmpeg resolution for the encoder, safe to call from the async runtime.
//!
//! `discover_cached` blocks for up to 2 x `PROBE_TIMEOUT` the first time it
//! runs (Phase 4 D4-1); on a daemon runtime thread that would stall a worker,
//! so `resolve_ffmpeg` runs it on tokio's blocking pool. The result is cached
//! once per process: a changed BLADE_FFMPEG needs `bladebro stop`.

use std::path::PathBuf;

use crate::record::ffmpeg::{
    discover_cached, install_hint, Discovery, EncoderSet, FfmpegVersion, Lookup, TargetOs,
};

use super::report::EncoderError;

/// A probed, usable ffmpeg. Format-agnostic: `Encoder::start` checks the
/// encoder for the requested format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFfmpeg {
    pub path: PathBuf,
    pub version: FfmpegVersion,
    pub encoders: EncoderSet,
}

/// Pure mapping from a `Discovery` to a usable ffmpeg or an honest error.
pub fn resolve_from(d: &Discovery, os: TargetOs) -> Result<ResolvedFfmpeg, EncoderError> {
    let found = match &d.lookup {
        Lookup::NotFound => {
            return Err(EncoderError::FfmpegNotFound {
                hint: install_hint(os),
            })
        }
        Lookup::OverrideUnusable { value, reason } => {
            return Err(EncoderError::OverrideUnusable {
                value: value.clone(),
                reason: reason.clone(),
            })
        }
        Lookup::Found(f) => f,
    };
    let path = found.path.display().to_string();
    let unusable = |reason: String| EncoderError::FfmpegUnusable {
        path: path.clone(),
        reason,
    };
    match &d.probe {
        None => Err(unusable("it was not probed".to_string())),
        Some(Err(e)) => Err(unusable(e.clone())),
        Some(Ok(info)) if !info.encoders.listed => {
            Err(unusable("could not read its encoder list".to_string()))
        }
        Some(Ok(info)) => Ok(ResolvedFfmpeg {
            path: found.path.clone(),
            version: info.version.clone(),
            encoders: info.encoders.clone(),
        }),
    }
}

/// Resolve the host's ffmpeg. The first call probes on the blocking pool.
pub async fn resolve_ffmpeg() -> Result<ResolvedFfmpeg, EncoderError> {
    match tokio::task::spawn_blocking(discover_cached).await {
        Ok(d) => resolve_from(d, TargetOs::current()),
        Err(e) => Err(EncoderError::FfmpegUnusable {
            path: "(discovery)".to_string(),
            reason: format!("discovery task failed: {e}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::ffmpeg::{parse_encoders, Found, ProbeInfo, Source};

    const ENCODERS: &str =
        " ------\n V....D libx264              H.264\n V....D libvpx-vp9           VP9\n";

    fn info(text: &str) -> ProbeInfo {
        ProbeInfo {
            version: FfmpegVersion::Release {
                display: "n9.0.2".into(),
                major: 9,
            },
            encoders: parse_encoders(text),
        }
    }

    fn found() -> Lookup {
        Lookup::Found(Found {
            path: PathBuf::from("/opt/ff"),
            source: Source::Path,
        })
    }

    fn disc(lookup: Lookup, probe: Option<Result<ProbeInfo, String>>) -> Discovery {
        Discovery { lookup, probe }
    }

    #[test]
    fn resolve_from_table() {
        let e = resolve_from(&disc(Lookup::NotFound, None), TargetOs::MacOs).unwrap_err();
        assert_eq!(
            e,
            EncoderError::FfmpegNotFound {
                hint: "brew install ffmpeg"
            }
        );
        assert!(e.to_string().contains("brew install ffmpeg"));
        let e = resolve_from(&disc(Lookup::NotFound, None), TargetOs::Windows).unwrap_err();
        assert_eq!(
            e,
            EncoderError::FfmpegNotFound {
                hint: "winget install ffmpeg"
            }
        );
        let o = Lookup::OverrideUnusable {
            value: "/x/ffmpeg".into(),
            reason: "not found".into(),
        };
        assert_eq!(
            resolve_from(&disc(o, None), TargetOs::Linux).unwrap_err(),
            EncoderError::OverrideUnusable {
                value: "/x/ffmpeg".into(),
                reason: "not found".into()
            }
        );
        match resolve_from(&disc(found(), None), TargetOs::Linux).unwrap_err() {
            EncoderError::FfmpegUnusable { reason, .. } => assert!(reason.contains("not probed")),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            resolve_from(
                &disc(found(), Some(Err("timed out after 5s".into()))),
                TargetOs::Linux
            )
            .unwrap_err(),
            EncoderError::FfmpegUnusable {
                path: "/opt/ff".into(),
                reason: "timed out after 5s".into()
            }
        );
        match resolve_from(
            &disc(found(), Some(Ok(info("no table here")))),
            TargetOs::Linux,
        )
        .unwrap_err()
        {
            EncoderError::FfmpegUnusable { reason, .. } => {
                assert!(reason.contains("encoder list"))
            }
            other => panic!("{other:?}"),
        }
        let ok = resolve_from(&disc(found(), Some(Ok(info(ENCODERS)))), TargetOs::Linux).unwrap();
        assert_eq!(ok.path, PathBuf::from("/opt/ff"));
        assert!(ok.encoders.has("libx264") && ok.encoders.has("libvpx-vp9"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resolve_ffmpeg_runs_on_blocking_pool() {
        let r = resolve_ffmpeg().await;
        let pure = resolve_from(discover_cached(), TargetOs::current());
        assert_eq!(r.is_ok(), pure.is_ok());
        if let Ok(ff) = r {
            assert!(ff.path.is_absolute());
        }
    }
}
