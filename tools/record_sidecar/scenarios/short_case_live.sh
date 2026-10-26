#!/usr/bin/env bash
# Live proof for Phase 11.1: a record-stop issued 0.1 s / 0.5 s after record-start on the static mark.html
# fixture gives a decodable file with >= 1 frame (MCP follow, CLI daemon, MCP per-tab), plus the docs'
# assertion-only case as written. Headless, scratch BLADE_HOME per run, 127.0.0.1 fixtures only.
#   [SHORT_BIN=<binary>] [EVIDENCE=<file>] [RUNS=3] bash tools/record_sidecar/scenarios/short_case_live.sh [--only id,id]
#   ids: mcp-0.1 mcp-0.5 daemon-0.1 daemon-0.5 pertab-0.1 pertab-0.5 doc-case
# Prints PASS [id #n] / FAIL [id #n] lines; exit code = number of FAIL lines.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/../env.sh"
trap bb_cleanup EXIT
fixture_up || exit 1
export SHORT_BIN="${SHORT_BIN:-$BB}" FIX MCP_DRIVE="$RS/scenarios/mcp_drive.py"
export FFPROBE="$BLADE_FFPROBE" FFMPEG="$BLADE_FFMPEG"
export EVIDENCE="${EVIDENCE:-$REPO/.planning/phases/11.1-short-case-frame-guarantee/evidence/11.1-01-live.txt}"
mkdir -p "$(dirname "$EVIDENCE")"
python3 -I "$RS/lib/short_case_live.py" "$@"
