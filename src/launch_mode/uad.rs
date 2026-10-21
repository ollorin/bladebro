//! Headless UA client-hints metadata (`Network.setUserAgentOverride`
//! `userAgentMetadata`).
//!
//! Measured (macOS, Chrome 154, 2026-10): once a headless browser gets a UA
//! override WITHOUT metadata, `navigator.userAgentData` reports empty
//! `brands`, empty `platform` and empty high-entropy values — a loud tell
//! (headed Chrome always reports a brand list). So the override must carry
//! metadata. Sources, best first:
//! 1. the browser's own `navigator.userAgentData` (only readable in a secure
//!    context — about:blank is not one), with `HeadlessChrome` renamed;
//! 2. derived from the real UA major version using Chromium's own
//!    deterministic GREASE/shuffle algorithm (`derive`), so the result equals
//!    what a stock headed Chrome of that major reports — never a hand-picked
//!    brand that matches no real build.

use serde_json::{json, Value};

/// Async probe run via `Runtime.evaluate` (awaitPromise) — Runtime stays
/// disabled. Returns a JSON string, or "" when userAgentData is unavailable.
pub const PROBE_JS: &str = "(async()=>{const u=navigator.userAgentData;if(!u||!u.brands||!u.brands.length)return '';\
const h=await u.getHighEntropyValues(['architecture','bitness','model','platformVersion','fullVersionList','wow64']);\
return JSON.stringify({brands:u.brands,platform:u.platform,mobile:u.mobile,h})})()";

fn rename(brand: &str) -> String {
    if brand == "HeadlessChrome" {
        "Google Chrome".into()
    } else {
        brand.to_string()
    }
}

fn rename_list(v: Option<&Value>) -> Vec<Value> {
    v.and_then(|l| l.as_array())
        .map(|l| {
            l.iter()
                .map(|b| {
                    json!({
                        "brand": rename(b["brand"].as_str().unwrap_or("")),
                        "version": b["version"].as_str().unwrap_or(""),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Metadata from the browser's own probe result (`PROBE_JS` output).
/// None when the probe carried no brands.
pub fn from_probe(probe: &str) -> Option<Value> {
    let p: Value = serde_json::from_str(probe).ok()?;
    let brands = rename_list(p.get("brands"));
    if brands.is_empty() {
        return None;
    }
    let h = p.get("h")?;
    let mut full = rename_list(h.get("fullVersionList"));
    if full.is_empty() {
        full = brands.clone();
    }
    let full_version = full
        .iter()
        .find(|b| b["brand"] == "Google Chrome")
        .or_else(|| full.iter().find(|b| b["brand"] == "Chromium"))
        .and_then(|b| b["version"].as_str())
        .unwrap_or("")
        .to_string();
    let s = |k: &str| h.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    Some(json!({
        "brands": brands,
        "fullVersionList": full,
        "fullVersion": full_version,
        "platform": p.get("platform").and_then(|v| v.as_str()).unwrap_or(""),
        "platformVersion": s("platformVersion"),
        "architecture": s("architecture"),
        "model": s("model"),
        "mobile": p.get("mobile").and_then(|v| v.as_bool()).unwrap_or(false),
        "bitness": s("bitness"),
        "wow64": h.get("wow64").and_then(|v| v.as_bool()).unwrap_or(false),
    }))
}

/// Chromium's GREASE brand + order for a major version (mirrors
/// `GenerateBrandVersionList`: seed = major).
fn grease_list(major: u32, chrome_version: &str) -> Vec<Value> {
    const CHARS: [&str; 11] = [" ", "(", ":", "-", ".", "/", ")", ";", "=", "?", "_"];
    const VERS: [&str; 3] = ["8", "99", "24"];
    const ORDERS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let seed = major as usize;
    let grease = format!("Not{}A{}Brand", CHARS[seed % 11], CHARS[(seed + 1) % 11]);
    let gv = VERS[seed % 3];
    let order = ORDERS[seed % 6];
    let items = [
        (grease, gv.to_string()),
        ("Chromium".to_string(), chrome_version.to_string()),
        ("Google Chrome".to_string(), chrome_version.to_string()),
    ];
    let mut out = vec![Value::Null; 3];
    for (i, (b, v)) in items.into_iter().enumerate() {
        out[order[i]] = json!({"brand": b, "version": v});
    }
    out
}

/// Pull `Chrome/<ver>` out of a UA string.
fn ua_version(ua: &str) -> Option<String> {
    let rest = ua.split("Chrome/").nth(1)?;
    let v: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    (!v.is_empty()).then_some(v)
}

/// Metadata derived from the (already de-Headless'd) UA string alone.
pub fn derive(ua: &str) -> Option<Value> {
    let full = ua_version(ua)?;
    let major: u32 = full.split('.').next()?.parse().ok()?;
    let platform = if ua.contains("Macintosh") {
        "macOS"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("Linux") || ua.contains("X11") {
        "Linux"
    } else {
        return None;
    };
    let arch = if ua.contains("aarch64") || (cfg!(target_arch = "aarch64") && platform != "Linux") {
        "arm"
    } else {
        "x86"
    };
    let major_list: Vec<Value> = grease_list(major, &major.to_string());
    let full_list: Vec<Value> = grease_list(major, &full);
    // GREASE entry keeps its own full version in the full list (Chromium
    // reports "<greasev>.0.0.0"); real brands carry the full version.
    let full_list: Vec<Value> = full_list
        .into_iter()
        .map(|b| {
            if b["brand"].as_str().is_some_and(|n| n.starts_with("Not")) {
                json!({"brand": b["brand"], "version": format!("{}.0.0.0", b["version"].as_str().unwrap_or("0"))})
            } else {
                b
            }
        })
        .collect();
    Some(json!({
        "brands": major_list,
        "fullVersionList": full_list,
        "fullVersion": full,
        "platform": platform,
        "platformVersion": "",
        "architecture": arch,
        "model": "",
        "mobile": false,
        "bitness": "64",
        "wow64": false,
    }))
}

/// Probe result first, UA derivation second.
pub fn build(probe: Option<&str>, ua: &str) -> Option<Value> {
    probe.and_then(from_probe).or_else(|| derive(ua))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/154.0.0.0 Safari/537.36";

    #[test]
    fn derive_matches_chromium_grease_for_154() {
        let m = derive(UA).unwrap();
        let b: Vec<String> = m["brands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| {
                format!(
                    "{}/{}",
                    b["brand"].as_str().unwrap(),
                    b["version"].as_str().unwrap()
                )
            })
            .collect();
        // Same list a headed Chrome 154 reports (baseline audit capture).
        assert_eq!(b, ["Chromium/154", "Google Chrome/154", "Not A(Brand/99"]);
        assert_eq!(m["platform"], "macOS");
        assert_eq!(m["fullVersion"], "154.0.0.0");
        assert_eq!(m["mobile"], false);
    }

    #[test]
    fn derive_rejects_unknown_ua() {
        assert!(derive("curl/8").is_none());
        assert!(derive("Mozilla/5.0 (Plan9) Chrome/9.0").is_none());
    }

    #[test]
    fn probe_renames_headless_brand() {
        let probe = r#"{"brands":[{"brand":"HeadlessChrome","version":"154"},{"brand":"Chromium","version":"154"},{"brand":"Not A(Brand","version":"99"}],
"platform":"macOS","mobile":false,"h":{"architecture":"arm","bitness":"64","model":"","platformVersion":"15.2.0","wow64":false,
"fullVersionList":[{"brand":"HeadlessChrome","version":"154.0.8037.58"},{"brand":"Chromium","version":"154.0.8037.58"}]}}"#;
        let m = from_probe(probe).unwrap();
        assert_eq!(m["brands"][0]["brand"], "Google Chrome");
        assert_eq!(m["fullVersion"], "154.0.8037.58");
        assert_eq!(m["platformVersion"], "15.2.0");
        assert!(!m.to_string().contains("Headless"));
    }

    #[test]
    fn empty_probe_falls_back_to_derive() {
        assert!(from_probe("").is_none());
        assert!(from_probe(r#"{"brands":[],"h":{}}"#).is_none());
        let m = build(Some(""), UA).unwrap();
        assert_eq!(m["brands"].as_array().unwrap().len(), 3);
        assert!(build(None, "nothing").is_none());
    }
}
