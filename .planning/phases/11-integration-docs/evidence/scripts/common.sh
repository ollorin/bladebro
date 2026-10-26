# Sourced by every Phase 11 doc-snippet scenario (bash). Headless, scratch BLADE_HOME,
# local fixtures on 127.0.0.1:8765 only, no real profile, daemons stopped at exit.
REPO=/Users/sokolov.st@brainrocket.com/claude/bladebro
PHASE="$REPO/.planning/phases/11-integration-docs"
SN="${SN:-$PHASE/evidence/snippets}"
SC="${SC:-$REPO/tools/record_sidecar/scenarios}"
unset BLADE_HOME
source "$REPO/tools/record_sidecar/env.sh"   # scratch BLADE_HOME, BLADE_FFMPEG, no warming, fixture_up, bb_cleanup
export PATH="$REPO/target/release:$HOME/.local/ffmpeg/bin:$HOME/.cargo/bin:$PATH"
export BLADE_HEADLESS=1
unset BLADE_RECORD BLADE_RECORD_FORMAT BLADE_RECORD_FPS BLADE_RECORD_SIZE BLADE_RECORD_TABS
FFPROBE="$HOME/.local/ffmpeg/bin/ffprobe"
finish() {
  bladebro stop >/dev/null 2>&1
  bb_cleanup
  sleep 1
  if pgrep -f "$BLADE_HOME" >/dev/null; then echo "LEFTOVER processes:"; pgrep -fl "$BLADE_HOME"; else echo "no leftover processes for $BLADE_HOME"; fi
}
trap finish EXIT
# snip LABEL FILE: print the SNIPPET header and the exact snippet text that is run next
snip() { echo; echo "SNIPPET $1"; echo "--- text ($2):"; cat "$SN/$2"; echo "--- run:"; }
# probe FILE: ffprobe codec,width,height,fps and duration of a recording
probe() { echo "ffprobe $(basename "$1"): $("$FFPROBE" -v error -select_streams v:0 -show_entries stream=codec_name,width,height,r_frame_rate -of csv=p=0 "$1") duration=$("$FFPROBE" -v error -show_entries format=duration -of csv=p=0 "$1")"; }
# saved_path TEXT: the video path from a `recording saved: <path> (bytes ...` result
saved_path() { printf '%s\n' "$1" | sed -nE 's/.*recording saved: (.+\.(webm|mp4)) \(bytes .*/\1/p' | head -1; }
echo "# $(date -u +%FT%TZ) git $(git -C "$REPO" rev-parse --short HEAD) $(bladebro --version 2>&1 | head -1)"
echo "# BLADE_HOME=$BLADE_HOME BLADE_HEADLESS=$BLADE_HEADLESS BLADE_FFMPEG=$BLADE_FFMPEG fixtures=$FIX"
fixture_up || { echo "fixture server did not start"; exit 1; }
