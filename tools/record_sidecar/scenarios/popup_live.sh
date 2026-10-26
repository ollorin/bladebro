#!/usr/bin/env bash
# Live proof of popup following (08.1): headless, scratch BLADE_HOME per scenario, 127.0.0.1 fixtures.
#   bash tools/record_sidecar/scenarios/popup_live.sh [--only ID]    ids: spike (Plan 02 adds the rest)
#   EVIDENCE  output file (default evidence/08.1-02-live.txt)
# spike: does keeping a clicked popup in front while recording (D1 option A) make it visible and
# recordable? Measures head-of-/json share, popup segment frames/tone, opener `see`/`vision` health.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/../env.sh"
source "$HERE/follow_common.sh"
export BLADE_HEADLESS=1 BLADE_NO_UPDATE_CHECK=1
FFPROBE="${BLADE_FFPROBE:-$HOME/.local/ffmpeg/bin/ffprobe}"
FFMPEG="${BLADE_FFMPEG:-$HOME/.local/ffmpeg/bin/ffmpeg}"
EVIDENCE="${EVIDENCE:-$REPO/.planning/phases/08.1-follow-every-popup/evidence/08.1-02-live.txt}"
mkdir -p "$(dirname "$EVIDENCE")"
ONLY=""
if [ "${1:-}" = "--only" ]; then ONLY="${2:-}"; else : > "$EVIDENCE"; fi
cleanup_all() {
  [ -n "$SCN_HOME" ] && { BLADE_HOME="$SCN_HOME" "$BB" stop >/dev/null 2>&1; rm -rf "$SCN_HOME"; }
  bb_cleanup
}
trap cleanup_all EXIT
fixture_up || exit 1
cd "$REPO" || exit 1
{ echo "# popup_live.sh  started $(date -u +%FT%TZ)  headless=$BLADE_HEADLESS  fixtures=$FIX"
  echo "# git: $(git rev-parse --short HEAD) (+worktree)"; } | tee -a "$EVIDENCE"

head_id() { curl -s "http://127.0.0.1:$1/json" | python3 -I -c 'import json,sys
for t in json.load(sys.stdin):
    if t.get("type")=="page": print(t["id"]); break'; }
secs() { python3 -I -c "import time;print(round(time.time(),3))"; }

# spike_kind KIND CLICK_TEXT RECORD(1|0)
spike_kind() {
  local kind="$1" text="$2" rec="$3" O port n=0 hits=0 H t0 t1 stop P pj
  new_home
  if [ "$rec" = 1 ]; then export BLADE_RECORD=1; else unset BLADE_RECORD; fi
  "$BB" nav "$FIX/popper.html" >/dev/null 2>&1
  O=$(cur_id); port=$(bb_port)
  say "[spike:$kind rec=$rec] opener=$O port=$port"
  t0=$(secs); "$BB" see content >/dev/null 2>&1; t1=$(secs)
  local see_before; see_before=$(python3 -I -c "print(round($t1-$t0,2))")
  [ "$rec" = 1 ] && "$BB" state record-start "sp-$kind" >/dev/null 2>&1
  sleep 1
  "$BB" act click "$text" 2>&1 | head -1 | sed "s/^/[spike:$kind] click: /" | tee -a "$EVIDENCE"
  local first_head="" last=""
  while [ $n -lt 30 ]; do
    H=$(head_id "$port")
    [ -z "$first_head" ] && first_head="$H"
    [ "$H" != "$O" ] && hits=$((hits + 1)); last="$H"; n=$((n + 1)); sleep 0.1
  done
  say "[spike:$kind rec=$rec] samples=$n popup_head=$hits share=$(python3 -I -c "print(round($hits/$n*100))")% last_head=$last"
  P="$last"
  if [ "$rec" = 1 ]; then
    t0=$(secs); local sv; sv=$("$BB" see content 2>&1); t1=$(secs)
    local see_after; see_after=$(python3 -I -c "print(round($t1-$t0,2))")
    say "[spike:$kind] see before=${see_before}s after=${see_after}s bytes_after=$(printf '%s' "$sv" | wc -c | tr -d ' ')"
    "$BB" vision >/dev/null 2>&1; say "[spike:$kind] vision exit=$?"
    sleep 1
    stop=$("$BB" state record-stop 2>&1); say "[spike:$kind] stop: $stop"
    local pp; pp=$(saved_path "$stop")
    local segs; segs=$(seg_ids "$stop" | tr '\n' ' ')
    say "[spike:$kind] segments: $segs  popup_segment_no_frames=$(grep -c "tab $P (no frames)" <<<"$stop")"
    if [ -n "$pp" ] && [ -s "$pp" ]; then
      local total; total=$("$FFPROBE" -v error -show_entries format=duration -of csv=p=0 "$pp")
      local n_seg; n_seg=$(seg_ids "$stop" | wc -l | tr -d ' ')
      if [ "$n_seg" -ge 2 ]; then
        say "[spike:$kind] tone opener-seg=$(tone_at "$pp" "$(mid_of "$stop" 0 "$total")") popup-seg(1)=$(tone_at "$pp" "$(mid_of "$stop" 1 "$total")") total=${total}s"
      local tl="" k=0; while [ "$k" -lt "$(python3 -I -c "print(int($total*2))")" ]; do tl="$tl $(python3 -I -c "print($k/2)")=$(tone_at "$pp" "$(python3 -I -c "print($k/2+0.2)")")"; k=$((k + 1)); done
      say "[spike:$kind] tone timeline:$tl"
      else say "[spike:$kind] only $n_seg segment: popup never recorded"; fi
    fi
    "$BB" act press Tab >/dev/null 2>&1; sleep 1
    say "[spike:$kind] head 1s after act press Tab: $(head_id "$port") (opener=$O)"
  else
    sleep 0.9
    say "[spike:$kind rec=0] head ~1s after click: $(head_id "$port") (opener=$O) -> $([ "$(head_id "$port")" = "$O" ] && echo returns-to-opener || echo STAYS-ON-POPUP)"
  fi
  unset BLADE_RECORD
  end_scn "spike:$kind"
}

sc_spike() {
  say "== spike: option A (skip opener re-front while recording)"
  spike_kind blank "open blank" 1
  spike_kind js "open js" 1
  spike_kind blank "open blank" 0
  spike_kind js "open js" 0
}
want spike && sc_spike
say "FAILS=$FAILS"
