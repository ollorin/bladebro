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

    #[test]
    fn source_literals_are_registered() {
        let sources: &[(&str, &str)] = &[
            ("record/values.rs", include_str!("../record/values.rs")),
            ("record/config.rs", include_str!("../record/config.rs")),
            ("record/layered.rs", include_str!("../record/layered.rs")),
            ("record/dir.rs", include_str!("../record/dir.rs")),
            (
                "record/ffmpeg/lookup.rs",
                include_str!("../record/ffmpeg/lookup.rs"),
            ),
            (
                "record/ffmpeg/report.rs",
                include_str!("../record/ffmpeg/report.rs"),
            ),
            ("launch_mode/mode.rs", include_str!("mode.rs")),
            ("launch_mode/display.rs", include_str!("display.rs")),
            ("launch_mode/state.rs", include_str!("state.rs")),
            ("launch_mode/find.rs", include_str!("find.rs")),
            ("launch_mode/project.rs", include_str!("project.rs")),
            ("launch_mode/user.rs", include_str!("user.rs")),
            ("launch_mode/cli.rs", include_str!("cli.rs")),
            ("launch_mode/daemon.rs", include_str!("daemon.rs")),
        ];
        let mut bad = Vec::new();
        for (name, text) in sources {
            for t in blade_tokens(text) {
                if !ENV_VARS.contains(&t.as_str()) && !UPSTREAM_ALLOWLIST.contains(&t.as_str()) {
                    bad.push(format!("{name}: {t}"));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "BLADE_* names in fork sources that are not registered: {bad:?}; add them to ENV_VARS (and the page) or UPSTREAM_ALLOWLIST"
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
