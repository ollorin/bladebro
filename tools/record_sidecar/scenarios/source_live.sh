#!/usr/bin/env bash
# Live screencast-source checks (06-02): real headless Chrome on local fixtures, the real
# FrameSource + Encoder (tests/record_live.rs), ffprobe + full-decode verification.
# Safety: BLADE_HEADLESS=1 always, a fresh scratch BLADE_HOME per scenario,
# BLADE_NO_UPDATE_CHECK=1, 127.0.0.1 fixtures only. Never touches a real profile or ~/.blade.
#
#   bash tools/record_sidecar/scenarios/source_live.sh [scenario ...]   (default: all)
#   scenarios: dur-idle dur-tick dur-anim resize bg bg-control   (bg-control runs inside bg)
# Writes: $EVIDENCE (default .planning/phases/06-frame-source/evidence/06-02-live.txt)
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../env.sh
source "$HERE/../env.sh"
export BLADE_HEADLESS=1 BLADE_NO_UPDATE_CHECK=1
# env.sh made BLADE_HOME only if unset; each scenario gets its own, so drop the shared one.
FFPROBE="${BLADE_FFPROBE:-$HOME/.local/ffmpeg/bin/ffprobe}"
FFMPEG="${BLADE_FFMPEG:-$HOME/.local/ffmpeg/bin/ffmpeg}"
EVIDENCE="${EVIDENCE:-$REPO/.planning/phases/06-frame-source/evidence/06-02-live.txt}"
mkdir -p "$(dirname "$EVIDENCE")"
: > "$EVIDENCE"

trap 'bb_cleanup; [ -n "${SCN_HOME:-}" ] && rm -rf "$SCN_HOME"' EXIT
fixture_up || exit 1

cd "$REPO" || exit 1
cargo build --release >/dev/null 2>&1 || { echo "cargo build failed" >&2; exit 1; }
BIN=$(cargo test --release --test record_live --no-run --message-format=json 2>/dev/null \
  | python3 -I -c 'import sys,json
for l in sys.stdin:
    try: m=json.loads(l)
    except Exception: continue
    if m.get("reason")=="compiler-artifact" and m.get("executable") and m["target"]["name"]=="record_live":
        print(m["executable"])' | tail -1)
[ -x "$BIN" ] || { echo "record_live test binary not found" >&2; exit 1; }
echo "# test binary: $BIN" >> "$EVIDENCE"
echo "# chrome: $("$BB" version 2>/dev/null | head -1)" >> "$EVIDENCE"
echo "# started: $(date -u +%FT%TZ)  headless=$BLADE_HEADLESS  fixtures=$FIX" >> "$EVIDENCE"

say() { echo "$*" | tee -a "$EVIDENCE"; }

# analyse ID OUT WALL_MS  -> ffprobe json, decode result, drift
analyse() {
  local id="$1" out="$2" wall="$3"
  if [ ! -s "$out" ]; then say "[$id] no output file (expected for a no-frames outcome)"; return; fi
  local pj dec_err dec_rc nb
  pj=$("$FFPROBE" -v error -show_entries stream=codec_name,width,height:format=duration,format_name -of json "$out")
  say "[$id] ffprobe: $(echo "$pj" | tr -d '\n ')"
  dec_err=$("$FFMPEG" -v warning -i "$out" -f null - 2>&1 >/dev/null); dec_rc=$?
  nb=$("$FFPROBE" -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 "$out")
  say "[$id] decode: exit=$dec_rc warning_lines=$(printf '%s' "$dec_err" | grep -c .) nb_read_frames=$nb"
  [ -n "$dec_err" ] && printf '%s\n' "$dec_err" | head -5 | sed "s/^/[$id] decode-warn: /" >> "$EVIDENCE"
  echo "$pj" | python3 -I -c "
import sys,json
d=json.load(sys.stdin); dur=float(d['format']['duration'])*1000; wall=$wall
print('[$id] drift: ffprobe_duration_ms=%.0f wall_ms=%d drift_pct=%+.2f' % (dur, wall, (dur-wall)/wall*100))" | tee -a "$EVIDENCE"
}

leftovers() {
  local id="$1" n
  "$BB" stop >/dev/null 2>&1
  sleep 1.5
  n=$( { pgrep -fl "$SCN_HOME"; pgrep -fl "$SCRATCH"; pgrep -fl record_live; pgrep -fl "target/release/bladebro"; } 2>/dev/null | grep -v "pgrep" | grep -c . )
  say "[$id] leftover processes after stop: $n"
  if [ "$n" != "0" ]; then { pgrep -fl "$SCN_HOME"; pgrep -fl "$SCRATCH"; pgrep -fl record_live; pgrep -fl "target/release/bladebro"; } 2>/dev/null | head >> "$EVIDENCE"; fi
}

# first_page_target PORT -> target id of the first real page
page_targets() {
  curl -s "127.0.0.1:$1/json" | python3 -I -c 'import sys,json
for t in json.load(sys.stdin):
    if t.get("type")=="page" and not t.get("url","").startswith("devtools://"): print(t["id"], t["url"])'
}

new_home() { SCN_HOME=$(mktemp -d -t bbrecscn); export BLADE_HOME="$SCN_HOME"; }
end_home() { leftovers "$1"; rm -rf "$SCN_HOME"; SCN_HOME=""; }

# record ID PORT TARGET SECS MODE OUT  -> prints wall_ms via global LAST_WALL; appends REPORT
record() {
  local id="$1" port="$2" tgt="$3" secs="$4" mode="$5" out="$6" line
  line=$(REC_LIVE_PORT="$port" REC_LIVE_TARGET="$tgt" REC_LIVE_OUT="$out" REC_LIVE_SECS="$secs" \
    REC_LIVE_MODE="$mode" "$BIN" --ignored --exact record_live --nocapture 2>"$SCRATCH/$id.stderr" | grep '^REC_LIVE_REPORT ')
  if [ -z "$line" ]; then say "[$id] REC_LIVE_REPORT missing; stderr tail:"; tail -5 "$SCRATCH/$id.stderr" | tee -a "$EVIDENCE"; LAST_WALL=0; return 1; fi
  say "[$id] $line"
  LAST_WALL=$(echo "${line#REC_LIVE_REPORT }" | python3 -I -c 'import sys,json; print(json.load(sys.stdin)["wall_ms"])')
  grep -i "no screencast frames" "$SCRATCH/$id.stderr" | head -2 | sed "s/^/[$id] stderr-warning: /" | tee -a "$EVIDENCE"
}

scenario_dur() {   # id page secs mode
  local id="$1" page="$2" secs="$3" mode="${4:-plain}" P T out
  new_home
  say "=== $id: nav $page, ${secs}s $mode ==="
  "$BB" nav "$FIX/$page" >/dev/null 2>&1
  P=$(bb_port) || { say "[$id] no chrome port"; end_home "$id"; return; }
  read -r T _ < <(page_targets "$P" | head -1)
  out="$SCRATCH/$id.webm"
  record "$id" "$P" "$T" "$secs" "$mode" "$out" && analyse "$id" "$out" "$LAST_WALL"
  end_home "$id"
}

scenario_bg() {
  local P T1 T2 out
  new_home
  say "=== bg: idle.html in T1, b.html opened in front, record T1 (8s) then the front tab ==="
  "$BB" nav "$FIX/idle.html" >/dev/null 2>&1
  P=$(bb_port) || { say "[bg] no chrome port"; end_home bg; return; }
  read -r T1 _ < <(page_targets "$P" | head -1)
  "$BB" state open-tab "$FIX/b.html" >/dev/null 2>&1
  sleep 1
  say "[bg] page targets: $(page_targets "$P" | tr '\n' ';')"
  T2=$(page_targets "$P" | grep "b.html" | awk '{print $1}' | head -1)
  out="$SCRATCH/bg.webm"
  record bg "$P" "$T1" 8 plain "$out"; analyse bg "$out" "$LAST_WALL"
  out="$SCRATCH/bg-control.webm"
  record bg-control "$P" "$T2" 8 plain "$out" && analyse bg-control "$out" "$LAST_WALL"
  end_home bg
}

WANT=("$@"); [ ${#WANT[@]} -eq 0 ] && WANT=(dur-idle dur-tick dur-anim resize bg)
for s in "${WANT[@]}"; do
  case "$s" in
    dur-idle) scenario_dur dur-idle idle.html 20 ;;
    dur-tick) scenario_dur dur-tick tick.html 20 ;;
    dur-anim) scenario_dur dur-anim anim.html 20 ;;
    resize)   scenario_dur resize anim.html 16 resize ;;
    bg)       scenario_bg ;;
    *) echo "unknown scenario $s" >&2 ;;
  esac
done
say "# finished: $(date -u +%FT%TZ)"
