//! `vision` handler: screenshot (+ optional Set-of-Marks overlay).

use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::BladeError;
use crate::page::Page;

static MARK_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Above this many base64 chars a capture is offloaded to a PNG artifact
/// instead of being inlined (see H2 below).
const INLINE_LIMIT: usize = 900_000;

/// One vision capture, before it is shaped for a surface: MCP wraps it as
/// text + image content blocks, the CLI prints `image_path`.
pub struct VisionShot {
    pub text: String,
    /// Base64 PNG to inline.
    pub image: Option<String>,
    /// PNG the handler already wrote to an artifact file (oversized
    /// captures). Returned as a field, not only inside `text`: the CLI used
    /// to see no image and print no `image_path` for exactly these.
    pub image_path: Option<String>,
    pub is_error: bool,
}

impl VisionShot {
    fn error(text: String) -> Self {
        VisionShot {
            text,
            image: None,
            image_path: None,
            is_error: true,
        }
    }
}

/// Capture a screenshot of the current page and return it as an MCP image
/// content block (base64 PNG).
///
/// This is the `vision` tool (decision D5) — a rare fallback for canvas
/// content, exotic layouts, or when the structural model fails.
pub async fn handle_vision(
    id: Option<Value>,
    args: &Value,
    page: &mut Page,
) -> std::result::Result<Value, BladeError> {
    let shot = capture_vision(args, page).await?;
    Ok(vision_response(id, shot))
}

/// The MCP JSON-RPC result for a capture: a text block, the image block
/// when inlined, `isError` only on failure.
fn vision_response(id: Option<Value>, shot: VisionShot) -> Value {
    let mut content = vec![json!({ "type": "text", "text": shot.text })];
    if let Some(data) = shot.image {
        content.push(json!({
            "type": "image",
            "data": data,
            "mimeType": "image/png"
        }));
    }
    let mut result = json!({ "content": content });
    if shot.is_error {
        result["isError"] = json!(true);
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

/// Screenshot (+ optional Set-of-Marks overlay) as a [`VisionShot`]. Shared
/// by the MCP handler and the CLI.
pub async fn capture_vision(
    args: &Value,
    page: &mut Page,
) -> std::result::Result<VisionShot, BladeError> {
    super::validate::vision(args)?;
    let marks = args.get("marks").and_then(|m| m.as_bool()).unwrap_or(false);
    let full_page = args
        .get("full_page")
        .and_then(|m| m.as_bool())
        .unwrap_or(false);
    let mut note = String::new();
    let overlay_id = serde_json::to_string(&format!(
        "blade-marks-{}-{}",
        std::process::id(),
        MARK_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))?;
    let mut overlay_installed = false;

    // A tab that is not the frontmost one can stall captureScreenshot —
    // Chromium throttles occluded/background renderers, so the call burns
    // its full CDP timeout and vision fails (reproduced live: vision right
    // after open-tab timed out at 30s, twice, on the new tab). Activate
    // only when the target is not already its window's active tab (#89:
    // raising the window on macOS steals focus); best-effort (fails
    // harmlessly when the target is mid-detach).
    let _ = page.activate_if_needed().await;

    if marks {
        // V14: Set-of-Marks. Paint numbered ref badges on
        // visible elements — the refs match the structural
        // model exactly, so a vision-capable agent can say
        // "click e5" and the act tool just works.
        let items: Vec<(String, String)> = page
            .model()
            .elements()
            .iter()
            .map(|e| (e.ref_id.clone(), e.raw.sig.clone()))
            .collect();
        let items_js = serde_json::to_string(&items)?;
        let overlay = "((items)=>{".to_string()
            + "const d=document;if(!d||!d.body)return 0;"
            + &crate::page::perception::JS_PREAMBLE
            // Never remove a page-owned node merely because its ID matches.
            + &format!("const id={overlay_id};if(d.getElementById(id))throw new Error('marks ID collision');")
            + "const ov=d.createElement('div');ov.id=id;"
            + "ov.style.cssText='position:fixed;inset:0;pointer-events:none;z-index:2147483647;';"
            + "const vw=innerWidth,vh=innerHeight;"
            + "const all=deepAll(d,sel);"
            + "const sigs=new Map();const counts={};"
            // Sig = '' frame prefix (main doc) | role | shortName | rank,
            // rank counted over ALL matches (vis-failing included) — matches
            // the capture script (V25c). Only vis-passing elements are mapped
            // for marking, but the rank counts everything so sigs agree.
            + "for(let i=0;i<all.length;i++){const n=all[i];const r=role(n);if(r==='hidden')continue;"
            + "const nm=name(n,false);const key=r+'\\u0000'+nm;counts[key]=(counts[key]||0)+1;"
            + "if(!vis(n))continue;"
            + "sigs.set('|'+r+'|'+nm+'|'+counts[key],n);}"
            + "let marked=0;"
            + "for(const[ref,sig]of items){const el=sigs.get(sig);if(!el)continue;"
            + "const rect=el.getBoundingClientRect();"
            + "if(rect.bottom<0||rect.top>vh||rect.right<0||rect.left>vw)continue;"
            + "const b=d.createElement('div');b.textContent=ref;"
            + "b.style.cssText='position:fixed;left:'+Math.max(0,rect.x+rect.width/2-12)+'px;top:'+Math.max(0,rect.y+rect.height/2-8)+'px;background:rgba(220,0,110,0.92);color:#fff;font:bold 11px/14px monospace;padding:0 4px;border-radius:3px;border:1px solid #fff;';"
            + "ov.appendChild(b);marked++;}"
            + "d.body.appendChild(ov);return marked;})(" + &items_js + ")";
        let res = page
            .cdp_ref()
            .send(
                "Runtime.evaluate",
                Some(json!({
                    "expression": overlay,
                    "returnByValue": true,
                })),
            )
            .await?;
        if let Some(exc) = res.get("exceptionDetails") {
            let msg = exc
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(|d| d.as_str())
                .unwrap_or("overlay failed");
            note = format!(
                " (marks overlay error: {})",
                crate::platform::truncate_utf8(msg, 120)
            );
        } else {
            overlay_installed = true;
            let marked = res
                .get("result")
                .and_then(|r| r.get("value"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            note = format!(" ({marked} elements marked; badge refs match the structural model)");
        }
    }

    let cdp = page.cdp_ref();
    let (capture_params, extent) = capture_plan(cdp, full_page).await;
    note.push_str(&extent);
    if let Some(spec) = page.viewport() {
        note.push_str(&format!(
            " (emulated viewport {})",
            crate::page::viewport::describe(&spec)
        ));
    }
    let result = page
        .cdp_ref()
        .send("Page.captureScreenshot", Some(capture_params))
        .await;

    // Remove the overlay BEFORE processing the result —
    // the page must never keep our marks.
    if overlay_installed {
        let _ = page.cdp_ref().send("Runtime.evaluate", Some(json!({
            "expression": format!("(()=>{{const o=document.getElementById({overlay_id});if(o)o.remove();return true;}})()"),
            "returnByValue": true,
        }))).await;
    }

    match result {
        Ok(res) => {
            let data = res.get("data").and_then(|d| d.as_str()).unwrap_or("");
            Ok(shot_from_capture(data, &note))
        }
        // Propagate Closed so serve() can self-heal.
        Err(BladeError::Closed) => Err(BladeError::Closed),
        Err(e) => Ok(VisionShot::error(format!("\u{2717} error: {e}"))),
    }
}

/// Tallest full-page capture attempted (CSS px); Chrome's texture limit.
const MAX_FULL_PAGE_HEIGHT: f64 = 16_384.0;

/// Build the `Page.captureScreenshot` params and the text that says what is
/// captured: the visible viewport (default) or the whole scrollable page.
async fn capture_plan(cdp: &crate::cdp::CdpSession, full_page: bool) -> (Value, String) {
    let metrics = cdp.send("Page.getLayoutMetrics", None).await.ok();
    let dim = |path: &str| {
        metrics
            .as_ref()
            .and_then(|m| m.pointer(path))
            .and_then(|v| v.as_f64())
    };
    let vp = (
        dim("/cssVisualViewport/clientWidth"),
        dim("/cssVisualViewport/clientHeight"),
    );
    let content = (dim("/cssContentSize/width"), dim("/cssContentSize/height"));
    match (full_page, content) {
        (true, (Some(w), Some(h))) => {
            let capped = h.min(MAX_FULL_PAGE_HEIGHT);
            let tail = if capped < h {
                format!(", capped at {capped:.0} of {h:.0} px tall")
            } else {
                String::new()
            };
            (
                json!({
                    "format": "png",
                    "captureBeyondViewport": true,
                    "clip": {"x": 0, "y": 0, "width": w, "height": capped, "scale": 1},
                }),
                format!(" (full page {w:.0}x{capped:.0} CSS px{tail})"),
            )
        }
        (true, _) => (
            json!({"format": "png"}),
            " (viewport only - full-page size unavailable)".to_string(),
        ),
        (false, _) => (
            json!({"format": "png"}),
            match vp {
                (Some(w), Some(h)) => {
                    format!(" (viewport {w:.0}x{h:.0} CSS px; full_page=true for the whole page)")
                }
                _ => " (viewport)".to_string(),
            },
        ),
    }
}

/// Inline a captured PNG, or offload it when oversized.
fn shot_from_capture(data: &str, note: &str) -> VisionShot {
    if data.is_empty() {
        return VisionShot::error("Screenshot returned no data.".into());
    }
    // H2: multi-MB screenshots (tall pages, 4K full-page) inline
    // through the single stdout writer backpressure the whole
    // server (stdin unread, signals unhandled, idle timer frozen)
    // on a slow client. Offload big ones to a PNG artifact and
    // return the path; vision-capable agents read the file.
    if data.len() > INLINE_LIMIT {
        if let Some(bytes) = crate::cli::base64_decode(data) {
            if let Ok(path) = crate::artifacts::write_artifact_bytes(&bytes, "png") {
                return VisionShot {
                    text: format!(
                        "screenshot{note} ({} KB — too large to inline, saved to {path})",
                        bytes.len() / 1024
                    ),
                    image: None,
                    image_path: Some(path),
                    is_error: false,
                };
            }
        }
    }
    VisionShot {
        text: format!("screenshot{note}"),
        image: Some(data.to_string()),
        image_path: None,
        is_error: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdp::{CdpClient, CdpSession};
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    /// An oversized capture is saved by the handler; the path must come back
    /// as a field (the CLI maps it to `image_path`), while the MCP response
    /// keeps its exact text-only shape.
    #[test]
    fn oversized_capture_returns_its_saved_path() {
        // 1,000,000 base64 chars → 750,000 PNG-sized bytes, over the limit.
        let data = "AAAA".repeat(250_000);
        assert!(data.len() > INLINE_LIMIT);
        let shot = shot_from_capture(&data, "");
        let path = shot.image_path.clone().expect("saved path returned");
        assert!(shot.image.is_none() && !shot.is_error);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 750_000);
        assert!(shot.text.contains(&path), "{}", shot.text);

        let v = vision_response(Some(json!(7)), shot);
        assert_eq!(v["id"], 7);
        let content = v["result"]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1, "no inline image for an offloaded capture");
        assert!(content[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("screenshot (732 KB — too large to inline, saved to "));
        assert!(v["result"].get("isError").is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn small_and_failed_captures_keep_their_mcp_shape() {
        let v = vision_response(
            None,
            shot_from_capture("iVBORw0KGgo=", " (3 elements marked)"),
        );
        assert_eq!(
            v,
            json!({"jsonrpc":"2.0","id":null,"result":{"content":[
                {"type":"text","text":"screenshot (3 elements marked)"},
                {"type":"image","data":"iVBORw0KGgo=","mimeType":"image/png"}
            ]}})
        );
        let v = vision_response(None, shot_from_capture("", ""));
        assert_eq!(
            v,
            json!({"jsonrpc":"2.0","id":null,"result":{"content":[
                {"type":"text","text":"Screenshot returned no data."}
            ],"isError":true}})
        );
    }

    async fn session_with_metrics() -> (CdpSession, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            while let Some(Ok(m)) = ws.next().await {
                let Ok(req) = serde_json::from_str::<Value>(m.to_text().unwrap_or("")) else {
                    continue;
                };
                let res = json!({"id": req["id"], "result": {
                    "cssVisualViewport": {"clientWidth": 375.0, "clientHeight": 800.0},
                    "cssContentSize": {"width": 375.0, "height": 2400.0},
                }});
                if ws
                    .send(Message::Text(res.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        let client = CdpClient::connect(&format!("ws://{addr}")).await.unwrap();
        (CdpSession::root(client), server)
    }

    #[tokio::test]
    async fn viewport_capture_is_the_default_and_says_so() {
        let (cdp, server) = session_with_metrics().await;
        let (params, note) = capture_plan(&cdp, false).await;
        assert_eq!(params, json!({"format": "png"}));
        assert!(note.contains("viewport 375x800"), "{note}");
        server.abort();
    }

    #[tokio::test]
    async fn full_page_capture_clips_to_the_content_size_and_says_so() {
        let (cdp, server) = session_with_metrics().await;
        let (params, note) = capture_plan(&cdp, true).await;
        assert_eq!(params["captureBeyondViewport"], true);
        assert_eq!(params["clip"]["width"], 375.0);
        assert_eq!(params["clip"]["height"], 2400.0);
        assert!(note.contains("full page 375x2400"), "{note}");
        server.abort();
    }
}
