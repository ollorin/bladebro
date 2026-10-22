#!/usr/bin/env bash
# Live proof of tab following (08-01): MCP stdio tests, CLI daemon lane (switch/open, popup, close of
# the recorded tab, cross-origin navigation), off-mode CDP/HTTP comparison against the pre-phase
# binary, and informational perf / audit numbers.
# Safety: BLADE_HEADLESS=1 (no window is ever shown), a fresh scratch BLADE_HOME per scenario,
# BLADE_NO_UPDATE_CHECK=1, BLADE_RECORD=1 only where recording is the subject, 127.0.0.1 fixtures only
# (the xorigin scenario uses http://localhost on the same fixture server). Never touches a real
# profile or ~/.blade; the real lane is not exercised.
#
#   bash tools/record_sidecar/scenarios/follow_live.sh [--only ID]   (default: every scenario)
#   ids: mcp daemon popup close xorigin off perf audit
#
# Inputs (harness inputs, not product options):
#   FOLLOW_BASE_BIN  pre-phase binary. Unset: built from commit bb9e7dd (the tree before the phase 8
#                    code) into target/follow-base (a temporary git worktree, removed at the end).
#   FIX_PORT         fixture port (default 8765)
#   EVIDENCE         output file (default evidence/08-01-live.txt; truncated unless --only is used)
# Per-tab files (08-02) will add their scenario id here.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../env.sh
source "$HERE/../env.sh"
export BLADE_HEADLESS=1 BLADE_NO_UPDATE_CHECK=1
FFPROBE="${BLADE_FFPROBE:-$HOME/.local/ffmpeg/bin/ffprobe}"
FFMPEG="${BLADE_FFMPEG:-$HOME/.local/ffmpeg/bin/ffmpeg}"
EVIDENCE="${EVIDENCE:-$REPO/.planning/phases/08-tab-following/evidence/08-01-live.txt}"
mkdir -p "$(dirname "$EVIDENCE")"

ONLY=""
if [ "${1:-}" = "--only" ]; then ONLY="${2:-}"; else : > "$EVIDENCE"; fi
FAILS=0
SCN_HOME=""
CUR_BIN="$BB"
BASE_WT=""
TAP_PID=""

say() { echo "$*" | tee -a "$EVIDENCE"; }
check() { # check ID "description" <command that follows>
  local id="$1" desc="$2"; shift 2
  if "$@"; then say "PASS [$id] $desc"; else say "FAIL [$id] $desc"; FAILS=$((FAILS + 1)); fi
}
want() { [ -z "$ONLY" ] || [ "$ONLY" = "$1" ]; }

new_home() { SCN_HOME="$(mktemp -d -t bbfol)"; export BLADE_HOME="$SCN_HOME"; }

# Stop our daemon, then show nothing of ours remains (home basename survives the /private symlink).
end_scn() {
  local id="$1" tag left
  [ -n "$TAP_PID" ] && kill "$TAP_PID" 2>/dev/null && TAP_PID=""
  [ -n "$SCN_HOME" ] || return 0
  tag="$(basename "$SCN_HOME")"
  BLADE_HOME="$SCN_HOME" "$CUR_BIN" stop >/dev/null 2>&1
  # Chrome helpers can outlive the browser by a few seconds: poll, then judge.
  local n=0
  while :; do
    left=$( { pgrep -fl "$tag"; pgrep -fl ffmpeg; pgrep -fl cdp_tap; } 2>/dev/null | grep -v pgrep)
    { [ -z "$left" ] || [ $n -ge 10 ]; } && break
    n=$((n + 1)); sleep 1
  done
  if [ -z "$left" ]; then say "PASS [$id] cleanup: no daemon/Chrome/ffmpeg/cdp_tap left"
  else say "FAIL [$id] cleanup: leftovers:"; echo "$left" | tee -a "$EVIDENCE"; FAILS=$((FAILS + 1))
       pkill -f "$tag" 2>/dev/null; fi
  rm -rf "$SCN_HOME"; SCN_HOME=""; CUR_BIN="$BB"
}

cleanup_all() {
  [ -n "$TAP_PID" ] && kill "$TAP_PID" 2>/dev/null
  [ -n "$SCN_HOME" ] && { BLADE_HOME="$SCN_HOME" "$CUR_BIN" stop >/dev/null 2>&1; pkill -f "$(basename "$SCN_HOME")" 2>/dev/null; rm -rf "$SCN_HOME"; }
  if [ -n "$BASE_WT" ]; then git -C "$REPO" worktree remove --force "$BASE_WT" 2>/dev/null; git -C "$REPO" worktree prune; fi
  bb_cleanup
}
trap cleanup_all EXIT
fixture_up || exit 1
cd "$REPO" || exit 1
cargo build --release >/dev/null 2>&1 || { echo "cargo build failed" >&2; exit 1; }
{ echo "# follow_live.sh 08-01  started $(date -u +%FT%TZ)  headless=$BLADE_HEADLESS  fixtures=$FIX"
  echo "# binary: $BB ($("$BB" --version 2>/dev/null | head -1))  chrome: $("$BB" version 2>/dev/null | head -1)"
  echo "# git: $(git rev-parse --short HEAD)"; } | tee -a "$EVIDENCE"

# --- helpers ------------------------------------------------------------------------------
now_ms() { python3 -I -c 'import time;print(int(time.time()*1000))'; }
saved_path() { sed -nE 's/.*recording saved: (.*) \(bytes .*/\1/p' <<<"$1" | head -1; }
num_after() { sed -nE "s/.*$2 ([0-9.]+).*/\1/p" <<<"$1" | head -1; }
cur_id() { "$BB" state tabs 2>&1 | sed -nE 's/^\* ([0-9A-F]{32}) .*/\1/p' | head -1; }
opened_id() { sed -nE 's/.*tab ([0-9A-F]{32}).*/\1/p' <<<"$1" | head -1; }
# seg_ids TEXT / seg_offs TEXT: one entry per segment of `segments [0.0 s tab A, 4.2 s tab B]`
seg_inner() { sed -nE 's/.*segments \[([^]]*)\].*/\1/p' <<<"$1" | head -1; }
seg_ids() { seg_inner "$1" | tr ',' '\n' | sed -E 's/^ *[0-9.]+ s tab //; s/ \(no frames\)//'; }
seg_offs() { seg_inner "$1" | tr ',' '\n' | sed -E 's/^ *([0-9.]+) s tab .*/\1/'; }
rgb_at() { "$FFMPEG" -v error -ss "$2" -i "$1" -frames:v 1 -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - 2>/dev/null | od -An -tu1 | tr -s ' ' | sed 's/^ //'; }
tone() { awk -v r="$1" -v g="$2" -v b="$3" 'BEGIN{ if(r>200&&g<60&&b<60)print "red"; else if(b>200&&r<60&&g<60)print "blue"; else if(g>120&&r<60&&b<60)print "green"; else if(r>200&&g>160&&b<60)print "yellow"; else print "other(" r "," g "," b ")"}'; }
tone_at() { local c; c=$(rgb_at "$1" "$2"); tone $c; }
# mid_of TEXT K TOTAL: midpoint (s) of segment K (0-based) on the video clock
mid_of() { python3 -I -c "
import sys
offs=[float(x) for x in '''$(seg_offs "$1")'''.split()]
k=int('$2'); tot=float('$3')
end=offs[k+1] if k+1<len(offs) else tot
print(round((offs[k]+end)/2,2))"; }
# analyse ID FILE TEXT: constant 1280x720 vp9, full clean decode, decoded frames == reported frames
analyse() {
  local id="$1" f="$2" text="$3" pj dec rc nb
  [ -s "$f" ] || { say "FAIL [$id] output file missing or empty: $f"; FAILS=$((FAILS + 1)); return; }
  pj=$("$FFPROBE" -v error -show_entries stream=codec_name,width,height:format=duration -of json "$f" | tr -d '\n ')
  say "[$id] ffprobe: $pj"
  dec=$("$FFMPEG" -v error -i "$f" -f null - 2>&1 >/dev/null); rc=$?
  check "$id" "full decode exit=$rc stderr_lines=$(printf '%s' "$dec" | grep -c .)" test "$rc" = 0 -a -z "$dec"
  check "$id" "codec vp9 1280x720 (constant output size)" bash -c "echo '$pj' | grep -q '\"codec_name\":\"vp9\"' && echo '$pj' | grep -q '\"width\":1280' && echo '$pj' | grep -q '\"height\":720'"
  nb=$("$FFPROBE" -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 "$f")
  check "$id" "decoded frames ($nb) == reported frames ($(num_after "$text" frames))" test "$nb" = "$(num_after "$text" frames)"
}
ffmpeg_pids() { pgrep -f "ffmpeg.*$(basename "$SCN_HOME")" 2>/dev/null | tr '\n' ' '; }

# --- scenarios ----------------------------------------------------------------------------
sc_mcp() {
  say "== mcp: tests/record_follow.rs live tests (MCP stdio, headless)"
  new_home
  local out rc
  out=$(REC_HUB_FIX="$FIX" REC_HUB_FFMPEG="$FFMPEG" REC_HUB_FFPROBE="$FFPROBE" \
        cargo test --release --test record_follow -- --ignored --test-threads=1 --nocapture 2>&1); rc=$?
  echo "$out" | grep -E "^(RESULT|test |running)|RESULT |test result" | sed 's/^/[mcp] /' | tee -a "$EVIDENCE" >/dev/null
  check mcp "three live follow tests pass (exit=$rc)" test "$rc" = 0
  end_scn mcp
}

sc_daemon() {
  say "== daemon: CLI daemon, record-start, open-tab, switch-tab back, record-stop"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/a.html" >/dev/null 2>&1
  local A B s o pid1 pid2 stop rc path total k
  A=$(cur_id)
  s=$("$BB" state record-start f 2>&1); say "[daemon] record-start: $s"
  sleep 2; pid1=$(ffmpeg_pids)
  o=$("$BB" state open-tab "$FIX/b.html" 2>&1); B=$(opened_id "$o")
  say "[daemon] A=$A B=$B"
  sleep 3.5; pid2=$(ffmpeg_pids)
  "$BB" state switch-tab "$A" >/dev/null 2>&1
  sleep 2
  stop=$("$BB" state record-stop 2>&1); rc=$?
  say "[daemon] record-stop rc=$rc: $stop"
  check daemon "stop exits 0 and says recording saved" test "$rc" = 0 -a "$(grep -c 'recording saved: ' <<<"$stop")" = 1
  check daemon "segments are A, B, A (ids match state tabs)" test "$(seg_ids "$stop" | tr '\n' ' ')" = "$A $B $A "
  check daemon "offsets strictly increasing" python3 -I -c "
o=[float(x) for x in '''$(seg_offs "$stop")'''.split()]
import sys; sys.exit(0 if o[0]==0.0 and all(b>a for a,b in zip(o,o[1:])) else 1)"
  say "[daemon] offsets: $(seg_offs "$stop" | tr '\n' ' ')"
  check daemon "one ffmpeg pid across the switch ($pid1 / $pid2)" test -n "$pid1" -a "$pid1" = "$pid2"
  path=$(saved_path "$stop")
  analyse daemon "$path" "$stop"
  total=$(num_after "$stop" duration)
  for k in 0 1 2; do
    say "[daemon] segment $k colour at $(mid_of "$stop" $k "$total") s: $(tone_at "$path" "$(mid_of "$stop" $k "$total")")"
  done
  check daemon "colours red, blue, red" test "$(tone_at "$path" "$(mid_of "$stop" 0 "$total")") $(tone_at "$path" "$(mid_of "$stop" 1 "$total")") $(tone_at "$path" "$(mid_of "$stop" 2 "$total")")" = "red blue red"
  unset BLADE_RECORD
  end_scn daemon
}

sc_popup() {
  say "== popup: link popup is not a segment; a window.open popup is read as the new front tab"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/popup.html" >/dev/null 2>&1
  local O stop1 stop2 before after P path1 path2
  O=$(cur_id)
  "$BB" state record-start p1 >/dev/null 2>&1
  "$BB" act click "open c" 2>&1 | head -1 | sed 's/^/[popup] click link: /' | tee -a "$EVIDENCE"
  sleep 2
  stop1=$("$BB" state record-stop 2>&1); say "[popup] stop 1: $stop1"
  check popup "link popup: exactly one segment, the opener" test "$(seg_ids "$stop1" | tr '\n' ' ')" = "$O "
  path1=$(saved_path "$stop1"); analyse popup "$path1" "$stop1"
  check popup "opener-only video is yellow" test "$(tone_at "$path1" 1.0)" = "yellow"
  before=$("$BB" state tabs 2>&1 | sed -nE 's/^[* ] +([0-9A-F]{32}) .*/\1/p' | sort)
  "$BB" state record-start p2 >/dev/null 2>&1
  "$BB" act click "window" 2>&1 | head -1 | sed 's/^/[popup] click window: /' | tee -a "$EVIDENCE"
  sleep 1.5
  after=$("$BB" state tabs 2>&1 | sed -nE 's/^[* ] +([0-9A-F]{32}) .*/\1/p' | sort)
  P=$(comm -13 <(echo "$before") <(echo "$after") | head -1)
  say "[popup] popup tab: $P"
  "$BB" state switch-tab "$P" >/dev/null 2>&1
  sleep 2
  stop2=$("$BB" state record-stop 2>&1); say "[popup] stop 2: $stop2"
  local first last cnt
  first=$(seg_ids "$stop2" | head -1); last=$(seg_ids "$stop2" | tail -1); cnt=$(seg_ids "$stop2" | wc -l | tr -d ' ')
  check popup "window popup: >= 2 segments ($cnt), first the opener, last the popup" test "$first" = "$O" -a "$last" = "$P" -a "$cnt" -ge 2
  path2=$(saved_path "$stop2"); analyse popup "$path2" "$stop2"
  unset BLADE_RECORD
  end_scn popup
}

sc_close() {
  say "== close: the recorded tab is closed (the agent was on it): rebind with a note, never a failure"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/a.html" >/dev/null 2>&1
  local A B o stop path total
  A=$(cur_id)
  "$BB" state record-start c >/dev/null 2>&1
  sleep 1.5
  o=$("$BB" state open-tab "$FIX/b.html" 2>&1); B=$(opened_id "$o")
  sleep 3.5   # a brand-new tab is followed once it stayed in front ~1.5 s
  "$BB" state close-tab "$B" 2>&1 | head -1 | sed 's/^/[close] close-tab: /' | tee -a "$EVIDENCE"
  sleep 2
  stop=$("$BB" state record-stop 2>&1); local rc=$?
  say "[close] record-stop rc=$rc: $stop"
  check close "stop exits 0 and says recording saved" test "$rc" = 0 -a "$(grep -c 'recording saved: ' <<<"$stop")" = 1
  check close "note says the recorded tab was gone" grep -q "tab $B gone at " <<<"$stop"
  check close "last segment is the tab the agent landed on ($A)" test "$(seg_ids "$stop" | tail -1)" = "$A"
  path=$(saved_path "$stop"); analyse close "$path" "$stop"
  total=$(num_after "$stop" duration)
  local n; n=$(seg_ids "$stop" | wc -l | tr -d ' ')
  check close "colours blue then red around the close" test "$(tone_at "$path" "$(mid_of "$stop" $((n - 2)) "$total")") $(tone_at "$path" "$(mid_of "$stop" $((n - 1)) "$total")")" = "blue red"
  unset BLADE_RECORD
  end_scn close
}

sc_xorigin() {
  say "== xorigin: cross-site navigation of the recorded tab (127.0.0.1 -> localhost)"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/a.html" >/dev/null 2>&1
  local stop path total rc blue=0 t
  "$BB" state record-start x >/dev/null 2>&1
  sleep 1.5
  "$BB" nav "http://localhost:$FIX_PORT/b.html" >/dev/null 2>&1
  sleep 3
  stop=$("$BB" state record-stop 2>&1); rc=$?
  say "[xorigin] record-stop rc=$rc: $stop"
  if grep -q "recording saved: " <<<"$stop"; then
    path=$(saved_path "$stop"); total=$(num_after "$stop" duration)
    for t in $(python3 -I -c "t=float('$total');print(' '.join(str(round(t-i,2)) for i in (0.4,0.9,1.4)))"); do
      [ "$(tone_at "$path" "$t")" = "blue" ] && blue=1
    done
    say "[xorigin] clean result; blue frame near the end: $blue; segments: $(seg_ids "$stop" | tr '\n' ' ')"
    check xorigin "a clean result has blue frames after the navigation" test "$blue" = 1
    analyse xorigin "$path" "$stop"
  else
    check xorigin "a not-clean result names why (note present)" grep -qE "gone at|no tab left|not recorded|connection lost|no frames" <<<"$stop"
  fi
  unset BLADE_RECORD
  end_scn xorigin
}

chrome_argv() { # normalized main-process argv of the Chrome launched under the current scratch home
  local tag; tag="$(basename "$SCN_HOME")"
  ps -ax -o args= | grep -- "--user-data-dir=" | grep "$tag" | grep -v -- "--type=" | grep -v grep \
    | sed -E "s#[^ =]*${tag}[^ ]*#SCRATCH#g; s#remote-debugging-port=[0-9]+#remote-debugging-port=PORT#g" | tr ' ' '\n' | grep -- '^--' | sort
}

ensure_base() {
  if [ -n "${FOLLOW_BASE_BIN:-}" ] && [ -x "$FOLLOW_BASE_BIN" ]; then BASE_BIN="$FOLLOW_BASE_BIN"; return 0; fi
  BASE_WT="$REPO/target/follow-base/src"
  rm -rf "$BASE_WT"; mkdir -p "$REPO/target/follow-base"
  git worktree add --detach "$BASE_WT" bb9e7dd >/dev/null 2>&1 || return 1
  (cd "$BASE_WT" && cargo build --release --target-dir "$REPO/target/follow-base/target" >/dev/null 2>&1) || return 1
  BASE_BIN="$REPO/target/follow-base/target/release/bladebro"
}

trace_run() { # trace_run LABEL BIN RUN -> method / http files in $SCRATCH
  local label="$1" bin="$2" run="$3" log port lp i id
  new_home; CUR_BIN="$bin"
  "$bin" nav "$FIX/a.html" >/dev/null 2>&1
  port=$(bb_port) || { say "FAIL [off] no Chrome port for $label"; FAILS=$((FAILS + 1)); end_scn off; return; }
  lp=$((19300 + RANDOM % 600))
  log="$SCRATCH/trace-$label-$run.log"; : > "$log"
  python3 -I "$RS/lib/cdp_tap.py" "$lp" "$port" "$log" &
  TAP_PID=$!
  i=0; while [ $i -lt 50 ] && ! curl -sf "http://127.0.0.1:$lp/json/version" >/dev/null 2>&1; do sleep 0.1; i=$((i + 1)); done
  : > "$log"   # drop the readiness probe
  local a=(--host 127.0.0.1 --port "$lp")
  "$bin" "${a[@]}" nav "$FIX/a.html" >/dev/null 2>&1
  id=$("$bin" "${a[@]}" state open-tab "$FIX/b.html" 2>&1 | sed -nE 's/.*tab ([0-9A-F]{32}).*/\1/p' | head -1)
  "$bin" "${a[@]}" state tabs >/dev/null 2>&1
  "$bin" "${a[@]}" state switch-tab "$id" >/dev/null 2>&1
  "$bin" "${a[@]}" state close-tab "$id" >/dev/null 2>&1
  sleep 1
  kill "$TAP_PID" 2>/dev/null; TAP_PID=""
  grep '^WS ' "$log" | sed 's/^WS //' | sort > "$SCRATCH/m-$label-$run.txt"
  grep '^HTTP GET /json' "$log" | sort | uniq -c > "$SCRATCH/h-$label-$run.txt"
  end_scn "off-trace-$label-$run"
}

sc_off() {
  say "== off: BLADE_RECORD unset: no follower, nothing created, CDP/HTTP trace identical to the pre-phase binary"
  unset BLADE_RECORD
  local rc1 rc2 rc3 rc4 rc5 o id
  new_home
  "$BB" nav "$FIX/a.html" >/dev/null 2>&1; rc1=$?
  o=$("$BB" state open-tab "$FIX/b.html" 2>&1); rc2=$?; id=$(opened_id "$o")
  "$BB" state switch-tab "$id" >/dev/null 2>&1; rc3=$?
  "$BB" state tabs >/dev/null 2>&1; rc4=$?
  "$BB" state close-tab "$id" >/dev/null 2>&1; rc5=$?
  check off "daemon nav/open-tab/switch-tab/tabs/close-tab all exit 0 ($rc1 $rc2 $rc3 $rc4 $rc5)" test "$rc1$rc2$rc3$rc4$rc5" = 00000
  check off "no recordings dir" test ! -e "$BLADE_HOME/recordings"
  check off "no ffmpeg process" test -z "$(pgrep -fl ffmpeg)"
  end_scn off-daemon
  ensure_base || { say "FAIL [off] could not build the base binary"; FAILS=$((FAILS + 1)); return; }
  say "[off] base binary: $BASE_BIN (bb9e7dd)"
  local lab run
  for run in 1 2; do trace_run base "$BASE_BIN" $run; trace_run new "$BB" $run; done
  for lab in base new; do
    cat "$SCRATCH/m-$lab-1.txt" "$SCRATCH/m-$lab-2.txt" | sort -u > "$SCRATCH/set-$lab.txt"
    say "[off] $lab distinct WS methods: $(wc -l < "$SCRATCH/set-$lab.txt" | tr -d ' ')"
    for run in 1 2; do say "[off] $lab run $run HTTP /json lines: $(awk '{n+=$1} END{print n+0}' "$SCRATCH/h-$lab-$run.txt")"; done
  done
  check off "traces are non-empty" test -s "$SCRATCH/set-base.txt" -a -s "$SCRATCH/set-new.txt"
  check off "distinct WS method sets identical (base vs new)" cmp -s "$SCRATCH/set-base.txt" "$SCRATCH/set-new.txt"
  check off "no screencast method in new" test -z "$(cat "$SCRATCH"/m-new-*.txt | grep -i screencast)"
  # /json count can vary by one with timing (attach retries); compare the maximum per run, base vs new.
  local bmax nmax
  bmax=$(for r in 1 2; do awk '{n+=$1} END{print n+0}' "$SCRATCH/h-base-$r.txt"; done | sort -n | tail -1)
  nmax=$(for r in 1 2; do awk '{n+=$1} END{print n+0}' "$SCRATCH/h-new-$r.txt"; done | sort -n | tail -1)
  say "[off] HTTP GET /json* max per run: base=$bmax new=$nmax"
  check off "HTTP /json* count identical (base vs new)" test "$bmax" = "$nmax"
}

cpu_avg() { # cpu_avg PID N: mean %cpu over N one-second samples
  local pid="$1" n="$2" i s=0 v
  for i in $(seq 1 "$n"); do v=$(ps -o %cpu= -p "$pid" 2>/dev/null | tr -d ' '); s=$(python3 -I -c "print($s+float('${v:-0}'))"); sleep 1; done
  python3 -I -c "print(round($s/$n,1))"
}

sc_perf() {
  say "== perf (informational): lane_matrix daemon 3 rounds while nothing records; daemon CPU single-tab vs following"
  local out rc pid
  new_home
  out=$(REC_HUB_FIX="$FIX" python3 -I "$RS/lib/lane_matrix_local.py" daemon 3 2>&1); rc=$?
  echo "$out" | tail -8 | sed 's/^/[perf:lane_matrix] /' | tee -a "$EVIDENCE" >/dev/null
  say "[perf] lane_matrix daemon exit=$rc (informational)"
  end_scn perf-lane
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/a.html" >/dev/null 2>&1
  pid=$(pgrep -f "bladebro daemon" | head -1)
  "$BB" state record-start single >/dev/null 2>&1
  say "[perf] single-tab recording: daemon CPU mean over 10 s = $(cpu_avg "$pid" 10) %"
  "$BB" state record-stop >/dev/null 2>&1
  "$BB" state record-start follow >/dev/null 2>&1
  local id lp
  ( for _ in 1 2 3 4; do
      id=$("$BB" state open-tab "$FIX/b.html" 2>&1 | sed -nE 's/.*tab ([0-9A-F]{32}).*/\1/p' | head -1)
      sleep 2.2; "$BB" state close-tab "$id" >/dev/null 2>&1; sleep 0.6
    done ) &
  lp=$!
  say "[perf] following recording (open b, close after 2.2 s, every ~2.8 s): daemon CPU mean over 10 s = $(cpu_avg "$pid" 10) %"
  wait "$lp"   # not a bare wait: the fixture server is also a background job
  "$BB" state record-stop >/dev/null 2>&1
  unset BLADE_RECORD
  end_scn perf
}

sc_audit() {
  say "== audit: bladebro audit (bar: 100/100, the Phase 7 macOS baseline)"
  new_home
  local out; out=$("$BB" audit 2>&1); local rc=$?
  echo "$out" | tail -12 | sed 's/^/[audit] /' | tee -a "$EVIDENCE" >/dev/null
  say "[audit] exit=$rc"
  check audit "100/100 vectors" grep -qE "100/100" <<<"$out"
  end_scn audit
}

for id in mcp daemon popup close xorigin off perf audit; do
  want "$id" || continue
  case "$id" in
    mcp) sc_mcp ;; daemon) sc_daemon ;; popup) sc_popup ;; close) sc_close ;; xorigin) sc_xorigin ;;
    off) sc_off ;; perf) sc_perf ;; audit) sc_audit ;;
  esac
done

left=$( { pgrep -fl "bladebro (mcp|daemon)"; pgrep -fl ffmpeg; pgrep -fl cdp_tap; } 2>/dev/null | grep -v pgrep)
if [ -z "$left" ]; then say "PASS [final] no bladebro daemon/mcp, ffmpeg or cdp_tap process left"
else say "FAIL [final] leftovers:"; echo "$left" | tee -a "$EVIDENCE"; FAILS=$((FAILS + 1)); fi
say "# finished $(date -u +%FT%TZ) failures=$FAILS"
exit "$FAILS"
