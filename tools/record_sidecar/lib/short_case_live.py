"""Phase 11.1 live check: a record-stop 0.1 s / 0.5 s after record-start on mark.html gives a playable file.

Driven by tools/record_sidecar/scenarios/short_case_live.sh (which sets SHORT_BIN, EVIDENCE, FIX,
MCP_DRIVE, FFPROBE, FFMPEG and sources env.sh for the scratch home conventions). Python 3.9 stdlib only.
Every check prints PASS [id #n] or FAIL [id #n] lines; a FAIL is reported, never weakened.
"""

import argparse
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

BIN = os.environ["SHORT_BIN"]
FIX = os.environ["FIX"]
MCP_DRIVE = os.environ["MCP_DRIVE"]
FFPROBE = os.environ["FFPROBE"]
FFMPEG = os.environ["FFMPEG"]
EVIDENCE = os.environ["EVIDENCE"]
RUNS = int(os.environ.get("RUNS", "3"))
IDS = ["mcp-0.1", "mcp-0.5", "daemon-0.1", "daemon-0.5", "pertab-0.1", "pertab-0.5", "doc-case"]
PASSES = 0
FAILS = 0


def say(line):
    print(line, flush=True)
    with open(EVIDENCE, "a") as f:
        f.write(line + "\n")


def child_env(home, per_tab):
    """Environment for a bladebro child under a scratch home; also returns the BLADE_* keys set here."""
    env = dict(os.environ)
    env.pop("BLADE_TRANSPORT", None)
    blade = {
        "BLADE_HOME": home,
        "BLADE_HEADLESS": "1",
        "BLADE_RECORD": "1",
        "BLADE_NO_UPDATE_CHECK": "1",
        "BLADE_NO_WARMING": "1",
        "BLADE_LANE": "agent",
        "BLADE_FFMPEG": FFMPEG,
    }
    if per_tab:
        blade["BLADE_RECORD_TABS"] = "per-tab"
    env.update(blade)
    return env, blade


def excerpt(text):
    return " | ".join(text.strip().splitlines())[:240]


def check(tag, text, stop_s=None):
    """Judge one record-stop reply. Prints PASS or one FAIL per reason; returns the reasons."""
    global PASSES, FAILS
    reasons = []
    if "recording saved" not in text:
        reasons.append(f"stop did not save: {excerpt(text)}")
    if "no frames reached the encoder" in text:
        reasons.append("no frames reached the encoder")
    if "agent was on tab" in text:
        reasons.append(f"agent-tab note present: {excerpt(text)}")
    nb, size, n = "-", "-", "-"
    m = re.search(r"(/[^ ;()]+\.(?:webm|mp4))", text)
    path = m.group(1) if m else None
    if path is None:
        reasons.append("no video path in stop text")
    elif not os.path.isfile(path) or os.path.getsize(path) == 0:
        reasons.append(f"video missing or empty: {path}")
    else:
        size = os.path.getsize(path)
        pr = subprocess.run(
            [FFPROBE, "-v", "error", "-count_frames", "-select_streams", "v:0",
             "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0", path],
            capture_output=True, text=True, timeout=120)
        try:
            nb = int(pr.stdout.strip())
            if nb < 1:
                reasons.append(f"ffprobe nb_read_frames={nb} (< 1): {path}")
        except ValueError:
            reasons.append(f"ffprobe gave no frame count: {excerpt(pr.stdout + pr.stderr)}")
        dec = subprocess.run([FFMPEG, "-nostdin", "-v", "error", "-i", path, "-f", "null", "-"],
                             capture_output=True, text=True, timeout=300)
        if dec.returncode != 0 or dec.stderr.strip():
            reasons.append(f"full decode failed (exit {dec.returncode}): {excerpt(dec.stderr)}")
    fm = re.search(r"frames (\d+)", text)
    if fm is None or int(fm.group(1)) < 1:
        reasons.append(f"stop text reports no frames: {excerpt(text)}")
    else:
        n = fm.group(1)
    if stop_s is not None and stop_s >= 4.0:
        reasons.append(f"stop took {stop_s} s (>= 4.0 s)")
    if not reasons:
        PASSES += 1
        say(f"PASS [{tag}] frames={n} nb_read_frames={nb} bytes={size} stop_s={stop_s if stop_s is not None else '-'}")
    else:
        for r in reasons:
            FAILS += 1
            say(f"FAIL [{tag}] {r}")
    return reasons


def homed_pids(home):
    out = subprocess.run(["ps", "-ax", "-o", "pid=,command="], capture_output=True, text=True).stdout
    pids = []
    for line in out.splitlines():
        parts = line.strip().split(None, 1)
        if len(parts) == 2 and home in parts[1] and int(parts[0]) != os.getpid():
            pids.append(int(parts[0]))
    return pids


def cleanup(tag, home):
    """Wait up to 10 s for processes under this scratch home to exit; kill and FAIL any leftovers."""
    deadline = time.monotonic() + 10
    pids = homed_pids(home)
    while pids and time.monotonic() < deadline:
        time.sleep(0.5)
        pids = homed_pids(home)
    if pids:
        say(f"FAIL [{tag}] leftovers {len(pids)}")
        for pid in pids:
            try:
                os.kill(pid, 9)
            except OSError:
                pass
    shutil.rmtree(home, ignore_errors=True)


def run_cmd(args, env):
    return subprocess.run(args, env=env, capture_output=True, text=True, timeout=120)


def run_daemon(x, n):
    tag = f"daemon-{x} #{n}"
    home = tempfile.mkdtemp(prefix="bbshort.")
    env, _ = child_env(home, False)
    try:
        run_cmd([BIN, "nav", FIX + "/mark.html"], env)
        run_cmd([BIN, "state", "record-start", f"short-daemon-{n}"], env)
        time.sleep(x)
        t0 = time.monotonic()
        r = subprocess.run([BIN, "state", "record-stop"], env=env, capture_output=True, text=True, timeout=120)
        stop_s = round(time.monotonic() - t0, 2)
        text = r.stdout + r.stderr
        for line in text.splitlines():
            say("RESULT " + line)
        check(tag, text, stop_s)
        run_cmd([BIN, "stop"], env)
    finally:
        cleanup(tag, home)


def drive_mcp(tag, home, per_tab, call_files):
    """Run mcp_drive.py against `bladebro mcp` with the given calls files; judge the last stop RESULT."""
    env, blade = child_env(home, per_tab)
    cfg = os.path.join(home, "cfg.json")
    with open(cfg, "w") as f:
        json.dump({"command": BIN, "args": ["mcp"], "env": blade, "cwd": home}, f)
    args = ["python3", "-I", MCP_DRIVE, "--config", cfg]
    for c in call_files:
        args += ["--calls", c]
    r = subprocess.run(args, env=env, capture_output=True, text=True, timeout=120)
    lines = r.stdout.splitlines()
    for line in lines:
        if line.startswith("CALL ") or line.startswith("RESULT "):
            say(line)
    results = [line for line in lines if line.startswith("RESULT ")]
    text = results[-1] if results else "no RESULT line from mcp_drive: " + excerpt(r.stdout + r.stderr)
    check(tag, text, None)


def run_mcp(x, n, per_tab):
    tag = f"{'pertab' if per_tab else 'mcp'}-{x} #{n}"
    home = tempfile.mkdtemp(prefix="bbshort.")
    try:
        calls = [
            {"tool": "act", "arguments": {"action": "navigate", "url": FIX + "/mark.html"}},
            {"tool": "state", "arguments": {"op": "record-start", "name": f"short-{n}"}},
            {"tool": "state", "arguments": {"op": "record-stop"}, "sleep_before": x},
        ]
        calls_path = os.path.join(home, "calls.json")
        with open(calls_path, "w") as f:
            json.dump(calls, f)
        drive_mcp(tag, home, per_tab, [calls_path])
    finally:
        cleanup(tag, home)


def run_doc_case():
    """The docs' assertion-only case, as written (docs/recording-and-launch-config.md), over MCP."""
    tag = "doc-case #1"
    home = tempfile.mkdtemp(prefix="bbshort.")
    try:
        say("SNIPPET short-case-saved")
        doc_lines = [
            '{"op": "record-start", "name": "07-03-assert-only"}',
            '{"tool": "see", "arguments": {"mode": "model"}}',
            '{"op": "record-stop", "value": "PASS: assertion-only case"}',
        ]
        for line in doc_lines:
            say(line)
        nav_path = os.path.join(home, "nav.json")
        with open(nav_path, "w") as f:
            json.dump([{"tool": "act", "arguments": {"action": "navigate", "url": FIX + "/mark.html"}}], f)
        doc_path = os.path.join(home, "doc.jsonl")
        with open(doc_path, "w") as f:
            f.write("\n".join(doc_lines) + "\n")
        drive_mcp(tag, home, False, [nav_path, doc_path])
    finally:
        cleanup(tag, home)


def main(argv=None):
    global FAILS
    ap = argparse.ArgumentParser(description="Phase 11.1 short-case live check")
    ap.add_argument("--only", default="", help="comma list of ids (default: all)")
    opts = ap.parse_args(argv)
    ids = [s for s in opts.only.split(",") if s] if opts.only else list(IDS)
    for i in ids:
        if i not in IDS:
            print(f"unknown id: {i} (known: {','.join(IDS)})", file=sys.stderr)
            sys.exit(2)
    if not opts.only:
        open(EVIDENCE, "w").close()
    ver = subprocess.run([BIN, "--version"], capture_output=True, text=True).stdout.splitlines()
    utc = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    say(f"# short_case_live  {utc}  bin={BIN} ({ver[0] if ver else '?'})  fixtures={FIX}")
    for i in ids:
        if i == "doc-case":
            try:
                run_doc_case()
            except Exception as e:  # noqa: BLE001 - every failure is reported as a FAIL line
                FAILS += 1
                say(f"FAIL [doc-case #1] exception {e}")
            continue
        kind, x = i.split("-", 1)
        for n in range(1, RUNS + 1):
            tag = f"{i} #{n}"
            try:
                if kind == "daemon":
                    run_daemon(float(x), n)
                elif kind == "mcp":
                    run_mcp(float(x), n, False)
                elif kind == "pertab":
                    run_mcp(float(x), n, True)
            except Exception as e:  # noqa: BLE001
                FAILS += 1
                say(f"FAIL [{tag}] exception {e}")
    say(f"# summary: {PASSES} PASS, {FAILS} FAIL")
    sys.exit(FAILS)


if __name__ == "__main__":
    main()
