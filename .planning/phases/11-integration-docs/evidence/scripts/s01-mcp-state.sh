#!/usr/bin/env bash
# 11-01: the state ops JSON block and the Claude Code .mcp.json block, run over MCP stdio as written;
# the start/stop result samples and the timeline JSON sample come from this run.
set -u
source "$(dirname "$0")/common.sh"
T=$(mktemp -d)
echo '{"action": "navigate", "url": "http://127.0.0.1:8765/mark.html"}' > "$T/nav.jsonl"
snip claude-code-mcp-json claude-code.mcp.json
snip state-ops state-ops.jsonl
python3 -I "$SC/mcp_drive.py" --config "$SN/claude-code.mcp.json" --calls "$T/nav.jsonl" --calls "$SN/state-ops.jsonl" --pause 1.5
echo
echo "SNIPPET start-result"
echo "SNIPPET stop-result"
echo "SNIPPET stop-result-extras"
echo "SNIPPET timeline-json"
cat > "$T/ev.json" <<'J'
[{"tool": "act", "arguments": {"action": "navigate", "url": "http://127.0.0.1:8765/mark.html"}},
 {"tool": "state", "arguments": {"op": "record-start", "name": "ev"}},
 {"tool": "act", "arguments": {"action": "click", "text": "turn blue"}, "sleep_before": 1.5},
 {"tool": "state", "arguments": {"op": "record-stop", "value": "x"}, "sleep_before": 1.5}]
J
OUT=$(python3 -I "$SC/mcp_drive.py" --config "$SN/claude-code.mcp.json" --calls "$T/ev.json")
printf '%s\n' "$OUT"
V=$(saved_path "$OUT")
echo "video: $V"
[ -n "$V" ] && probe "$V"
TL="${V%.*}.timeline.json"; PO="${V%.*}.poster.png"
echo "poster: $(ls -l "$PO" 2>&1)"
echo "timeline file ($TL):"; python3 -I -m json.tool "$TL"
rm -rf "$T"
