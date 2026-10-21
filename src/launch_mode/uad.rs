//! Headless UA client-hints metadata (`Network.setUserAgentOverride`
//! `userAgentMetadata`).
//!
//! Measured (macOS, Chrome 154, 2026-10): once a headless browser gets a UA
//! override WITHOUT metadata, `navigator.userAgentData` reports empty
//! `brands`, empty `platform` and empty high-entropy values: a loud tell
//! (headed Chrome always reports a brand list). So the override must carry
//! metadata. It is always derived (`derive`): the brand list from Chromium's
//! own deterministic GREASE/shuffle algorithm for the UA's major version, the
//! full version from CDP `Browser.getVersion` (what headed Chrome reports in
//! `fullVersionList`/`uaFullVersion`; the UA string itself only carries the
//! reduced `154.0.0.0`) and `platformVersion` from the OS (`platform_version`).
//! No page-side probe: `navigator.userAgentData` is only readable on a secure
//! origin, and evaluating in the page main world is page-observable.
//!
//! Known remaining difference: Windows `platformVersion` stays empty
//! (headed Chrome reports the build-derived value; not measured here).

use serde_json::{json, Value};

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

/// Parse a `Browser.getVersion` product string (`HeadlessChrome/154.0.8037.58`)
/// to its dotted full version. Only a four-part numeric version is accepted.
pub fn product_version(product: &str) -> Option<String> {
    let v = product.split('/').nth(1)?.trim();
    let parts: Vec<&str> = v.split('.').collect();
    (parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
    .then(|| v.to_string())
}

/// Normalize an OS version string to Chrome's `major.minor.patch`: leading
/// numeric dotted prefix (`6.8.0-45-generic` -> `6.8.0`), padded with `.0`.
/// Empty when nothing numeric leads (never a guess).
pub fn normalize_platform_version(raw: &str) -> String {
    let lead: String = raw
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut parts: Vec<&str> = lead.split('.').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return String::new();
    }
    parts.truncate(3);
    while parts.len() < 3 {
        parts.push("0");
    }
    parts.join(".")
}

/// The OS version headed Chrome reports as `platformVersion`: macOS
/// `sw_vers -productVersion`, Linux kernel release (`uname -r`). Fixed args,
/// no shell. Windows (and any failure) gives "" rather than a guess.
pub fn platform_version() -> String {
    static CACHE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let (prog, arg) = if cfg!(target_os = "macos") {
                ("sw_vers", "-productVersion")
            } else if cfg!(any(target_os = "linux", target_os = "android")) {
                ("uname", "-r")
            } else {
                return String::new();
            };
            std::process::Command::new(prog)
                .arg(arg)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| normalize_platform_version(&String::from_utf8_lossy(&o.stdout)))
                .unwrap_or_default()
        })
        .clone()
}

/// Metadata derived from the (already de-Headless'd) UA string. `real_full`
/// is the browser's true full version (`product_version`; the UA only has the
/// reduced `X.0.0.0`), `platform_version` the OS version (may be empty).
pub fn derive(ua: &str, real_full: Option<&str>, platform_version: &str) -> Option<Value> {
    let ua_full = ua_version(ua)?;
    let major: u32 = ua_full.split('.').next()?.parse().ok()?;
    // Only trust a real version whose major matches the UA's.
    let full = real_full
        .filter(|v| v.split('.').next().and_then(|m| m.parse::<u32>().ok()) == Some(major))
        .map(str::to_string)
        .unwrap_or(ua_full);
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
        "platformVersion": platform_version,
        "architecture": arch,
        "model": "",
        "mobile": false,
        "bitness": "64",
        "wow64": false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/154.0.0.0 Safari/537.36";

    #[test]
    fn derive_matches_chromium_grease_for_154() {
        let m = derive(UA, Some("154.0.8037.58"), "26.6.2").unwrap();
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
        assert_eq!(m["fullVersion"], "154.0.8037.58");
        assert_eq!(m["platformVersion"], "26.6.2");
        assert_eq!(m["mobile"], false);
        let full: Vec<String> = m["fullVersionList"]
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
        assert_eq!(
            full,
            [
                "Chromium/154.0.8037.58",
                "Google Chrome/154.0.8037.58",
                "Not A(Brand/99.0.0.0"
            ]
        );
        assert!(!m.to_string().contains("Headless"));
    }

    #[test]
    fn derive_falls_back_to_ua_version_when_real_is_missing_or_foreign() {
        assert_eq!(derive(UA, None, "").unwrap()["fullVersion"], "154.0.0.0");
        // A different major (stale/foreign product string) is never mixed in.
        assert_eq!(
            derive(UA, Some("120.0.1.2"), "").unwrap()["fullVersion"],
            "154.0.0.0"
        );
    }

    #[test]
    fn derive_rejects_unknown_ua() {
        assert!(derive("curl/8", None, "").is_none());
        assert!(derive("Mozilla/5.0 (Plan9) Chrome/9.0", None, "").is_none());
    }

    #[test]
    fn product_version_parses_only_four_part_numeric() {
        assert_eq!(
            product_version("HeadlessChrome/154.0.8037.58").as_deref(),
            Some("154.0.8037.58")
        );
        assert_eq!(
            product_version("Chrome/154.0.0.0").as_deref(),
            Some("154.0.0.0")
        );
        assert_eq!(product_version("Chrome/154"), None);
        assert_eq!(product_version("Chrome/154.0.x.1"), None);
        assert_eq!(product_version("nothing"), None);
    }

    #[test]
    fn platform_version_normalizes_os_strings() {
        assert_eq!(normalize_platform_version("26.6.2\n"), "26.6.2");
        assert_eq!(normalize_platform_version("15.2"), "15.2.0");
        assert_eq!(normalize_platform_version("6.8.0-45-generic\n"), "6.8.0");
        assert_eq!(normalize_platform_version("6"), "6.0.0");
        assert_eq!(normalize_platform_version(""), "");
        assert_eq!(normalize_platform_version("garbage"), "");
    }
}
