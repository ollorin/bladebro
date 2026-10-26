#!/usr/bin/env bash
# Live proof of record-stop evidence extras (10-01): the poster PNG of the finished video, the marker
# offset on the video clock, and the stop latency against a baseline binary. Headless, CLI daemon lane,
# scratch BLADE_HOME per scenario, 127.0.0.1 fixtures only (mark.html: red page, "turn blue"/"turn green").
#   bash tools/record_sidecar/scenarios/evidence_live.sh [--only ID]
#     ids: poster poster-mp4 marker marker-bad per-tab latency (default: all)
#   EVIDENCE   output file (default .planning/phases/10-evidence-extras/evidence/10-01-live.txt)
#   BASE_BIN   baseline binary for `latency` (the d18dae1 build; required for latency)
# Every check prints PASS [id] ... or FAIL [id] ...; a FAIL is reported, never weakened.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/../env.sh"
source "$HERE/follow_common.sh"
export BLADE_HEADLESS=1 BLADE_NO_UPDATE_CHECK=1
FFPROBE="${BLADE_FFPROBE:-$HOME/.local/ffmpeg/bin/ffprobe}"
FFMPEG="${BLADE_FFMPEG:-$HOME/.local/ffmpeg/bin/ffmpeg}"
EVIDENCE="${EVIDENCE:-$REPO/.planning/phases/10-evidence-extras/evidence/10-01-live.txt}"
NEW_BIN="$BB"
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
{ echo "# evidence_live.sh  started $(date -u +%FT%TZ)  headless=$BLADE_HEADLESS  fixtures=$FIX"
  echo "# new binary: $NEW_BIN ($("$NEW_BIN" --version 2>/dev/null | head -1))  git: $(git rev-parse --short HEAD)"
  echo "# baseline binary: ${BASE_BIN:-unset} ($([ -x "${BASE_BIN:-/nonexistent}" ] && "$BASE_BIN" --version 2>/dev/null | head -1))"; } | tee -a "$EVIDENCE"

secs() { python3 -I -c "import time;print(round(time.time(),3))"; }
mode_of() { if [ "$(uname)" = Darwin ]; then stat -f %Lp "$1"; else stat -c %a "$1"; fi; }
png_magic() { od -An -tx1 -N8 "$1" | tr -d ' \n'; }
# png_tone PNG: colour of the full frame scaled to one pixel (the same sampling for a PNG or a video frame)
png_tone() { tone $("$FFMPEG" -nostdin -v error -i "$1" -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - 2>/dev/null | od -An -tu1); }
frame_tone() { tone $("$FFMPEG" -nostdin -v error -ss "$2" -i "$1" -frames:v 1 -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - 2>/dev/null | od -An -tu1); }  # frame_tone FILE SECONDS
# dur_of FILE: container duration in seconds
dur_of() { "$FFPROBE" -v error -show_entries format=duration -of csv=p=0 "$1"; }
absent() { ! grep -q -- "$1" <<<"$2"; }   # absent NEEDLE TEXT
poster_path() { sed -nE 's/.*Poster: ([^ ]+\.png).*/\1/p' <<<"$1" | head -1; }
marker_at() { sed -nE 's/.*\. Marker at ([0-9.]+) s.*/\1/p' <<<"$1" | head -1; }

# poster_checks ID STOP VIDEO [COLOUR]: the poster contract, on one finished video (COLOUR: the last frame, default blue)
poster_checks() {
  local id="$1" stop="$2" v="$3" want="${4:-blue}" p pj vj nb dec rc stem
  p=$(poster_path "$stop")
  say "[$id] poster: ${p:-none}"
  check "$id" "stop text has '. Poster: <abs>.poster.png'" grep -qE '\. Poster: /[^ ]+\.poster\.png' <<<"$stop"
  [ -s "$p" ] || { say "FAIL [$id] poster file missing: ${p:-none}"; FAILS=$((FAILS + 1)); return; }
  stem=$(basename "$v"); stem="${stem%.*}"
  check "$id" "poster sits next to the video, named <video stem>.poster.png" test "$(dirname "$p")" = "$(dirname "$v")" -a "$(basename "$p")" = "$stem.poster.png"
  check "$id" "poster mode 600 (is $(mode_of "$p"))" test "$(mode_of "$p")" = 600
  check "$id" "poster starts with the PNG magic (is $(png_magic "$p"))" test "$(png_magic "$p")" = 89504e470d0a1a0a
  pj=$("$FFPROBE" -v error -select_streams v:0 -show_entries stream=codec_name,width,height -of csv=p=0 "$p")
  vj=$("$FFPROBE" -v error -select_streams v:0 -show_entries stream=codec_name,width,height -of csv=p=0 "$v")
  say "[$id] poster ffprobe: $pj   video ffprobe: $vj"
  check "$id" "poster is png at the video's width x height" test "$pj" = "png,$(cut -d, -f2,3 <<<"$vj")"
  check "$id" "poster shows the last frame: 1x1 tone is $want (rgb $("$FFMPEG" -nostdin -v error -i "$p" -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - 2>/dev/null | od -An -tu1 | tr -s ' ' | sed 's/^ //'))" test "$(png_tone "$p")" = "$want"
  dec=$("$FFMPEG" -nostdin -v error -i "$v" -f null - 2>&1 >/dev/null); rc=$?
  check "$id" "video full decode exit=$rc stderr_lines=$(printf '%s' "$dec" | grep -c .)" test "$rc" = 0 -a -z "$dec"
  nb=$("$FFPROBE" -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 "$v")
  check "$id" "decoded frames ($nb) == reported frames ($(num_after "$stop" frames))" test "$nb" = "$(num_after "$stop" frames)"
}

# poster_case ID FORMAT [VALUE]: fixture page, click "turn blue", stop (with --value VALUE when given)
poster_case() {
  local id="$1" fmt="$2" val="${3-__none__}" s
  new_home; export BLADE_RECORD=1 BLADE_RECORD_FORMAT="$fmt"
  "$BB" nav "$FIX/mark.html" >/dev/null 2>&1
  s=$("$BB" state record-start "$id" 2>&1); say "[$id] record-start: $s"
  sleep 1.5
  "$BB" act click "turn blue" 2>&1 | head -1 | sed "s/^/[$id] click: /" | tee -a "$EVIDENCE"
  sleep 1.5
  if [ "$val" = __none__ ]; then STOP=$("$BB" state record-stop 2>&1); STOP_RC=$?
  else STOP=$("$BB" state record-stop --value "$val" 2>&1); STOP_RC=$?; fi
  say "[$id] stop rc=$STOP_RC: $STOP"
  unset BLADE_RECORD_FORMAT BLADE_RECORD
}   # the caller runs its checks, then end_scn (the home holds the files the checks read)

sc_poster() { # poster (webm) and poster-mp4: same contract, one format each
  local fmt id="$1"
  case "$id" in poster-mp4) fmt=mp4 ;; *) fmt=webm ;; esac
  poster_case "$id" "$fmt"
  check "$id" "record-stop exits 0 and saves the $fmt" test "$STOP_RC" = 0 -a "$(grep -c 'recording saved: ' <<<"$STOP")" = 1
  poster_checks "$id" "$STOP" "$(saved_path "$STOP")"
  end_scn "$id"
}

sc_marker() {
  poster_case marker webm "FAIL: demo note"
  local v dur t
  v=$(saved_path "$STOP"); dur=$(num_after "$STOP" duration); t=$(marker_at "$STOP")
  check marker "record-stop exits 0 and saves the file" test "$STOP_RC" = 0 -a -n "$v"
  check marker "marker offset present (T=${t:-none} s, duration ${dur} s)" test -n "$t"
  check marker "marker T in [duration-0.6, duration]" python3 -I -c "import sys; d=float('$dur'); t=float('$t'); sys.exit(0 if d-0.6 <= t <= d else 1)"
  check marker "note text is not echoed in the result" absent "demo note" "$STOP"
  poster_checks marker "$STOP" "$v"
  end_scn marker
}

sc_marker_bad() {
  poster_case marker-bad webm ""
  check marker-bad "record-stop exits 0 and saves the file" test "$STOP_RC" = 0 -a "$(grep -c 'recording saved: ' <<<"$STOP")" = 1
  check marker-bad "empty note: '. Marker not placed: empty note'" grep -q '\. Marker not placed: empty note' <<<"$STOP"
  check marker-bad "no marker offset in the result" absent "Marker at" "$STOP"
  end_scn marker-bad
}

sc_per_tab() {
  new_home; export BLADE_RECORD=1 BLADE_RECORD_TABS=per-tab
  "$BB" nav "$FIX/mark.html" >/dev/null 2>&1
  local s o t2 stop rc f1 f2 pname
  s=$("$BB" state record-start pt 2>&1); say "[per-tab] record-start: $s"
  sleep 1.5
  o=$("$BB" state open-tab "$FIX/mark.html" 2>&1); say "[per-tab] open-tab: $o"; t2=$(opened_id "$o")
  sleep 0.5
  "$BB" act click "turn green" 2>&1 | head -1 | sed 's/^/[per-tab] click: /' | tee -a "$EVIDENCE"
  sleep 1.5
  stop=$("$BB" state record-stop 2>&1); rc=$?
  say "[per-tab] stop rc=$rc: $stop"
  f2=$(grep -oE '/[^ ;(]+-tab2-[^ ;(]+\.webm' <<<"$stop" | head -1)
  f1=$(grep -oE '/[^ ;(]+-tab1-[^ ;(]+\.webm' <<<"$stop" | head -1)
  check per-tab "stop exits 0 with two per-tab files" test "$rc" = 0 -a "$(grep -c 'recording saved (per-tab, 2 files): ' <<<"$stop")" = 1
  check per-tab "exactly one Poster in the result" test "$(grep -o 'Poster: ' <<<"$stop" | wc -l | tr -d ' ')" = 1
  pname=$(basename "$(poster_path "$stop")")
  check per-tab "poster is named after the -tab2- file (the followed tab)" test -n "$f2" -a "$pname" = "$(basename "$f2" .webm).poster.png"
  poster_checks per-tab "$stop" "$f2" green
  local end2; end2=$(python3 -I -c "print(round($(dur_of "$f2")-0.3,2))")
  check per-tab "tab2 file ends green (frame at ${end2} s: $(frame_tone "$f2" "$end2"))" test "$(frame_tone "$f2" "$end2")" = green
  unset BLADE_RECORD_TABS BLADE_RECORD
  end_scn per-tab
}

# stop_wall BIN: one recording on mark.html, returns the wall time of the record-stop call (LAT_S)
stop_wall() {
  local bin="$1" t0 t1 s
  BB="$bin"; CUR_BIN="$bin"; new_home; export BLADE_RECORD=1 BLADE_RECORD_FORMAT=webm
  "$BB" nav "$FIX/mark.html" >/dev/null 2>&1; "$BB" state record-start lat >/dev/null 2>&1; sleep 1.5
  "$BB" act click "turn blue" >/dev/null 2>&1; sleep 1.5
  t0=$(secs); s=$("$BB" state record-stop 2>&1); t1=$(secs)
  LAT_S=$(python3 -I -c "print(round($t1-$t0,3))"); LAT_TXT="$s"
  # poster ffmpeg alone (same argv as make_poster), timed before teardown removes the video
  local v; v=$(saved_path "$s")
  if [ "$bin" = "$NEW_BIN" ] && [ -s "$v" ]; then
    local tmp i a b; tmp=$(mktemp -d -t bbposter)
    for i in 1 2 3 4 5; do
      a=$(secs)
      "$FFMPEG" -nostdin -hide_banner -loglevel error -sseof -1 -i "$v" -map 0:v:0 -an -update 1 -f image2 -c:v png -y "$tmp/p$i.png" 2>/dev/null
      b=$(secs); POSTER_T="$POSTER_T $(python3 -I -c "print(round($b-$a,3))")"
    done
    rm -rf "$tmp"
  fi
  unset BLADE_RECORD BLADE_RECORD_FORMAT
  end_scn latency
  BB="$NEW_BIN"; CUR_BIN="$BB"
}
sc_latency() {
  if [ ! -x "${BASE_BIN:-/nonexistent}" ]; then say "FAIL [latency] BASE_BIN is not an executable baseline binary"; FAILS=$((FAILS + 1)); return; fi
  local i base="" new="" b n
  POSTER_T=""
  say "== latency: 3 interleaved runs each of the same poster scenario (stop call wall time)"
  for i in 1 2 3; do
    stop_wall "$BASE_BIN"; b=$LAT_S; base="$base $b"; say "[latency] base run $i: ${b}s  stop text: $LAT_TXT"
    stop_wall "$NEW_BIN";  n=$LAT_S; new="$new $n";  say "[latency] new run $i: ${n}s  stop text: $LAT_TXT"
  done
  local bm nm
  bm=$(python3 -I -c "import statistics as s; print(round(s.median([$(tr ' ' ',' <<<"${base# }")]),3))")
  nm=$(python3 -I -c "import statistics as s; print(round(s.median([$(tr ' ' ',' <<<"${new# }")]),3))")
  say "record-stop latency base $bm s (runs${base}) / new $nm s (runs${new}) median"
  check latency "new - base median <= 1.0 s (delta $(python3 -I -c "print(round($nm-$bm,3))") s)" python3 -I -c "import sys; sys.exit(0 if $nm-$bm <= 1.0 else 1)"
  if [ -n "${POSTER_T// /}" ]; then
    say "poster ffmpeg alone (3 recordings x 5 runs, same argv as make_poster):${POSTER_T}"
    say "poster ffmpeg alone: typ $(python3 -I -c "import statistics as s; print(round(s.median([$(tr ' ' ',' <<<"${POSTER_T# }")]),3))") s, max $(python3 -I -c "print(max([$(tr ' ' ',' <<<"${POSTER_T# }")]))") s"
  fi
}

case "$ONLY" in
  ""|poster|poster-mp4|marker|marker-bad|per-tab|latency) : ;;
  *) echo "unknown id: $ONLY" >&2; exit 2 ;;
esac
want poster && sc_poster poster
want poster-mp4 && sc_poster poster-mp4
want marker && sc_marker
want marker-bad && sc_marker_bad
want per-tab && sc_per_tab
want latency && sc_latency
BB="$NEW_BIN"; CUR_BIN="$BB"

pleft=$( { pgrep -fl "bbfol|bbrec|bbproj|bbposter|bbev"; } 2>/dev/null | grep -v pgrep)
say "final pgrep of our fixtures/daemons: ${pleft:-none}"
say "== total FAIL count: $FAILS"
exit "$([ "$FAILS" = 0 ] && echo 0 || echo 1)"
