#!/usr/bin/env python3
"""Live exit-path scenarios for the recording hub (Phase 9, LIFE-01..03). Run it through
scenarios/exit_live.sh, which owns the safety header (headless, scratch homes, 127.0.0.1 fixtures).

    python3 -I exit_live.py [--only ID] [--list]

Every scenario: fresh scratch BLADE_HOME (mktemp), BLADE_HEADLESS=1 BLADE_RECORD=1
BLADE_NO_UPDATE_CHECK=1, fixture anim.html on 127.0.0.1, record-start, ~4 s of frames, one exit,
then: ffprobe + full decode of the file, and a `ps` sweep for ffmpeg / bladebro / Chrome whose command
line mentions the scratch home (zero within LEFT_BOUND_S, measured). Only pids found under the
scratch home are ever signalled: no pkill by name, the user's own Chrome/ffmpeg is never matched.

Inputs (harness inputs, not product options): REC_HUB_FIX (fixture base URL, must be 127.0.0.1),
EXIT_BIN (default target/release/bladebro), EXIT_EVIDENCE (output file), BLADE_FFMPEG/BLADE_FFPROBE.
Exit code = number of FAIL lines.
"""
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import threading
import time

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
BIN = os.environ.get("EXIT_BIN", os.path.join(REPO, "target", "release", "bladebro"))
FIX = os.environ.get("REC_HUB_FIX", "").rstrip("/")
FFMPEG = os.environ.get("BLADE_FFMPEG", os.path.expanduser("~/.local/ffmpeg/bin/ffmpeg"))
FFPROBE = os.environ.get("BLADE_FFPROBE", os.path.expanduser("~/.local/ffmpeg/bin/ffprobe"))
EVIDENCE = os.environ.get("EXIT_EVIDENCE")
LEFT_BOUND_S = 10.0
IDLE_S = 20

if not FIX.startswith("http://127.0.0.1"):
    sys.exit("REC_HUB_FIX must be a http://127.0.0.1:PORT fixture base URL")

FAILS = 0
# bladebro's own command line does not hold the scratch home, so its pids are tracked explicitly
# (MCP: the child we spawned; daemon: cli.pid of the scratch home). Nothing else is ever signalled.
OWN_PIDS = set()
OUT = open(EVIDENCE, "a") if EVIDENCE else None


def say(s):
    print(s, flush=True)
    if OUT:
        OUT.write(s + "\n")
        OUT.flush()


def check(sid, desc, ok):
    global FAILS
    say(("PASS" if ok else "FAIL") + f" [{sid}] {desc}")
    if not ok:
        FAILS += 1
    return ok


def base_env(home, extra=None):
    e = dict(os.environ)
    for k in ("BLADE_TRANSPORT",):
        e.pop(k, None)
    e.update(
        BLADE_HOME=home,
        BLADE_HEADLESS="1",
        BLADE_RECORD="1",
        BLADE_NO_UPDATE_CHECK="1",
        BLADE_NO_WARMING="1",
        BLADE_FFMPEG=FFMPEG,
        BLADE_FFPROBE=FFPROBE,
    )
    e.update(extra or {})
    return e


def ps_rows(tag):
    """(pid, ppid, command) of every process whose command line holds `tag`."""
    out = subprocess.run(["ps", "-axww", "-o", "pid=,ppid=,command="], capture_output=True, text=True).stdout
    rows = []
    for line in out.splitlines():
        m = re.match(r"\s*(\d+)\s+(\d+)\s+(.*)", line)
        if m and tag in m.group(3) and "exit_live" not in m.group(3):
            rows.append((int(m.group(1)), int(m.group(2)), m.group(3)))
    return rows


def kind(cmd):
    if re.search(r"(^|/)ffmpeg ", cmd):
        return "ffmpeg"
    if "bladebro" in cmd.split(" ")[0] or re.search(r"/bladebro (mcp|daemon)", cmd):
        return "bladebro"
    if "Chrome" in cmd or "chrome" in cmd:
        return "chrome"
    return "other"


def pids_of(tag, which):
    return [p for p, _, c in ps_rows(tag) if kind(c) == which]


def chrome_main(tag):
    for p, _, c in ps_rows(tag):
        if kind(c) == "chrome" and "--type=" not in c and "--user-data-dir=" in c:
            return p
    return None


def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    # a zombie child of ours counts as gone
    st = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    return bool(st) and not st.startswith("Z")


def wait_gone(pid, limit):
    t0 = time.time()
    while time.time() - t0 < limit:
        if not alive(pid):
            return int((time.time() - t0) * 1000)
        time.sleep(0.02)
    return None


def leftovers(sid, tag, allow_chrome=False):
    """Poll until nothing of ours (ffmpeg / bladebro / Chrome under the scratch home) remains.
    allow_chrome: the orphaned Chrome of a kill -9'd MCP server is NOT reaped by upstream's reaper
    (same with recording off: .planning/phases/09-*/deferred-items.md); it is reported, then killed
    by pid (scratch home only), and only ffmpeg/bladebro must be zero."""
    t0 = time.time()
    left = []
    while time.time() - t0 < LEFT_BOUND_S:
        left = [(p, k) for p, _, c in ps_rows(tag) for k in [kind(c)] if k in ("ffmpeg", "chrome")]
        left += [(p, "bladebro") for p in OWN_PIDS if alive(p)]
        if not left:
            break
        time.sleep(0.25)
    ms = int((time.time() - t0) * 1000)
    if allow_chrome:
        chrome = [(p, k) for p, k in left if k == "chrome"]
        say(f"[{sid}] INFO orphaned scratch Chrome processes left by the kill -9 (upstream reaper gap, recording off too): {len(chrome)}")
        left = [(p, k) for p, k in left if k != "chrome"]
        for p, _ in chrome:
            try:
                os.kill(p, signal.SIGKILL)
            except ProcessLookupError:
                pass
    check(sid, f"leftovers == 0 within {LEFT_BOUND_S:.0f} s (ffmpeg/bladebro/Chrome under the scratch home; took {ms} ms)", not left)
    if left:
        say(f"[{sid}] leftover: {left}")
        for p, k in left:
            try:
                os.kill(p, signal.SIGKILL)  # a scratch-home process we started
            except ProcessLookupError:
                pass


def analyse(sid, path, expect_playable=True):
    if not path or not os.path.exists(path):
        return check(sid, f"file exists: {path}", False)
    size = os.path.getsize(path)
    if not expect_playable:
        say(f"[{sid}] file {path} size {size} (not expected to be playable)")
        return True
    pj = subprocess.run(
        [FFPROBE, "-v", "error", "-show_entries", "stream=codec_name,width,height:format=duration", "-of", "json", path],
        capture_output=True, text=True,
    ).stdout
    try:
        j = json.loads(pj)
        dur = float(j["format"]["duration"])
    except Exception:
        dur = 0.0
    say(f"[{sid}] ffprobe size={size}: {' '.join(pj.split())}")
    dec = subprocess.run([FFMPEG, "-v", "error", "-i", path, "-f", "null", "-"], capture_output=True, text=True)
    check(sid, f"duration > 0 ({dur:.2f} s) and full decode exit={dec.returncode} stderr_lines={len(dec.stderr.splitlines())}",
          dur > 0 and dec.returncode == 0 and not dec.stderr.strip())
    return True


def recordings(home):
    d = os.path.join(home, "recordings")
    return sorted(os.path.join(d, f) for f in os.listdir(d) if f.endswith((".webm", ".mp4"))) if os.path.isdir(d) else []


# ---------------------------------------------------------------- lane drivers
class Mcp:
    """`bladebro mcp` over stdio JSON-RPC."""

    lane = "mcp"

    def __init__(self, home, extra=None):
        self.home = home
        self.p = subprocess.Popen([BIN, "mcp"], env=base_env(home, extra), stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1)
        OWN_PIDS.add(self.p.pid)
        self.id = 0
        self.q = []
        self.cv = threading.Condition()
        threading.Thread(target=self._rd, daemon=True).start()
        self.req("initialize", {"protocolVersion": "2025-03-26", "capabilities": {},
                                "clientInfo": {"name": "exit_live", "version": "0"}}, 30)

    def _rd(self):
        for line in self.p.stdout:
            try:
                v = json.loads(line)
            except Exception:
                continue
            with self.cv:
                self.q.append(v)
                self.cv.notify_all()

    def req(self, method, params, timeout=90):
        self.id += 1
        i = self.id
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": i, "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        t0 = time.time()
        with self.cv:
            while True:
                for v in self.q:
                    if v.get("id") == i:
                        self.q.remove(v)
                        return v
                left = timeout - (time.time() - t0)
                if left <= 0:
                    raise TimeoutError(method)
                self.cv.wait(min(left, 1.0))

    def call(self, tool, args):
        r = self.req("tools/call", {"name": tool, "arguments": args})
        res = r.get("result") or {}
        err = bool(res.get("isError")) or "error" in r
        text = "\n".join(b.get("text", "") for b in res.get("content", []) if isinstance(b, dict))
        return err, text or json.dumps(r.get("error", ""))

    def nav(self, url):
        return self.call("act", {"action": "navigate", "url": url})

    def state(self, op, **kw):
        return self.call("state", dict(op=op, **kw))

    @property
    def pid(self):
        return self.p.pid

    def eof(self):
        self.p.stdin.close()

    def exited(self, limit):
        t0 = time.time()
        while time.time() - t0 < limit:
            if self.p.poll() is not None:
                return int((time.time() - t0) * 1000)
            time.sleep(0.02)
        return None

    def stop_all(self):
        try:
            self.p.stdin.close()
        except Exception:
            pass
        if self.exited(25) is None:
            self.p.kill()


class Cli:
    """CLI client against the auto-started daemon."""

    lane = "daemon"

    def __init__(self, home, extra=None):
        self.home = home
        self.env = base_env(home, extra)

    def run(self, *args, timeout=90):
        r = subprocess.run([BIN, *args], env=self.env, capture_output=True, text=True, timeout=timeout)
        return r.returncode != 0, (r.stdout + r.stderr)

    def nav(self, url):
        return self.run("nav", url)

    def state(self, op, **kw):
        a = ["state", op] + ([kw["name"]] if "name" in kw else [])
        return self.run(*a)

    @property
    def pid(self):
        try:
            return int(open(os.path.join(self.home, "cli.pid")).read().strip())
        except Exception:
            return None

    def exited(self, limit):
        return wait_gone(self.pid, limit) if self.pid else 0

    def stop_all(self):
        self.run("stop", timeout=60)


def pid_exit_ms(lane, pid, limit):
    return wait_gone(pid, limit)


# ---------------------------------------------------------------- scenarios
def started(sid, d, name):
    d.nav(f"{FIX}/anim.html")
    e, t = d.state("record-start", name=name)
    say(f"[{sid}] record-start err={e}: {t}")
    if getattr(d, "lane", "") == "daemon" and d.pid:
        OWN_PIDS.add(d.pid)
    m = re.search(r"recording started: (.*?) \(tab ", t)
    check(sid, "record-start succeeded", not e and bool(m))
    time.sleep(4)
    return m.group(1) if m else None


def scenario(sid, lane, extra=None, per_tab=False):
    home = tempfile.mkdtemp(prefix="bbexit")
    tag = os.path.basename(home)
    ex = dict(extra or {})
    d = (Mcp if lane == "mcp" else Cli)(home, ex)
    return home, tag, d


def finish_checks(sid, home, tag, path):
    analyse(sid, path)
    leftovers(sid, tag)


def sc_signal(lane, sig, signame):
    sid = f"{lane}_{signame.lower()}"
    home, tag, d = scenario(sid, lane)
    try:
        path = started(sid, d, "x")
        pid = d.pid
        ff = pids_of(tag, "ffmpeg")
        t0 = time.time()
        os.kill(pid, sig)
        ms = d.exited(40)
        check(sid, f"bladebro pid {pid} exited after {signame} (took {ms} ms)", ms is not None)
        say(f"[{sid}] ffmpeg pids before: {ff}; all gone after {int((time.time()-t0)*1000)} ms: {not any(alive(p) for p in ff)}")
        finish_checks(sid, home, tag, path)
    finally:
        d.stop_all()
        leftovers(sid + "-cleanup", tag)


def sc_eof():
    sid = "mcp_eof"
    home, tag, d = scenario(sid, "mcp")
    try:
        path = started(sid, d, "x")
        d.eof()
        ms = d.exited(40)
        check(sid, f"MCP process exited after stdin EOF (took {ms} ms)", ms is not None)
        finish_checks(sid, home, tag, path)
    finally:
        d.stop_all()


def sc_stop():
    sid = "daemon_stop"
    home, tag, d = scenario(sid, "daemon")
    try:
        path = started(sid, d, "x")
        pid = d.pid
        t0 = time.time()
        e, t = d.run("stop", timeout=60)
        say(f"[{sid}] bladebro stop rc_err={e} after {int((time.time()-t0)*1000)} ms: {t.strip()[:200]}")
        check(sid, "stop returned without 'cleanup is still running'", "cleanup is still running" not in t)
        check(sid, "daemon process gone", not alive(pid))
        finish_checks(sid, home, tag, path)
    finally:
        d.stop_all()


def sc_idle(lane):
    sid = f"{lane}_idle"
    home, tag, d = scenario(sid, lane, {"BLADE_IDLE_TIMEOUT": str(IDLE_S)})
    try:
        path = started(sid, d, "x")
        t0 = time.time()
        gone = None
        while time.time() - t0 < 60:
            if not pids_of(tag, "chrome") and not pids_of(tag, "ffmpeg"):
                gone = time.time() - t0
                break
            time.sleep(0.5)
        check(sid, f"idle timeout ({IDLE_S} s, checked every 15 s) closed Chrome and ffmpeg (after {gone and round(gone, 1)} s)", gone is not None)
        e, t = d.state("record-stop")
        say(f"[{sid}] record-stop err={e}: {t}")
        check(sid, "text contains 'previous recording ended with the browser: ' (after the self-heal note line)", "previous recording ended with the browser: " in t)
        analyse(sid, path)
        e, t = d.nav(f"{FIX}/anim.html")
        e, t = d.state("record-start", name="again")
        say(f"[{sid}] record-start after idle err={e}: {t}")
        check(sid, "next record-start succeeds", not e)
        time.sleep(1)
        e, t = d.state("record-stop")
        check(sid, "next record-stop saves", "recording saved" in t)
    finally:
        d.stop_all()
        leftovers(sid, tag)


def sc_kill9(lane):
    sid = f"{lane}_kill9"
    home, tag, d = scenario(sid, lane)
    try:
        path = started(sid, d, "x")
        pid = d.pid
        ff = pids_of(tag, "ffmpeg")
        say(f"[{sid}] bladebro pid {pid}, ffmpeg pids {ff}")
        t0 = time.time()
        os.kill(pid, signal.SIGKILL)
        gone = {p: wait_gone(p, 15) for p in ff}
        say(f"[{sid}] ffmpeg exit ms after kill -9 of the parent: {gone}")
        check(sid, "ffmpeg exited by itself (stdin EOF)", all(v is not None for v in gone.values()) and bool(ff))
        analyse(sid, path)
        say(f"[{sid}] right after: " + "; ".join(f"{k}:{p}" for p, _, c in ps_rows(tag) for k in [kind(c)] if k != 'other'))
        # next launch on the same home: reaper (+ sweeper) removes the orphaned scratch Chrome
        d2 = (Mcp if lane == "mcp" else Cli)(home, {})
        d2.nav(f"{FIX}/anim.html")
        say(f"[{sid}] after next launch: ffmpeg={pids_of(tag, 'ffmpeg')} chrome_main={chrome_main(tag)}")
        check(sid, "no ffmpeg after the next launch", not pids_of(tag, "ffmpeg"))
        d2.stop_all()
        d = d2
    finally:
        d.stop_all()
        leftovers(sid, tag, allow_chrome=(lane == "mcp"))


def sc_kill_chrome(lane):
    sid = f"{lane}_kill_chrome"
    home, tag, d = scenario(sid, lane)
    try:
        path = started(sid, d, "x")
        cp = chrome_main(tag)
        check(sid, f"found scratch Chrome main pid {cp}", cp is not None)
        os.kill(cp, signal.SIGKILL)
        t0 = time.time()
        while time.time() - t0 < 10 and pids_of(tag, "ffmpeg"):
            time.sleep(0.1)
        say(f"[{sid}] ffmpeg gone {int((time.time()-t0)*1000)} ms after Chrome kill: {not pids_of(tag, 'ffmpeg')}")
        check(sid, "ffmpeg finalized and gone within 10 s (guard bound ~2.6 s)", not pids_of(tag, "ffmpeg"))
        time.sleep(1)
        e, t = d.state("record-stop")
        say(f"[{sid}] record-stop err={e}: {t}")
        check(sid, "text: 'previous recording interrupted: browser connection lost' (or the relaunch hook's 'ended with the browser')",
              "previous recording interrupted: browser connection lost" in t or "ended with the browser" in t)
        check(sid, "never claims a clean save", not t.startswith("recording saved"))
        analyse(sid, path)
        e, t = d.nav(f"{FIX}/a.html")
        say(f"[{sid}] nav after kill err={e}: {t.strip()[:160]}")
        check(sid, "browsing works again (relaunch)", not e and "navigated" in t)
        e, t = d.state("record-start", name="again")
        check(sid, "next record-start succeeds", not e)
        time.sleep(1.5)
        e, t = d.state("record-stop")
        check(sid, "next record-stop saves", "recording saved" in t)
    finally:
        d.stop_all()
        leftovers(sid, tag)


def sc_kill_ffmpeg(lane):
    sid = f"{lane}_kill_ffmpeg"
    home, tag, d = scenario(sid, lane)
    try:
        path = started(sid, d, "x")
        ff = pids_of(tag, "ffmpeg")
        check(sid, f"found scratch ffmpeg pid(s) {ff}", len(ff) == 1)
        for p in ff:
            os.kill(p, signal.SIGKILL)
        time.sleep(3)
        e, t = d.state("record-stop")
        say(f"[{sid}] record-stop err={e}: {t}")
        check(sid, "text names 'encoder exited'", "encoder exited" in t)
        check(sid, "never 'recording saved'", "recording saved" not in t)
        analyse(sid, path, expect_playable=False)
        e, t = d.state("record-start", name="again")
        check(sid, "next record-start succeeds", not e)
        time.sleep(1.5)
        e, t = d.state("record-stop")
        check(sid, "next record-stop saves", "recording saved" in t)
    finally:
        d.stop_all()
        leftovers(sid, tag)


def sc_oneshot():
    sid = "oneshot"
    home = tempfile.mkdtemp(prefix="bbexit")
    tag = os.path.basename(home)
    try:
        r = subprocess.run([BIN, "--no-daemon", "state", "record-start", "x"], env=base_env(home),
                           capture_output=True, text=True, timeout=120)
        t = r.stdout + r.stderr
        say(f"[{sid}] rc={r.returncode}: {' '.join(l for l in t.splitlines() if not l.startswith('['))}")
        check(sid, "refused (non-zero exit)", r.returncode != 0)
        check(sid, "text mentions the long-lived session", "long-lived session" in t)
        check(sid, "no recordings dir / file", not recordings(home))
        leftovers(sid, tag)
    finally:
        subprocess.run([BIN, "stop"], env=base_env(home), capture_output=True)


SCENARIOS = {
    "mcp_eof": sc_eof,
    "mcp_sigterm": lambda: sc_signal("mcp", signal.SIGTERM, "SIGTERM"),
    "mcp_sigint": lambda: sc_signal("mcp", signal.SIGINT, "SIGINT"),
    "mcp_idle": lambda: sc_idle("mcp"),
    "daemon_stop": sc_stop,
    "daemon_sigterm": lambda: sc_signal("daemon", signal.SIGTERM, "SIGTERM"),
    "daemon_sigint": lambda: sc_signal("daemon", signal.SIGINT, "SIGINT"),
    "daemon_idle": lambda: sc_idle("daemon"),
    "mcp_kill9": lambda: sc_kill9("mcp"),
    "daemon_kill9": lambda: sc_kill9("daemon"),
    "mcp_kill_chrome": lambda: sc_kill_chrome("mcp"),
    "daemon_kill_chrome": lambda: sc_kill_chrome("daemon"),
    "mcp_kill_ffmpeg": lambda: sc_kill_ffmpeg("mcp"),
    "daemon_kill_ffmpeg": lambda: sc_kill_ffmpeg("daemon"),
    "oneshot": sc_oneshot,
}


def main():
    a = sys.argv[1:]
    if "--list" in a:
        print(" ".join(SCENARIOS))
        return 0
    only = a[a.index("--only") + 1] if "--only" in a else None
    for sid, fn in SCENARIOS.items():
        if only and only != sid:
            continue
        say(f"== {sid}")
        try:
            fn()
        except Exception as e:  # a harness error is a FAIL, never a silent skip
            check(sid, f"scenario raised {type(e).__name__}: {e}", False)
    say(f"# exit_live finished {time.strftime('%FT%TZ', time.gmtime())} failures={FAILS}")
    return FAILS


if __name__ == "__main__":
    sys.exit(min(main(), 255))
