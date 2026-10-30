//! Project-file discovery: walk up from the working directory.
//!
//! Pure (the filesystem is injected) so the stop rules are table-tested. Each
//! stop rule closes a threat: a parent folder's file must not steer a checkout
//! (repo boundary), the home dir is never a project (user settings belong in
//! `launch.json`), and a runaway path cannot loop forever (64 levels).

use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = ".bladebro.json";
const MAX_LEVELS: usize = 64;

/// First `.bladebro.json` from `start` upward. Stops BEFORE reading `home`,
/// AFTER a directory that is a repo root, at the filesystem root, and after
/// 64 levels.
pub fn find_project_file(
    start: &Path,
    home: Option<&Path>,
    exists: &dyn Fn(&Path) -> bool,
    is_repo_root: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let mut cur = start;
    for _ in 0..MAX_LEVELS {
        if home == Some(cur) {
            return None;
        }
        let cand = cur.join(PROJECT_FILE);
        if exists(&cand) {
            return Some(cand);
        }
        if is_repo_root(cur) {
            return None;
        }
        match cur.parent() {
            Some(p) if p != cur && !p.as_os_str().is_empty() => cur = p,
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn run(
        start: &str,
        home: Option<&str>,
        files: &[&str],
        repos: &[&str],
    ) -> (Option<PathBuf>, Vec<PathBuf>) {
        let calls = RefCell::new(Vec::new());
        let exists = |q: &Path| {
            calls.borrow_mut().push(q.to_path_buf());
            files.iter().any(|f| q == Path::new(f))
        };
        let is_repo = |q: &Path| repos.iter().any(|r| q == Path::new(r));
        let home = home.map(p);
        let r = find_project_file(&p(start), home.as_deref(), &exists, &is_repo);
        let c = calls.borrow().clone();
        (r, c)
    }

    #[test]
    fn found_in_start_and_in_grandparent() {
        let (r, _) = run("/w/a", Some("/h"), &["/w/a/.bladebro.json"], &[]);
        assert_eq!(r, Some(p("/w/a/.bladebro.json")));
        let (r, _) = run("/w/a/b", Some("/h"), &["/w/.bladebro.json"], &[]);
        assert_eq!(r, Some(p("/w/.bladebro.json")));
    }

    #[test]
    fn nearest_file_wins() {
        let (r, _) = run(
            "/w/a",
            None,
            &["/w/.bladebro.json", "/w/a/.bladebro.json"],
            &[],
        );
        assert_eq!(r, Some(p("/w/a/.bladebro.json")));
    }

    #[test]
    fn home_is_never_read() {
        let (r, calls) = run("/h", Some("/h"), &["/h/.bladebro.json"], &[]);
        assert_eq!(r, None);
        assert!(calls.is_empty(), "{calls:?}");
        let (r, calls) = run("/h/proj", Some("/h"), &["/h/.bladebro.json"], &[]);
        assert_eq!(r, None);
        assert!(!calls.contains(&p("/h/.bladebro.json")));
    }

    #[test]
    fn repo_boundary_stops_the_walk() {
        let (r, _) = run("/w/repo/sub", None, &["/w/.bladebro.json"], &["/w/repo"]);
        assert_eq!(r, None);
        let (r, _) = run(
            "/w/repo/sub",
            None,
            &["/w/repo/.bladebro.json"],
            &["/w/repo"],
        );
        assert_eq!(r, Some(p("/w/repo/.bladebro.json")));
    }

    #[test]
    fn walks_to_root_outside_home() {
        let (r, _) = run("/x/y/z", Some("/h"), &["/.bladebro.json"], &[]);
        assert_eq!(r, Some(p("/.bladebro.json")));
    }

    #[test]
    fn depth_is_capped_at_64() {
        let deep: String = (0..70).map(|i| format!("/d{i}")).collect();
        let (r, calls) = run(&deep, None, &["/.bladebro.json"], &[]);
        assert_eq!(r, None);
        assert_eq!(calls.len(), 64);
    }
}
