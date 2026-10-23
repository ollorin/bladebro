//! ffmpeg orphan sweeper (LIFE-03): remove an encoder that a dead bladebro
//! left behind, and nothing else.
//!
//! Why it can exist at all: ffmpeg exits by itself when its parent dies (stdin
//! EOF, measured in `.planning/phases/09-*/evidence/09-02-kill9.txt`), so an
//! orphan is the rare case (SIGSTOPped parent, stuck encode, a wedged pipe).
//! The sweep is the backstop for exactly that, run at every Chrome launch (one
//! hook line in `session_profile/reap.rs`, see .planning/HOOKS.md) and at every
//! record-start (`hub/guard.rs`).
//!
//! The rule (`classify`), ALL of these must hold or the process is `Keep`:
//! 1. the program's basename is `ffmpeg` / `ffmpeg.exe`;
//! 2. the command line holds bladebro's exact input sequence from
//!    `encoder/args.rs`: `-f image2pipe` ... `-i pipe:0` (a user's ffmpeg does
//!    not read JPEG frames from a pipe with these flags);
//! 3. the output (everything after `-f webm|mp4 `, the last argument) is an
//!    absolute path whose parent is exactly the recordings dir and whose file
//!    name has bladebro's pattern `<stem>-<YYYYMMDDTHHMMSSZ>-<6 hex>.<webm|mp4>`;
//! 4. its parent is pid 1 or not alive (a live parent is a running bladebro:
//!    that recording is not an orphan).
//!
//! The command line is one string (`ps` form, also used for Linux): the output
//! path is matched as the TAIL of that string, never by splitting on spaces,
//! so a recordings dir with spaces still matches and a near miss never does.
//!
//! Kill: SIGTERM, wait up to 2 s, then SIGKILL only if that pid still has the
//! same command line (pid reuse guard). Windows: the sweep is a no-op, there is
//! no cheap command-line read (reap.rs makes the same call); the job object
//! (`platform/windows_job.rs`, KILL_ON_JOB_CLOSE) ends ffmpeg with its parent.

use std::path::{Path, PathBuf};

/// bladebro's input sequence (`encoder/args.rs`; a test pins them together).
const IN_FORMAT: &str = " -f image2pipe ";
const IN_SOURCE: &str = " -i pipe:0 ";
const MUXERS: [&str; 2] = [" -f webm ", " -f mp4 "];
/// How long a SIGTERMed orphan gets before the SIGKILL check.
const TERM_WAIT_MS: u64 = 2_000;

/// One process as listed by the OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub ppid: u32,
    /// The whole command line as one string.
    pub cmd: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Orphan,
    Keep(&'static str),
}

/// `<stem>-<YYYYMMDDTHHMMSSZ>-<6 hex>.<webm|mp4>` (names.rs `file_name`).
fn is_recording_name(name: &str) -> bool {
    let Some((base, ext)) = name.rsplit_once('.') else {
        return false;
    };
    if ext != "webm" && ext != "mp4" {
        return false;
    }
    let b = base.as_bytes();
    // stem (>= 1) + '-' + 16 stamp + '-' + 6 hex
    if b.len() < 1 + 1 + 16 + 1 + 6 {
        return false;
    }
    let (rest, id) = b.split_at(b.len() - 6);
    let (rest, sep2) = rest.split_at(rest.len() - 1);
    let (rest, stamp) = rest.split_at(rest.len() - 16);
    let (stem, sep1) = rest.split_at(rest.len() - 1);
    let stamp_ok = stamp.len() == 16
        && stamp.iter().enumerate().all(|(i, c)| match i {
            8 => *c == b'T',
            15 => *c == b'Z',
            _ => c.is_ascii_digit(),
        });
    sep1 == b"-"
        && sep2 == b"-"
        && !stem.is_empty()
        && stamp_ok
        && id.iter().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

fn basename(program: &str) -> &str {
    program.rsplit(['/', '\\']).next().unwrap_or(program)
}

/// The rule above. `dir` is the resolved recordings dir; `alive` says whether
/// a pid is a running process.
pub fn classify(p: &ProcInfo, dir: &Path, alive: impl Fn(u32) -> bool) -> Verdict {
    // Program = the text before the first " -" (an install path may hold spaces).
    let line = format!("{} ", p.cmd.trim());
    let program = line.split(" -").next().unwrap_or("").trim();
    let base = basename(program);
    if base != "ffmpeg" && base != "ffmpeg.exe" {
        return Verdict::Keep("not ffmpeg");
    }
    let Some(i) = line.find(IN_FORMAT) else {
        return Verdict::Keep("not a bladebro command");
    };
    let after_fmt = &line[i + IN_FORMAT.len() - 1..];
    let Some(j) = after_fmt.find(IN_SOURCE) else {
        return Verdict::Keep("not a bladebro command");
    };
    let after_in = &after_fmt[j + IN_SOURCE.len() - 1..];
    let Some((_, tail)) = MUXERS
        .iter()
        .find_map(|m| after_in.find(m).map(|k| (k, &after_in[k + m.len()..])))
    else {
        return Verdict::Keep("not a bladebro command");
    };
    let out = Path::new(tail.trim_end());
    if !out.is_absolute() {
        return Verdict::Keep("output path not absolute");
    }
    if out.parent() != Some(dir) {
        return Verdict::Keep("outside recordings dir");
    }
    let name = out.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !is_recording_name(name) {
        return Verdict::Keep("not a recording name");
    }
    if p.ppid != 1 && alive(p.ppid) {
        return Verdict::Keep("parent alive");
    }
    Verdict::Orphan
}

/// Classify every process, kill the orphans. Returns what was decided (for
/// logs and tests). `lister` and `killer` are injected so the rule is tested
/// without touching real processes.
pub fn sweep_in(
    dir: &Path,
    lister: &dyn Fn() -> Vec<ProcInfo>,
    killer: &dyn Fn(&ProcInfo),
    alive: &dyn Fn(u32) -> bool,
) -> Vec<(u32, Verdict)> {
    let me = std::process::id();
    let mut out = Vec::new();
    for p in lister() {
        // Never ourselves, init, or pid 0 (a parse artefact).
        if p.pid <= 1 || p.pid == me {
            continue;
        }
        let v = classify(&p, dir, alive);
        if v == Verdict::Orphan {
            killer(&p);
        }
        if v == Verdict::Orphan || !matches!(v, Verdict::Keep("not ffmpeg")) {
            out.push((p.pid, v));
        }
    }
    out
}

/// `ps -axww -o pid=,ppid=,command=` (macOS and Linux). Unparseable lines are
/// skipped, and so is a failed `ps`: the sweep is best effort.
#[cfg(unix)]
pub fn list_procs() -> Vec<ProcInfo> {
    let Ok(o) = std::process::Command::new("ps")
        .args(["-axww", "-o", "pid=,ppid=,command="])
        .output()
    else {
        return Vec::new();
    };
    parse_ps(&String::from_utf8_lossy(&o.stdout))
}

#[cfg(unix)]
pub(crate) fn parse_ps(text: &str) -> Vec<ProcInfo> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            let (pid, rest) = l.split_once(char::is_whitespace)?;
            let rest = rest.trim_start();
            let (ppid, cmd) = rest.split_once(char::is_whitespace)?;
            Some(ProcInfo {
                pid: pid.parse().ok()?,
                ppid: ppid.parse().ok()?,
                cmd: cmd.trim().to_string(),
            })
        })
        .collect()
}

/// The command line of one pid right now (None when it is gone).
#[cfg(unix)]
fn cmdline_of(pid: u32) -> Option<String> {
    let o = std::process::Command::new("ps")
        .args(["-ww", "-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// SIGTERM, wait, then SIGKILL only if the same pid still has the same command line.
#[cfg(unix)]
pub fn kill_orphan(p: &ProcInfo) {
    // SAFETY: plain signal to a pid classified as a bladebro orphan above.
    unsafe { libc::kill(p.pid as libc::pid_t, libc::SIGTERM) };
    let mut waited = 0;
    while waited < TERM_WAIT_MS {
        std::thread::sleep(std::time::Duration::from_millis(100));
        waited += 100;
        if cmdline_of(p.pid).as_deref() != Some(p.cmd.as_str()) {
            tracing::info!(target: "bladebro.record", "swept orphaned ffmpeg pid {}", p.pid);
            return;
        }
    }
    if cmdline_of(p.pid).as_deref() == Some(p.cmd.as_str()) {
        // SAFETY: same pid, same command line as when it was classified.
        unsafe { libc::kill(p.pid as libc::pid_t, libc::SIGKILL) };
        tracing::info!(target: "bladebro.record", "killed orphaned ffmpeg pid {} (ignored SIGTERM)", p.pid);
    }
}

/// Directories whose orphans this process may remove: the resolved recordings
/// dir (env/record.json layers, no project layer: that one is swept by the
/// guard, which knows the real dir) and `<data root>/recordings`. Only
/// directories that exist, so a machine that never recorded costs nothing.
#[cfg(unix)]
fn candidate_dirs() -> Vec<PathBuf> {
    use crate::record::{config, values::CONFIG_FILE};
    let bd = crate::platform::blade_dir();
    let mut v = vec![bd.join("recordings")];
    let env = |k: &str| std::env::var(k).ok().filter(|x| !x.trim().is_empty());
    if let Ok(file) = config::load_file_config(&bd.join(CONFIG_FILE)) {
        if let Ok(cfg) = config::resolve(
            &config::RecordOverrides::default(),
            &env,
            file.as_ref(),
            &bd,
        ) {
            v.push(cfg.dir);
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for d in v {
        if let Ok(c) = std::fs::canonicalize(&d) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// Fire-and-forget sweep of the usual recordings dirs. Returns at once: the
/// `ps` scan runs on a detached thread so Chrome launch latency is unchanged.
pub fn sweep() {
    #[cfg(unix)]
    for d in candidate_dirs() {
        sweep_dir(d);
    }
}

/// Sweep one recordings dir on a detached thread (no-op when it does not exist).
#[cfg(unix)]
pub fn sweep_dir(dir: PathBuf) {
    let Ok(dir) = std::fs::canonicalize(&dir) else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("ffmpeg-orphan-sweep".into())
        .spawn(move || {
            sweep_in(
                &dir,
                &list_procs,
                &kill_orphan,
                &crate::platform::process_alive,
            );
        });
}

/// Windows: no cheap command-line read; the job object ends ffmpeg with its parent.
#[cfg(not(unix))]
pub fn sweep_dir(_dir: PathBuf) {}
