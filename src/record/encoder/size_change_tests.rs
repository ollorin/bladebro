//! Real ffmpeg: frames of two different JPEG sizes into ONE encoder (what a
//! tab switch to a differently sized tab or popup does). The output must keep
//! the configured size (letterboxed) and the full frame count. Skips with an
//! eprintln when ffmpeg/ffprobe/lavfi are missing.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::test_util::Scratch;
use super::{EncodeSpec, Encoder, EncoderConfig, ResolvedFfmpeg, FINISH_TIMEOUT};
use crate::record::ffmpeg::{probe, ENV_FFMPEG, PROBE_TIMEOUT};
use crate::record::values::{Format, FrameSize};

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn real_ffmpeg() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os(ENV_FFMPEG) {
        let p = PathBuf::from(v);
        if p.is_absolute() && p.is_file() {
            return Some(p);
        }
    }
    let p = crate::platform::home_dir()
        .join(".local/ffmpeg/bin")
        .join(exe("ffmpeg"));
    p.is_file().then_some(p)
}

fn jpeg(ff: &Path, dir: &Path, name: &str, size: &str) -> Option<Vec<u8>> {
    let out = dir.join(name);
    let r = Command::new(ff)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size={size}:rate=1"),
            "-frames:v",
            "1",
            "-c:v",
            "mjpeg",
            "-pix_fmt",
            "yuvj420p",
            "-y",
        ])
        .arg(&out)
        .output();
    if !matches!(&r, Ok(o) if o.status.success()) {
        return None;
    }
    std::fs::read(&out).ok().filter(|b| !b.is_empty())
}

async fn run(format: Format) {
    let Some(ff) = real_ffmpeg() else {
        eprintln!("skip: no real ffmpeg (set {ENV_FFMPEG} or install to ~/.local/ffmpeg/bin)");
        return;
    };
    let info = match probe(&ff, PROBE_TIMEOUT) {
        Ok(i) => i,
        Err(e) => panic!("ffmpeg at {} is present but unusable: {e}", ff.display()),
    };
    if !info.encoders.has(format.encoder()) {
        eprintln!("skip: this ffmpeg has no {} encoder", format.encoder());
        return;
    }
    let s = Scratch::new();
    let (Some(big), Some(small)) = (
        jpeg(&ff, &s.0, "big.jpg", "1280x720"),
        jpeg(&ff, &s.0, "small.jpg", "500x400"),
    ) else {
        eprintln!("skip: this ffmpeg cannot generate test frames (no lavfi?)");
        return;
    };
    let cfg = EncoderConfig {
        ffmpeg: ResolvedFfmpeg {
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
    let out = s.0.join(format!("size change.{}", format.ext()));
    let enc = Encoder::start(&cfg, &out).expect("start");
    for i in 0..20u64 {
        assert!(enc.send_frame(big.clone(), i * 100), "big frame {i}");
    }
    for i in 20..40u64 {
        assert!(enc.send_frame(small.clone(), i * 100), "small frame {i}");
    }
    let r = enc.finish().await;
    assert!(r.ok(), "{}", r.verdict());
    assert_eq!(r.frames_dropped, 0);

    let probe_bin = ff.with_file_name(exe("ffprobe"));
    if !probe_bin.is_file() {
        eprintln!("skip probe asserts: no ffprobe next to ffmpeg");
        return;
    }
    let o = Command::new(&probe_bin)
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,nb_read_frames",
            "-of",
            "json",
        ])
        .arg(&out)
        .output()
        .expect("ffprobe runs");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let j: serde_json::Value = serde_json::from_slice(&o.stdout).expect("json");
    let st = &j["streams"][0];
    eprintln!(
        "size-change {} {}x{} read_frames={} frames_written={}",
        format.ext(),
        st["width"],
        st["height"],
        st["nb_read_frames"],
        r.frames_written
    );
    assert_eq!(st["width"], 1280);
    assert_eq!(st["height"], 720);
    let decoded: u64 = st["nb_read_frames"]
        .as_str()
        .and_then(|n| n.parse().ok())
        .expect("frame count");
    assert_eq!(decoded, r.frames_written, "decoded count equals written");
    let dec = Command::new(&ff)
        .args(["-v", "error", "-i"])
        .arg(&out)
        .args(["-f", "null", "-"])
        .output()
        .expect("decode runs");
    assert!(
        dec.status.success() && dec.stderr.is_empty(),
        "full decode clean: {}",
        String::from_utf8_lossy(&dec.stderr)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_frame_sizes_webm() {
    run(Format::WebM).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_frame_sizes_mp4() {
    run(Format::Mp4).await;
}
