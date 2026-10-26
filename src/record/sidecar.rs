//! Sidecar files next to a finished recording (poster PNG, later the
//! timeline JSON): named after the video stem, created with the same
//! `create_unique` rules as the video (O_EXCL, 0600, single component), so a
//! planted symlink is never followed and nothing lands outside the dir.
//!
//! Module map:
//! - `sidecar_name`: `<video stem><suffix>`, None for a pathless video.
//! - `create_sidecar`: create the next free `<stem>[.n]<suffix>` beside the video.
//! - `write_sidecar`: create and write bytes; the file is removed on a write error.
//!
//! Fork-only module (SYNC-01).

use std::io::Write;
use std::path::{Path, PathBuf};

use super::dir::create_unique;

/// `<video stem><suffix>`, or None when the video path has no UTF-8 file stem.
pub fn sidecar_name(video: &Path, suffix: &str) -> Option<String> {
    let stem = video.file_stem()?.to_str()?;
    if stem.is_empty() {
        return None;
    }
    Some(format!("{stem}{suffix}"))
}

/// Create the sidecar for `video` in the video's own directory. The first
/// free name wins: `<stem><suffix>`, then `<stem>.1<suffix>`, and so on.
pub fn create_sidecar(video: &Path, suffix: &str) -> Result<(std::fs::File, PathBuf), String> {
    let stem = video
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("video {} has no usable file name", video.display()))?;
    let dir = video.parent().unwrap_or_else(|| Path::new(""));
    create_unique(dir, &mut |n| {
        if n == 0 {
            format!("{stem}{suffix}")
        } else {
            format!("{stem}.{n}{suffix}")
        }
    })
}

/// Create the sidecar and write `bytes` into it. On a write or flush error the
/// partial file is removed so no half-written sidecar is left behind.
pub fn write_sidecar(video: &Path, suffix: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let (mut file, path) = create_sidecar(video, suffix)?;
    let written = file.write_all(bytes).and_then(|()| file.flush());
    if let Err(e) = written {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(format!("cannot write {}: {e}", path.display()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::encoder::test_util::Scratch;

    #[test]
    fn name_replaces_the_video_extension() {
        let webm = Path::new("/d/x-20261026T000000Z-abc123.webm");
        assert_eq!(
            sidecar_name(webm, ".poster.png").as_deref(),
            Some("x-20261026T000000Z-abc123.poster.png")
        );
        let mp4 = Path::new("/d/x-20261026T000000Z-abc123.mp4");
        assert_eq!(
            sidecar_name(mp4, ".poster.png").as_deref(),
            Some("x-20261026T000000Z-abc123.poster.png")
        );
    }

    #[test]
    fn name_of_pathless_video_is_none() {
        assert_eq!(sidecar_name(Path::new(""), ".poster.png"), None);
    }

    #[cfg(unix)]
    #[test]
    fn create_is_private_and_next_to_the_video() {
        use std::os::unix::fs::PermissionsExt;
        let s = Scratch::new();
        let video = s.0.join("clip.webm");
        let (_f, path) = create_sidecar(&video, ".poster.png").expect("create");
        assert_eq!(path.parent(), video.parent());
        assert_eq!(path.file_name().unwrap(), "clip.poster.png");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn second_create_gets_a_numbered_name() {
        let s = Scratch::new();
        let video = s.0.join("clip.webm");
        let (_a, first) = create_sidecar(&video, ".poster.png").expect("first");
        let (_b, second) = create_sidecar(&video, ".poster.png").expect("second");
        assert_eq!(first.file_name().unwrap(), "clip.poster.png");
        assert_eq!(second.file_name().unwrap(), "clip.1.poster.png");
    }

    #[cfg(unix)]
    #[test]
    fn planted_symlink_is_not_followed() {
        let s = Scratch::new();
        let video = s.0.join("clip.webm");
        let target = s.0.join("victim.txt");
        std::fs::write(&target, b"keep me").unwrap();
        std::os::unix::fs::symlink(&target, s.0.join("clip.poster.png")).unwrap();
        let (_f, path) = create_sidecar(&video, ".poster.png").expect("create");
        assert_eq!(path.file_name().unwrap(), "clip.1.poster.png");
        assert_eq!(std::fs::read(&target).unwrap(), b"keep me");
    }

    #[cfg(unix)]
    #[test]
    fn write_sidecar_writes_bytes() {
        use std::os::unix::fs::PermissionsExt;
        let s = Scratch::new();
        let video = s.0.join("clip.webm");
        let path = write_sidecar(&video, ".timeline.json", b"{\"version\":1}").expect("write");
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"version\":1}");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
