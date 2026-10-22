//! Live tests of tab following (08-01) over the real binary's MCP stdio server.
//! All `#[ignore]`: they need Chrome + ffmpeg and local fixtures, and run through
//! `tools/record_sidecar/scenarios/follow_live.sh` (headless, scratch home,
//! 127.0.0.1 fixtures only).
//!
//! Harness inputs (not product options): REC_HUB_FIX, REC_HUB_FFMPEG, REC_HUB_FFPROBE
//! as in tests/record_hub.rs; unset REC_HUB_FIX skips the live tests.

mod record_support;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use record_support::*;
use serde_json::json;

const IGNORE: &str = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/follow_live.sh)";

#[derive(Debug, PartialEq)]
enum Tone {
    Red,
    Blue,
    Green,
    Yellow,
    Other(u8, u8, u8),
}

fn tone(rgb: (u8, u8, u8)) -> Tone {
    match rgb {
        (r, g, b) if r > 200 && g < 60 && b < 60 => Tone::Red,
        (r, g, b) if b > 200 && r < 60 && g < 60 => Tone::Blue,
        (r, g, b) if g > 120 && r < 60 && b < 60 => Tone::Green,
        (r, g, b) if r > 200 && g > 160 && b < 60 => Tone::Yellow,
        (r, g, b) => Tone::Other(r, g, b),
    }
}

/// Average colour of the frame at `t` seconds (1x1 downscale).
fn color_at(path: &Path, t: f64) -> Tone {
    let out = Command::new(ffmpeg())
        .args(["-v", "error", "-ss", &format!("{t:.2}"), "-i"])
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale=1:1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("ffmpeg runs");
    assert!(out.stdout.len() >= 3, "no frame at {t}s in {path:?}");
    tone((out.stdout[0], out.stdout[1], out.stdout[2]))
}

/// Frames the file decodes to.
fn decoded_frames(path: &Path) -> u64 {
    let out = Command::new(ffprobe())
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .expect("ffprobe runs");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("frame count of {path:?}"))
}

/// `(offset_s, label)` from `segments [0.0 s tab A, 4.2 s tab B (no frames)]`.
fn segments(text: &str) -> Vec<(f64, String)> {
    let inner = text
        .split("segments [")
        .nth(1)
        .and_then(|r| r.split(']').next())
        .unwrap_or_else(|| panic!("segments in {text}"));
    inner
        .split(", ")
        .map(|p| {
            let (off, tab) = p.split_once(" s tab ").expect("segment shape");
            (off.parse().expect("offset"), tab.to_string())
        })
        .collect()
}

/// `(current, id, url)` per tab from `state tabs`.
fn tabs(mcp: &mut Mcp) -> Vec<(bool, String, String)> {
    let (e, t) = mcp.state(json!({"op":"tabs"}));
    assert!(!e, "tabs: {t}");
    t.lines()
        .filter_map(|l| {
            let cur = l.trim_start().starts_with('*');
            let rest = l.trim_start().trim_start_matches('*').trim();
            let id = rest.split_whitespace().next()?;
            if id.len() != 32 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let url = rest.rsplit(" — ").next().unwrap_or("").to_string();
            Some((cur, id.to_string(), url))
        })
        .collect()
}

fn current_id(mcp: &mut Mcp) -> String {
    tabs(mcp)
        .into_iter()
        .find(|t| t.0)
        .map(|t| t.1)
        .expect("a current tab")
}

fn opened_id(text: &str) -> String {
    text.split("tab ")
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
        .map(String::from)
        .unwrap_or_else(|| panic!("tab id in {text}"))
}

fn sleep(s: f64) {
    std::thread::sleep(Duration::from_secs_f64(s));
}

/// Midpoint of segment `k` on the video clock.
fn mid(segs: &[(f64, String)], k: usize, total: f64) -> f64 {
    let end = segs.get(k + 1).map_or(total, |s| s.0);
    (segs[k].0 + end) / 2.0
}

fn assert_video(path: &Path, text: &str) {
    let (codec, w, h) = probe(path).expect("ffprobe reads the file");
    eprintln!("RESULT ffprobe: {codec} {w}x{h}");
    assert_eq!((codec.as_str(), w, h), ("vp9", 1280, 720), "constant size");
    assert!(decodes_clean(path), "full decode must be clean");
    assert_eq!(
        decoded_frames(path) as f64,
        field_after(text, "frames "),
        "decoded frames equal the reported count: {text}"
    );
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/follow_live.sh)"]
fn follow_switch_open() {
    let _ = IGNORE;
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("fsw", &live_env(&[("BLADE_RECORD", "1")], &ff));
    assert!(!mcp.nav(&format!("{fix}/a.html")).0);
    let a = current_id(&mut mcp);
    let (e, start) = mcp.state(json!({"op":"record-start","name":"f"}));
    assert!(!e, "{start}");
    sleep(2.0);
    let (e, t) = mcp.state(json!({"op":"open-tab","url":format!("{fix}/b.html")}));
    assert!(!e, "{t}");
    let b = opened_id(&t);
    assert_eq!(current_id(&mut mcp), b);
    // A brand-new tab is followed once it has stayed in front for ~1.5 s.
    sleep(3.5);
    let (e, t) = mcp.state(json!({"op":"switch-tab","target_id":a}));
    assert!(!e, "{t}");
    assert_eq!(current_id(&mut mcp), a);
    sleep(2.0);
    let (e, stop) = mcp.state(json!({"op":"record-stop"}));
    eprintln!("RESULT follow stop: {stop}");
    assert!(!e && stop.contains("recording saved: "), "{stop}");
    let segs = segments(&stop);
    let ids: Vec<&str> = segs.iter().map(|s| s.1.as_str()).collect();
    assert_eq!(ids, [a.as_str(), b.as_str(), a.as_str()], "{stop}");
    assert!(
        segs[0].0 == 0.0 && segs[1].0 > 0.5 && segs[2].0 > segs[1].0 + 0.5,
        "{stop}"
    );
    let path = saved_path(&stop);
    assert_video(&path, &stop);
    let total = field_after(&stop, "duration ");
    let colors: Vec<Tone> = (0..3)
        .map(|k| color_at(&path, mid(&segs, k, total)))
        .collect();
    eprintln!("RESULT follow colors: {colors:?}");
    assert_eq!(colors, [Tone::Red, Tone::Blue, Tone::Red], "{stop}");
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/follow_live.sh)"]
fn follow_popup() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("fpop", &live_env(&[("BLADE_RECORD", "1")], &ff));
    assert!(!mcp.nav(&format!("{fix}/popup.html")).0);
    let opener = current_id(&mut mcp);
    // 1. A target=_blank link: the opener stays the agent's page and stays first.
    let (e, t) = mcp.state(json!({"op":"record-start","name":"p1"}));
    assert!(!e, "{t}");
    let (e, t) = mcp.call("act", json!({"action":"click","text":"open c"}));
    assert!(!e && t.contains("new tab opened"), "{t}");
    sleep(2.0);
    let (e, stop) = mcp.state(json!({"op":"record-stop"}));
    eprintln!("RESULT popup link stop: {stop}");
    assert!(!e, "{stop}");
    let segs = segments(&stop);
    assert_eq!(segs.len(), 1, "a link popup is not a segment: {stop}");
    assert_eq!(segs[0].1, opener);
    let path = saved_path(&stop);
    assert_eq!(color_at(&path, 1.0), Tone::Yellow, "opener only: {stop}");
    assert_video(&path, &stop);

    // 2. window.open with window features (a differently sized popup window).
    let before: Vec<String> = tabs(&mut mcp).into_iter().map(|t| t.1).collect();
    let (e, t) = mcp.state(json!({"op":"record-start","name":"p2"}));
    assert!(!e, "{t}");
    let (e, t) = mcp.call("act", json!({"action":"click","text":"window"}));
    assert!(!e, "{t}");
    sleep(1.5);
    let popup = tabs(&mut mcp)
        .into_iter()
        .map(|t| t.1)
        .find(|id| !before.contains(id))
        .expect("a new popup tab");
    let (e, t) = mcp.state(json!({"op":"switch-tab","target_id":popup}));
    assert!(!e, "{t}");
    sleep(2.0);
    let (e, stop) = mcp.state(json!({"op":"record-stop"}));
    eprintln!("RESULT popup window stop: {stop}");
    assert!(!e, "{stop}");
    let segs = segments(&stop);
    assert!(segs.len() >= 2, "{stop}");
    assert_eq!(segs.first().expect("first").1, opener, "{stop}");
    assert_eq!(segs.last().expect("last").1, popup, "{stop}");
    // Letterboxed into the configured size, whole file decodes.
    assert_video(&saved_path(&stop), &stop);
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/follow_live.sh)"]
fn follow_close_recorded() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("fcl", &live_env(&[("BLADE_RECORD", "1")], &ff));
    assert!(!mcp.nav(&format!("{fix}/a.html")).0);
    let a = current_id(&mut mcp);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"c"}));
    assert!(!e, "{t}");
    sleep(1.5);
    let (e, t) = mcp.state(json!({"op":"open-tab","url":format!("{fix}/b.html")}));
    assert!(!e, "{t}");
    let b = opened_id(&t);
    sleep(3.5);
    let (e, t) = mcp.state(json!({"op":"close-tab","target_id":b}));
    assert!(!e, "{t}");
    sleep(2.0);
    let (e, stop) = mcp.state(json!({"op":"record-stop"}));
    eprintln!("RESULT follow close stop: {stop}");
    assert!(
        !e,
        "closing the recorded tab must not fail the recording: {stop}"
    );
    assert!(stop.contains("recording saved: "), "{stop}");
    assert!(stop.contains("gone at "), "{stop}");
    let segs = segments(&stop);
    assert_eq!(segs.last().expect("last").1, a, "{stop}");
    let path = saved_path(&stop);
    let total = field_after(&stop, "duration ");
    let k = segs.len() - 2;
    assert_eq!(color_at(&path, mid(&segs, k, total)), Tone::Blue, "{stop}");
    assert_eq!(
        color_at(&path, mid(&segs, segs.len() - 1, total)),
        Tone::Red,
        "{stop}"
    );
    assert_video(&path, &stop);
}
