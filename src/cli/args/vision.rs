//! The `vision` command parser — `[--marks] [--full-page | --viewport]`.

use crate::error::{BladeError, Result};
use serde_json::{json, Value};

/// Parse `vision` args: [--marks] [--full-page | --viewport].
pub(crate) fn parse_vision_args(args: &[String]) -> Result<Value> {
    let mut marks = false;
    let mut full_page: Option<bool> = None;
    for a in args {
        match a.as_str() {
            "--marks" => marks = true,
            "--full-page" | "--viewport" => {
                let want = a == "--full-page";
                if full_page.is_some_and(|prev| prev != want) {
                    return Err(BladeError::Usage(
                        "vision: --full-page and --viewport are mutually exclusive".into(),
                    ));
                }
                full_page = Some(want);
            }
            s if s.starts_with("--") => {
                return Err(BladeError::Usage(format!(
                    "unknown flag {s} for vision — see 'bladebro help vision'"
                )))
            }
            s => {
                return Err(BladeError::Usage(format!(
                    "unexpected argument '{s}' for vision — bladebro vision [--marks] [--full-page|--viewport]"
                )))
            }
        }
    }
    Ok(json!({ "marks": marks, "full_page": full_page.unwrap_or(false) }))
}
