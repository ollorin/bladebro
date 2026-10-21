//! Daemon mode-mismatch guard (HEAD-03, daemon half).
//!
//! A running daemon's browser is pinned to the mode it launched with. A CLI
//! command that resolves to the other mode is refused with a plain instruction
//! instead of silently using the old mode. No auto-restart: that would kill
//! another session's browser. The message must not contain the phrases that
//! make `run_tool` restart the daemon ("Connection refused", "No such file",
//! "daemon not running").

use serde_json::{json, Value};

use super::mode::LaunchMode;

fn word(headless: bool) -> &'static str {
    if headless {
        "headless"
    } else {
        "headed"
    }
}

/// `Some(text)` when the request's headless bit differs from the daemon's.
/// Different sources with the same behavior are not a mismatch.
pub fn mismatch_message(
    daemon: &LaunchMode,
    req_headless: bool,
    req_source: &str,
) -> Option<String> {
    let have = daemon.forced_headless();
    if have == req_headless {
        return None;
    }
    Some(format!(
        "browser mode mismatch: the running daemon's browser is {} (source: {}) but this command resolves to {} (source: {}). Run `bladebro stop`, then retry; the next command starts the browser in the requested mode.",
        word(have),
        daemon.source.label(),
        word(req_headless),
        req_source
    ))
}

pub fn request_fields(m: &LaunchMode) -> Value {
    json!({"headless": m.forced_headless(), "source": m.source.label()})
}

/// Lane-aware core of `request_tag`: on the real lane headless does not apply,
/// so a bad BLADE_HEADLESS must not block it (null = no claim).
pub fn request_fields_for(
    lane_real: bool,
    res: Result<LaunchMode, String>,
) -> Result<Value, String> {
    match (lane_real, res) {
        (true, Err(_)) => Ok(Value::Null),
        (true, Ok(_)) => Ok(Value::Null),
        (false, Ok(m)) => Ok(request_fields(&m)),
        (false, Err(e)) => Err(e),
    }
}

/// `(headless, source)` from a daemon request; absent/malformed -> None (an
/// old or foreign client is never refused).
pub fn parse_request(req: &Value) -> Option<(bool, String)> {
    let l = req.get("launch")?;
    Some((
        l.get("headless")?.as_bool()?,
        l.get("source")?.as_str()?.to_string(),
    ))
}

/// True when this process resolves to the real-browser lane.
pub fn lane_is_real() -> bool {
    crate::realbrowser::lane_from(
        std::env::var("BLADE_LANE").ok().as_deref(),
        crate::realbrowser::config().enabled,
    )
}

/// The `launch` field this client puts on every daemon request. Also the
/// client-side validation: `run_tool` calls it BEFORE auto-starting a daemon,
/// so an invalid setting never starts a daemon that would inherit it.
pub fn request_tag() -> Result<Value, String> {
    request_fields_for(lane_is_real(), super::state::effective())
}

/// The daemon's environment cannot change after it started, so an invalid
/// launch setting there stays invalid until it is stopped.
pub fn invalid_setting_message(e: &str) -> String {
    format!(
        "this daemon was started with an invalid launch setting ({e}); run `bladebro stop`, fix the setting, then retry"
    )
}

/// Daemon side: error text when this request must be refused. Skipped on the
/// real-browser lane (headless does not apply there).
pub fn daemon_mismatch(req: &Value) -> Option<String> {
    let (headless, source) = parse_request(req)?;
    if crate::realbrowser::real_lane() {
        return None;
    }
    match super::state::effective() {
        Err(e) => Some(invalid_setting_message(&e)),
        Ok(m) => mismatch_message(&m, headless, &source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch_mode::mode::Source;

    fn m(h: Option<bool>, s: Source) -> LaunchMode {
        LaunchMode {
            headless: h,
            source: s,
        }
    }

    #[test]
    fn mismatch_table() {
        let a = mismatch_message(&m(Some(true), Source::Env), false, "default").unwrap();
        assert!(a.starts_with("browser mode mismatch: the running daemon's browser is headless"));
        let b = mismatch_message(&m(None, Source::Default), true, "env BLADE_HEADLESS").unwrap();
        assert!(b.contains("is headed") && b.contains("resolves to headless"));
        assert_eq!(
            mismatch_message(&m(Some(false), Source::Env), false, "default"),
            None
        );
        assert_eq!(
            mismatch_message(&m(None, Source::Default), false, "env BLADE_HEADLESS"),
            None
        );
        for t in [&a, &b] {
            assert!(t.contains("bladebro stop"));
            for bad in ["Connection refused", "No such file", "daemon not running"] {
                assert!(!t.contains(bad), "{t}");
            }
        }
    }

    #[test]
    fn request_roundtrip_and_lenient_parse() {
        let f = request_fields(&m(Some(true), Source::Env));
        assert_eq!(f["headless"], true);
        assert_eq!(f["source"], "env BLADE_HEADLESS");
        let req = json!({"tool": "nav", "launch": f});
        assert_eq!(
            parse_request(&req),
            Some((true, "env BLADE_HEADLESS".to_string()))
        );
        assert_eq!(parse_request(&json!({"tool": "stop"})), None);
        assert_eq!(parse_request(&json!({"launch": {"headless": "x"}})), None);
        assert_eq!(parse_request(&Value::Null), None);
        assert_eq!(parse_request(&json!({"launch": null})), None);
    }

    #[test]
    fn poisoned_daemon_message_says_stop() {
        let t = invalid_setting_message("BLADE_HEADLESS: \"maybe\" is not valid");
        assert!(t.contains("invalid launch setting") && t.contains("bladebro stop"));
        for bad in ["Connection refused", "No such file", "daemon not running"] {
            assert!(!t.contains(bad), "{t}");
        }
    }

    #[test]
    fn real_lane_never_blocks_on_a_bad_value() {
        assert_eq!(request_fields_for(true, Err("bad".into())), Ok(Value::Null));
        assert!(request_fields_for(false, Err("bad".into())).is_err());
        let v = request_fields_for(false, Ok(m(None, Source::Default))).unwrap();
        assert_eq!(v["headless"], false);
    }
}
