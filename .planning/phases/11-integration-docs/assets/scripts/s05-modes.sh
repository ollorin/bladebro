#!/usr/bin/env bash
# 11-01: launch-mode examples. Example 4 (toggle) and the one-off line, run as written against a
# fixture server on 127.0.0.1:8000 (the URL the docs use); the mode-mismatch message; the QA
# server .mcp.json (BLADE_HEADLESS from the snippet's env); the interactive entry (no env) is
# started and listed only: it would open a visible window on macOS, which this run must not do.
set -u
source "$(dirname "$0")/common.sh"
python3 -m http.server 8000 --bind 127.0.0.1 --directory "$REPO/tools/record_sidecar/fixtures" >/dev/null 2>&1 &
P8000=$!; sleep 1
curl -sf http://127.0.0.1:8000/a.html >/dev/null && echo "fixture server on :8000 up"

snip toggle toggle.sh
( unset BLADE_HEADLESS; bash -x "$SN/toggle.sh" ) 2>&1 | grep -vE '^(e[0-9]+ |--- content|Page:|title:)'
echo "launch.json now: $(cat "$BLADE_HOME/launch.json" 2>&1)"
bladebro stop >/dev/null 2>&1

snip one-off one-off.sh
( unset BLADE_HEADLESS; bash -x "$SN/one-off.sh" ) 2>&1 | head -4

echo; echo "SNIPPET mismatch"
echo '$ (daemon running headless) BLADE_HEADLESS=0 bladebro nav http://127.0.0.1:8000/a.html'
BLADE_HEADLESS=0 bladebro nav http://127.0.0.1:8000/a.html; echo "exit=$?"
bladebro stop

snip qa-server qa-server.mcp.json
echo '[]' > "$BLADE_HOME/none.json"
echo '{"action": "navigate", "url": "http://127.0.0.1:8765/a.html"}' > "$BLADE_HOME/nav.jsonl"
( unset BLADE_HEADLESS; python3 -I "$SC/mcp_drive.py" --config "$SN/qa-server.mcp.json" --calls "$BLADE_HOME/nav.jsonl" ) 2>&1 | grep -E '^(SERVER|TOOLS|CALL|RESULT|STDERR \[bladebro\] browser mode|EXIT)'

snip interactive interactive.mcp.json
rm -f "$BLADE_HOME/launch.json"   # the toggle above left headless off in the user file
echo '$ BLADE_HEADLESS unset: bladebro headless status'
( unset BLADE_HEADLESS; bladebro headless status )
echo '$ the server starts and lists its tools; no tool call, so no browser is launched (a visible window is not opened in this run)'
( unset BLADE_HEADLESS; python3 -I "$SC/mcp_drive.py" --config "$SN/interactive.mcp.json" --calls "$BLADE_HOME/none.json" ) 2>&1
kill $P8000 2>/dev/null
