#!/usr/bin/env bash
# 11-02: a real Claude Code client (claude -p, model haiku) loads the .mcp.json server entry from
# the docs and records one test case through mcp__bladebro__state; a second run does the same
# through a subagent whose tools list is the documented agent tools line (minus Read, Bash).
# Costs two short haiku sessions. Headless, scratch BLADE_HOME, fixture on :8765.
set -u
source "$(dirname "$0")/common.sh"
command -v claude >/dev/null || { echo "claude CLI not found: Claude Code client run NOT VERIFIED"; exit 0; }
W=$(mktemp -d)   # neutral working dir: no CLAUDE.md, no .bladebro.json
cp "$SN/claude-code.mcp.json" "$W/.mcp.json"
cd "$W" || exit 1
echo "# claude $(claude --version 2>&1 | head -1); cwd $W"

snip claude-code-client claude-code.mcp.json
P1='Use only the bladebro MCP tools, in this order, one call each: 1) mcp__bladebro__act with {"action":"navigate","url":"http://127.0.0.1:8765/mark.html"} 2) mcp__bladebro__state with {"op":"record-start","name":"cc-t01"} 3) mcp__bladebro__act with {"action":"click","text":"turn blue"} 4) mcp__bladebro__state with {"op":"record-stop","value":"PASS: cc-t01"}. Then reply with only the full text that step 4 returned, verbatim.'
R1=$(claude -p "$P1" --model haiku --mcp-config "$W/.mcp.json" --strict-mcp-config --output-format text --allowedTools mcp__bladebro__act mcp__bladebro__state </dev/null 2>&1)
echo "claude reply: $R1"
V=$(saved_path "$R1"); echo "video: ${V:-none}"; [ -n "$V" ] && [ -s "$V" ] && probe "$V" && echo "PASS client run produced the recording at the reported path" || echo "NOT VERIFIED: no recording at a reported path (see the claude reply above)"

echo; echo "SNIPPET claude-code-subagent"
AG='{"qa-recorder":{"description":"Runs one browser test case with bladebro and records it","prompt":"You run one browser test case with the bladebro tools and record it. Call mcp__bladebro__state {\"op\":\"record-start\",\"name\":<test id>} first, then do the steps with mcp__bladebro__act, then mcp__bladebro__state {\"op\":\"record-stop\",\"value\":\"PASS: <test id>\" or \"FAIL: <reason>\"}. Return the record-stop text verbatim.","tools":["mcp__bladebro__act","mcp__bladebro__see","mcp__bladebro__state","mcp__bladebro__run","mcp__bladebro__vision"]}}'
echo "agents: $AG"
P2='Delegate to the qa-recorder agent this test case: test id cc-t02; open http://127.0.0.1:8765/mark.html, click the button "turn green"; it passes when the click succeeds. Then reply with only the record-stop text it returned, verbatim.'
R2=$(claude -p "$P2" --model haiku --mcp-config "$W/.mcp.json" --strict-mcp-config --agents "$AG" --output-format text --allowedTools Agent Task mcp__bladebro__act mcp__bladebro__see mcp__bladebro__state mcp__bladebro__run mcp__bladebro__vision </dev/null 2>&1)
echo "claude reply: $R2"
V=$(saved_path "$R2"); echo "video: ${V:-none}"; [ -n "$V" ] && [ -s "$V" ] && probe "$V" && echo "PASS subagent run produced the recording at the reported path" || echo "NOT VERIFIED: no recording at a reported path (see the claude reply above)"
echo "recordings dir: $(ls "$BLADE_HOME/recordings" 2>&1 | tr '\n' ' ')"
cd "$REPO"; rm -rf "$W"
