//! Project file (`.bladebro.json`): loading, trust rules, `record.dir` vetting.
//!
//! A cloned repository controls this file, so it is untrusted input. Rules and
//! the threat each closes are documented per function.

use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use super::find::{find_project_file, PROJECT_FILE};
use crate::record::FileConfig;

/// A handful of keys; anything bigger is a mistake or an attack on the parser.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;

/// `{ "headless": bool, "record": { ... } }`. Both levels are strict.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub headless: Option<bool>,
    pub record: Option<FileConfig>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectLoaded {
    pub path: PathBuf,
    /// Directory holding the file (relative `record.dir` is joined to it).
    pub root: PathBuf,
    pub file: ProjectFile,
    /// `file.record` with `dir` already vetted (dropped when it escapes).
    pub record: Option<FileConfig>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectOutcome {
    Loaded(Box<ProjectLoaded>),
    /// Untrusted file: skipped, with the reason (printed as a warning).
    Ignored(String),
}

/// Why the file may not be used, or None when trusted. Not followed through
/// symlinks (a planted link must not borrow another file's content), and on
/// Unix it must be ours and not writable by group/other (a file in a shared
/// parent directory must not steer or break our runs). Windows has no
/// owner/mode check here.
fn untrusted_reason(path: &Path) -> Result<Option<String>, String> {
    let md = std::fs::symlink_metadata(path)
        .map_err(|e| format!("cannot read project file {}: {e}", path.display()))?;
    if md.file_type().is_symlink() {
        return Ok(Some("is a symlink".into()));
    }
    if !md.file_type().is_file() {
        return Ok(Some("is not a regular file".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no preconditions and cannot fail.
        let euid = unsafe { libc::geteuid() };
        if md.uid() != euid {
            return Ok(Some(format!(
                "is owned by uid {} (expected {euid})",
                md.uid()
            )));
        }
        if md.mode() & 0o022 != 0 {
            return Ok(Some(
                "is group/other-writable (chmod go-w to trust it)".into(),
            ));
        }
    }
    Ok(None)
}

/// Load and vet one project file. Untrusted -> `Ignored`; oversized, invalid
/// JSON, unknown keys or bad types -> `Err` naming the file.
pub fn load_project(path: &Path, blade_dir: &Path) -> Result<ProjectOutcome, String> {
    if let Some(why) = untrusted_reason(path)? {
        return Ok(ProjectOutcome::Ignored(format!(
            "{}: project file ignored: it {why}",
            path.display()
        )));
    }
    let md = std::fs::metadata(path)
        .map_err(|e| format!("cannot read project file {}: {e}", path.display()))?;
    if md.len() > MAX_FILE_BYTES {
        return Err(format!(
            "project file {} is {} bytes; the limit is 64 KiB",
            path.display(),
            md.len()
        ));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read project file {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!(
            "project file {} is {} bytes; the limit is 64 KiB",
            path.display(),
            bytes.len()
        ));
    }
    let file: ProjectFile = serde_json::from_slice(&bytes).map_err(|e| {
        format!(
            "invalid project file {}: {e} (fields: headless, record)",
            path.display()
        )
    })?;
    let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut warnings = Vec::new();
    let mut record = file.record.clone();
    if let Some(rec) = record.as_mut() {
        if let Some(raw) = rec.dir.take() {
            let canon = |p: &Path| crate::record::dir::resolve_lenient(p);
            match vet_project_dir(&raw, &root, blade_dir, &canon) {
                Ok(d) => rec.dir = Some(d),
                Err(w) => warnings.push(format!("{}: {w}", path.display())),
            }
        }
    }
    Ok(ProjectOutcome::Loaded(Box::new(ProjectLoaded {
        path: path.to_path_buf(),
        root,
        file,
        record,
        warnings,
    })))
}

/// Lexical `.`/`..` normalization (no filesystem access).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A project's `record.dir` is accepted only inside the project root or
/// `<data root>/recordings`: otherwise a cloned repo could aim recordings (which
/// may hold secrets) at any path it likes. `canon` resolves symlinks of the
/// longest existing ancestor, so a symlinked subdir cannot smuggle a path out.
/// The returned path is the canonical one that was vetted.
pub fn vet_project_dir(
    raw: &Path,
    root: &Path,
    blade_dir: &Path,
    canon: &dyn Fn(&Path) -> PathBuf,
) -> Result<PathBuf, String> {
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        root.join(raw)
    };
    let resolved = canon(&normalize(&joined));
    let allowed_root = canon(root);
    let allowed_rec = canon(&blade_dir.join(crate::record::values::RECORDINGS_SUBDIR));
    if resolved.starts_with(&allowed_root) || resolved.starts_with(&allowed_rec) {
        Ok(resolved)
    } else {
        Err(format!(
            "record.dir {} ignored: must be inside the project ({}) or {}",
            raw.display(),
            root.display(),
            allowed_rec.display()
        ))
    }
}

/// Discover and load the project file for `cwd`. Returns the loaded file (if
/// any) plus warnings (ignored file, dropped dir) for the caller to print.
pub fn project_in(
    cwd: &Path,
    home: Option<&Path>,
    blade_dir: &Path,
) -> Result<(Option<ProjectLoaded>, Vec<String>), String> {
    // Canonical forms so the home comparison survives symlinked homes.
    let cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let home = home.map(|h| std::fs::canonicalize(h).unwrap_or_else(|_| h.to_path_buf()));
    let exists = |p: &Path| std::fs::symlink_metadata(p).is_ok();
    let is_repo = |p: &Path| std::fs::symlink_metadata(p.join(".git")).is_ok();
    let Some(path) = find_project_file(&cwd, home.as_deref(), &exists, &is_repo) else {
        return Ok((None, Vec::new()));
    };
    debug_assert!(path.ends_with(PROJECT_FILE));
    match load_project(&path, blade_dir)? {
        ProjectOutcome::Ignored(w) => Ok((None, vec![w])),
        ProjectOutcome::Loaded(l) => {
            let l = *l;
            let w = l.warnings.clone();
            Ok((Some(l), w))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    /// Canonical scratch dir (macOS temp is a symlink), never a real home.
    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "bladebro-project-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&d).expect("scratch");
        std::fs::canonicalize(&d).expect("canon")
    }

    fn put(dir: &Path, body: &str) -> PathBuf {
        let f = dir.join(PROJECT_FILE);
        std::fs::write(&f, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        f
    }

    fn loaded(o: ProjectOutcome) -> ProjectLoaded {
        match o {
            ProjectOutcome::Loaded(l) => *l,
            ProjectOutcome::Ignored(w) => panic!("ignored: {w}"),
        }
    }

    #[test]
    fn load_valid_and_empty() {
        let d = scratch();
        let f = put(&d, r#"{"headless":true,"record":{"fps":8}}"#);
        let l = loaded(load_project(&f, &d.join("bd")).unwrap());
        assert_eq!(l.file.headless, Some(true));
        assert_eq!(l.record.unwrap().fps, Some(8));
        let f = put(&d, "{}");
        let l = loaded(load_project(&f, &d.join("bd")).unwrap());
        assert_eq!(l.file, ProjectFile::default());
        assert!(l.record.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn strict_keys_size_and_json_errors() {
        let d = scratch();
        let bd = d.join("bd");
        let f = put(&d, r#"{"headles":true}"#);
        let e = load_project(&f, &bd).unwrap_err();
        assert!(
            e.contains("headles") && e.contains(&f.display().to_string()),
            "{e}"
        );
        put(&d, r#"{"record":{"fsp":1}}"#);
        let e = load_project(&f, &bd).unwrap_err();
        assert!(e.contains("fsp"), "{e}");
        put(
            &d,
            &format!(
                "{{\"headless\":true,\"pad\":\"{}\"}}",
                "x".repeat(65 * 1024)
            ),
        );
        let e = load_project(&f, &bd).unwrap_err();
        assert!(e.contains("64 KiB"), "{e}");
        put(&d, "not json");
        let e = load_project(&f, &bd).unwrap_err();
        assert!(e.contains(&f.display().to_string()), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn untrusted_files_are_ignored() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch();
        let bd = d.join("bd");
        // symlink
        let real = d.join("real.json");
        std::fs::write(&real, r#"{"headless":true}"#).unwrap();
        let sub = d.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let link = sub.join(PROJECT_FILE);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        match load_project(&link, &bd).unwrap() {
            ProjectOutcome::Ignored(w) => assert!(w.contains("symlink"), "{w}"),
            other => panic!("{other:?}"),
        }
        // group-writable
        let f = put(&d, r#"{"headless":true}"#);
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o664)).unwrap();
        match load_project(&f, &bd).unwrap() {
            ProjectOutcome::Ignored(w) => assert!(w.contains("writable"), "{w}"),
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    fn ident(p: &Path) -> PathBuf {
        p.to_path_buf()
    }

    #[test]
    fn vet_dir_pure_rows() {
        let root = Path::new("/proj");
        let bd = Path::new("/bd");
        let ok = |s: &str| vet_project_dir(Path::new(s), root, bd, &ident);
        assert_eq!(ok("rec").unwrap(), PathBuf::from("/proj/rec"));
        assert_eq!(ok("./a/../rec").unwrap(), PathBuf::from("/proj/rec"));
        assert!(ok("../out").unwrap_err().contains("ignored"));
        assert_eq!(ok("/proj/x/y").unwrap(), PathBuf::from("/proj/x/y"));
        assert_eq!(
            ok("/bd/recordings/x").unwrap(),
            PathBuf::from("/bd/recordings/x")
        );
        assert!(ok("/bd/sessions").is_err());
        assert!(ok("/bd").is_err());
        assert!(ok("/etc/cron.d").is_err());
        assert!(ok("/proj/../etc").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn vet_dir_symlinked_subdir_escape() {
        let d = scratch();
        let root = d.join("proj");
        let outside = d.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let canon = |p: &Path| crate::record::dir::resolve_lenient(p);
        let e = vet_project_dir(Path::new("link/rec"), &root, &d.join("bd"), &canon).unwrap_err();
        assert!(e.contains("ignored"), "{e}");
        assert!(vet_project_dir(Path::new("rec"), &root, &d.join("bd"), &canon).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn load_project_drops_escaping_dir_with_warning() {
        let d = scratch();
        let f = put(&d, r#"{"record":{"dir":"../../elsewhere","fps":8}}"#);
        let l = loaded(load_project(&f, &d.join("bd")).unwrap());
        let rec = l.record.unwrap();
        assert_eq!(rec.dir, None);
        assert_eq!(rec.fps, Some(8));
        assert_eq!(l.warnings.len(), 1);
        assert!(l.warnings[0].contains("record.dir"), "{:?}", l.warnings);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn project_in_walks_up_and_reports_ignored() {
        let d = scratch();
        let deep = d.join("a").join("b");
        std::fs::create_dir_all(&deep).unwrap();
        put(&d, r#"{"headless":true}"#);
        let (l, w) = project_in(&deep, Some(&d.join("home")), &d.join("bd")).unwrap();
        assert!(w.is_empty());
        assert_eq!(l.unwrap().path, d.join(PROJECT_FILE));
        // home itself is never a project
        let (l, _) = project_in(&deep, Some(&d), &d.join("bd")).unwrap();
        assert!(l.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
