//! `see mode=audit`: read-only page audit - contrast, accessible names,
//! duplicate ids, horizontal overflow, touch-target size, link status.
//!
//! The scan is self-implemented in `js/audit.js` (no vendored axe-core: it is
//! ~550 KB of MPL-2.0 file-level copyleft in a ~7 MB binary, and these checks
//! are the ones a layout QA pass needs). Output is counts plus the top N
//! findings per category; `full=true` lifts the cap.

use serde_json::{json, Value};

use crate::error::{BladeError, Result};
use crate::page::Page;

use super::artifact_hint;

const AUDIT_EXPR: &str = include_str!("js/audit.js");
const DEFAULT_TOP: usize = 5;
const DEFAULT_MIN_TARGET: u64 = 24;
const DEFAULT_MAX_LINKS: u64 = 20;
const MAX_LINKS: u64 = 200;
const INLINE_CAP: usize = 12_000;
const CATEGORIES: &[(&str, &str)] = &[
    ("contrast", "contrast"),
    ("names", "names"),
    ("duplicate_ids", "duplicate-ids"),
    ("overflow", "overflow"),
    ("targets", "small-targets"),
];

/// Audit settings parsed from `see` args.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct AuditOpts {
    pub top: usize,
    pub full: bool,
    pub min_target: u64,
    pub links: String,
    pub max_links: u64,
    pub json_out: bool,
}

pub(super) fn parse_opts(args: &Value) -> Result<AuditOpts> {
    let links = args
        .get("links")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("same-origin")
        .to_string();
    if !matches!(links.as_str(), "same-origin" | "all" | "off") {
        return Err(BladeError::Usage(
            "audit links must be same-origin (default), all, or off".into(),
        ));
    }
    let min_target = args
        .get("min_target")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MIN_TARGET);
    if !(1..=200).contains(&min_target) {
        return Err(BladeError::Usage(
            "audit min_target must be 1-200 CSS px (default 24, WCAG 2.2 SC 2.5.8)".into(),
        ));
    }
    let max_links = args
        .get("max_links")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MAX_LINKS);
    if max_links > MAX_LINKS {
        return Err(BladeError::Usage(format!(
            "audit max_links must be 0-{MAX_LINKS}"
        )));
    }
    Ok(AuditOpts {
        top: args
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_TOP),
        full: args.get("full").and_then(Value::as_bool).unwrap_or(false),
        min_target,
        links,
        max_links,
        json_out: args.get("format").and_then(Value::as_str) == Some("json"),
    })
}

pub(super) async fn handle_audit(args: &Value, page: &mut Page) -> Result<String> {
    let opts = parse_opts(args)?;
    let cfg =
        json!({"min_target": opts.min_target, "links": opts.links, "max_links": opts.max_links});
    let expression = format!("({AUDIT_EXPR})({cfg})");
    let res = page
        .cdp_ref()
        .send_with_timeout(
            "Runtime.evaluate",
            Some(json!({"expression": expression, "returnByValue": true, "awaitPromise": true})),
            std::time::Duration::from_secs(90),
        )
        .await?;
    if let Some(exc) = res.get("exceptionDetails") {
        let msg = exc
            .pointer("/exception/description")
            .and_then(Value::as_str)
            .or_else(|| exc.get("text").and_then(Value::as_str))
            .unwrap_or("audit script failed");
        return Err(BladeError::Other(format!(
            "audit failed: {}",
            crate::platform::truncate_utf8(msg, 200)
        )));
    }
    let mut report = res
        .pointer("/result/value")
        .cloned()
        .filter(Value::is_object)
        .ok_or_else(|| BladeError::Other("audit returned no report".into()))?;
    if opts.links == "all" {
        check_cross_origin(&mut report).await;
    }
    let body = if opts.json_out {
        trim_json(&report, &opts).to_string()
    } else {
        format_text(&report, &opts)
    };
    if body.len() > INLINE_CAP {
        let path =
            crate::artifacts::write_artifact(&body, if opts.json_out { "json" } else { "txt" })?;
        let head: String = body.chars().take(1500).collect();
        return Ok(format!(
            "{head}\n… ({} bytes, offloaded)\n{}",
            body.len(),
            artifact_hint(&path)
        ));
    }
    Ok(body)
}

/// HEAD (then GET on 405/501) each cross-origin link from the driver process.
/// The page cannot read cross-origin statuses (CORS), so this is the only
/// honest way; no browser cookies are sent.
async fn check_cross_origin(report: &mut Value) {
    let list: Vec<Value> = report
        .pointer("/links/cross_origin")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let proxy = crate::browser::proxy::env_blade_proxy().ok().flatten();
    let Ok(client) = super::files::body_client(proxy) else {
        return;
    };
    let mut checked = 0u64;
    let mut broken = Vec::new();
    for l in &list {
        let Some(url) = l.get("url").and_then(Value::as_str) else {
            continue;
        };
        let probe = async {
            let r = client.head(url).send().await?;
            if matches!(r.status().as_u16(), 405 | 501) {
                return client.get(url).send().await;
            }
            Ok(r)
        };
        checked += 1;
        match tokio::time::timeout(std::time::Duration::from_secs(6), probe).await {
            Ok(Ok(r)) if r.status().as_u16() < 400 => {}
            Ok(Ok(r)) => broken.push(
                json!({"url": url, "selector": l["selector"], "status": r.status().as_u16()}),
            ),
            Ok(Err(e)) => broken.push(
                json!({"url": url, "selector": l["selector"], "status": 0, "error": e.without_url().to_string()}),
            ),
            Err(_) => broken
                .push(json!({"url": url, "selector": l["selector"], "status": 0, "error": "timeout"})),
        }
    }
    if let Some(links) = report.get_mut("links").and_then(Value::as_object_mut) {
        let base = links.get("checked").and_then(Value::as_u64).unwrap_or(0);
        links.insert("checked".into(), json!(base + checked));
        if let Some(b) = links.get_mut("broken").and_then(Value::as_array_mut) {
            b.extend(broken);
        }
        links.remove("cross_origin");
    }
}

fn take(list: &Value, opts: &AuditOpts) -> Vec<Value> {
    let all = list.as_array().cloned().unwrap_or_default();
    if opts.full {
        all
    } else {
        all.into_iter().take(opts.top).collect()
    }
}

fn broken_count(report: &Value) -> usize {
    report
        .pointer("/links/broken")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn trim_json(report: &Value, opts: &AuditOpts) -> Value {
    let mut findings = serde_json::Map::new();
    for (key, _) in CATEGORIES {
        findings.insert(
            (*key).into(),
            Value::Array(take(&report["findings"][*key], opts)),
        );
    }
    let mut links = report["links"].clone();
    let trimmed = Value::Array(take(&links["broken"], opts));
    links["broken"] = trimmed;
    if let Some(o) = links.as_object_mut() {
        o.remove("cross_origin");
    }
    let mut counts = report["counts"].clone();
    counts["broken_links"] = json!(broken_count(report));
    json!({
        "url": report["url"], "viewport": report["viewport"], "counts": counts,
        "findings": findings, "links": links, "full": opts.full,
    })
}

pub(super) fn format_text(report: &Value, opts: &AuditOpts) -> String {
    let vp = &report["viewport"];
    let n = |v: &Value| v.as_u64().unwrap_or(0);
    let mut out = format!(
        "audit {} (viewport {}x{} dpr {}, scrollWidth {})\n",
        report["url"].as_str().unwrap_or(""),
        n(&vp["width"]),
        n(&vp["height"]),
        vp["dpr"],
        n(&vp["scroll_width"]),
    );
    let mut summary: Vec<String> = CATEGORIES
        .iter()
        .map(|(k, label)| format!("{label} {}", n(&report["counts"][*k])))
        .collect();
    let links = &report["links"];
    if links["mode"] == "off" {
        summary.push("links off".into());
    } else {
        summary.push(format!(
            "broken-links {} of {} checked",
            broken_count(report),
            n(&links["checked"])
        ));
    }
    out.push_str(&format!("summary: {}\n", summary.join(", ")));
    for (key, label) in CATEGORIES {
        let total = n(&report["counts"][*key]) as usize;
        if total == 0 {
            continue;
        }
        let shown = take(&report["findings"][*key], opts);
        out.push_str(&format!("{label} ({total}):\n"));
        for f in &shown {
            out.push_str(&format!(
                "  {} - {}\n",
                f["selector"].as_str().unwrap_or("?"),
                f["reason"].as_str().unwrap_or("")
            ));
        }
        if total > shown.len() {
            out.push_str(&format!(
                "  (+{} more; full=true for all)\n",
                total - shown.len()
            ));
        }
    }
    let broken = take(&links["broken"], opts);
    if !broken.is_empty() {
        out.push_str(&format!("broken-links ({}):\n", broken_count(report)));
        for b in &broken {
            let status = match b["status"].as_u64() {
                Some(0) | None => b["error"].as_str().unwrap_or("no response").to_string(),
                Some(s) => s.to_string(),
            };
            out.push_str(&format!(
                "  {} - {status} (at {})\n",
                b["url"].as_str().unwrap_or("?"),
                b["selector"].as_str().unwrap_or("?")
            ));
        }
        if broken_count(report) > broken.len() {
            out.push_str(&format!(
                "  (+{} more; full=true for all)\n",
                broken_count(report) - broken.len()
            ));
        }
    }
    if let Some(note) = links["note"].as_str().filter(|s| !s.is_empty()) {
        out.push_str(&format!("note: {note}\n"));
    }
    if n(&links["skipped"]) > 0 {
        out.push_str(&format!(
            "note: {} same-origin links not checked (max_links cap)\n",
            n(&links["skipped"])
        ));
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        json!({
            "url": "http://t/", "viewport": {"width": 375, "height": 800, "dpr": 1, "scroll_width": 520},
            "counts": {"contrast": 7, "names": 0, "duplicate_ids": 0, "overflow": 2, "targets": 1},
            "findings": {
                "contrast": (0..7).map(|i| json!({"selector": format!(".c{i}"), "reason": "contrast 2.00:1 < 4.5:1"})).collect::<Vec<_>>(),
                "names": [], "duplicate_ids": [],
                "overflow": [{"selector": "html", "reason": "document scrollWidth 520 > viewport width 375"},
                             {"selector": ".wide", "reason": "extends 145px past the viewport edge"}],
                "targets": [{"selector": "a.x", "reason": "10x10px < 24px"}],
            },
            "links": {"mode": "same-origin", "checked": 3, "broken": [{"url": "http://t/gone", "selector": "a.l", "status": 404}], "skipped": 0, "cross_origin": [], "note": ""},
        })
    }

    #[test]
    fn default_output_is_counts_plus_top_n() {
        let opts = parse_opts(&json!({})).unwrap();
        let t = format_text(&sample(), &opts);
        assert!(t.contains("summary: contrast 7, names 0, duplicate-ids 0, overflow 2, small-targets 1, broken-links 1 of 3 checked"), "{t}");
        assert!(t.contains(".c4 "), "top 5 shown: {t}");
        assert!(!t.contains(".c5 "), "cap respected: {t}");
        assert!(t.contains("(+2 more; full=true for all)"), "{t}");
        assert!(t.contains("http://t/gone - 404"), "{t}");
        assert!(
            !t.contains("names ("),
            "empty category is summary-only: {t}"
        );
    }

    #[test]
    fn full_lifts_the_cap_and_json_trims_by_default() {
        let full = parse_opts(&json!({"full": true})).unwrap();
        assert!(format_text(&sample(), &full).contains(".c6 "));
        let opts = parse_opts(&json!({"format": "json", "limit": 2})).unwrap();
        let j = trim_json(&sample(), &opts);
        assert_eq!(j["findings"]["contrast"].as_array().unwrap().len(), 2);
        assert_eq!(j["counts"]["contrast"], 7);
        assert_eq!(j["counts"]["broken_links"], 1);
    }

    #[test]
    fn options_validate() {
        assert_eq!(parse_opts(&json!({})).unwrap().min_target, 24);
        assert!(parse_opts(&json!({"links": "some"})).is_err());
        assert!(parse_opts(&json!({"min_target": 0})).is_err());
        assert!(parse_opts(&json!({"max_links": 999})).is_err());
    }
}
