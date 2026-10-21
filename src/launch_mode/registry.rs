//! Option registry (DOC-02): the code-side list of every fork option, bound to
//! docs/recording-and-launch-config.md by tests that fail on drift in either
//! direction. Env names come from the consts the parsers read; config keys are
//! checked against the serde structs themselves (never a free-text grep).

use super::find::PROJECT_FILE;
use super::mode::ENV_HEADLESS;
use super::user::USER_LAUNCH_FILE;
use crate::record::ffmpeg::ENV_FFMPEG;
use crate::record::values::{
    CONFIG_FILE, ENV_RECORD, ENV_RECORD_FORMAT, ENV_RECORD_FPS, ENV_RECORD_SIZE,
};

/// Every environment variable the fork defines.
pub const ENV_VARS: &[&str] = &[
    ENV_HEADLESS,
    ENV_RECORD,
    ENV_RECORD_FORMAT,
    ENV_RECORD_FPS,
    ENV_RECORD_SIZE,
    ENV_FFMPEG,
];

/// Keys of `record.json` and of the project file's `record` section.
pub const RECORD_FILE_KEYS: &[&str] = &["enabled", "dir", "format", "fps", "size", "quality"];

/// Top-level keys of `.bladebro.json`.
pub const PROJECT_FILE_KEYS: &[&str] = &["headless", "record"];

/// Keys of `<data root>/launch.json`.
pub const USER_LAUNCH_KEYS: &[&str] = &["headless"];

/// Config files the page must name.
pub const CONFIG_FILES: &[&str] = &[PROJECT_FILE, USER_LAUNCH_FILE, CONFIG_FILE];

/// CLI commands the page must show.
pub const CLI_COMMANDS: &[&str] = &["bladebro headless"];

/// Upstream variables the page may mention without the fork owning them.
pub const UPSTREAM_ALLOWLIST: &[&str] = &[
    "BLADE_HOME",            // data root; the page explains where files live
    "BLADE_NO_UPDATE_CHECK", // examples disable update checks
    "BLADE_CHROME_FLAGS",    // the old raw-flag headless workaround, kept for the migration note
    "BLADE_TRANSPORT",       // MCP transport note
    "BLADE_LANE",            // real-lane note (headless is ignored there)
    "BLADE_IDLE_TIMEOUT",    // daemon lifetime note
];

/// All `BLADE_[A-Z0-9_]+` tokens in `text`, trailing `_` trimmed. A token must
/// start at a non-identifier boundary and carry something after `BLADE_`.
pub fn blade_tokens(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let is_id = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out = Vec::new();
    let mut i = 0;
    while i + 6 <= b.len() {
        if &b[i..i + 6] == b"BLADE_" && (i == 0 || !is_id(b[i - 1])) {
            let mut j = i + 6;
            while j < b.len()
                && (b[j].is_ascii_uppercase() || b[j].is_ascii_digit() || b[j] == b'_')
            {
                j += 1;
            }
            let tok = text[i..j].trim_end_matches('_');
            if tok.len() > "BLADE".len() {
                out.push(tok.to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch_mode::{ProjectFile, UserLaunchFile};
    use crate::record::{FileConfig, RecordOverrides};
    use std::collections::BTreeSet;

    const PAGE: &str = include_str!("../../docs/recording-and-launch-config.md");

    #[test]
    fn tokens_scan() {
        assert_eq!(blade_tokens("x BLADE_A_B, y"), vec!["BLADE_A_B"]);
        assert_eq!(blade_tokens("`BLADE_RECORD_`"), vec!["BLADE_RECORD"]);
        assert!(blade_tokens("NOBLADE_X").is_empty());
        assert!(blade_tokens("BLADE_ alone").is_empty());
    }

    /// Field names a serde struct accepts, read from its own unknown-field error.
    fn serde_keys<T: serde::de::DeserializeOwned>() -> BTreeSet<String> {
        let err = match serde_json::from_str::<T>(r#"{"__drift_probe__":0}"#) {
            Ok(_) => panic!("struct accepted an unknown field; deny_unknown_fields is gone"),
            Err(e) => e.to_string(),
        };
        let mut keys = BTreeSet::new();
        for (n, part) in err.split('`').enumerate() {
            if n % 2 == 1 && part != "__drift_probe__" {
                keys.insert(part.to_string());
            }
        }
        keys
    }

    fn set(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn serde_keys_match_registry() {
        assert_eq!(
            serde_keys::<FileConfig>(),
            set(RECORD_FILE_KEYS),
            "record.json keys"
        );
        assert_eq!(
            serde_keys::<ProjectFile>(),
            set(PROJECT_FILE_KEYS),
            ".bladebro.json keys"
        );
        assert_eq!(
            serde_keys::<UserLaunchFile>(),
            set(USER_LAUNCH_KEYS),
            "launch.json keys"
        );
    }

    #[test]
    fn every_code_option_is_documented() {
        let page_tokens: BTreeSet<String> = blade_tokens(PAGE).into_iter().collect();
        let mut missing = Vec::new();
        for v in ENV_VARS {
            if !page_tokens.contains(*v) {
                missing.push(format!("env var {v}"));
            }
        }
        for k in RECORD_FILE_KEYS
            .iter()
            .chain(PROJECT_FILE_KEYS)
            .chain(USER_LAUNCH_KEYS)
        {
            if !PAGE.contains(&format!("`{k}`")) {
                missing.push(format!("config key `{k}`"));
            }
        }
        for f in CONFIG_FILES.iter().chain(CLI_COMMANDS) {
            if !PAGE.contains(f) {
                missing.push(format!("name {f}"));
            }
        }
        assert!(
            missing.is_empty(),
            "defined in code but missing from the page: {missing:?}; add it to docs/recording-and-launch-config.md"
        );
    }

    #[test]
    fn every_documented_var_exists_in_code() {
        let unknown: BTreeSet<String> = blade_tokens(PAGE)
            .into_iter()
            .filter(|t| {
                !ENV_VARS.contains(&t.as_str()) && !UPSTREAM_ALLOWLIST.contains(&t.as_str())
            })
            .collect();
        assert!(
            unknown.is_empty(),
            "on the page but unknown to the code: {unknown:?}; remove it from the page, or add it to ENV_VARS (fork option) or UPSTREAM_ALLOWLIST (with a reason)"
        );
    }

    /// Every `.rs` file below `dir`, recursively. A directory that cannot be
    /// read fails the test: a silent skip would hide files from the scan.
    fn rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let rd = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        for entry in rd {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                rs_files(&path, out);
            } else if path.extension().is_some_and(|x| x == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn source_literals_are_registered() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        for sub in ["launch_mode", "record"] {
            rs_files(&src.join(sub), &mut files);
        }
        assert!(
            files.len() >= 20,
            "scan found too few files: {}",
            files.len()
        );
        let mut bad = Vec::new();
        for path in &files {
            // This file's own tests feed deliberately fake tokens (BLADE_A_B,
            // BLADE_ alone, NOBLADE_X) to the scanner; scanning it would flag
            // the fixtures. Its non-test content is only registered names.
            if path.file_name().is_some_and(|n| n == "registry.rs") {
                continue;
            }
            let text =
                std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            for t in blade_tokens(&text) {
                if !ENV_VARS.contains(&t.as_str()) && !UPSTREAM_ALLOWLIST.contains(&t.as_str()) {
                    bad.push(format!(
                        "{}: {t}",
                        path.strip_prefix(&src).unwrap().display()
                    ));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "BLADE_* names in fork sources that are not registered: {bad:?}; add them to ENV_VARS (and the page) or UPSTREAM_ALLOWLIST"
        );
    }

    /// `(file label, key)` rows of the page's `## Config keys` table.
    fn page_config_keys() -> BTreeSet<(String, String)> {
        let start = PAGE
            .find("\n## Config keys\n")
            .expect("page has no `## Config keys` section")
            + 1;
        let end = PAGE[start + 1..]
            .find("\n## ")
            .map_or(PAGE.len(), |i| start + 1 + i);
        let section = &PAGE[start..end];
        let mut rows = BTreeSet::new();
        for line in section.lines().filter(|l| l.starts_with('|')) {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            if cells.len() < 2 || cells[0] == "File" || cells[0].starts_with("---") {
                continue;
            }
            let strip = |c: &str| c.trim_matches('`').to_string();
            assert!(
                rows.insert((strip(cells[0]), strip(cells[1]))),
                "duplicate row: {line}"
            );
        }
        rows
    }

    fn code_config_keys() -> BTreeSet<(String, String)> {
        let mut rows = BTreeSet::new();
        for (file, keys) in [
            (CONFIG_FILE.to_string(), serde_keys::<FileConfig>()),
            (PROJECT_FILE.to_string(), serde_keys::<ProjectFile>()),
            (format!("{PROJECT_FILE} record"), serde_keys::<FileConfig>()),
            (USER_LAUNCH_FILE.to_string(), serde_keys::<UserLaunchFile>()),
        ] {
            for k in keys {
                rows.insert((file.clone(), k));
            }
        }
        rows
    }

    #[test]
    fn page_config_keys_table_equals_the_serde_keys() {
        let page = page_config_keys();
        let code = code_config_keys();
        let phantom: Vec<_> = page.difference(&code).collect();
        let missing: Vec<_> = code.difference(&page).collect();
        assert!(
            phantom.is_empty() && missing.is_empty(),
            "`## Config keys` drifted from the serde structs. On the page but not in code: {phantom:?}; in code but not on the page: {missing:?}"
        );
    }

    #[test]
    fn env_consts_are_what_parsers_read() {
        let env = |k: &str| (k == ENV_RECORD_FPS).then(|| "16".to_string());
        let e = crate::record::resolve_layered(
            &RecordOverrides::default(),
            &env,
            None,
            None,
            std::path::Path::new("/nonexistent-blade"),
        )
        .unwrap_err();
        assert!(e.starts_with("BLADE_RECORD_FPS:"), "{e}");
        let e = crate::launch_mode::parse_headless_env(Some("maybe")).unwrap_err();
        assert!(e.starts_with(ENV_HEADLESS), "{e}");
    }
}
