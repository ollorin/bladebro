#!/usr/bin/env bash
# 11-03 (gap 1): a QA test case that ends before its first frame reaches the encoder. An assertion-only
# case (record-start, see, record-stop, no wait) on a static page stops with "no frames reached the
# encoder" over MCP, and the file is a 0-byte placeholder. Two recipes measured to work: a deliberate
# 1 s run wait before record-stop (wait step with optional:true), and a repaint (an act click that
# changes the page). Headless, scratch BLADE_HOME, fixtures on 127.0.0.1:8765, no real profile.
set -u
source "$(dirname "$0")/common.sh"
T=$(mktemp -d)
CFG="$SN/claude-code.mcp.json"
printf '%s\n' '{"action": "navigate", "url": "http://127.0.0.1:8765/mark.html"}' > "$T/nav.jsonl"
mcp() {  # mcp CALLS [ENV=VAL...]: one MCP session, the client-visible lines only
  local calls=$1; shift
  env "$@" python3 -I "$SC/mcp_drive.py" --config "$CFG" --calls "$T/nav.jsonl" --calls "$calls" --pause 0 2>&1 \
    | grep -E '^(SERVER|CALL|RESULT|STDERR \[bladebro\] browser mode|EXIT)'
}

snip no-frames-short no-frames-short.jsonl
echo; echo "SNIPPET no-frames-error"
for i in 1 2 3; do echo "# session $i (no wait)"; mcp "$SN/no-frames-short.jsonl"; done
echo "# per-tab mode, same case"; mcp "$SN/no-frames-short.jsonl" BLADE_RECORD_TABS=per-tab
echo

snip no-frames-wait no-frames-wait.jsonl
echo; echo "SNIPPET no-frames-wait-result"
for i in 1 2 3; do echo "# session $i (1 s run wait)"; mcp "$SN/no-frames-wait.jsonl"; done

snip no-frames-repaint no-frames-repaint.jsonl
echo; echo "SNIPPET no-frames-repaint-result"
for i in 1 2; do echo "# session $i (repaint)"; mcp "$SN/no-frames-repaint.jsonl"; done
rm -rf "$T"
