# Source this file (bash). Sets up a scratch BLADE_HOME, local fixture server and paths
# for the record_sidecar probe. Never points at a real browser profile or ~/.blade.
#
#   source tools/record_sidecar/env.sh; trap bb_cleanup EXIT; fixture_up

_RS_SELF="${BASH_SOURCE[0]:-$0}"
REPO="$(cd "$(dirname "$_RS_SELF")/../.." && pwd)"
RS="$REPO/tools/record_sidecar"
BB="$REPO/target/release/bladebro"

# Scratch home: created by this file only when the caller did not supply one.
if [ -z "${BLADE_HOME:-}" ]; then
  export BLADE_HOME="$(mktemp -d -t bbrec)"
  _RS_OWN_HOME=1
else
  _RS_OWN_HOME=0
fi
export BLADE_NO_UPDATE_CHECK=1 BLADE_NO_WARMING=1 BLADE_LANE=agent
unset BLADE_TRANSPORT   # WS transport only; pipe would make webdriver==true

export PATH="$HOME/.cargo/bin:$PATH"
export BLADE_FFMPEG="${BLADE_FFMPEG:-$HOME/.local/ffmpeg/bin/ffmpeg}"
export BLADE_FFPROBE="${BLADE_FFPROBE:-$HOME/.local/ffmpeg/bin/ffprobe}"

FIX_PORT="${FIX_PORT:-8765}"
FIX="http://127.0.0.1:$FIX_PORT"
SCRATCH="${SCRATCH:-$(mktemp -d -t bbrecvid)}"
FIX_PID=""

fixture_up() {
  python3 -m http.server "$FIX_PORT" --bind 127.0.0.1 --directory "$RS/fixtures" >/dev/null 2>&1 &
  FIX_PID=$!
  local i=0
  while [ $i -lt 50 ]; do
    if curl -sf "$FIX/idle.html" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
    i=$((i + 1))
  done
  echo "fixture_up: server on $FIX did not answer within 5s" >&2
  return 1
}

# Print the remote-debugging port of the daemon Chrome launched under $BLADE_HOME.
# Primary: the main Chrome process command line (--remote-debugging-port=<N>), because the
# agent lane arms a FIXED port and Chrome does NOT write DevToolsActivePort for that
# (it only does for --remote-debugging-port=0; see src/browser/launch.rs).
# Secondary: DevToolsActivePort line 1, if a file exists anywhere under $BLADE_HOME.
bb_port() {
  local p f d
  for d in "$BLADE_HOME" "/private$BLADE_HOME"; do
    p=$(ps -ax -o command= | grep -- "--user-data-dir=$d" | grep -v Helper | grep -v grep \
        | sed -nE 's/.*--remote-debugging-port=([0-9]+).*/\1/p' | head -1)
    if [ -n "$p" ]; then echo "$p"; return 0; fi
  done
  f=$(find "$BLADE_HOME" -maxdepth 4 -name DevToolsActivePort 2>/dev/null | head -1)
  if [ -n "$f" ] && [ -s "$f" ]; then
    head -1 "$f"
    return 0
  fi
  return 1
}

bb_cleanup() {
  "$BB" stop >/dev/null 2>&1
  if [ -n "${FIX_PID:-}" ]; then kill "$FIX_PID" 2>/dev/null; fi
  if [ -n "${SCRATCH:-}" ]; then
    pkill -f "$SCRATCH" 2>/dev/null
    rm -rf "$SCRATCH"
  fi
  if [ "${_RS_OWN_HOME:-0}" = "1" ] && [ -n "${BLADE_HOME:-}" ]; then
    rm -rf "$BLADE_HOME"
  fi
}
