#!/usr/bin/env python3
"""Run the diff_oracle probe battery against the CURRENT bladebro tab and diff runs.

Probe for the Phase 2 gate only (not shipped). Unlike oracle.bladebro_probe it never
restarts the daemon, so the battery can be run before/during/after a sidecar attach on the
same browser. parse_bladebro_eval is copied from tools/diff_oracle/oracle.py (~L255-286),
minus the sys.exit on failure (it raises instead), so this file has no import-time coupling.

  python3 battery_probe.py run <out.json>
  python3 battery_probe.py classify <a.json> <b.json> <a2.json>
"""
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))


def flatten(d, prefix=""):
    out = {}
    for k, v in d.items():
        key = prefix + "." + k if prefix else k
        if isinstance(v, dict):
            out.update(flatten(v, key))
        else:
            out[key] = v
    return out


def diff_keys(a, b):
    return sorted(k for k in set(a) | set(b) if a.get(k, _MISSING) != b.get(k, _MISSING))


_MISSING = object()


def classify(a, b, a2):
    """divergent = keys differing between A and B that are stable between A and A2."""
    noise = diff_keys(a, a2)
    divergent = sorted(set(diff_keys(a, b)) - set(noise))
    return {"divergent": divergent, "noise": noise}


def assemble_battery(repo=REPO):
    with open(os.path.join(repo, "tests", "fixtures", "native_behavior.js"), encoding="utf-8") as f:
        behavior = f.read()
    with open(os.path.join(repo, "tools", "diff_oracle", "battery.js"), encoding="utf-8") as f:
        body = f.read()
    return "(async()=>{" + behavior + body + "})()"


def parse_bladebro_eval(out):
    try:
        envelope = json.loads(out)
    except ValueError:
        envelope = None
    if isinstance(envelope, dict):
        if not envelope.get("ok") or envelope.get("is_error"):
            raise RuntimeError("eval failed: " + str(envelope)[:400])
        value = json.loads(envelope["text"])
        if isinstance(value, dict) and "artifact" in value:
            with open(value["artifact"], encoding="utf-8") as artifact:
                value = json.load(artifact)
        if isinstance(value, dict) and "data" in value and "note" in value:
            value = value["data"]
        if isinstance(value, str):
            value = json.loads(value)
        if not isinstance(value, dict):
            raise RuntimeError("eval battery did not return a JSON object")
        return value
    for line in reversed(out.splitlines()):
        line = line.strip()
        if line.startswith("result: "):
            v1 = json.loads(line[len("result: "):])
            return json.loads(v1) if isinstance(v1, str) else v1
    raise RuntimeError("no `result:` line in bladebro output")


def run(out_path):
    bladebro = os.environ.get("BLADEBRO") or os.path.join(REPO, "target", "release", "bladebro")
    r = subprocess.run(
        [bladebro, "--json", "act", "eval", assemble_battery(), "--format", "json"],
        env=os.environ, capture_output=True, text=True, timeout=240)
    if r.returncode != 0:
        raise RuntimeError("act eval failed rc=%d: %s %s" % (r.returncode, r.stdout[-300:], r.stderr[-300:]))
    flat = flatten(parse_bladebro_eval(r.stdout))
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(flat, f, indent=1, sort_keys=True)
    return flat


def main(argv):
    if len(argv) == 3 and argv[1] == "run":
        flat = run(argv[2])
        print(json.dumps({"keys": len(flat), "out": argv[2]}))
        return 0
    if len(argv) == 5 and argv[1] == "classify":
        maps = []
        for p in argv[2:5]:
            with open(p, encoding="utf-8") as f:
                maps.append(json.load(f))
        r = classify(*maps)
        r["keys"] = len(maps[0])
        print(json.dumps(r, indent=1))
        return 0
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
