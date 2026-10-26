//! Live tests of popup following (08.1) over the real binary's MCP stdio server: every popup kind
//! becomes a segment without switch-tab, the video returns to the opener when the popup closes,
//! per-tab mode gets one file per followed popup. All `#[ignore]`: they need Chrome + ffmpeg and
//! local fixtures, and run through `tools/record_sidecar/scenarios/popup_live.sh` (headless,
//! scratch home, 127.0.0.1 fixtures only). Harness inputs as in tests/record_follow.rs.

mod record_support;

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use record_support::*;
use serde_json::json;

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
        (r, g, b) if b > 200 && r < 60 && g < 100 => Tone::Blue,
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
            // Rows are `* ID`, `*R ID`, `*  ID`, ` R ID`, `  ID` (marker width
            // differs with and without a recording): current = the `*` rows.
            let cur = l.trim_start().starts_with('*');
            let rest = l.trim_start().trim_start_matches(['*', 'R']).trim();
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

const SKIP: &str = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/popup_live.sh)";

fn click(mcp: &mut Mcp, text: &str) {
    let (e, t) = mcp.call("act", json!({"action":"click","text":text}));
    assert!(!e && t.contains("new tab opened"), "click {text}: {t}");
}

fn tabs_text(mcp: &mut Mcp) -> String {
    let (e, t) = mcp.state(json!({"op":"tabs"}));
    assert!(!e, "{t}");
    t
}

/// Seconds from now until `state tabs` marks a tab other than `o` as the video tab (` R`).
fn video_delay(mcp: &mut Mcp, o: &str) -> Option<f64> {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(8) {
        let t = tabs_text(mcp);
        if t.lines().any(|l| l.starts_with(" R ") && !l.contains(o)) {
            return Some(t0.elapsed().as_secs_f64());
        }
        sleep(0.05);
    }
    None
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/popup_live.sh)"]
fn popup_link_follow_and_return() {
    let _ = SKIP;
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("ppl", &live_env(&[("BLADE_RECORD", "1")], &ff));
    assert!(!mcp.nav(&format!("{fix}/popper.html")).0);
    let o = current_id(&mut mcp);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"pl"}));
    assert!(!e, "{t}");
    sleep(1.0);
    click(&mut mcp, "open selfclose");
    // The popup closes itself 2.5 s after load: sample while it is still open.
    assert!(
        video_delay(&mut mcp, &o).is_some(),
        "video followed the popup"
    );
    let during = tabs_text(&mut mcp);
    eprintln!("RESULT popup tabs during:\n{during}");
    let rows: Vec<&str> = during.lines().collect();
    assert!(
        rows.iter().any(|l| l.starts_with("* ") && l.contains(&o)),
        "agent marker on the opener: {during}"
    );
    assert!(rows.iter().any(|l| l.starts_with(" R ")), "{during}");
    assert!(during.contains("(* agent, R video: differ;"), "{during}");
    sleep(3.5);
    let (e, stop) = mcp.state(json!({"op":"record-stop"}));
    eprintln!("RESULT popup link stop: {stop}");
    assert!(!e && stop.contains("recording saved: "), "{stop}");
    let segs = segments(&stop);
    assert_eq!(segs.len(), 3, "{stop}");
    assert_eq!(
        (segs[0].1.as_str(), segs[2].1.as_str()),
        (o.as_str(), o.as_str()),
        "{stop}"
    );
    let s = segs[1].1.clone();
    assert!(stop.contains(&format!("popup {s} closed at ")), "{stop}");
    let path = saved_path(&stop);
    assert_video(&path, &stop);
    let total = field_after(&stop, "duration ");
    let colors: Vec<Tone> = (0..3)
        .map(|k| color_at(&path, mid(&segs, k, total)))
        .collect();
    eprintln!("RESULT popup colors: {colors:?}");
    assert_eq!(colors, [Tone::Red, Tone::Blue, Tone::Red], "{stop}");
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/popup_live.sh)"]
fn popup_kinds_delay() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("ppk", &live_env(&[("BLADE_RECORD", "1")], &ff));
    for (kind, text, want) in [
        ("blank", "open blank", None),
        ("js", "open js", None),
        ("window", "open window", None),
        ("xorigin", "open xorigin", Some(Tone::Green)),
    ] {
        assert!(!mcp.nav(&format!("{fix}/popper.html")).0);
        let o = current_id(&mut mcp);
        let (e, t) = mcp.state(json!({"op":"record-start","name":format!("k{kind}")}));
        assert!(!e, "{t}");
        sleep(1.0);
        click(&mut mcp, text);
        let d = video_delay(&mut mcp, &o).expect("the video followed the popup within 8 s");
        eprintln!("RESULT follow delay {kind} {d:.2}");
        sleep(2.5);
        let (e, t) = mcp.call("act", json!({"action":"press","key":"Tab"}));
        assert!(!e, "{t}");
        sleep(1.5);
        // A window-feature popup is its own window: the opener is still visible there, the
        // agent's input does not re-front it, so the video stays on the popup until it closes.
        let stays = kind == "window";
        if stays {
            assert!(
                tabs_text(&mut mcp).contains("(* agent, R video: differ;"),
                "{kind}"
            );
        }
        let (e, stop) = mcp.state(json!({"op":"record-stop"}));
        eprintln!("RESULT popup {kind} stop: {stop}");
        assert!(!e, "{stop}");
        let segs = segments(&stop);
        assert!(segs.len() >= 2, "{kind}: popup segment present: {stop}");
        assert_eq!(segs[0].1, o);
        if !stays {
            assert_eq!(segs.last().expect("last").1, o, "{kind}: returned: {stop}");
        }
        let path = saved_path(&stop);
        assert_video(&path, &stop);
        if let Some(w) = want {
            let total = field_after(&stop, "duration ");
            assert_eq!(color_at(&path, mid(&segs, 1, total)), w, "{kind}: {stop}");
        }
        // close the popup so the next kind starts from one tab
        for (_, id, _) in tabs(&mut mcp) {
            if id != o {
                let _ = mcp.state(json!({"op":"close-tab","target_id":id}));
            }
        }
    }
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/popup_live.sh)"]
fn popup_chain() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn("ppc", &live_env(&[("BLADE_RECORD", "1")], &ff));
    assert!(!mcp.nav(&format!("{fix}/popper.html")).0);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"ch"}));
    assert!(!e, "{t}");
    sleep(1.0);
    let o = current_id(&mut mcp);
    click(&mut mcp, "open chain");
    sleep(1.5);
    let p = tabs(&mut mcp)
        .into_iter()
        .map(|t| t.1)
        .find(|id| *id != o)
        .expect("chain popup tab");
    let (e, t) = mcp.state(json!({"op":"switch-tab","target_id":p}));
    assert!(!e, "{t}");
    let (e, t) = mcp.call("act", json!({"action":"click","text":"open grandchild"}));
    assert!(!e && t.contains("new tab opened"), "{t}");
    let q = t
        .split("(tab ")
        .nth(1)
        .and_then(|r| r.split(')').next())
        .expect("popup tab id")
        .to_string();
    sleep(2.5);
    let (e, t) = mcp.state(json!({"op":"close-tab","target_id":q}));
    assert!(!e, "{t}");
    sleep(2.0);
    let (e, stop) = mcp.state(json!({"op":"record-stop"}));
    eprintln!("RESULT popup chain stop: {stop}");
    assert!(!e, "{stop}");
    let segs = segments(&stop);
    let ids: Vec<&str> = segs.iter().map(|s| s.1.as_str()).collect();
    let pi = ids.iter().position(|i| *i == p).expect("P segment");
    let qi = ids.iter().position(|i| *i == q).expect("Q segment");
    assert!(
        pi < qi && ids[pi + 1..].contains(&p.as_str()),
        "P, Q, P in order: {stop}"
    );
    let path = saved_path(&stop);
    assert_video(&path, &stop);
    let total = field_after(&stop, "duration ");
    assert_eq!(color_at(&path, mid(&segs, pi, total)), Tone::Blue, "{stop}");
    assert_eq!(
        color_at(&path, mid(&segs, qi, total)),
        Tone::Green,
        "{stop}"
    );
}

/// The per-file lines of a per-tab result: `<path> (bytes ... segments [...] ...)`.
fn file_lines(text: &str) -> Vec<String> {
    text.match_indices(" (bytes ")
        .map(|(i, _)| {
            let pre = &text[..i];
            let start = [
                pre.rfind("; ").map(|x| x + 2),
                pre.rfind("): ").map(|x| x + 3),
            ]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(0);
            let end = i + text[i..].find(')').map_or(0, |c| c + 1);
            text[start..end].to_string()
        })
        .collect()
}

fn line_path(line: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(line.split(" (bytes ").next().expect("path"))
}

#[test]
#[ignore = "live: needs Chrome + ffmpeg (tools/record_sidecar/scenarios/popup_live.sh)"]
fn popup_per_tab() {
    let Some(fix) = fix() else { return };
    let ff = ffmpeg();
    let mut mcp = Mcp::spawn(
        "ppt",
        &live_env(
            &[("BLADE_RECORD", "1"), ("BLADE_RECORD_TABS", "per-tab")],
            &ff,
        ),
    );
    assert!(!mcp.nav(&format!("{fix}/popper.html")).0);
    let (e, t) = mcp.state(json!({"op":"record-start","name":"pt"}));
    assert!(!e, "{t}");
    sleep(1.0);
    click(&mut mcp, "open selfclose");
    sleep(5.5);
    let (e, stop) = mcp.state(json!({"op":"record-stop"}));
    eprintln!("RESULT popup per-tab stop: {stop}");
    assert!(!e && stop.contains("(per-tab, 2 files)"), "{stop}");
    let lines = file_lines(&stop);
    assert_eq!(lines.len(), 2, "{stop}");
    for (i, l) in lines.iter().enumerate() {
        let path = line_path(l);
        assert_video(&path, l);
        let segs = segments(l);
        let total = field_after(l, "duration ");
        let want = if i == 0 { Tone::Red } else { Tone::Blue };
        // the opener file holds red only, the popup file blue only
        for k in 0..segs.len() {
            assert_eq!(color_at(&path, mid(&segs, k, total)), want, "file {i}: {l}");
        }
    }
}
