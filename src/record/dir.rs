//! Private, canonical recordings directory and `create_new` file creation.
//!
//! Nothing in bladebro rotates, prunes or deletes files in the recordings
//! dir. It is deliberately not under `artifacts/` (src/artifacts.rs
//! `rotate_artifacts` keeps 300 files). The harness prunes. (OUT-04, D-02.)

use super::config::RecordConfig;
use super::names::{file_name, sanitize_stem, short_id};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Attempts at finding a free file name before giving up.
pub const MAX_CREATE_ATTEMPTS: u32 = 8;

/// Validate/create the recordings dir and return its canonical path.
///
/// Mirrors session_profile.rs `prepare_custom_profile_dir`: refuse symlinks
/// and foreign owners, mkdir (not create_dir_all) for the last component so a
/// planted object fails, then re-check. D3-10: on Unix the dir becomes 0700
/// when created here or when it pre-existed empty; a non-empty dir that is
/// group/other-accessible is refused and never chmodded (BLADE_RECORD may
/// point at a user's shared folder). D3-11: on Windows there are no mode bits;
/// the file inherits the directory ACL (see 03-03 WINDOWS-ACL-DECISION.md).
pub fn prepare_dir(dir: &Path) -> Result<PathBuf, String> {
    let d = dir.display();
    #[cfg(unix)]
    crate::platform::validate_dir_ancestors(dir).map_err(|e| format!("recordings dir {d}: {e}"))?;

    let mut created = false;
    match std::fs::symlink_metadata(dir) {
        Ok(m) => check_kind(&m, dir)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = dir.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create recordings dir {d}: {e}"))?;
            }
            std::fs::create_dir(dir)
                .map_err(|e| format!("cannot create recordings dir {d}: {e}"))?;
            created = true;
            let m = std::fs::symlink_metadata(dir)
                .map_err(|e| format!("cannot stat recordings dir {d}: {e}"))?;
            check_kind(&m, dir)?;
        }
        Err(e) => return Err(format!("cannot stat recordings dir {d}: {e}")),
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let m = std::fs::symlink_metadata(dir)
            .map_err(|e| format!("cannot stat recordings dir {d}: {e}"))?;
        // SAFETY: geteuid has no preconditions and cannot fail.
        if m.uid() != unsafe { libc::geteuid() } {
            return Err(format!(
                "recordings dir {d} is owned by another user - refusing it"
            ));
        }
        let empty = std::fs::read_dir(dir)
            .map_err(|e| format!("cannot read recordings dir {d}: {e}"))?
            .next()
            .is_none();
        let mode = m.permissions().mode() & 0o777;
        if created || empty {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("cannot set recordings dir {d} to 0700: {e}"))?;
        } else if mode & 0o077 != 0 {
            return Err(format!(
                "recordings dir {d} is group/other-accessible (mode {mode:o}) and not empty; \
                 run chmod 700 {d} or point BLADE_RECORD at a dedicated directory"
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = created;

    std::fs::canonicalize(dir).map_err(|e| format!("cannot resolve recordings dir {d}: {e}"))
}

fn check_kind(m: &std::fs::Metadata, dir: &Path) -> Result<(), String> {
    let d = dir.display();
    if m.file_type().is_symlink() {
        return Err(format!(
            "recordings dir {d} is a symlink - refusing it; point BLADE_RECORD at the real directory"
        ));
    }
    if !m.is_dir() {
        return Err(format!("recordings dir {d} is not a directory"));
    }
    Ok(())
}

/// Create a new file in `dir` using names from `next_name(attempt)`.
/// `create_new` is O_EXCL: it never follows a planted symlink and never
/// overwrites (Pitfall 12, D3-9). A name that is not a single normal path
/// component is an error, so nothing can land outside `dir`.
pub fn create_unique(
    dir: &Path,
    next_name: &mut dyn FnMut(u32) -> String,
) -> Result<(std::fs::File, PathBuf), String> {
    for attempt in 0..MAX_CREATE_ATTEMPTS {
        let name = next_name(attempt);
        let mut comps = Path::new(&name).components();
        match (comps.next(), comps.next()) {
            (Some(Component::Normal(_)), None) => {}
            _ => {
                return Err(format!(
                    "recording file name {name:?} is not a single component"
                ))
            }
        }
        let path = dir.join(&name);
        if path.parent() != Some(dir) {
            return Err(format!(
                "recording file {} escapes {}",
                path.display(),
                dir.display()
            ));
        }
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        match opts.open(&path) {
            Ok(f) => return Ok((f, path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "cannot create recording file {}: {e}",
                    path.display()
                ))
            }
        }
    }
    Err(format!(
        "could not create a unique recording file in {} after {MAX_CREATE_ATTEMPTS} attempts",
        dir.display()
    ))
}

static SEQ: AtomicU32 = AtomicU32::new(0);

/// Prepare the dir and create `<stem>-<stamp>-<id>.<ext>` in it.
pub fn create_recording_file(cfg: &RecordConfig) -> Result<(std::fs::File, PathBuf), String> {
    let dir = prepare_dir(&cfg.dir)?;
    let stem = sanitize_stem(cfg.name.as_deref().unwrap_or(""));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("system clock is before 1970: {e}"))?;
    let (secs, nanos, pid) = (now.as_secs(), now.subsec_nanos(), std::process::id());
    create_unique(&dir, &mut |_| {
        let id = short_id(nanos, pid, SEQ.fetch_add(1, Ordering::Relaxed));
        file_name(&stem, cfg.format, secs, &id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::config::{resolve, RecordOverrides};
    use crate::record::values::Format;

    static NEXT: AtomicU32 = AtomicU32::new(0);

    fn abs(p: &str) -> String {
        if cfg!(windows) {
            format!("C:{}", p.replace('/', "\\"))
        } else {
            p.to_string()
        }
    }

    fn scratch() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "bladebro-rec-dir-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn cfg(dir: &Path, name: Option<&str>, format: Format) -> RecordConfig {
        let bd = PathBuf::from(abs("/bh"));
        let mut c = resolve(
            &RecordOverrides::default(),
            &|_: &str| -> Option<String> { None },
            None,
            &bd,
        )
        .unwrap();
        c.dir = dir.to_path_buf();
        c.name = name.map(String::from);
        c.format = format;
        c
    }

    #[test]
    fn prepare_creates_nested_and_canonicalizes() {
        let root = scratch();
        let p = root.join("a").join("b");
        let got = prepare_dir(&p).unwrap();
        assert_eq!(got, std::fs::canonicalize(&p).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prepare_rejects_regular_file() {
        let root = scratch();
        let f = root.join("f");
        std::fs::write(&f, "x").unwrap();
        let e = prepare_dir(&f).unwrap_err();
        assert!(e.contains("not a directory"), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_unique_retries_without_overwrite() {
        let root = scratch();
        std::fs::write(root.join("a.webm"), "keep").unwrap();
        let (_f, p) = create_unique(&root, &mut |i| {
            if i == 0 { "a.webm" } else { "b.webm" }.to_string()
        })
        .unwrap();
        assert_eq!(p, root.join("b.webm"));
        assert_eq!(
            std::fs::read_to_string(root.join("a.webm")).unwrap(),
            "keep"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_unique_exhausts_after_8() {
        let root = scratch();
        std::fs::write(root.join("a.webm"), "keep").unwrap();
        let calls = std::cell::Cell::new(0u32);
        let e = create_unique(&root, &mut |_| {
            calls.set(calls.get() + 1);
            "a.webm".to_string()
        })
        .unwrap_err();
        assert!(e.contains("8 attempts"), "{e}");
        assert!(e.contains(&root.display().to_string()), "{e}");
        assert_eq!(calls.get(), 8);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_unique_rejects_non_single_component() {
        let root = scratch();
        let inner = root.join("inner");
        std::fs::create_dir_all(inner.join("sub")).unwrap();
        for bad in ["../escape.webm", "sub/x.webm"] {
            assert!(
                create_unique(&inner, &mut |_| bad.to_string()).is_err(),
                "{bad}"
            );
        }
        assert!(!root.join("escape.webm").exists());
        assert!(!inner.join("sub").join("x.webm").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_recording_file_is_sanitized_and_unique() {
        let root = scratch();
        let c = cfg(&root.join("rec"), Some("../../evil"), Format::Mp4);
        let (_f1, p1) = create_recording_file(&c).unwrap();
        let (_f2, p2) = create_recording_file(&c).unwrap();
        assert_ne!(p1, p2);
        let canon = std::fs::canonicalize(root.join("rec")).unwrap();
        assert_eq!(p1.parent(), Some(canon.as_path()));
        let n = p1.file_name().unwrap().to_str().unwrap();
        assert!(n.starts_with("evil-") && n.ends_with(".mp4"), "{n}");
        assert!(p1.exists() && p2.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn default_dir_is_recordings_never_artifacts() {
        let c = cfg(Path::new("x"), None, Format::WebM);
        let bd = PathBuf::from(abs("/bh"));
        let d = resolve(
            &RecordOverrides::default(),
            &|_: &str| -> Option<String> { None },
            None,
            &bd,
        )
        .unwrap()
        .dir;
        assert!(
            d.components().all(|c| c.as_os_str() != "artifacts"),
            "{d:?}"
        );
        assert_eq!(d.components().last().unwrap().as_os_str(), "recordings");
        drop(c);
    }

    #[cfg(unix)]
    mod unix {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn mode(p: &Path) -> u32 {
            std::fs::metadata(p).unwrap().permissions().mode() & 0o777
        }
        fn chmod(p: &Path, m: u32) {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap();
        }

        #[test]
        fn new_dir_0700_and_file_0600() {
            let root = scratch();
            let c = cfg(&root.join("rec"), None, Format::WebM);
            let (_f, p) = create_recording_file(&c).unwrap();
            assert_eq!(mode(&root.join("rec")), 0o700);
            assert_eq!(mode(&p), 0o600);
            let _ = std::fs::remove_dir_all(&root);
        }

        #[test]
        fn preexisting_empty_dir_tightened() {
            let root = scratch();
            let d = root.join("rec");
            std::fs::create_dir(&d).unwrap();
            chmod(&d, 0o755);
            prepare_dir(&d).unwrap();
            assert_eq!(mode(&d), 0o700);
            let _ = std::fs::remove_dir_all(&root);
        }

        #[test]
        fn preexisting_nonempty_open_dir_refused() {
            let root = scratch();
            let d = root.join("rec");
            std::fs::create_dir(&d).unwrap();
            std::fs::write(d.join("f"), "x").unwrap();
            chmod(&d, 0o755);
            let e = prepare_dir(&d).unwrap_err();
            assert!(
                e.contains("chmod 700") && e.contains(&d.display().to_string()),
                "{e}"
            );
            assert_eq!(mode(&d), 0o755);
            let _ = std::fs::remove_dir_all(&root);
        }

        #[test]
        fn preexisting_nonempty_private_dir_ok() {
            let root = scratch();
            let d = root.join("rec");
            std::fs::create_dir(&d).unwrap();
            std::fs::write(d.join("f"), "x").unwrap();
            chmod(&d, 0o700);
            prepare_dir(&d).unwrap();
            assert_eq!(mode(&d), 0o700);
            let _ = std::fs::remove_dir_all(&root);
        }

        #[test]
        fn symlinked_dir_refused() {
            let root = scratch();
            let real = root.join("real");
            std::fs::create_dir(&real).unwrap();
            chmod(&real, 0o755);
            let link = root.join("link");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let e = prepare_dir(&link).unwrap_err();
            assert!(e.contains("symlink"), "{e}");
            assert_eq!(mode(&real), 0o755);
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}
