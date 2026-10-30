#!/usr/bin/env bash
# 11-02: the QA-flow snippets. Per test case over MCP (the Claude Code server entry), the agent
# tools line checked against the tool names the server really exposes, the CLI path extraction
# a Bash-only agent (gsd-test-writer) uses, and the DSH profile with recording (plugin stub).
set -u
source "$(dirname "$0")/common.sh"
T=$(mktemp -d)
BIN="$REPO/target/release/bladebro"

snip qa-per-test qa-per-test.jsonl
OUT=$(python3 -I "$SC/mcp_drive.py" --config "$SN/claude-code.mcp.json" --calls "$SN/qa-per-test.jsonl" --pause 1.2 2>&1)
printf '%s\n' "$OUT" | grep -E '^(SERVER|TOOLS|CALL|RESULT isError=(true|false) (recording|outcome|✗)|STDERR \[bladebro\] browser mode|EXIT)'
V=$(saved_path "$OUT"); echo "video: $V"; [ -n "$V" ] && probe "$V"
ls -l "${V%.*}.poster.png" "${V%.*}.timeline.json" 2>&1 | awk '{print $1, $5, $NF}'

snip gsd-agent-tools gsd-agent-tools.yml
TOOLS=$(printf '%s\n' "$OUT" | sed -n 's/^TOOLS //p')
for t in $(grep -oE 'mcp__bladebro__[a-z]+' "$SN/gsd-agent-tools.yml"); do
  case ",$TOOLS," in *",$t,"*) echo "PASS $t is a tool the server exposes";; *) echo "FAIL $t is not in: $TOOLS";; esac
done

snip extract-path extract-path.sh
export BLADE_RECORD=1
bladebro nav "http://127.0.0.1:8765/mark.html" >/dev/null 2>&1
bladebro state record-start t03 | head -1; sleep 1
bladebro act click "turn green" | head -1; sleep 1
( source "$SN/extract-path.sh" ) > "$T/paths.txt"; cat "$T/paths.txt"
while read -r k p; do [ -s "$p" ] && echo "PASS $k file exists ($(wc -c < "$p" | tr -d ' ') bytes)" || echo "FAIL $k missing: $p"; done < "$T/paths.txt"
bladebro stop

snip dsh-profile-record dsh-profile-record.yml
echo "--- not loaded by a DSH host (dsh is not installed). Its config object (dsh-profile-record.json, binaryPath substituted) through the plugin stub, then the per-test calls:"
sed "s|/abs/path/to/bladebro|$BIN|" "$SN/dsh-profile-record.json" > "$T/cfg.json"; cat "$T/cfg.json"
( unset BLADE_HEADLESS BLADE_RECORD
  node "$SC/dsh_stub.mjs" "$REPO" "$(cat "$T/cfg.json")" "$T/bridge.json"
  python3 -I "$SC/mcp_drive.py" --config "$T/bridge.json" --calls "$SN/qa-per-test.jsonl" --pause 1.2 2>&1 | grep -E '^(SERVER|TOOLS|CALL|RESULT isError=(true|false) (recording|outcome|✗)|STDERR \[bladebro\] browser mode|EXIT)' )
rm -rf "$T"
