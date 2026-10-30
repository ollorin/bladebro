#!/usr/bin/env python3
"""Count duplicate/non-monotonic packet timestamps in results/tabswitch_sample.webm (SC1 decode
check: the strict-decode warnings come from these). Writes results/sample_timestamps.json."""
import json, os, subprocess
RS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ffprobe = os.environ.get("BLADE_FFPROBE", os.path.expanduser("~/.local/ffmpeg/bin/ffprobe"))
ffmpeg = os.environ.get("BLADE_FFMPEG", os.path.expanduser("~/.local/ffmpeg/bin/ffmpeg"))
f = os.path.join(RS, "results", "tabswitch_sample.webm")
out = subprocess.run([ffprobe, "-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts", "-of", "csv=p=0", f], capture_output=True, text=True).stdout
pts = [int(x) for x in out.split() if x.lstrip("-").isdigit()]
dec = subprocess.run([ffmpeg, "-v", "error", "-i", f, "-f", "null", "-"], capture_output=True, text=True)
lines = [l for l in dec.stderr.splitlines() if l.strip()]
res = {"file": "results/tabswitch_sample.webm", "packets": len(pts), "duplicate_pts": len(pts) - len(set(pts)),
       "non_monotonic_steps": sum(1 for a, b in zip(pts, pts[1:]) if b <= a),
       "strict_decode_exit": dec.returncode, "strict_decode_error_lines": len(lines),
       "all_lines_are_dts_warnings": all(("non monotonically increasing dts" in l) or ("Last message repeated" in l) for l in lines)}
json.dump(res, open(os.path.join(RS, "results", "sample_timestamps.json"), "w"), indent=1)
print(json.dumps(res))
