//! Real ffmpeg end-to-end per format (05-03): ~3 s of JPEG frames in, then
//! ffprobe checks codec, dimensions and duration. Skips cleanly (with an
//! eprintln) when no ffmpeg is found, so CI without one stays green.

use std::path::PathBuf;
use std::process::Command;

use super::test_util::Scratch;
use super::{Encoder, EncoderConfig, ResolvedFfmpeg, FINISH_TIMEOUT};
use crate::record::ffmpeg::{probe, ENV_FFMPEG, PROBE_TIMEOUT};
use crate::record::values::{Format, FrameSize};

use super::EncodeSpec;

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// The override when it is a real absolute file, else the local install dir.
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

async fn run_real(format: Format, codec: &str) {
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
    let jpg = s.0.join("f.jpg");
    let gen = Command::new(&ff)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1001x641:rate=1",
            "-frames:v",
            "1",
            "-c:v",
            "mjpeg",
            "-pix_fmt",
            "yuvj420p",
            "-y",
        ])
        .arg(&jpg)
        .output();
    if !matches!(&gen, Ok(o) if o.status.success()) {
        eprintln!("skip: this ffmpeg cannot generate the test frame (no lavfi?)");
        return;
    }
    let bytes = std::fs::read(&jpg).unwrap();
    assert!(!bytes.is_empty());

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
    let out = s.0.join(format!("rec e2e.{}", format.ext()));
    let enc = Encoder::start(&cfg, &out).unwrap();
    #[cfg(unix)]
    let pid = enc.pid().unwrap();
    for i in 0..=30u64 {
        assert!(enc.send_frame(bytes.clone(), i * 100), "frame {i} dropped");
    }
    let r = enc.finish().await;
    assert!(r.ok(), "{}", r.verdict());
    assert_eq!(r.frames_dropped, 0);
    assert!(
        (31..=33).contains(&r.frames_written),
        "{}",
        r.frames_written
    );
    assert!(r.bytes > 0);
    #[cfg(unix)]
    assert!(super::test_util::pid_gone(pid));

    let probe_bin = ff.with_file_name(exe("ffprobe"));
    if !probe_bin.is_file() {
        eprintln!("skip probe asserts: no ffprobe next to ffmpeg");
        return;
    }
    let o = Command::new(&probe_bin)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,width,height:format=duration",
            "-of",
            "json",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let st = &j["streams"][0];
    let dur: f64 = j["format"]["duration"].as_str().unwrap().parse().unwrap();
    eprintln!(
        "real {} codec={} {}x{} duration={dur:.3}s bytes={} frames_written={} finalize_ms={}",
        format.ext(),
        st["codec_name"],
        st["width"],
        st["height"],
        r.bytes,
        r.frames_written,
        r.finalize_ms
    );
    assert_eq!(st["codec_name"], codec);
    assert_eq!(st["width"], 1280);
    assert_eq!(st["height"], 720);
    assert!((2.7..=3.3).contains(&dur), "duration {dur}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_webm_end_to_end() {
    run_real(Format::WebM, "vp9").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_mp4_end_to_end() {
    run_real(Format::Mp4, "h264").await;
}

/// What the kernel does when bladebro is kill -9'd: the pipe's write end
/// closes, ffmpeg reads EOF. With the exact Encoder argv and real frames,
/// ffmpeg must exit 0 on its own, quickly, and leave a playable file.
#[cfg(unix)]
#[test]
fn parent_death_eof_finalizes() {
    use std::io::Write;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let Some(ff) = real_ffmpeg() else {
        eprintln!("skip: no real ffmpeg (set {ENV_FFMPEG} or install to ~/.local/ffmpeg/bin)");
        return;
    };
    let info = probe(&ff, PROBE_TIMEOUT).expect("usable ffmpeg");
    if !info.encoders.has(Format::WebM.encoder()) {
        eprintln!("skip: no VP9 encoder");
        return;
    }
    let s = Scratch::new();
    let jpg = s.0.join("f.jpg");
    let gen = Command::new(&ff)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1001x641:rate=1",
            "-frames:v",
            "1",
            "-c:v",
            "mjpeg",
            "-pix_fmt",
            "yuvj420p",
            "-y",
        ])
        .arg(&jpg)
        .output();
    if !matches!(&gen, Ok(o) if o.status.success()) {
        eprintln!("skip: cannot generate the test frame");
        return;
    }
    let frame = std::fs::read(&jpg).expect("frame");
    let out = s.0.join("parent-death.webm");
    let spec = EncodeSpec {
        format: Format::WebM,
        fps: 10,
        size: FrameSize::default(),
    };
    let args = super::args::build_args(&spec, &info.version, &out).expect("args");
    let mut child = Command::new(&ff)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn ffmpeg");
    let mut stdin = child.stdin.take().expect("stdin");
    for _ in 0..20 {
        stdin.write_all(&frame).expect("frame");
    }
    let t = Instant::now();
    drop(stdin); // no finish(): the parent is "dead"
    let status = loop {
        if let Some(st) = child.try_wait().expect("wait") {
            break st;
        }
        if t.elapsed() > Duration::from_secs(2) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ffmpeg did not exit within 2 s of stdin EOF");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    eprintln!(
        "parent death: ffmpeg exited {status} {} ms after EOF",
        t.elapsed().as_millis()
    );
    assert!(status.success(), "{status}");
    assert!(std::fs::metadata(&out).expect("file").len() > 0);
    let probe_bin = ff.with_file_name(exe("ffprobe"));
    if probe_bin.is_file() {
        let o = Command::new(&probe_bin)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(&out)
            .output()
            .expect("ffprobe");
        let dur: f64 = String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse()
            .expect("duration");
        assert!(dur > 0.0, "duration {dur}");
        let d = Command::new(&ff)
            .args(["-v", "error", "-i"])
            .arg(&out)
            .args(["-f", "null", "-"])
            .output()
            .expect("decode");
        assert!(d.status.success(), "{}", String::from_utf8_lossy(&d.stderr));
    }
}
