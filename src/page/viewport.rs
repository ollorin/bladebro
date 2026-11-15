//! Viewport emulation (`act viewport`): CDP device-metrics override that
//! survives navigations, so a QA pass can check one page at several widths.
//!
//! The override lives on the CDP target, so a navigation keeps it; the spec is
//! also remembered on [`Page`] and re-applied when the session moves to another
//! tab. It is never applied unless the caller asks, so stealth defaults are
//! untouched.

use serde_json::{json, Value};

use super::Page;
use crate::cdp::CdpSession;
use crate::error::{BladeError, Result};

const MIN_DIM: u64 = 100;
const MAX_DIM: u64 = 10_000;

/// A requested device-metrics override.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportSpec {
    pub width: u32,
    pub height: u32,
    pub dpr: f64,
    pub mobile: bool,
}

/// Outcome of [`parse_args`]: set an override, or clear it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewportRequest {
    Set(ViewportSpec),
    Reset,
}

/// Parse `act viewport` arguments: `width`+`height` (both required), optional
/// `dpr` (0.5-5, default 1) and `mobile`; or `reset:true` on its own.
pub fn parse_args(args: &Value) -> Result<ViewportRequest> {
    let reset = args.get("reset").and_then(Value::as_bool).unwrap_or(false);
    let has = |k: &str| args.get(k).is_some();
    if reset {
        if has("width") || has("height") || has("dpr") || has("mobile") {
            return Err(BladeError::Usage(
                "viewport reset takes no width/height/dpr/mobile — drop them or drop reset".into(),
            ));
        }
        return Ok(ViewportRequest::Reset);
    }
    let dim = |k: &str| -> Result<u32> {
        let n = args.get(k).and_then(Value::as_u64).ok_or_else(|| {
            BladeError::Usage(format!(
                "viewport needs integer width and height (CSS px) — `act viewport --width 375 --height 800`, or `--reset`; missing/invalid {k}"
            ))
        })?;
        if !(MIN_DIM..=MAX_DIM).contains(&n) {
            return Err(BladeError::Usage(format!(
                "viewport {k} must be {MIN_DIM}-{MAX_DIM} CSS px, got {n}"
            )));
        }
        Ok(n as u32)
    };
    let width = dim("width")?;
    let height = dim("height")?;
    let dpr = match args.get("dpr") {
        None => 1.0,
        Some(v) => v
            .as_f64()
            .filter(|d| (0.5..=5.0).contains(d))
            .ok_or_else(|| BladeError::Usage("viewport dpr must be a number 0.5-5".into()))?,
    };
    let mobile = match args.get("mobile") {
        None => false,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| BladeError::Usage("viewport mobile must be a boolean".into()))?,
    };
    Ok(ViewportRequest::Set(ViewportSpec {
        width,
        height,
        dpr,
        mobile,
    }))
}

/// `Emulation.setDeviceMetricsOverride` params for a spec.
pub fn override_params(spec: &ViewportSpec) -> Value {
    json!({
        "width": spec.width,
        "height": spec.height,
        "deviceScaleFactor": spec.dpr,
        "mobile": spec.mobile,
    })
}

/// Apply the override to a target.
pub async fn apply(cdp: &CdpSession, spec: &ViewportSpec) -> Result<()> {
    cdp.send(
        "Emulation.setDeviceMetricsOverride",
        Some(override_params(spec)),
    )
    .await?;
    Ok(())
}

/// Remove the override from a target.
pub async fn clear(cdp: &CdpSession) -> Result<()> {
    cdp.send("Emulation.clearDeviceMetricsOverride", None)
        .await?;
    Ok(())
}

/// What the page itself reports: `(innerWidth, innerHeight, devicePixelRatio)`.
pub async fn readback(cdp: &CdpSession) -> Result<(u64, u64, f64)> {
    let res = cdp
        .send(
            "Runtime.evaluate",
            Some(json!({
                "expression": "[innerWidth, innerHeight, devicePixelRatio]",
                "returnByValue": true,
            })),
        )
        .await?;
    let v = res
        .pointer("/result/value")
        .and_then(Value::as_array)
        .ok_or_else(|| BladeError::Other("viewport readback returned no value".into()))?;
    let num = |i: usize| v.get(i).and_then(Value::as_f64).unwrap_or(0.0);
    Ok((num(0) as u64, num(1) as u64, num(2)))
}

/// One-line description, e.g. `375x800 dpr=2 mobile`.
pub fn describe(spec: &ViewportSpec) -> String {
    format!(
        "{}x{} dpr={}{}",
        spec.width,
        spec.height,
        spec.dpr,
        if spec.mobile { " mobile" } else { "" }
    )
}

impl Page {
    /// The active emulated viewport, if one was set and not reset.
    pub fn viewport(&self) -> Option<ViewportSpec> {
        self.viewport
    }

    /// Set the emulated viewport (persists across navigations and tab switches
    /// until [`Page::reset_viewport`]) and return the page's own readback.
    pub async fn set_viewport(&mut self, spec: ViewportSpec) -> Result<(u64, u64, f64)> {
        apply(&self.cdp, &spec).await?;
        self.viewport = Some(spec);
        readback(&self.cdp).await
    }

    /// Clear the emulated viewport.
    pub async fn reset_viewport(&mut self) -> Result<(u64, u64, f64)> {
        clear(&self.cdp).await?;
        self.viewport = None;
        readback(&self.cdp).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_set_with_defaults_and_options() {
        let r = parse_args(&json!({"width": 375, "height": 800})).unwrap();
        assert_eq!(
            r,
            ViewportRequest::Set(ViewportSpec {
                width: 375,
                height: 800,
                dpr: 1.0,
                mobile: false
            })
        );
        let r = parse_args(&json!({"width": 390, "height": 844, "dpr": 3, "mobile": true}));
        assert!(matches!(r, Ok(ViewportRequest::Set(s)) if s.dpr == 3.0 && s.mobile));
    }

    #[test]
    fn reset_is_exclusive_and_dimensions_are_bounded() {
        assert_eq!(
            parse_args(&json!({"reset": true})).unwrap(),
            ViewportRequest::Reset
        );
        assert!(parse_args(&json!({"reset": true, "width": 300})).is_err());
        assert!(parse_args(&json!({"width": 375})).is_err());
        assert!(parse_args(&json!({"width": 10, "height": 800})).is_err());
        assert!(parse_args(&json!({"width": 375, "height": 99999})).is_err());
        assert!(parse_args(&json!({"width": 375, "height": 800, "dpr": 9})).is_err());
        assert!(parse_args(&json!({"width": 375, "height": 800, "mobile": "yes"})).is_err());
    }

    #[test]
    fn override_params_match_the_cdp_contract() {
        let p = override_params(&ViewportSpec {
            width: 375,
            height: 800,
            dpr: 2.0,
            mobile: true,
        });
        assert_eq!(p["width"], 375);
        assert_eq!(p["height"], 800);
        assert_eq!(p["deviceScaleFactor"], 2.0);
        assert_eq!(p["mobile"], true);
    }
}
