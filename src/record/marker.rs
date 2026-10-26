//! Marker note for `record-stop` (OUT-06, D10-2): the agent's free text is
//! cleaned before it is used anywhere. It is read by an LLM and shown in
//! files, so control characters cannot split a line, and bidi/zero-width
//! format characters cannot disguise what the note says.
//!
//! Module map:
//! - `clean_text`: control chars to spaces, format chars dropped, whitespace
//!   collapsed, bounded length (also used for timeline verdict lines).
//! - `MarkerIn` / `marker_from_args`: the `value` param of `record-stop`.
//!
//! Fork-only module (SYNC-01).

use serde_json::Value;

/// Longest note kept (chars, not bytes). Longer notes are cut with "...".
pub const MAX_MARKER_CHARS: usize = 200;

/// Zero-width and bidi format characters. They are dropped, not replaced:
/// they carry no visible text and can reorder what a reviewer reads.
fn is_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

/// Clean free text for a single output line: every control char becomes a
/// space, format chars are dropped, runs of whitespace collapse, the result is
/// trimmed and holds at most `max_chars` chars (a cut ends with "...").
pub fn clean_text(raw: &str, max_chars: usize) -> String {
    let spaced: String = raw
        .chars()
        .filter(|c| !is_format_char(*c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let joined = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() <= max_chars {
        return joined;
    }
    let keep = max_chars.saturating_sub(3);
    let head: String = joined.chars().take(keep).collect();
    format!("{}...", head.trim_end())
}

/// What the `value` param of a `record-stop` call means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerIn {
    /// No marker given (absent or null).
    None,
    /// A cleaned, non-empty note.
    Note(String),
    /// Given but unusable; the stop still happens and says why.
    Bad(&'static str),
}

/// Read the marker from a tool call's arguments.
pub fn marker_from_args(args: &Value) -> MarkerIn {
    match args.get("value") {
        None | Some(Value::Null) => MarkerIn::None,
        Some(Value::String(s)) => {
            let note = clean_text(s, MAX_MARKER_CHARS);
            if note.is_empty() {
                MarkerIn::Bad("empty note")
            } else {
                MarkerIn::Note(note)
            }
        }
        Some(_) => MarkerIn::Bad("value must be a string"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clean_keeps_plain_text() {
        let t = "FAIL: login button did nothing";
        assert_eq!(clean_text(t, 200), t);
    }

    #[test]
    fn clean_replaces_controls_and_newlines() {
        assert_eq!(clean_text("a\nb\r\tc\u{0007}d", 200), "a b c d");
    }

    #[test]
    fn clean_drops_bidi_and_zero_width() {
        assert_eq!(
            clean_text("ab\u{202E}cd\u{200B}e\u{FEFF}\u{2066}f", 200),
            "abcdef"
        );
    }

    #[test]
    fn clean_collapses_whitespace_and_trims() {
        assert_eq!(clean_text("  a   b  ", 200), "a b");
    }

    #[test]
    fn clean_truncates_on_char_boundary() {
        let cyr: String = "ж".repeat(300);
        let out = clean_text(&cyr, MAX_MARKER_CHARS);
        assert_eq!(out.chars().count(), 200);
        assert!(out.ends_with("..."));
        let emoji: String = "\u{1F600}".repeat(300);
        let out = clean_text(&emoji, MAX_MARKER_CHARS);
        assert_eq!(out.chars().count(), 200);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn marker_absent_or_null_is_none() {
        assert_eq!(marker_from_args(&json!({})), MarkerIn::None);
        assert_eq!(marker_from_args(&json!({"value": null})), MarkerIn::None);
    }

    #[test]
    fn marker_string_is_cleaned() {
        assert_eq!(
            marker_from_args(&json!({"value": " x\ny "})),
            MarkerIn::Note("x y".into())
        );
    }

    #[test]
    fn marker_empty_after_clean_is_bad() {
        assert_eq!(
            marker_from_args(&json!({"value": "\n\u{200B} "})),
            MarkerIn::Bad("empty note")
        );
    }

    #[test]
    fn marker_non_string_is_bad() {
        assert_eq!(
            marker_from_args(&json!({"value": 5})),
            MarkerIn::Bad("value must be a string")
        );
        assert_eq!(
            marker_from_args(&json!({"value": {}})),
            MarkerIn::Bad("value must be a string")
        );
    }
}
