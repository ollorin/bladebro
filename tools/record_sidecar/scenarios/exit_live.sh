#!/usr/bin/env bash
# Live proof that a recording is finalized (or honestly reported) on every drivable exit path, in
# both long-lived lanes (MCP stdio, CLI daemon), plus the one-shot refusal (Phase 9, 09-03).
# Safety: BLADE_HEADLESS=1, BLADE_RECORD=1, BLADE_NO_UPDATE_CHECK=1, a fresh scratch BLADE_HOME per
# scenario (mktemp), 127.0.0.1 fixtures only, only pids found under the scratch home are signalled.
# The real-browser lane is never touched.
#
#   bash tools/record_sidecar/scenarios/exit_live.sh [--only ID]
#   ids: mcp_eof mcp_sigterm mcp_sigint mcp_idle daemon_stop daemon_sigterm daemon_sigint daemon_idle
#        mcp_kill9 daemon_kill9 mcp_kill_chrome daemon_kill_chrome mcp_kill_ffmpeg daemon_kill_ffmpeg oneshot
#
# Inputs (harness inputs, not product options): EXIT_EVIDENCE (default
# .planning/phases/09-finalization-on-every-exit-path/evidence/09-03-live.txt, truncated unless --only),
# FIX_PORT (default 8765), EXIT_BIN, BLADE_FFMPEG / BLADE_FFPROBE. Exit code = number of FAIL lines.
# Not driven live (documented in the docs): disk low and the 2 h / 1 GiB limits (unit-level, injected).
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../env.sh
source "$HERE/../env.sh"
export BLADE_HEADLESS=1 BLADE_NO_UPDATE_CHECK=1
EXIT_EVIDENCE="${EXIT_EVIDENCE:-$REPO/.planning/phases/09-finalization-on-every-exit-path/evidence/09-03-live.txt}"
mkdir -p "$(dirname "$EXIT_EVIDENCE")"
ONLY=()
if [ "${1:-}" = "--only" ]; then ONLY=(--only "${2:-}"); else : > "$EXIT_EVIDENCE"; fi

# env.sh created a scratch home we do not use for scenarios; it only backs bb_cleanup's `stop`.
trap bb_cleanup EXIT
fixture_up || exit 1
cd "$REPO" || exit 1
cargo build --release >/dev/null 2>&1 || { echo "cargo build failed" >&2; exit 1; }
{ echo "# exit_live.sh 09-03  started $(date -u +%FT%TZ)  headless=$BLADE_HEADLESS  fixtures=$FIX"
  echo "# binary: $BB ($("$BB" --version 2>/dev/null | head -1))  git: $(git rev-parse --short HEAD)"
} | tee -a "$EXIT_EVIDENCE"
"$BB" stop >/dev/null 2>&1   # the old-inode gotcha: never test a daemon of a previous build
REC_HUB_FIX="$FIX" EXIT_EVIDENCE="$EXIT_EVIDENCE" EXIT_BIN="$BB" python3 -I "$RS/lib/exit_live.py" ${ONLY[@]+"${ONLY[@]}"}
rc=$?
exit "$rc"
