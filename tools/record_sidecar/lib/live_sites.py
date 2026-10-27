#!/usr/bin/env python3
"""Live stealth sites with recording OFF vs ON (Phase 12, VER-04).

  python3 -I tools/record_sidecar/lib/live_sites.py --mode off|on --out FILE [--bin PATH] [--only a,b] [--headed]
  python3 -I tools/record_sidecar/lib/live_sites.py --self-test

CLI daemon lane, BLADE_HEADLESS=1 (--headed: BLADE_HEADLESS unset, a visible window), a fresh scratch BLADE_HOME per mode (never a real profile). Per site:
nav, (on: state record-start), act reload (the detection scripts run again, while the screencast runs in
mode on), wait, read the result with one `act eval`, (on: state record-stop, which must say `recording
saved`, and ffprobe must count >= 1 frame: proof the numbers were taken while recording). One `SITE` line
per site; a page that cannot be parsed gets its first 40 `see content` lines, never a guess.
Sites: bot.sannysoft.com, bot.incolumitas.com, abrahamjuliot.github.io/creepjs. Exit code = number of
record-stop failures (mode on) + unreachable sites.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
FFPROBE = os.environ.get("BLADE_FFPROBE", os.path.expanduser("~/.local/ffmpeg/bin/ffprobe"))
FFMPEG = os.environ.get("BLADE_FFMPEG", os.path.expanduser("~/.local/ffmpeg/bin/ffmpeg"))

JS_SANNY = (
    "(()=>{const f=[...document.querySelectorAll('td.failed')].map(td=>{const r=td.parentElement;"
    "return ((r&&r.cells[0])?r.cells[0].innerText:td.innerText).trim().slice(0,50)});"
    "return JSON.stringify({passed:document.querySelectorAll('td.passed').length,"
    "warn:document.querySelectorAll('td.warn').length,failed:f})})()"
)
JS_INCOL = (
    "(()=>{const t=document.body?document.body.innerText:'';"
    "const fails=[...t.matchAll(/\"?([A-Za-z][\\w ]{1,40}?)\"?\\s*:\\s*\"?FAIL\\b/g)].map(m=>m[1]).slice(0,30);"
    "const s=t.match(/behavioralClassificationScore[^\\n]{0,80}/);"
    "return JSON.stringify({ok:(t.match(/\\bOK\\b/g)||[]).length,fail:(t.match(/\\bFAIL\\b/g)||[]).length,"
    "fails:fails,score:s?s[0]:''})})()"
)
JS_CREEP = (
    "(()=>{const t=document.body?document.body.innerText:'';"
    "return JSON.stringify({m:(t.match(/\\d+(\\.\\d+)?%\\s*(like headless|headless|stealth)"
    "|(like headless|headless|stealth)\\s*:?\\s*\\d+(\\.\\d+)?%/gi)||[]).slice(0,12)})})()"
)
SITES = [
    ("sannysoft", "https://bot.sannysoft.com/", 10, JS_SANNY),
    ("incolumitas", "https://bot.incolumitas.com/", 20, JS_INCOL),
    ("creepjs", "https://abrahamjuliot.github.io/creepjs/", 25, JS_CREEP),
]

OUT = None


def say(line):
    print(line, flush=True)
    if OUT:
        with open(OUT, "a") as f:
            f.write(line + "\n")


def first_line(text):
    for line in text.splitlines():
        if line.strip():
            return line.strip()
    return ""


def eval_value(out):
    """`act eval` output -> the page's JSON.stringify result as a dict (None when not parseable)."""
    line = first_line(out)
    if not line.startswith("result:"):
        return None
    try:
        v = json.loads(line[len("result:"):].strip())
        return json.loads(v) if isinstance(v, str) else None
    except Exception:
        return None


def parse_creep(matches):
    """['33% like headless', '0% headless', 'stealth: 0%'] -> {'like headless': 33.0, 'headless': 0.0, 'stealth': 0.0}"""
    out = {}
    for m in matches:
        num = re.search(r"\d+(?:\.\d+)?", m)
        key = re.search(r"like headless|headless|stealth", m, re.I)
        if num and key and key.group(0).lower() not in out:
            out[key.group(0).lower()] = float(num.group(0))
    return out


def metrics(name, v):
    """One SITE metrics string, or None when the page gave nothing usable."""
    if v is None:
        return None
    if name == "sannysoft":
        if v.get("passed", 0) + len(v.get("failed", [])) == 0:
            return None
        return f"passed={v['passed']} warn={v.get('warn', 0)} failed={len(v['failed'])} failed_rows={v['failed']}"
    if name == "incolumitas":
        if v.get("ok", 0) + v.get("fail", 0) == 0:
            return None
        return f"ok={v['ok']} fail={v['fail']} fail_keys={v.get('fails', [])} score={v.get('score', '')!r}"
    c = parse_creep(v.get("m", []))
    if "headless" not in c and "stealth" not in c:
        return None
    return " ".join(f"{k.replace(' ', '_')}={c[k]:g}%" for k in ("like headless", "headless", "stealth") if k in c)


def self_test():
    fails = 0
    cases = [
        (parse_creep(["33% like headless", "0% headless", "0% stealth"]), {"like headless": 33.0, "headless": 0.0, "stealth": 0.0}),
        (parse_creep(["headless: 12.5%", "Stealth: 0%"]), {"headless": 12.5, "stealth": 0.0}),
        (eval_value('result: "{\\"passed\\":3,\\"warn\\":0,\\"failed\\":[]}"'), {"passed": 3, "warn": 0, "failed": []}),
        (eval_value("error: no page"), None),
        (metrics("sannysoft", {"passed": 0, "warn": 0, "failed": []}), None),
        (metrics("creepjs", {"m": ["0% headless", "0% stealth"]}), "headless=0% stealth=0%"),
    ]
    for i, (got, want) in enumerate(cases):
        ok = got == want
        fails += 0 if ok else 1
        print(f"{'PASS' if ok else 'FAIL'} self-test {i}: {got!r}")
    print(f"# self-test: {fails} FAIL")
    return fails


def run(mode, binary, only, headed):
    home = tempfile.mkdtemp(prefix="bblive.")
    env = dict(os.environ)
    for k in ("BLADE_TRANSPORT", "BLADE_RECORD", "BLADE_RECORD_TABS"):
        env.pop(k, None)
    env.update(BLADE_HOME=home, BLADE_NO_UPDATE_CHECK="1", BLADE_NO_WARMING="1", BLADE_LANE="agent")
    if headed:
        env.pop("BLADE_HEADLESS", None)
    else:
        env["BLADE_HEADLESS"] = "1"
    if mode == "on":
        env.update(BLADE_RECORD="1", BLADE_FFMPEG=FFMPEG)

    def cli(*args, timeout=180):
        r = subprocess.run([binary, *args], env=env, capture_output=True, text=True, timeout=timeout)
        return r.returncode, (r.stdout or "") + (r.stderr or "")

    version = first_line(subprocess.run([binary, "--version"], capture_output=True, text=True).stdout)
    say(f"# live_sites mode={mode} {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} bin={binary} ({version}) {'headed' if headed else 'headless'} home={home}")
    bad = 0
    try:
        for name, url, wait, js in SITES:
            if only and name not in only:
                continue
            rc, out = cli("nav", url)
            if rc != 0:
                say(f"SITE {name} mode={mode} UNREACHABLE nav: {first_line(out)}")
                bad += 1
                continue
            if mode == "on":
                rc, out = cli("state", "record-start", f"live-{name}")
                say(f"  record-start: {first_line(out)}")
            cli("act", "reload")
            time.sleep(wait)
            m = None
            for _ in range(3):
                m = metrics(name, eval_value(cli("act", "eval", js)[1]))
                if m:
                    break
                time.sleep(5)
            if m:
                say(f"SITE {name} mode={mode} {m}")
            else:
                say(f"SITE {name} mode={mode} UNPARSED (first 40 content lines follow)")
                for line in cli("see", "content")[1].splitlines()[:40]:
                    say(f"  | {line}")
            if mode == "on":
                rc, out = cli("state", "record-stop")
                text = " ".join(out.split())
                path = re.search(r"(/[^ ;()]+\.(?:webm|mp4))", text)
                frames = -1
                if path and os.path.exists(path.group(1)):
                    p = subprocess.run([FFPROBE, "-v", "error", "-count_frames", "-select_streams", "v:0",
                                        "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0", path.group(1)],
                                       capture_output=True, text=True)
                    try:
                        frames = int(first_line(p.stdout))
                    except ValueError:
                        frames = -1
                ok = "recording saved" in text and frames >= 1
                bad += 0 if ok else 1
                say(f"  record-stop {'OK' if ok else 'FAIL'} nb_read_frames={frames}: {text[:300]}")
    finally:
        cli("stop", timeout=60)
        deadline = time.time() + 10
        left = []
        while time.time() < deadline:
            ps = subprocess.run(["ps", "-ax", "-o", "pid=,command="], capture_output=True, text=True).stdout
            left = [int(l.split()[0]) for l in ps.splitlines() if home in l and int(l.split()[0]) != os.getpid()]
            if not left:
                break
            time.sleep(0.5)
        for pid in left:
            try:
                os.kill(pid, 9)
            except OSError:
                pass
        say(f"# live_sites mode={mode} done: failures={bad} leftovers_killed={len(left)}")
        shutil.rmtree(home, ignore_errors=True)
    return bad


def main():
    global OUT
    args = sys.argv[1:]
    if args == ["--self-test"]:
        sys.exit(self_test())
    mode, binary, only, headed = None, os.path.join(REPO, "target", "release", "bladebro"), None, False
    while args:
        a = args.pop(0)
        if a == "--mode" and args:
            mode = args.pop(0)
        elif a == "--out" and args:
            OUT = args.pop(0)
        elif a == "--bin" and args:
            binary = args.pop(0)
        elif a == "--headed":
            headed = True
        elif a == "--only" and args:
            only = args.pop(0).split(",")
        else:
            print(f"unknown argument: {a}")
            sys.exit(2)
    if mode not in ("off", "on") or not OUT:
        print(__doc__)
        sys.exit(2)
    sys.exit(run(mode, binary, only, headed))


if __name__ == "__main__":
    main()
