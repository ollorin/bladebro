//! Parse a `Target.getTargets` reply into the page rows the follower needs.

use serde_json::Value;

/// One page target with the tab that opened it (if Chrome knows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetRow {
    pub id: String,
    pub opener: Option<String>,
    /// Always true for parsed rows (only `type == "page"` is kept); explicit so
    /// callers never have to guess what the filter dropped.
    pub kind_page: bool,
    pub url: String,
}

/// Keep real pages only: service workers, browser UI (`chrome://`), devtools
/// and extension pages are never a recordable popup (R1).
pub fn parse_targets(v: &Value) -> Vec<TargetRow> {
    let Some(infos) = v["targetInfos"].as_array() else {
        return Vec::new();
    };
    infos
        .iter()
        .filter(|t| t["type"].as_str() == Some("page"))
        .filter_map(|t| {
            let url = t["url"].as_str().unwrap_or("");
            let ok = ["http://", "https://", "about:", "file:"]
                .iter()
                .any(|p| url.starts_with(p));
            if !ok {
                return None;
            }
            Some(TargetRow {
                id: t["targetId"].as_str()?.to_string(),
                opener: t["openerId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                kind_page: true,
                url: url.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_pages_with_opener_and_drops_the_rest() {
        let v = json!({"targetInfos": [
            {"targetId": "P", "type": "page", "url": "http://x/b.html", "openerId": "A"},
            {"targetId": "A", "type": "page", "url": "https://x/a.html"},
            {"targetId": "U", "type": "page", "url": "chrome://newtab/"},
            {"targetId": "S", "type": "service_worker", "url": "http://x/sw.js"},
            {"targetId": "D", "type": "page", "url": "devtools://devtools/x"}
        ]});
        let rows = parse_targets(&v);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (rows[0].id.as_str(), rows[0].opener.as_deref()),
            ("P", Some("A"))
        );
        assert_eq!(
            (rows[1].id.as_str(), rows[1].opener.as_deref()),
            ("A", None)
        );
        assert!(rows.iter().all(|r| r.kind_page));
    }

    #[test]
    fn malformed_reply_is_empty() {
        assert!(parse_targets(&json!({})).is_empty());
    }
}
