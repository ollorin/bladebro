#!/usr/bin/env bash
# 11-01: the DSH plugin options block and the DSH profile YAML. No DSH host is installed here, so
# npm/bladebro/dsh-plugin.mjs is loaded with a stub bridge (dsh_stub.mjs); the options it hands the
# bridge (command, args, env, cwd) are then started as the stdio MCP server and driven over a
# fixture. Placeholders /abs/path/to/bladebro and /path/to/project are substituted for the run.
set -u
source "$(dirname "$0")/common.sh"
T=$(mktemp -d)
BIN="$REPO/target/release/bladebro"
echo '{"action": "navigate", "url": "http://127.0.0.1:8765/mark.html"}' > "$T/nav.jsonl"
printf '%s\n' '{"op": "record-start", "name": "dsh"}' '{"action": "click", "text": "turn blue"}' '{"op": "record-stop", "value": "PASS: dsh"}' > "$T/rec.jsonl"

snip dsh-options dsh-plugin-options.json
sed -e "s|/abs/path/to/bladebro|$BIN|" -e "s|/path/to/project|$T|" "$SN/dsh-plugin-options.json" > "$T/opts.json"
echo "--- substituted: $(cat "$T/opts.json")"
node "$SC/dsh_stub.mjs" "$REPO" "$(cat "$T/opts.json")" "$T/bridge.json"; echo "stub exit=$?"
python3 -I "$SC/mcp_drive.py" --config "$T/bridge.json" --calls "$T/nav.jsonl" --calls "$T/rec.jsonl" --pause 1.2 2>&1 | grep -E '^(SERVER|TOOLS|CALL|RESULT isError=(true|false) (recording|outcome|✗)|STDERR \[bladebro\] browser mode|EXIT)'
echo '--- without binaryPath the plugin looks for the npm platform package (the published upstream binary, which has no recording):'
node "$SC/dsh_stub.mjs" "$REPO" '{"env":{"BLADE_RECORD":"1"}}' "$T/x.json" 2>&1 | grep -m1 'Error'

snip dsh-profile-headless dsh-profile-headless.yml
echo '--- a DSH host is not installed (dsh not on PATH), so this YAML is not loaded by DSH here. The config object it sets, {"env":{"BLADE_HEADLESS":"1"}}, plus binaryPath, is run through the plugin:'
( unset BLADE_HEADLESS
  node "$SC/dsh_stub.mjs" "$REPO" "{\"binaryPath\":\"$BIN\",\"env\":{\"BLADE_HEADLESS\":\"1\"}}" "$T/bridge2.json"
  python3 -I "$SC/mcp_drive.py" --config "$T/bridge2.json" --calls "$T/nav.jsonl" 2>&1 | grep -E '^(SERVER|TOOLS|RESULT isError|STDERR \[bladebro\] browser mode|EXIT)' | cut -c1-200 )
rm -rf "$T"
