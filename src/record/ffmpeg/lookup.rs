//! ffmpeg lookup: BLADE_FFMPEG, then PATH, then known per-OS locations.
//!
//! The core (`find_ffmpeg`, `known_locations`) takes env, home and the known
//! list as parameters and never reads or mutates the process env, so tests
//! need no env mutation. `find_ffmpeg_from_process` is the thin wiring.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Env lookup injected into the pure core.
pub type EnvFn<'a> = &'a dyn Fn(&str) -> Option<OsString>;

/// Which OS's known-location list to use. All three lists compile everywhere
/// so they can be tested on any host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    MacOs,
    Linux,
    Windows,
}

impl TargetOs {
    /// The host OS. Other unixes (BSDs) use the Linux list.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }

    /// Executable file name.
    pub fn exe_name(self) -> &'static str {
        if self == Self::Windows {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        }
    }
}

fn env_nonblank(env: EnvFn, key: &str) -> Option<OsString> {
    env(key).filter(|v| !v.to_string_lossy().trim().is_empty())
}

/// Known install locations for `os`, in search order (D4-4), as full paths to
/// the executable.
pub fn known_locations(os: TargetOs, home: &Path, env: EnvFn) -> Vec<PathBuf> {
    let exe = os.exe_name();
    let dirs: Vec<PathBuf> = match os {
        TargetOs::MacOs => vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            home.join(".local").join("ffmpeg").join("bin"),
            home.join("bin"),
        ],
        TargetOs::Linux => vec![
            PathBuf::from("/usr/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/snap/bin"),
            home.join(".local").join("bin"),
        ],
        TargetOs::Windows => {
            let mut v = Vec::new();
            // winget installs a shim into %LOCALAPPDATA%\Microsoft\WinGet\Links.
            if let Some(l) = env_nonblank(env, "LOCALAPPDATA") {
                v.push(PathBuf::from(l).join("Microsoft\\WinGet\\Links"));
            }
            // Manual unzip convention.
            v.push(PathBuf::from("C:\\ffmpeg\\bin"));
            // Scoop shims.
            v.push(match env_nonblank(env, "SCOOP") {
                Some(s) => PathBuf::from(s).join("shims"),
                None => home.join("scoop").join("shims"),
            });
            // Chocolatey shims.
            v.push(match env_nonblank(env, "ChocolateyInstall") {
                Some(c) => PathBuf::from(c).join("bin"),
                None => PathBuf::from("C:\\ProgramData\\chocolatey\\bin"),
            });
            v
        }
    };
    dirs.into_iter().map(|d| d.join(exe)).collect()
}

/// Where a found binary came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Override,
    Path,
    Known,
}

impl Source {
    /// Label for doctor output.
    pub fn label(self) -> &'static str {
        match self {
            Self::Override => "BLADE_FFMPEG",
            Self::Path => "PATH",
            Self::Known => "known location",
        }
    }
}

/// A usable executable and how it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub source: Source,
}

/// Result of the lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Found(Found),
    /// BLADE_FFMPEG was set but unusable. PATH and known dirs are NOT tried:
    /// silently running a different binary than the user asked for is the
    /// PATH-hijack failure mode (D4-2).
    OverrideUnusable {
        value: String,
        reason: String,
    },
    NotFound,
}

/// `Ok` when `path` is an existing regular file that is executable. Follows
/// symlinks (homebrew installs are symlinks).
fn check_executable(path: &Path) -> Result<(), String> {
    let md = std::fs::metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "not found".to_string()
        } else {
            e.to_string()
        }
    })?;
    if md.is_dir() {
        return Err("is a directory".into());
    }
    if !md.is_file() {
        return Err("not a regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if md.permissions().mode() & 0o111 == 0 {
            return Err("not executable (chmod +x)".into());
        }
    }
    Ok(())
}

/// Find ffmpeg: BLADE_FFMPEG (blank = unset; if set it must be usable and the
/// search stops there), then each absolute PATH entry (empty and relative
/// entries are skipped: CWD binary hijack, D4-3), then `known`. Reads only
/// through `env`; never touches the process env.
pub fn find_ffmpeg(env: EnvFn, known: &[PathBuf], os: TargetOs) -> Lookup {
    if let Some(v) = env_nonblank(env, "BLADE_FFMPEG") {
        let value = v.to_string_lossy().trim().to_string();
        let p = PathBuf::from(&value);
        if !p.is_absolute() {
            return Lookup::OverrideUnusable {
                value,
                reason: "must be an absolute path (no ~ expansion)".into(),
            };
        }
        return match check_executable(&p) {
            Ok(()) => Lookup::Found(Found {
                path: p,
                source: Source::Override,
            }),
            Err(reason) => Lookup::OverrideUnusable { value, reason },
        };
    }
    if let Some(path_var) = env("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if dir.as_os_str().is_empty() || !dir.is_absolute() {
                continue;
            }
            let cand = dir.join(os.exe_name());
            if check_executable(&cand).is_ok() {
                return Lookup::Found(Found {
                    path: cand,
                    source: Source::Path,
                });
            }
        }
    }
    for cand in known {
        if check_executable(cand).is_ok() {
            return Lookup::Found(Found {
                path: cand.clone(),
                source: Source::Known,
            });
        }
    }
    Lookup::NotFound
}

/// Lookup against the real process env and home. Wires tested pieces together,
/// so it has no unit test of its own (like `resolve_from_env`).
pub fn find_ffmpeg_from_process() -> Lookup {
    let env = process_env;
    let os = TargetOs::current();
    let known = known_locations(os, &crate::platform::home_dir(), &env);
    find_ffmpeg(&env, &known, os)
}

/// `std::env::var_os` as an [`EnvFn`].
pub(super) fn process_env(key: &str) -> Option<OsString> {
    std::env::var_os(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let m: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |k| m.get(k).cloned()
    }

    fn strs(v: &[PathBuf]) -> Vec<String> {
        v.iter().map(|p| p.to_string_lossy().into_owned()).collect()
    }

    /// Forward-slash view: `Path::join` emits `\` on Windows hosts, and the
    /// macOS/Linux location tests compare against `/` literals.
    fn strs_fwd(v: &[PathBuf]) -> Vec<String> {
        strs(v).into_iter().map(|s| s.replace('\\', "/")).collect()
    }

    #[test]
    fn macos_known_locations() {
        let env = env_of(&[]);
        let v = strs_fwd(&known_locations(TargetOs::MacOs, Path::new("/h"), &env));
        assert_eq!(v.len(), 4, "{v:?}");
        let want = [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/h/.local/ffmpeg/bin",
            "/h/bin",
        ];
        for (got, w) in v.iter().zip(want) {
            assert!(got.starts_with(w), "{got} vs {w}");
            assert!(got.ends_with("ffmpeg"), "{got}");
        }
    }

    #[test]
    fn linux_known_locations() {
        let env = env_of(&[]);
        let v = strs_fwd(&known_locations(TargetOs::Linux, Path::new("/h"), &env));
        let want = ["/usr/bin", "/usr/local/bin", "/snap/bin", "/h/.local/bin"];
        assert_eq!(v.len(), 4, "{v:?}");
        for (got, w) in v.iter().zip(want) {
            assert!(got.starts_with(w), "{got} vs {w}");
            assert!(got.ends_with("ffmpeg"), "{got}");
        }
    }

    #[test]
    fn windows_known_locations_with_env() {
        let env = env_of(&[
            ("LOCALAPPDATA", "C:\\Users\\u\\AppData\\Local"),
            ("SCOOP", "D:\\scoop"),
            ("ChocolateyInstall", "E:\\choco"),
        ]);
        let v = strs(&known_locations(TargetOs::Windows, Path::new("/h"), &env));
        assert_eq!(v.len(), 4, "{v:?}");
        assert!(v[0].contains("WinGet"), "{}", v[0]);
        assert!(v[1].contains("C:\\ffmpeg\\bin"), "{}", v[1]);
        assert!(
            v[2].contains("D:\\scoop") && v[2].contains("shims"),
            "{}",
            v[2]
        );
        assert!(
            v[3].contains("E:\\choco") && v[3].contains("bin"),
            "{}",
            v[3]
        );
        for p in &v {
            assert!(p.ends_with("ffmpeg.exe"), "{p}");
        }
    }

    #[test]
    fn windows_known_locations_fallbacks() {
        let env = env_of(&[]);
        let v = strs(&known_locations(TargetOs::Windows, Path::new("/h"), &env));
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(!v.iter().any(|p| p.contains("WinGet")));
        assert!(v[0].contains("C:\\ffmpeg\\bin"));
        assert!(v[1].contains("scoop") && v[1].contains("shims") && v[1].starts_with("/h"));
        assert!(v[2].contains("C:\\ProgramData\\chocolatey\\bin"));
    }

    #[test]
    fn blank_windows_env_is_unset() {
        let env = env_of(&[("LOCALAPPDATA", "  "), ("SCOOP", "")]);
        let v = strs(&known_locations(TargetOs::Windows, Path::new("/h"), &env));
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(v[1].starts_with("/h"));
    }

    #[test]
    fn exe_names_and_current() {
        assert_eq!(TargetOs::Windows.exe_name(), "ffmpeg.exe");
        assert_eq!(TargetOs::MacOs.exe_name(), "ffmpeg");
        assert_eq!(TargetOs::Linux.exe_name(), "ffmpeg");
        let cur = TargetOs::current();
        assert_eq!(cur == TargetOs::Windows, cfg!(target_os = "windows"));
        assert_eq!(cur == TargetOs::MacOs, cfg!(target_os = "macos"));
    }

    #[test]
    fn source_labels() {
        assert_eq!(Source::Override.label(), "BLADE_FFMPEG");
        assert_eq!(Source::Path.label(), "PATH");
        assert_eq!(Source::Known.label(), "known location");
    }

    #[cfg(unix)]
    mod unix {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicU32, Ordering};

        static NEXT: AtomicU32 = AtomicU32::new(0);

        /// Scratch root removed on drop.
        struct Scratch(PathBuf);

        impl Scratch {
            fn new() -> Self {
                let p = std::env::temp_dir().join(format!(
                    "bladebro-ffmpeg-lookup-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::SeqCst)
                ));
                std::fs::create_dir_all(&p).unwrap();
                Self(p)
            }

            fn dir(&self, name: &str) -> PathBuf {
                let d = self.0.join(name);
                std::fs::create_dir_all(&d).unwrap();
                d
            }

            fn file(&self, dir: &str, mode: u32) -> PathBuf {
                let p = self.dir(dir).join("ffmpeg");
                std::fs::write(&p, "#!/bin/sh\necho 'ffmpeg version 6.0-fake'\n").unwrap();
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
                p
            }
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        fn s(p: &Path) -> String {
            p.to_string_lossy().into_owned()
        }

        fn found(path: &Path, source: Source) -> Lookup {
            Lookup::Found(Found {
                path: path.to_path_buf(),
                source,
            })
        }

        fn unusable(l: &Lookup) -> &str {
            match l {
                Lookup::OverrideUnusable { reason, .. } => reason,
                other => panic!("expected OverrideUnusable, got {other:?}"),
            }
        }

        #[test]
        fn override_beats_path_and_known() {
            let r = Scratch::new();
            let o = r.file("o", 0o755);
            let p = r.file("p", 0o755);
            let k = r.file("k", 0o755);
            let env = env_of(&[("BLADE_FFMPEG", &s(&o)), ("PATH", &s(p.parent().unwrap()))]);
            assert_eq!(
                find_ffmpeg(&env, &[k], TargetOs::Linux),
                found(&o, Source::Override)
            );
        }

        #[test]
        fn unusable_override_does_not_fall_through() {
            let r = Scratch::new();
            let p = r.file("p", 0o755);
            let missing = r.dir("none").join("ffmpeg");
            let env = env_of(&[
                ("BLADE_FFMPEG", &s(&missing)),
                ("PATH", &s(p.parent().unwrap())),
            ]);
            let l = find_ffmpeg(&env, std::slice::from_ref(&p), TargetOs::Linux);
            assert!(unusable(&l).contains("not found"), "{l:?}");
        }

        #[test]
        fn override_reasons() {
            let r = Scratch::new();
            let nonexec = r.file("ne", 0o644);
            let env = env_of(&[("BLADE_FFMPEG", &s(&nonexec))]);
            let l = find_ffmpeg(&env, &[], TargetOs::Linux);
            assert!(unusable(&l).contains("not executable"), "{l:?}");

            let d = r.dir("dir");
            let env = env_of(&[("BLADE_FFMPEG", &s(&d))]);
            let l = find_ffmpeg(&env, &[], TargetOs::Linux);
            assert!(unusable(&l).contains("is a directory"), "{l:?}");

            let env = env_of(&[("BLADE_FFMPEG", "relative/ffmpeg")]);
            let l = find_ffmpeg(&env, &[], TargetOs::Linux);
            assert!(unusable(&l).contains("absolute path"), "{l:?}");
            let env = env_of(&[("BLADE_FFMPEG", "~/bin/ffmpeg")]);
            let l = find_ffmpeg(&env, &[], TargetOs::Linux);
            assert!(unusable(&l).contains("absolute path"), "{l:?}");
        }

        #[test]
        fn blank_override_is_unset() {
            let r = Scratch::new();
            let p = r.file("p", 0o755);
            let env = env_of(&[("BLADE_FFMPEG", "   "), ("PATH", &s(p.parent().unwrap()))]);
            assert_eq!(
                find_ffmpeg(&env, &[], TargetOs::Linux),
                found(&p, Source::Path)
            );
        }

        #[test]
        fn path_order_and_skips() {
            let r = Scratch::new();
            let a = r.file("a", 0o755);
            let b = r.file("b", 0o755);
            let both = format!("{}:{}", s(a.parent().unwrap()), s(b.parent().unwrap()));
            let env = env_of(&[("PATH", &both)]);
            assert_eq!(
                find_ffmpeg(&env, &[], TargetOs::Linux),
                found(&a, Source::Path)
            );

            let only_b = format!("{}:{}", r.dir("empty").display(), s(b.parent().unwrap()));
            let env = env_of(&[("PATH", &only_b)]);
            assert_eq!(
                find_ffmpeg(&env, &[], TargetOs::Linux),
                found(&b, Source::Path)
            );

            // Empty and relative entries are skipped, never resolved against CWD.
            let hostile = format!(".::bin:{}", s(b.parent().unwrap()));
            let env = env_of(&[("PATH", &hostile)]);
            assert_eq!(
                find_ffmpeg(&env, &[], TargetOs::Linux),
                found(&b, Source::Path)
            );
        }

        #[test]
        fn path_skips_non_executable() {
            let r = Scratch::new();
            let ne = r.file("ne", 0o644);
            let ok = r.file("ok", 0o755);
            let path = format!("{}:{}", s(ne.parent().unwrap()), s(ok.parent().unwrap()));
            let env = env_of(&[("PATH", &path)]);
            assert_eq!(
                find_ffmpeg(&env, &[], TargetOs::Linux),
                found(&ok, Source::Path)
            );
        }

        #[test]
        fn known_used_when_path_unset() {
            let r = Scratch::new();
            let k1 = r.dir("k1").join("ffmpeg");
            let k2 = r.file("k2", 0o755);
            let env = env_of(&[]);
            assert_eq!(
                find_ffmpeg(&env, &[k1, k2.clone()], TargetOs::Linux),
                found(&k2, Source::Known)
            );
        }

        #[test]
        fn nothing_found() {
            let r = Scratch::new();
            let env = env_of(&[("PATH", &s(&r.dir("x")))]);
            assert_eq!(
                find_ffmpeg(&env, &[r.dir("y").join("ffmpeg")], TargetOs::Linux),
                Lookup::NotFound
            );
        }

        #[test]
        fn override_symlink_is_followed() {
            let r = Scratch::new();
            let real = r.file("real", 0o755);
            let link = r.dir("link").join("ffmpeg");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let env = env_of(&[("BLADE_FFMPEG", &s(&link))]);
            assert_eq!(
                find_ffmpeg(&env, &[], TargetOs::Linux),
                found(&link, Source::Override)
            );
        }
    }
}
