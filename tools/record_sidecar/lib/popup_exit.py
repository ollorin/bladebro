#!/usr/bin/env python3
"""Exit paths with an active POPUP segment (08.1): MCP stdin EOF, MCP SIGTERM, daemon `stop`, daemon
SIGTERM. Reuses the Phase 9 harness (lib/exit_live.py: scratch homes, pid-scoped signalling, ps sweep).
Run through scenarios/popup_live.sh (`exit` id); env as exit_live.py (REC_HUB_FIX, EXIT_BIN, EXIT_EVIDENCE).

Each scenario: popper.html (red opener) -> record-start -> click "open chain" (blue popup that stays) ->
wait until the popup is the video tab -> ~3 s of popup frames -> the exit -> the file must be playable
(ffprobe + full decode) and its LAST frame must be the popup's blue while the first is the opener's red,
then zero leftovers (ffmpeg / bladebro / Chrome under the scratch home)."""
import os
import signal
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import exit_live as X  # noqa: E402


def tone(rgb):
    r, g, b = rgb
    if r > 200 and g < 60 and b < 60:
        return "red"
    if b > 200 and r < 60 and g < 100:
        return "blue"
    if g > 120 and r < 60 and b < 60:
        return "green"
    return f"other{rgb}"


def frame_tone(path, t=None, last=False):
    cmd = [X.FFMPEG, "-nostdin", "-v", "error"]
    if last:
        cmd += ["-sseof", "-0.4"]
    else:
        cmd += ["-ss", str(t)]
    cmd += ["-i", path, "-frames:v", "1", "-vf", "scale=1:1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]
    out = subprocess.run(cmd, capture_output=True).stdout
    return tone(tuple(out[:3])) if len(out) >= 3 else "none"


def run_to_popup(sid, d, ex=None):
    """nav popper, record-start, popup click, wait for the video to be on the popup, ~3 s of it."""
    d.nav(f"{X.FIX}/popper.html")
    e, t = d.state("record-start", name="x")
    X.say(f"[{sid}] record-start err={e}: {t[:160]}")
    if getattr(d, "lane", "") == "daemon" and d.pid:
        X.OWN_PIDS.add(d.pid)
    path = None
    import re
    m = re.search(r"recording started: (.*?) \(tab ", t)
    path = m.group(1) if m else None
    X.check(sid, "record-start succeeded", not e and bool(path))
    time.sleep(1.5)
    if d.lane == "mcp":
        e, t = d.call("act", {"action": "click", "text": "open chain"})
    else:
        e, t = d.run("act", "click", "open chain")
    X.say(f"[{sid}] click: {t.strip().splitlines()[0][:120] if t.strip() else ''}")
    t0 = time.time()
    on_popup = False
    while time.time() - t0 < 8:
        e, tabs = d.state("tabs") if d.lane == "mcp" else d.run("state", "tabs")
        if any(l.startswith(" R ") for l in tabs.splitlines()):
            on_popup = True
            X.say(f"[{sid}] video on the popup {time.time()-t0:.2f} s after the click returned")
            break
        time.sleep(0.1)
    X.check(sid, "video followed the popup", on_popup)
    time.sleep(3)
    return path


def after(sid, home, tag, path):
    X.analyse(sid, path)
    if path and os.path.exists(path):
        first = frame_tone(path, 0.4)
        last = frame_tone(path, last=True)
        X.say(f"[{sid}] first frame {first}, last frame {last}")
        X.check(sid, "popup segment survived the exit: first frame red (opener), last frame blue (popup)", (first, last) == ("red", "blue"))
    X.leftovers(sid, tag)


def sc_eof():
    sid = "popup_mcp_eof"
    home, tag, d = X.scenario(sid, "mcp")
    try:
        path = run_to_popup(sid, d)
        d.eof()
        ms = d.exited(40)
        X.check(sid, f"MCP process exited after stdin EOF (took {ms} ms)", ms is not None)
        after(sid, home, tag, path)
    finally:
        d.stop_all()


def sc_term(lane, signame, sig):
    sid = f"popup_{lane}_{signame.lower()}"
    home, tag, d = X.scenario(sid, lane)
    try:
        path = run_to_popup(sid, d)
        pid = d.pid
        os.kill(pid, sig)
        ms = d.exited(40)
        X.check(sid, f"bladebro pid {pid} exited after {signame} (took {ms} ms)", ms is not None)
        after(sid, home, tag, path)
    finally:
        d.stop_all()


def sc_stop():
    sid = "popup_daemon_stop"
    home, tag, d = X.scenario(sid, "daemon")
    try:
        path = run_to_popup(sid, d)
        pid = d.pid
        e, t = d.run("stop", timeout=60)
        X.say(f"[{sid}] bladebro stop: {t.strip()[:200]}")
        X.check(sid, "daemon process gone", not X.alive(pid))
        after(sid, home, tag, path)
    finally:
        d.stop_all()


SCEN = {
    "mcp_eof": sc_eof,
    "mcp_sigterm": lambda: sc_term("mcp", "SIGTERM", signal.SIGTERM),
    "daemon_stop": sc_stop,
    "daemon_sigterm": lambda: sc_term("daemon", "SIGTERM", signal.SIGTERM),
}

if __name__ == "__main__":
    for name, fn in SCEN.items():
        X.say(f"== popup exit: {name}")
        try:
            fn()
        except Exception as e:
            X.check(name, f"scenario raised {type(e).__name__}: {e}", False)
    X.say(f"# popup_exit finished failures={X.FAILS}")
    sys.exit(min(X.FAILS, 255))
