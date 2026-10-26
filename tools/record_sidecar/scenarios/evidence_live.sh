#!/usr/bin/env bash
# Live proof of record-stop evidence extras. 10-01: the poster PNG of the finished video, the marker offset
# on the video clock, and the stop latency against a baseline binary. 10-02: the action timeline sidecar
# (offsets checked against the video's own frames), per-tab timeline, the MCP stdio lane, and recording-off
# identity against a baseline binary (CDP method trace + normalized stdout). Headless, CLI daemon lane unless
# stated, scratch BLADE_HOME per scenario, 127.0.0.1 fixtures only (mark.html: red page, "turn blue"/"turn green").
#   bash tools/record_sidecar/scenarios/evidence_live.sh [--only ID[,ID...]]
#     10-01 ids: poster poster-mp4 marker marker-bad per-tab latency
#     10-02 ids: timeline timeline-mp4 timeline-run per-tab-timeline off mcp
#     (default, no --only: the 10-01 ids; pass --only for the 10-02 ids)
#   EVIDENCE   output file (default .planning/phases/10-evidence-extras/evidence/10-01-live.txt; the 10-02 run sets it)
#   BASE_BIN   baseline binary for `latency` and `off` (the d18dae1 build; required for those ids)
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
KNOWN=" poster poster-mp4 marker marker-bad per-tab latency timeline timeline-mp4 timeline-run per-tab-timeline off mcp "
for tok in ${ONLY//,/ }; do
  case "$KNOWN" in *" $tok "*) : ;; *) echo "unknown id: $tok" >&2; exit 2 ;; esac
done
# want ID: true when no --only, or ID is in the comma list
want() { [ -z "$ONLY" ] || [[ ",$ONLY," == *",$1,"* ]]; }
cleanup_all() {
  [ -n "$SCN_HOME" ] && { BLADE_HOME="$SCN_HOME" "$BB" stop >/dev/null 2>&1; rm -rf "$SCN_HOME"; }
  bb_cleanup
}
trap cleanup_all EXIT
fixture_up || exit 1
cd "$REPO" || exit 1
{ echo "# evidence_live.sh  started $(date -u +%FT%TZ)  headless=$BLADE_HEADLESS  fixtures=$FIX  ids=${ONLY:-all-10-01}"
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
timeline_path() { sed -nE 's/.*Timeline: ([^ ]+\.timeline\.json).*/\1/p' <<<"$1" | head -1; }
timeline_n() { sed -nE 's/.*Timeline: [^ ]+\.timeline\.json \(([0-9]+) actions.*/\1/p' <<<"$1" | head -1; }
# tl_is FILE EXPR: exit 0 when the Python expression over the parsed timeline `d` is true
tl_is() { python3 -I -c "import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if ($2) else 1)" "$1"; }
# tl_py FILE EXPR: print the value of EXPR over the parsed timeline `d`
tl_py() { python3 -I -c "import json,sys; d=json.load(open(sys.argv[1])); print($2)" "$1"; }
addt() { python3 -I -c "print(max(0.0, round($1 + ($2), 2)))"; }   # addt T DELTA: T + DELTA, floored at 0

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

# ---------------------------------------------------------------- 10-02: timeline sidecar

# tl_case ID FORMAT: fixture page, record, blue click, green click, a click that fails, stop with a marker
tl_case() {
  local id="$1" fmt="$2" s
  new_home; export BLADE_RECORD=1 BLADE_RECORD_FORMAT="$fmt"
  "$BB" nav "$FIX/mark.html" >/dev/null 2>&1
  s=$("$BB" state record-start "$id" 2>&1); say "[$id] record-start: $s"
  sleep 1.5
  "$BB" act click "turn blue" 2>&1 | head -1 | sed "s/^/[$id] click: /" | tee -a "$EVIDENCE"
  sleep 1.5
  "$BB" act click "turn green" 2>&1 | head -1 | sed "s/^/[$id] click: /" | tee -a "$EVIDENCE"
  sleep 1.5
  "$BB" act click "no such button" 2>&1 | head -1 | sed "s/^/[$id] click: /" | tee -a "$EVIDENCE"
  sleep 1
  STOP=$("$BB" state record-stop --value "FAIL: demo" 2>&1); STOP_RC=$?
  say "[$id] stop rc=$STOP_RC: $STOP"
  unset BLADE_RECORD_FORMAT BLADE_RECORD
}

# align_report ID NAME VIDEO END_S WANT: the first frame (0.1 s steps from END_S-0.5 to END_S+1.5) showing
# WANT, and its offset from END_S. Resolution is the 0.1 s step; a negative value means the colour was
# already WANT before the click ended.
align_report() {
  local id="$1" name="$2" v="$3" e="$4" want="$5" first="" t err
  for t in $(python3 -I -c "print(' '.join('%.2f' % (max(0.0, $e - 0.5) + 0.1 * i) for i in range(21)))"); do
    if [ "$(frame_tone "$v" "$t")" = "$want" ]; then first=$t; break; fi
  done
  if [ -z "$first" ]; then say "FAIL [$id] alignment: no $want frame in end_s-0.5 .. end_s+1.5 for the $name click"; FAILS=$((FAILS + 1)); return; fi
  err=$(python3 -I -c "print(round($first - $e, 2))")
  say "alignment error $name click: $err s (end_s $e, first $want frame at $first s; 0.1 s scan step)"
  echo "$err" >> "$SCRATCH/align-$id.txt"
}

sc_timeline_fmt() { # ID FORMAT: the three-click timeline on one finished video; colours and offsets from its frames
  local id="$1" fmt="$2" v tl stem blue_end blue_at green_end green_at t
  tl_case "$id" "$fmt"
  check "$id" "record-stop exits 0 and saves the $fmt" test "$STOP_RC" = 0 -a "$(grep -c 'recording saved: ' <<<"$STOP")" = 1
  v=$(saved_path "$STOP"); tl=$(timeline_path "$STOP")
  say "[$id] timeline: ${tl:-none}  video: ${v:-none}"
  if [ -z "$tl" ] || [ ! -s "$tl" ] || [ ! -s "$v" ]; then
    say "FAIL [$id] timeline or video missing"; FAILS=$((FAILS + 1)); end_scn "$id"; return
  fi
  stem=$(basename "$v"); stem="${stem%.*}"
  check "$id" "timeline path printed: '. Timeline: <abs>.timeline.json (N actions)'" grep -qE '\. Timeline: /[^ ]+\.timeline\.json \([0-9]+ actions\)' <<<"$STOP"
  check "$id" "timeline mode 600 (is $(mode_of "$tl"))" test "$(mode_of "$tl")" = 600
  check "$id" "timeline named <video stem>.timeline.json next to the video" test "$(dirname "$tl")" = "$(dirname "$v")" -a "$(basename "$tl")" = "$stem.timeline.json"
  check "$id" "JSON version 1, basenames only, dropped 0" tl_is "$tl" "d['version']==1 and d['video']=='$(basename "$v")' and '/' not in d['video'] and all('/' not in f['file'] for f in d['files']) and d['dropped']==0"
  check "$id" "result N actions == action entries in the JSON" test "$(timeline_n "$STOP")" = "$(tl_py "$tl" "sum(1 for e in d['entries'] if e['kind']=='action')")"
  say "[$id] entries: $(tl_py "$tl" "' | '.join((e['kind']+' '+str(e.get('action'))+' ok='+str(e.get('ok'))+' at_s='+str(e['at_s'])+' end_s='+str(e.get('end_s'))+' verdict='+repr(e.get('verdict'))+' note='+repr(e.get('note'))) for e in d['entries'])")"
  check "$id" "entries in order: click(ok) click(ok) click(ok:false) marker" tl_is "$tl" "[(e['kind'], e.get('action'), e.get('ok')) for e in d['entries']] == [('action','click',True),('action','click',True),('action','click',False),('marker',None,None)]"
  check "$id" "failed click carries a non-empty verdict" tl_is "$tl" "d['entries'][2]['verdict'] != ''"
  check "$id" "marker entry note is 'FAIL: demo' and last on the clock" tl_is "$tl" "d['entries'][3]['note']=='FAIL: demo' and d['entries'][3]['at_s']>=d['entries'][2]['end_s']"
  check "$id" "note text is not echoed in the result" absent "FAIL: demo" "$STOP"
  check "$id" "every at_s and end_s <= duration_s ($(tl_py "$tl" "d['duration_s']") s)" tl_is "$tl" "all(e['at_s']<=d['duration_s'] and e.get('end_s',0)<=d['duration_s'] for e in d['entries'])"
  say "[$id] container duration: $(dur_of "$v") s"
  blue_end=$(tl_py "$tl" "d['entries'][0]['end_s']"); blue_at=$(tl_py "$tl" "d['entries'][0]['at_s']")
  green_end=$(tl_py "$tl" "d['entries'][1]['end_s']"); green_at=$(tl_py "$tl" "d['entries'][1]['at_s']")
  t=$(addt "$blue_end" 0.4); tone_check "$id" "blue click: end_s+0.4 s is blue" "$v" "$t" blue
  t=$(addt "$blue_at" -0.3); tone_check "$id" "blue click: at_s-0.3 s is the previous colour (red)" "$v" "$t" red
  t=$(addt "$green_end" 0.4); tone_check "$id" "green click: end_s+0.4 s is green" "$v" "$t" green
  t=$(addt "$green_at" -0.3); tone_check "$id" "green click: at_s-0.3 s is the previous colour (blue)" "$v" "$t" blue
  align_report "$id" blue "$v" "$blue_end" blue
  align_report "$id" green "$v" "$green_end" green
  end_scn "$id"
}

# tone_check ID LABEL VIDEO T WANT: one frame's colour against WANT
tone_check() {
  local c; c=$(frame_tone "$3" "$4")
  check "$1" "$2 at ${4} s (frame is $c)" test "$c" = "$5"
}

sc_timeline_run() {
  local id=timeline-run r rc stop tl
  new_home; export BLADE_RECORD=1 BLADE_RECORD_FORMAT=webm
  "$BB" nav "$FIX/mark.html" >/dev/null 2>&1
  "$BB" state record-start "$id" >/dev/null 2>&1; sleep 1.5
  r=$("$BB" run '[{"action":"click","text":"turn blue"},{"action":"wait","condition":"settle"},{"action":"click","text":"turn green"}]' 2>&1); rc=$?
  say "[$id] run rc=$rc: $(head -1 <<<"$r")"
  sleep 1.5
  stop=$("$BB" state record-stop 2>&1); say "[$id] stop: $stop"
  tl=$(timeline_path "$stop")
  check "$id" "run exits 0 and the stop saves a file with a timeline" test "$rc" = 0 -a -n "$tl" -a -s "$tl"
  if [ -s "$tl" ]; then
    say "[$id] entries: $(tl_py "$tl" "[(e['kind'], e.get('tool'), e.get('action'), e.get('steps'), e.get('ok')) for e in d['entries']]")"
    check "$id" "one entry: tool run, action run, steps 3, ok true" tl_is "$tl" "len(d['entries'])==1 and d['entries'][0]['kind']=='action' and d['entries'][0]['tool']=='run' and d['entries'][0]['action']=='run' and d['entries'][0]['steps']==3 and d['entries'][0]['ok'] is True"
  fi
  unset BLADE_RECORD BLADE_RECORD_FORMAT BLADE_RECORD_TABS
  end_scn "$id"
}

sc_per_tab_timeline() {
  local id=per-tab-timeline o t2 stop rc tl f1 f2 s2 d2 e2 a2 local_end local_at dur2 b_end
  new_home; export BLADE_RECORD=1 BLADE_RECORD_TABS=per-tab BLADE_RECORD_FORMAT=webm
  "$BB" nav "$FIX/mark.html" >/dev/null 2>&1
  "$BB" state record-start "$id" >/dev/null 2>&1; say "[$id] record-start done"
  sleep 1.5
  "$BB" act click "turn blue" 2>&1 | head -1 | sed "s/^/[$id] click tab1: /" | tee -a "$EVIDENCE"
  sleep 1.5
  o=$("$BB" state open-tab "$FIX/mark.html" 2>&1); say "[$id] open-tab: $o"; t2=$(opened_id "$o")
  sleep 0.5
  "$BB" act click "turn green" 2>&1 | head -1 | sed "s/^/[$id] click tab2: /" | tee -a "$EVIDENCE"
  sleep 1.5
  stop=$("$BB" state record-stop 2>&1); rc=$?
  say "[$id] stop rc=$rc: $stop"
  check "$id" "stop exits 0 with two per-tab files" test "$rc" = 0 -a "$(grep -c 'recording saved (per-tab, 2 files): ' <<<"$stop")" = 1
  tl=$(timeline_path "$stop")
  check "$id" "exactly one Timeline in the result, named after the -tab1- file" test "$(grep -o 'Timeline: ' <<<"$stop" | wc -l | tr -d ' ')" = 1 -a -n "$tl" -a "$(grep -c -- '-tab1-' <<<"$(basename "$tl")")" = 1
  if [ -z "$tl" ] || [ ! -s "$tl" ]; then say "FAIL [$id] timeline missing"; FAILS=$((FAILS + 1)); end_scn "$id"; return; fi
  check "$id" "one timeline file in the recordings dir" test "$(find "$SCN_HOME/recordings" -name '*.timeline.json' | wc -l | tr -d ' ')" = 1
  check "$id" "files[] lists 2 files; file 2 starts after the clock origin" tl_is "$tl" "len(d['files'])==2 and d['files'][1]['start_s'] is not None and d['files'][1]['start_s']>0"
  f1="$(dirname "$tl")/$(tl_py "$tl" "d['files'][0]['file']")"
  f2="$(dirname "$tl")/$(tl_py "$tl" "d['files'][1]['file']")"
  say "[$id] files: $(tl_py "$tl" "d['files']")"
  check "$id" "both per-tab videos exist and decode" bash -c "[ -s '$f1' ] && [ -s '$f2' ] && '$FFMPEG' -nostdin -v error -i '$f2' -f null - && '$FFMPEG' -nostdin -v error -i '$f1' -f null -"
  say "[$id] entries: $(tl_py "$tl" "[(e['kind'], e.get('action'), e.get('ok'), e['at_s'], e.get('end_s')) for e in d['entries']]")"
  check "$id" "entries: click(ok), open-tab(ok), click(ok) in order" tl_is "$tl" "[(e.get('action'), e.get('ok')) for e in d['entries']] == [('click',True),('open-tab',True),('click',True)]"
  s2=$(tl_py "$tl" "d['files'][1]['start_s']"); dur2=$(tl_py "$tl" "d['files'][1]['duration_s']")
  e2=$(tl_py "$tl" "d['entries'][2]['end_s']"); a2=$(tl_py "$tl" "d['entries'][2]['at_s']")
  local_end=$(python3 -I -c "print(round($e2 - $s2, 2))"); local_at=$(python3 -I -c "print(round($a2 - $s2, 2))")
  check "$id" "green click starts inside file 2 (local at_s $local_at s, file 2 is $dur2 s)" python3 -I -c "import sys; sys.exit(0 if 0 <= $local_at <= $dur2 else 1)"
  t=$(addt "$local_end" 0.4); tone_check "$id" "green click, file 2: local end_s+0.4 s is green" "$f2" "$t" green
  b_end=$(tl_py "$tl" "d['entries'][0]['end_s']")
  t=$(addt "$b_end" 0.4); tone_check "$id" "blue click, file 1: end_s+0.4 s is blue" "$f1" "$t" blue
  align_report "$id-file2" green "$f2" "$local_end" green
  unset BLADE_RECORD BLADE_RECORD_FORMAT BLADE_RECORD_TABS
  end_scn "$id"
}

# ---------------------------------------------------------------- 10-02: recording-off identity

# off_trace LABEL BIN RUN: recording OFF, one CLI session through cdp_tap. Writes the normalized stdout and the
# method log; records the sidecars and recordings the session left in its home.
off_trace() {
  local label="$1" bin="$2" run="$3" log port lp i out a
  new_home; unset BLADE_RECORD BLADE_RECORD_FORMAT BLADE_RECORD_TABS; CUR_BIN="$bin"
  "$bin" nav "$FIX/mark.html" >/dev/null 2>&1
  port=$(bb_port) || { say "FAIL [off] no Chrome port for $label run $run"; FAILS=$((FAILS + 1)); end_scn "off-trace-$label-$run"; return; }
  lp=$((19300 + RANDOM % 600)); log="$SCRATCH/trace-$label-$run.log"; : > "$log"
  python3 -I "$RS/lib/cdp_tap.py" "$lp" "$port" "$log" & TAP_PID=$!
  i=0; while [ $i -lt 50 ] && ! curl -sf "http://127.0.0.1:$lp/json/version" >/dev/null 2>&1; do sleep 0.1; i=$((i + 1)); done
  : > "$log"
  a=(--host 127.0.0.1 --port "$lp")
  out="$SCRATCH/out-$label-$run.txt"; : > "$out"
  {
    echo "== nav";   "$bin" "${a[@]}" nav "$FIX/mark.html" 2>/dev/null
    echo "== click"; "$bin" "${a[@]}" act click "turn blue" 2>/dev/null
    echo "== tabs";  "$bin" "${a[@]}" state tabs 2>/dev/null
    echo "== see";   "$bin" "${a[@]}" see content 2>/dev/null
    echo "== run";   "$bin" "${a[@]}" run '[{"action":"click","text":"turn green"},{"action":"see","mode":"content"}]' 2>/dev/null
  } | sed -E 's/[0-9A-F]{32}/ID/g; s/[0-9][0-9.]* ?(ms|s)/N/g' > "$out"
  sleep 1
  kill "$TAP_PID" 2>/dev/null; TAP_PID=""
  grep '^WS ' "$log" | sed 's/^WS //' > "$SCRATCH/m-$label-$run.txt"
  find "$SCN_HOME" \( -name '*.timeline.json' -o -name '*.poster.png' \) > "$SCRATCH/side-$label-$run.txt"
  ls -A "$SCN_HOME/recordings" 2>/dev/null | wc -l | tr -d ' ' > "$SCRATCH/recs-$label-$run.txt"
  end_scn "off-trace-$label-$run"
}

sc_off() {
  say "== off: recording OFF: CDP method trace, normalized stdout and sidecar absence, base d18dae1 vs HEAD"
  [ -x "${BASE_BIN:-/nonexistent}" ] || { say "FAIL [off] BASE_BIN (binary built from d18dae1) missing"; FAILS=$((FAILS + 1)); return; }
  say "[off] base: $BASE_BIN ($("$BASE_BIN" --version 2>/dev/null | head -1))"
  say "[off] new:  $BB ($("$BB" --version 2>/dev/null | head -1))"
  local run lab m
  for run in 1 2; do off_trace base "$BASE_BIN" $run; off_trace new "$BB" $run; done
  for lab in base new; do
    cat "$SCRATCH/m-$lab-1.txt" "$SCRATCH/m-$lab-2.txt" | sort -u > "$SCRATCH/set-$lab.txt"
    say "[off] $lab distinct WS methods: $(wc -l < "$SCRATCH/set-$lab.txt" | tr -d ' ')"
    for run in 1 2; do say "[off] $lab run $run: total=$(wc -l < "$SCRATCH/m-$lab-$run.txt" | tr -d ' ') Page.bringToFront=$(grep -c '^Page.bringToFront$' "$SCRATCH/m-$lab-$run.txt") Target.getTargets=$(grep -c '^Target.getTargets$' "$SCRATCH/m-$lab-$run.txt")"; done
  done
  check off "traces non-empty" test -s "$SCRATCH/set-base.txt" -a -s "$SCRATCH/set-new.txt"
  check off "distinct WS method sets equal (base vs new)" cmp -s "$SCRATCH/set-base.txt" "$SCRATCH/set-new.txt"
  for run in 1 2; do
    check off "per-method counts equal, base run $run vs new run $run" \
      cmp -s <(sort "$SCRATCH/m-base-$run.txt" | uniq -c) <(sort "$SCRATCH/m-new-$run.txt" | uniq -c)
  done
  # Input.dispatchMouseEvent is random by design (biometric mouse paths seeded from the clock, src/stealth/biometrics.rs;
  # idle hum, src/stealth/hum.rs): the same binary differs from itself. Strict equality above stays the bar; these
  # lines show the noise floor and the narrower check on every other method.
  say "[off] noise probe: Input.dispatchMouseEvent base run1 $(grep -c '^Input.dispatchMouseEvent$' "$SCRATCH/m-base-1.txt") run2 $(grep -c '^Input.dispatchMouseEvent$' "$SCRATCH/m-base-2.txt") | new run1 $(grep -c '^Input.dispatchMouseEvent$' "$SCRATCH/m-new-1.txt") run2 $(grep -c '^Input.dispatchMouseEvent$' "$SCRATCH/m-new-2.txt")"
  for run in 1 2; do
    check off "per-method counts equal except Input.dispatchMouseEvent (random mouse path), base vs new run $run" \
      cmp -s <(grep -v '^Input.dispatchMouseEvent$' "$SCRATCH/m-base-$run.txt" | sort | uniq -c) <(grep -v '^Input.dispatchMouseEvent$' "$SCRATCH/m-new-$run.txt" | sort | uniq -c)
  done
  check off "no screencast method in new" test -z "$(cat "$SCRATCH"/m-new-*.txt | grep -i screencast)"
  for run in 1 2; do
    check off "normalized stdout byte-identical, base vs new (run $run)" cmp -s "$SCRATCH/out-base-$run.txt" "$SCRATCH/out-new-$run.txt"
  done
  check off "normalized stdout byte-identical, base run 1 vs base run 2 (same binary, noise check)" cmp -s "$SCRATCH/out-base-1.txt" "$SCRATCH/out-base-2.txt"
  check off-files "no timeline/poster anywhere under the off homes" test -z "$(cat "$SCRATCH"/side-*.txt)"
  check off-files "recordings dir absent or empty in every off home (counts: $(cat "$SCRATCH"/recs-*.txt | tr '\n' ' '))" bash -c "! grep -qvx 0 '$SCRATCH'/recs-*.txt"
}

# ---------------------------------------------------------------- 10-02: MCP stdio lane

sc_mcp() {
  say "== mcp: tests/record_evidence.rs live tests (MCP stdio, headless), one cargo run per test (--exact)"
  local out rc t
  for t in mcp_stop_has_poster_timeline_marker mcp_recording_off_has_no_sidecars; do
    # --nocapture interleaves the test's own output with the harness line, so the verdict is the exit code
    out=$(env -u BLADE_RECORD -u BLADE_RECORD_FORMAT -u BLADE_RECORD_TABS REC_HUB_FIX="$FIX" REC_HUB_FFMPEG="$FFMPEG" REC_HUB_FFPROBE="$FFPROBE" cargo test --release --test record_evidence -- --ignored --exact "$t" --nocapture 2>&1); rc=$?
    grep -oE 'RESULT .*' <<<"$out" | cut -c1-600 | sed 's/^/[mcp] /' | tee -a "$EVIDENCE" >/dev/null
    grep -E 'panicked|test result' <<<"$out" | cut -c1-400 | sed 's/^/[mcp] /' | tee -a "$EVIDENCE" >/dev/null
    if [ "$rc" = 0 ] && grep -q 'test result: ok. 1 passed' <<<"$out"; then say "PASS [mcp] $t"; else say "FAIL [mcp] $t (cargo exit $rc)"; FAILS=$((FAILS + 1)); fi
  done
}

# ---------------------------------------------------------------- dispatch

want poster && sc_poster poster
want poster-mp4 && sc_poster poster-mp4
want marker && sc_marker
want marker-bad && sc_marker_bad
want per-tab && sc_per_tab
want latency && sc_latency
want timeline && sc_timeline_fmt timeline webm
want timeline-mp4 && sc_timeline_fmt timeline-mp4 mp4
want timeline-run && sc_timeline_run
want per-tab-timeline && sc_per_tab_timeline
want off && sc_off
want mcp && sc_mcp
BB="$NEW_BIN"; CUR_BIN="$BB"

pleft=$( { pgrep -fl "bbfol|bbrec|bbproj|bbposter|bbev|bbbase"; } 2>/dev/null | grep -v pgrep)
say "final pgrep of our fixtures/daemons: ${pleft:-none}"
say "== total FAIL count: $FAILS"
exit "$([ "$FAILS" = 0 ] && echo 0 || echo 1)"
