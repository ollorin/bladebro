//! User-level launch config: `<data root>/launch.json` (written by
//! `bladebro headless on|off`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::project::MAX_FILE_BYTES;

pub const USER_LAUNCH_FILE: &str = "launch.json";

/// Strict like record.json: a typo must not silently do nothing.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UserLaunchFile {
    pub headless: Option<bool>,
}

/// Missing file is `Ok(None)`; anything else wrong is an error naming it.
pub fn load_user_launch(path: &Path) -> Result<Option<UserLaunchFile>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read launch config {}: {e}", path.display())),
    };
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!(
            "launch config {} is {} bytes; the limit is 64 KiB",
            path.display(),
            bytes.len()
        ));
    }
    serde_json::from_slice::<UserLaunchFile>(&bytes)
        .map(Some)
        .map_err(|e| {
            format!(
                "invalid launch config {}: {e} (fields: headless)",
                path.display()
            )
        })
}

/// Atomic, 0600 on Unix (via `secure_write_file`). Returns the written path.
pub fn write_user_launch(blade_dir: &Path, headless: bool) -> Result<PathBuf, String> {
    let path = blade_dir.join(USER_LAUNCH_FILE);
    crate::platform::secure_create_dir_all(blade_dir)
        .map_err(|e| format!("cannot create {}: {e}", blade_dir.display()))?;
    let mut text = serde_json::to_string_pretty(&UserLaunchFile {
        headless: Some(headless),
    })
    .map_err(|e| e.to_string())?;
    text.push('\n');
    crate::platform::secure_write_file(&path, text.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "bladebro-user-launch-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&d).expect("scratch");
        d
    }

    #[test]
    fn load_cases() {
        let d = scratch();
        let f = d.join(USER_LAUNCH_FILE);
        assert_eq!(load_user_launch(&f), Ok(None));
        std::fs::write(&f, r#"{"headless":false}"#).unwrap();
        assert_eq!(load_user_launch(&f).unwrap().unwrap().headless, Some(false));
        std::fs::write(&f, r#"{"mode":1}"#).unwrap();
        let e = load_user_launch(&f).unwrap_err();
        assert!(
            e.contains("mode") && e.contains(&f.display().to_string()),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn write_then_load_roundtrip() {
        let d = scratch();
        let path = write_user_launch(&d, true).unwrap();
        assert_eq!(path, d.join(USER_LAUNCH_FILE));
        assert_eq!(
            load_user_launch(&path).unwrap().unwrap().headless,
            Some(true)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(m & 0o777, 0o600);
        }
        write_user_launch(&d, false).unwrap();
        assert_eq!(
            load_user_launch(&path).unwrap().unwrap().headless,
            Some(false)
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
