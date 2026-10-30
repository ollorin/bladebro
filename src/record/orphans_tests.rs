//! Orphan sweeper tests: the pure rule as a table, then real fake processes
//! (a copy-free `ffmpeg` symlink to /bin/sh so `ps` shows a program named
//! ffmpeg) proving only matching orphans die. Unix only.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::orphans::{classify, classify_as, list_procs, parse_ps, sweep_in, ProcInfo, Verdict};
use crate::record::encoder::test_util::{pid_gone, Scratch};
use crate::record::{build_args, EncodeSpec, FfmpegVersion, Format, FrameSize};

const NAME: &str = "demo-20250101T120000Z-a1b2c3.webm";
const HEAD: &str = "-hide_banner -nostats -loglevel error -y -f image2pipe -framerate 10 -c:v mjpeg -i pipe:0 -vf scale=1280:720:force_original_aspect_ratio=decrease:force_divisible_by=2,pad=1280:720:(ow-iw)/2:(oh-ih)/2,format=yuv420p -fps_mode passthrough -an";

fn line(prog: &str, muxer: &str, out: &str) -> String {
    format!("{prog} {HEAD} -c:v libvpx-vp9 -deadline realtime -f {muxer} {out}")
}

fn p(ppid: u32, cmd: String) -> ProcInfo {
    ProcInfo {
        pid: 4000,
        ppid,
        cmd,
    }
}

fn dead(_: u32) -> bool {
    false
}
fn live(_: u32) -> bool {
    true
}

#[test]
fn classify_table() {
    let d = Path::new("/tmp/x/recordings");
    let out = format!("/tmp/x/recordings/{NAME}");
    let o = |v: Verdict| v == Verdict::Orphan;
    // bladebro orphans
    assert!(o(classify(
        &p(1, line("/usr/bin/ffmpeg", "webm", &out)),
        d,
        live
    )));
    assert!(o(classify(
        &p(
            1,
            line(
                "ffmpeg",
                "mp4",
                "/tmp/x/recordings/a-20250101T120000Z-00ff00.mp4"
            )
        ),
        d,
        live
    )));
    assert!(o(classify(&p(4242, line("ffmpeg", "webm", &out)), d, dead)));
    assert_eq!(
        classify(&p(4242, line("ffmpeg", "webm", &out)), d, live),
        Verdict::Keep("parent alive")
    );
    // user's ffmpeg, same dir
    assert_eq!(
        classify(&p(1, "ffmpeg -i input.mov out.webm".into()), d, dead),
        Verdict::Keep("not a bladebro command")
    );
    assert_eq!(
        classify(&p(1, format!("ffmpeg -i input.mov {out}")), d, dead),
        Verdict::Keep("not a bladebro command")
    );
    assert_eq!(
        classify(
            &p(
                1,
                line(
                    "ffmpeg",
                    "webm",
                    "/tmp/other/demo-20250101T120000Z-a1b2c3.webm"
                )
            ),
            d,
            dead
        ),
        Verdict::Keep("outside recordings dir")
    );
    // a sub-directory is not the recordings dir
    assert_eq!(
        classify(
            &p(
                1,
                line("ffmpeg", "webm", &format!("/tmp/x/recordings/sub/{NAME}"))
            ),
            d,
            dead
        ),
        Verdict::Keep("outside recordings dir")
    );
    assert_eq!(
        classify(
            &p(1, line("ffmpeg", "webm", "/tmp/x/recordings/movie.webm")),
            d,
            dead
        ),
        Verdict::Keep("not a recording name")
    );
    for bad in [
        "/tmp/x/recordings/demo-20250101T120000Z-a1b2c3.mkv",
        "/tmp/x/recordings/demo-20250101T120000Z-A1B2C3.webm",
        "/tmp/x/recordings/-20250101T120000Z-a1b2c3.webm",
        "/tmp/x/recordings/demo-2025010TT120000Z-a1b2c3.webm",
    ] {
        assert_eq!(
            classify(&p(1, line("ffmpeg", "webm", bad)), d, dead),
            Verdict::Keep("not a recording name"),
            "{bad}"
        );
    }
    assert!(matches!(
        classify(&p(1, line("ffmpeg", "webm", "rec.webm")), d, dead),
        Verdict::Keep(_)
    ));
    for prog in ["ffprobe", "/usr/bin/ffmpegx", "xffmpeg"] {
        assert_eq!(
            classify(&p(1, line(prog, "webm", &out)), d, dead),
            Verdict::Keep("not ffmpeg"),
            "{prog}"
        );
    }
    for junk in ["", "   ", "-", "ffmpeg", "ffmpeg -f image2pipe -i pipe:0"] {
        assert!(
            matches!(classify(&p(1, junk.into()), d, dead), Verdict::Keep(_)),
            "{junk:?}"
        );
    }
    // `ps` string form with spaces in the dir and in the program path.
    let sp = Path::new("/tmp/x y/recordings");
    let sp_out = format!("/tmp/x y/recordings/{NAME}");
    assert!(o(classify(
        &p(1, line("/opt/my tools/ffmpeg", "webm", &sp_out)),
        sp,
        dead
    )));
    assert_eq!(
        classify(&p(1, line("ffmpeg", "webm", &sp_out)), d, dead),
        Verdict::Keep("outside recordings dir")
    );
}

#[test]
fn args_builder_and_classifier_cannot_drift() {
    let spec = EncodeSpec {
        format: Format::WebM,
        fps: 10,
        size: FrameSize {
            width: 1280,
            height: 720,
        },
    };
    let v = FfmpegVersion::Release {
        display: "7.0".into(),
        major: 7,
    };
    let out = std::env::temp_dir().join(NAME);
    let args = build_args(&spec, &v, &out).expect("args");
    let cmd = format!(
        "/usr/bin/ffmpeg {}",
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let dir = out.parent().expect("parent");
    assert_eq!(classify(&p(1, cmd), dir, dead), Verdict::Orphan);
    let spec = EncodeSpec {
        format: Format::Mp4,
        ..spec
    };
    let out = std::env::temp_dir().join("demo-20250101T120000Z-a1b2c3.mp4");
    let args = build_args(&spec, &v, &out).expect("args");
    let cmd = format!(
        "ffmpeg {}",
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    );
    assert_eq!(classify(&p(1, cmd), dir, dead), Verdict::Orphan);
}

#[test]
fn ps_output_is_parsed() {
    let t = "  101     1 /sbin/launchd\n 4000  3999 ffmpeg -i a b\nbogus line\n   7 x\n";
    let v = parse_ps(t);
    assert_eq!(v.len(), 2);
    assert_eq!(
        v[1],
        ProcInfo {
            pid: 4000,
            ppid: 3999,
            cmd: "ffmpeg -i a b".into()
        }
    );
}

#[test]
fn sweep_never_touches_init_or_itself_and_kills_only_orphans() {
    let d = Path::new("/tmp/x/recordings");
    let out = format!("/tmp/x/recordings/{NAME}");
    let me = std::process::id();
    let procs = vec![
        ProcInfo {
            pid: 1,
            ppid: 0,
            cmd: line("ffmpeg", "webm", &out),
        },
        ProcInfo {
            pid: me,
            ppid: 1,
            cmd: line("ffmpeg", "webm", &out),
        },
        ProcInfo {
            pid: 5001,
            ppid: 1,
            cmd: line("ffmpeg", "webm", &out),
        },
        ProcInfo {
            pid: 5002,
            ppid: 1,
            cmd: "ffmpeg -i in.mov out.webm".into(),
        },
    ];
    let killed = std::sync::Mutex::new(Vec::new());
    let r = sweep_in(
        d,
        &|| procs.clone(),
        &|p| killed.lock().expect("k").push(p.pid),
        &dead,
    );
    assert_eq!(*killed.lock().expect("k"), vec![5001]);
    assert!(r.contains(&(5001, Verdict::Orphan)));
}

/// SIGKILL + reap every pid this test started, always.
struct Cleanup(Vec<u32>);
impl Drop for Cleanup {
    fn drop(&mut self) {
        for p in &self.0 {
            // SAFETY: pids this test spawned.
            unsafe { libc::kill(*p as libc::pid_t, libc::SIGKILL) };
        }
    }
}

const FAKE_BODY: &str = "trap 'exit 0' TERM; while :; do sleep 0.2; done";

fn fake_args(out: &Path) -> Vec<String> {
    let mut a = vec![
        "-c".to_string(),
        FAKE_BODY.to_string(),
        "ffmpeg".to_string(),
    ];
    a.extend(HEAD.split(' ').map(String::from));
    a.extend(["-f".into(), "webm".into(), out.display().to_string()]);
    a
}

/// A process whose program is named `ffmpeg` and that ignores nothing but TERM.
fn spawn_direct(bin: &Path, args: &[String]) -> std::process::Child {
    Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn fake")
}

/// Same, but orphaned: an intermediate shell backgrounds it and exits, so the
/// fake is re-parented to init. Its pid comes back on stdout.
fn spawn_orphan(bin: &Path, args: &[String]) -> u32 {
    let mut sh = Command::new("/bin/sh")
        .args(["-c", r#""$@" >/dev/null 2>&1 </dev/null & echo $!"#, "_"])
        .arg(bin)
        .args(args)
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn intermediate");
    let mut first = String::new();
    BufReader::new(sh.stdout.take().expect("stdout"))
        .read_line(&mut first)
        .expect("pid line");
    sh.wait().expect("intermediate exits");
    first.trim().parse().expect("pid")
}

fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(8) {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out: {what}");
}

#[test]
fn fake_ffmpeg_sweep_kills_only_matching_orphans() {
    let s = Scratch::new();
    let root = std::fs::canonicalize(&s.0).expect("canon");
    let dir = root.join("recordings");
    let other = root.join("other");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::create_dir_all(&other).expect("other");
    let bin_dir = root.join("bin");
    std::fs::create_dir_all(&bin_dir).expect("bin");
    let bin = bin_dir.join("ffmpeg");
    std::os::unix::fs::symlink("/bin/sh", &bin).expect("symlink");
    let name = |tag: &str| format!("{tag}-20250101T120000Z-a1b2c3.webm");

    let mut spawned = Cleanup(Vec::new());
    // two matching orphans
    let o1 = spawn_orphan(&bin, &fake_args(&dir.join(name("o1"))));
    let o2 = spawn_orphan(&bin, &fake_args(&dir.join(name("o2"))));
    // matching argv, live parent (our child)
    let mut live_child = spawn_direct(&bin, &fake_args(&dir.join(name("live"))));
    // orphan, other dir
    let other_dir = spawn_orphan(&bin, &fake_args(&other.join(name("od"))));
    // orphan, user-like argv in our dir
    let user = spawn_orphan(
        &bin,
        &[
            "-c".into(),
            FAKE_BODY.into(),
            "ffmpeg".into(),
            "-i".into(),
            "in.mov".into(),
            dir.join(name("user")).display().to_string(),
        ],
    );
    spawned.0.extend([o1, o2, live_child.id(), other_dir, user]);

    // The listing must show our fakes as ffmpeg (argv0 override is by symlink).
    wait_until("fakes listed", || {
        let l = list_procs();
        [o1, o2, other_dir, user]
            .iter()
            .all(|pid| l.iter().any(|x| x.pid == *pid && x.ppid == 1))
    });

    let decided = sweep_in(
        &dir,
        &list_procs,
        &super::orphans::kill_orphan,
        &crate::platform::process_alive,
    );
    wait_until("orphans gone", || pid_gone(o1) && pid_gone(o2));
    let verdict = |pid: u32| {
        decided
            .iter()
            .find(|(p, _)| *p == pid)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(verdict(o1), Some(Verdict::Orphan));
    assert_eq!(verdict(o2), Some(Verdict::Orphan));
    assert_eq!(
        verdict(live_child.id()),
        Some(Verdict::Keep("parent alive"))
    );
    assert_eq!(
        verdict(other_dir),
        Some(Verdict::Keep("outside recordings dir"))
    );
    assert_eq!(verdict(user), Some(Verdict::Keep("not a bladebro command")));
    assert!(!pid_gone(live_child.id()), "live-parent ffmpeg survives");
    assert!(!pid_gone(other_dir), "other-dir ffmpeg survives");
    assert!(!pid_gone(user), "user ffmpeg survives");

    // Cleanup: kill everything this test started, then prove none remain.
    let _ = live_child.kill();
    let _ = live_child.wait();
    drop(spawned);
    wait_until("all fakes gone", || {
        [o1, o2, other_dir, user].iter().all(|p| pid_gone(*p))
    });
}

const REAP: &str = include_str!("../session_profile/reap.rs");

#[test]
fn reaper_hook_is_the_first_statement() {
    let i = REAP.find("pub fn reap_orphans() {").expect("reap_orphans");
    let body = REAP[i + "pub fn reap_orphans() {".len()..].trim_start();
    assert!(
        body.starts_with("crate::record::orphans::sweep();"),
        "{}",
        &body[..80]
    );
}

#[test]
fn sweeping_a_missing_dir_starts_nothing() {
    // No recordings dir: returns at once (canonicalize fails before any thread or ps).
    super::orphans::sweep_dir(std::env::temp_dir().join("bladebro-no-such-recordings-dir-9f3a"));
}

#[test]
fn bladebro_as_pid1_keeps_its_own_live_encoder() {
    // Container without init: our encoder has ppid 1 == our own pid.
    let d = std::path::Path::new("/rec");
    let out = format!("/rec/{NAME}");
    let proc = p(1, line("ffmpeg", "webm", &out));
    // Parent 1 is us: keep. Parent 1 is somebody else (real orphan): reap.
    proc_keep(classify_as(&proc, d, |_| false, 1));
    assert!(matches!(
        classify_as(
            &{
                let mut q = proc.clone();
                q.ppid = 1;
                q
            },
            d,
            |_| false,
            777
        ),
        Verdict::Orphan
    ));
}

fn proc_keep(v: Verdict) {
    assert!(matches!(v, Verdict::Keep("parent alive")), "{v:?}");
}
