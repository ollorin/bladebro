#!/usr/bin/env python3
"""Drive `bladebro mcp` over stdio the way an MCP client (Claude Code, DSH) does, so the
docs' MCP snippets can be run as written against local fixtures (Phase 11, DOC-01).

  python3 -I mcp_drive.py --config CFG.json [--server NAME] --calls CALLS.json

CFG   an .mcp.json ({"mcpServers": {NAME: {command, args, env}}}) or a flat
      {"command", "args", "env", "cwd"} object (what the DSH plugin hands its bridge).
      `command` is looked up on PATH like a client does; `env` is added to this
      process's environment.
CALLS a JSON array of {"tool": "state", "arguments": {...}, "sleep_before": 1.5}, or JSON
      lines: one object per line, either such a call or bare tool arguments (an object
      with "op" is a `state` call, one with "action" is an `act` call), so a docs block
      of state ops can be fed as written. --calls may be repeated (run in order).
--pause S sleeps S seconds before every call after the first (default 0).

Prints SERVER, TOOLS, one CALL + one RESULT line per call, the server's STDERR lines
and EXIT. Exit status 0 when every call got a response, 1 otherwise."""
import argparse, json, os, queue, shutil, subprocess, sys, threading, time


def load_calls(path):
    with open(path) as f:
        raw = f.read()
    try:
        items = json.loads(raw)
        if not isinstance(items, list):
            items = [items]
    except ValueError:
        items = [json.loads(l) for l in raw.splitlines() if l.strip()]
    calls = []
    for it in items:
        if "tool" in it:
            calls.append(it)
        elif "op" in it:
            calls.append({"tool": "state", "arguments": it})
        elif "action" in it:
            calls.append({"tool": "act", "arguments": it})
        else:
            raise SystemExit(f"{path}: cannot tell the tool for {it}")
    return calls


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", required=True)
    ap.add_argument("--server")
    ap.add_argument("--calls", required=True, action="append")
    ap.add_argument("--pause", type=float, default=0.0)
    a = ap.parse_args()
    with open(a.config) as f:
        cfg = json.load(f)
    if "mcpServers" in cfg:
        name = a.server or next(iter(cfg["mcpServers"]))
        srv = cfg["mcpServers"][name]
    else:
        name, srv = "bladebro", cfg
    cmd = shutil.which(srv["command"]) or srv["command"]
    args = list(srv.get("args", []))
    extra = srv.get("env") or {}
    env = dict(os.environ)
    env.update(extra)
    cwd = srv.get("cwd") or os.getcwd()
    print(f"SERVER {name}: {cmd} {' '.join(args)} env={json.dumps(extra)} cwd={cwd}", flush=True)
    p = subprocess.Popen([cmd] + args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE, env=env, cwd=cwd)
    out, errs = queue.Queue(), []

    def read_out():
        for line in p.stdout:
            out.put(line)
        out.put(None)

    def read_err():
        for line in p.stderr:
            errs.append(line.decode(errors="replace").rstrip())

    threading.Thread(target=read_out, daemon=True).start()
    threading.Thread(target=read_err, daemon=True).start()
    nid = [0]

    def send(obj):
        p.stdin.write((json.dumps(obj) + "\n").encode())
        p.stdin.flush()

    def request(method, params, timeout):
        nid[0] += 1
        i = nid[0]
        send({"jsonrpc": "2.0", "id": i, "method": method, "params": params})
        end = time.time() + timeout
        while True:
            try:
                line = out.get(timeout=max(0.01, end - time.time()))
            except queue.Empty:
                return None
            if line is None:
                return None
            try:
                v = json.loads(line)
            except ValueError:
                continue
            if v.get("id") == i:
                return v

    ok = True
    init = request("initialize", {"protocolVersion": "2025-03-26", "capabilities": {},
                                  "clientInfo": {"name": "mcp_drive", "version": "0"}}, 20)
    if init is None:
        print("RESULT no initialize response", flush=True)
        ok = False
    else:
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        tl = request("tools/list", {}, 20) or {}
        names = [t.get("name") for t in (tl.get("result") or {}).get("tools", [])]
        print("TOOLS " + ",".join(f"mcp__{name}__{n}" for n in names), flush=True)
        calls = []
        for path in a.calls:
            calls.extend(load_calls(path))
        for n, c in enumerate(calls):
            time.sleep(c.get("sleep_before", a.pause if n else 0))
            print("CALL", c["tool"], json.dumps(c.get("arguments", {})), flush=True)
            v = request("tools/call", {"name": c["tool"], "arguments": c.get("arguments", {})}, 90)
            if v is None:
                print("RESULT no response within 90 s", flush=True)
                ok = False
                break
            r = v.get("result") or {}
            text = "\n".join(b.get("text", "") for b in r.get("content", [])
                             if isinstance(b, dict) and b.get("type") == "text")
            if not text and "error" in v:
                text = json.dumps(v["error"])
            is_err = bool(r.get("isError")) or "error" in v
            print(f"RESULT isError={str(is_err).lower()} {text}", flush=True)
    p.stdin.close()
    try:
        p.wait(timeout=30)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait()
        print("SERVER killed after 30 s", flush=True)
    time.sleep(0.2)
    for e in errs:
        print("STDERR", e)
    print("EXIT", p.returncode, flush=True)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
