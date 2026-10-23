#!/usr/bin/env python3
"""File size per minute at several recording settings (Phase 9, 09-03). Daemon lane, headless,
scratch BLADE_HOME per setting, local fixture only (anim.html: a moving box plus a clock, so every
frame differs; real pages with less motion compress better, busy video worse).

    REC_HUB_FIX=http://127.0.0.1:8765 EXIT_BIN=target/release/bladebro python3 -I sizes_live.py [SECONDS]

Prints one line per setting: bytes, ffprobe duration, MB/min (1 MB = 10^6 bytes, duration from ffprobe).
"""
import json, os, re, subprocess, sys, tempfile, time

BIN = os.environ.get("EXIT_BIN", "target/release/bladebro")
FIX = os.environ.get("REC_HUB_FIX", "").rstrip("/")
FFPROBE = os.environ.get("BLADE_FFPROBE", os.path.expanduser("~/.local/ffmpeg/bin/ffprobe"))
SECS = int(sys.argv[1]) if len(sys.argv) > 1 else 60
if not FIX.startswith("http://127.0.0.1"):
    sys.exit("REC_HUB_FIX must be a http://127.0.0.1:PORT fixture base URL")

SETTINGS = [
    ("default: webm VP9 10 fps 1280x720", {}),
    ("mp4 H.264 10 fps 1280x720", {"BLADE_RECORD_FORMAT": "mp4"}),
    ("webm 15 fps 1280x720", {"BLADE_RECORD_FPS": "15"}),
    ("webm 10 fps 1920x1080", {"BLADE_RECORD_SIZE": "1920x1080"}),
    ("max: webm 15 fps 3840x2160", {"BLADE_RECORD_FPS": "15", "BLADE_RECORD_SIZE": "3840x2160"}),
    ("max: mp4 15 fps 3840x2160", {"BLADE_RECORD_FPS": "15", "BLADE_RECORD_SIZE": "3840x2160", "BLADE_RECORD_FORMAT": "mp4"}),
]
for label, extra in SETTINGS:
    home = tempfile.mkdtemp(prefix="bbsize")
    env = dict(os.environ, BLADE_HOME=home, BLADE_HEADLESS="1", BLADE_RECORD="1", BLADE_NO_UPDATE_CHECK="1", BLADE_NO_WARMING="1")
    env.update(extra)
    run = lambda *a: subprocess.run([BIN, *a], env=env, capture_output=True, text=True, timeout=180)
    try:
        run("nav", FIX + "/anim.html")
        t = run("state", "record-start", "sz").stdout
        m = re.search(r"recording started: (.*?) \(tab ", t)
        time.sleep(SECS)
        stop = run("state", "record-stop").stdout.strip()
        path = m.group(1) if m else None
        size = os.path.getsize(path) if path and os.path.exists(path) else 0
        pj = subprocess.run([FFPROBE, "-v", "error", "-show_entries", "stream=width,height:format=duration", "-of", "json", path], capture_output=True, text=True).stdout
        dur = float(json.loads(pj)["format"]["duration"])
        print(f"{label}: bytes={size} duration={dur:.2f} s MB/min={size / 1e6 / (dur / 60):.2f}  probe={' '.join(pj.split())}", flush=True)
        print(f"    record-stop: {stop[:240]}", flush=True)
    finally:
        run("stop")
