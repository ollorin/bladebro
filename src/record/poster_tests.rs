//! Tests for `poster.rs`: argv shape, PNG magic, honest failures with the
//! poster file removed, bounded timeout, and real ffmpeg (skipped when absent).
//! Fake encoder scripts are named `fake-enc-poster-*` (see test_util.rs).

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use super::poster::{is_png, make_poster, make_poster_with, poster_args, POSTER_SUFFIX};
use crate::record::encoder::test_util::{fake_ffmpeg, Scratch};
use crate::record::encoder::{real_tests, EncodeSpec, Encoder, EncoderConfig, FINISH_TIMEOUT};
use crate::record::ffmpeg::{probe, PROBE_TIMEOUT};
use crate::record::values::{Format, FrameSize};

fn os(v: &[&str]) -> Vec<OsString> {
    v.iter().map(OsString::from).collect()
}

fn poster_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(POSTER_SUFFIX))
        .collect()
}

#[test]
fn args_seek_tail_shape() {
    let v = Path::new("/d/clip.webm");
    let o = Path::new("/d/clip.poster.png");
    let mut want = os(&[
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-sseof",
        "-1",
        "-i",
    ]);
    want.push(v.into());
    want.extend(os(&[
        "-map", "0:v:0", "-an", "-update", "1", "-f", "image2", "-c:v", "png", "-y",
    ]));
    want.push(o.into());
    assert_eq!(poster_args(v, o, true), want);
}

#[test]
fn args_without_seek() {
    let v = Path::new("/d/clip.webm");
    let o = Path::new("/d/clip.poster.png");
    let got = poster_args(v, o, false);
    assert_eq!(got.len(), poster_args(v, o, true).len() - 2);
    assert!(!got.iter().any(|a| a == "-sseof"));
    assert_eq!(got[4], OsString::from("-i"));
}

#[test]
fn args_paths_are_single_items() {
    let v = Path::new("/d/my clip; rm -rf.webm");
    let o = Path::new("/d/my clip.poster.png");
    let got = poster_args(v, o, true);
    assert!(got.contains(&OsString::from(v)));
    assert!(got.contains(&OsString::from(o)));
    assert_eq!(got.last().unwrap(), &OsString::from(o));
}

#[test]
fn png_magic() {
    assert!(is_png(b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"));
    assert!(!is_png(b"GIF89a......"));
    assert!(!is_png(b"\x89PNG"));
    assert!(!is_png(b""));
}

#[tokio::test]
async fn no_png_encoder_is_an_honest_error_and_no_file() {
    let s = Scratch::new();
    let video = s.0.join("clip.webm");
    let ff = fake_ffmpeg(&s.0.join("ffmpeg-not-run"), &["libvpx-vp9"]);
    let err = make_poster(&ff, &video).await.unwrap_err();
    assert!(err.contains("no png encoder"), "{err}");
    assert!(poster_files(&s.0).is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn fake_failure_removes_the_file() {
    let s = Scratch::new();
    let script = s.script("fake-enc-poster-fail", "exit 1");
    let video = s.0.join("clip.webm");
    let ff = fake_ffmpeg(&script, &["png"]);
    let err = make_poster(&ff, &video).await.unwrap_err();
    assert!(err.contains("exited"), "{err}");
    assert!(poster_files(&s.0).is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn fake_timeout_kills_and_removes() {
    let s = Scratch::new();
    let script = s.script("fake-enc-poster-hang", "exec sleep 30");
    let video = s.0.join("clip.webm");
    let ff = fake_ffmpeg(&script, &["png"]);
    let started = std::time::Instant::now();
    let err = make_poster_with(&ff, &video, Duration::from_millis(300))
        .await
        .unwrap_err();
    assert!(err.contains("timed out"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(poster_files(&s.0).is_empty());
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while std::process::Command::new("pgrep")
        .args(["-f", "fake-enc-poster-hang"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        assert!(
            std::time::Instant::now() < deadline,
            "fake encoder survived"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(unix)]
#[tokio::test]
async fn fake_success_needs_png_magic() {
    let s = Scratch::new();
    let script = s.script(
        "fake-enc-poster-text",
        "for a; do last=\"$a\"; done\nprintf notpng > \"$last\"",
    );
    let video = s.0.join("clip.webm");
    let ff = fake_ffmpeg(&script, &["png"]);
    let err = make_poster(&ff, &video).await.unwrap_err();
    assert!(err.contains("not a PNG"), "{err}");
    assert!(poster_files(&s.0).is_empty());
}

/// Real ffmpeg: ~2 s of the test frame into a finished video, then the poster.
async fn real_poster(format: Format) {
    let Some(ff) = real_tests::real_ffmpeg() else {
        eprintln!("skip: no real ffmpeg (set BLADE_FFMPEG or install to ~/.local/ffmpeg/bin)");
        return;
    };
    let info = probe(&ff, PROBE_TIMEOUT).expect("usable ffmpeg");
    if !info.encoders.has(format.encoder()) || !info.encoders.has("png") {
        eprintln!("skip: no {} or png encoder", format.encoder());
        return;
    }
    let s = Scratch::new();
    let Some(jpg) = real_tests::make_test_jpeg(&ff, &s.0) else {
        eprintln!("skip: this ffmpeg cannot generate the test frame");
        return;
    };
    let bytes = std::fs::read(&jpg).unwrap();
    let cfg = EncoderConfig {
        ffmpeg: crate::record::ResolvedFfmpeg {
            path: ff.clone(),
            version: info.version,
            encoders: info.encoders,
        },
        spec: EncodeSpec {
            format,
            fps: 10,
            size: FrameSize::default(),
        },
        queue_len: 64,
        finish_timeout: FINISH_TIMEOUT,
    };
    let video = s.0.join(format!("clip.{}", format.ext()));
    let enc = Encoder::start(&cfg, &video).unwrap();
    for i in 0..=20u64 {
        assert!(enc.send_frame(bytes.clone(), i * 100), "frame {i} dropped");
    }
    let r = enc.finish().await;
    assert!(r.ok(), "{}", r.verdict());

    let out = make_poster(&cfg.ffmpeg, &video).await.expect("poster");
    assert_eq!(
        out.file_name().unwrap(),
        format!("clip{POSTER_SUFFIX}").as_str()
    );
    let head = std::fs::read(&out).unwrap();
    assert!(is_png(&head), "poster is not a PNG");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&out).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let probe_bin = ff.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    if !probe_bin.is_file() {
        eprintln!("skip size asserts: no ffprobe next to ffmpeg");
        return;
    }
    let o = std::process::Command::new(&probe_bin)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
        ])
        .arg(&video)
        .output()
        .unwrap();
    let video_size = String::from_utf8_lossy(&o.stdout).trim().to_string();
    let o = std::process::Command::new(&probe_bin)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
        ])
        .arg(&out)
        .output()
        .unwrap();
    let poster_size = String::from_utf8_lossy(&o.stdout).trim().to_string();
    assert_eq!(poster_size, video_size);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_poster_webm() {
    real_poster(Format::WebM).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_poster_mp4() {
    real_poster(Format::Mp4).await;
}
