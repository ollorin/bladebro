//! Parsing of `Page.screencastFrame` params (pure, never panics).

use base64::Engine;
use serde_json::Value;

/// What the recorder needs out of one screencast frame event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FrameEvent {
    /// `params.sessionId`: the screencast session id the ack must echo.
    pub ack_id: Option<i64>,
    /// Base64 JPEG, still encoded (decoded only for forwarded frames).
    pub data_b64: Option<String>,
    /// Capture time, epoch ms (from `metadata.timestamp`).
    pub ts_ms: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

fn ack_id(params: &Value) -> Option<i64> {
    let v = params.get("sessionId")?;
    v.as_i64().or_else(|| {
        let f = v.as_f64()?;
        (f.is_finite() && f.fract() == 0.0).then_some(f as i64)
    })
}

/// Convert `metadata.timestamp` (CDP TimeSinceEpoch, seconds) to epoch ms.
/// Only finite, positive values qualify; the caller falls back to the wall
/// clock, which is the SAME clock domain (epoch), so the Pacer's gap math
/// stays consistent (D6-7).
pub fn ts_ms_from_metadata(ts_secs: Option<f64>) -> Option<u64> {
    let s = ts_secs?;
    (s.is_finite() && s > 0.0).then(|| (s * 1000.0).round() as u64)
}

/// Borrowing variant (copies `data`); prefer [`parse_frame_owned`].
pub fn parse_frame(params: &Value) -> FrameEvent {
    parse_frame_owned(params.clone())
}

/// Consuming variant: `data` is moved out, not copied (frames are MBs).
pub fn parse_frame_owned(mut params: Value) -> FrameEvent {
    let ack = ack_id(&params);
    let data_b64 = match params.get_mut("data").map(Value::take) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    };
    let meta = params.get("metadata");
    let dim = |k: &str| {
        meta.and_then(|m| m.get(k))
            .and_then(Value::as_f64)
            .filter(|f| f.is_finite() && *f > 0.0)
            .map(|f| f.round() as u32)
    };
    FrameEvent {
        ack_id: ack,
        data_b64,
        ts_ms: ts_ms_from_metadata(
            meta.and_then(|m| m.get("timestamp"))
                .and_then(Value::as_f64),
        ),
        width: dim("deviceWidth"),
        height: dim("deviceHeight"),
    }
}

/// Decode a base64 JPEG payload.
pub fn decode_jpeg(b64: &str) -> Result<Vec<u8>, String> {
    if b64.is_empty() {
        return Err("empty frame".into());
    }
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("bad base64: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn full_params_parse() {
        let p = json!({"data": "/9j/", "sessionId": 7,
            "metadata": {"timestamp": 1700000000.1234, "deviceWidth": 1280, "deviceHeight": 720}});
        let f = parse_frame(&p);
        assert_eq!(f.ack_id, Some(7));
        assert_eq!(f.data_b64.as_deref(), Some("/9j/"));
        assert_eq!(f.ts_ms, Some(1700000000123));
        assert_eq!((f.width, f.height), (Some(1280), Some(720)));
        assert_eq!(parse_frame_owned(p), f);
    }

    #[test]
    fn session_id_shapes() {
        assert_eq!(parse_frame(&json!({"sessionId": 7.0})).ack_id, Some(7));
        assert_eq!(parse_frame(&json!({})).ack_id, None);
        assert_eq!(parse_frame(&json!({"sessionId": "7"})).ack_id, None);
        assert_eq!(parse_frame(&json!({"sessionId": 7.5})).ack_id, None);
    }

    #[test]
    fn data_must_be_a_string() {
        assert_eq!(parse_frame(&json!({"data": 5})).data_b64, None);
        assert_eq!(parse_frame(&json!({})).data_b64, None);
    }

    #[test]
    fn timestamp_rows() {
        let ts = |v: Value| parse_frame(&json!({"metadata": {"timestamp": v}})).ts_ms;
        assert_eq!(ts(json!(0)), None);
        assert_eq!(ts(json!(-1)), None);
        assert_eq!(ts(Value::Null), None); // NaN serializes to null
        assert_eq!(ts(json!("1.5")), None);
        assert_eq!(ts(json!(1.5)), Some(1500));
        assert_eq!(ts_ms_from_metadata(Some(f64::NAN)), None);
        assert_eq!(ts_ms_from_metadata(None), None);
    }

    #[test]
    fn decode_rows() {
        assert_eq!(decode_jpeg("/9j/").unwrap(), vec![0xFF, 0xD8, 0xFF]);
        assert!(decode_jpeg("!!!").is_err());
        assert_eq!(decode_jpeg(""), Err("empty frame".to_string()));
    }
}
