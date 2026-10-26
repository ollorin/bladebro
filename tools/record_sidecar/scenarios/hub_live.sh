#!/usr/bin/env bash
# Live proof of the recording state ops (07-03): MCP stdio tests, CLI daemon lane, off-mode
# CDP method comparison against the pre-phase binary, and the regression instruments.
# Safety: BLADE_HEADLESS=1, a fresh scratch BLADE_HOME per scenario, BLADE_NO_UPDATE_CHECK=1,
# 127.0.0.1 fixtures only. Never touches a real profile or ~/.blade; the lane_matrix `real` lane is
# skipped on purpose (it drives the user's own browser).
#
#   bash tools/record_sidecar/scenarios/hub_live.sh [--only ID]   (default: every scenario)
#   ids: mcp daemon-rec daemon-double oneshot daemon-off cdp-trace audit qol lanes
#
# Inputs (harness inputs, not product options):
#   HUB_BASE_BIN   pre-phase binary. Unset: the script builds it from `git rev-parse be1a3d6^`
#                  into target/hub-base (a temporary git worktree, removed at the end).
#   EVIDENCE       output file (default evidence/07-03-live.txt; truncated unless --only is used)
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../env.sh
source "$HERE/../env.sh"
export BLADE_HEADLESS=1 BLADE_NO_UPDATE_CHECK=1
FFPROBE="${BLADE_FFPROBE:-$HOME/.local/ffmpeg/bin/ffprobe}"
FFMPEG="${BLADE_FFMPEG:-$HOME/.local/ffmpeg/bin/ffmpeg}"
EVIDENCE="${EVIDENCE:-$REPO/.planning/phases/07-recordhub-and-state-ops/evidence/07-03-live.txt}"
mkdir -p "$(dirname "$EVIDENCE")"

ONLY=""
if [ "${1:-}" = "--only" ]; then ONLY="${2:-}"; else : > "$EVIDENCE"; fi
FAILS=0
SCN_HOME=""
CUR_BIN="$BB"
BASE_WT=""
TAP_PID=""

say() { echo "$*" | tee -a "$EVIDENCE"; }
check() { # check ID "description" <exit status of the command that follows>
  local id="$1" desc="$2"; shift 2
  if "$@"; then say "PASS [$id] $desc"; else say "FAIL [$id] $desc"; FAILS=$((FAILS + 1)); fi
}
want() { [ -z "$ONLY" ] || [ "$ONLY" = "$1" ]; }

new_home() { SCN_HOME="$(mktemp -d -t bbhub)"; export BLADE_HOME="$SCN_HOME"; }

# Stop our daemon, then show nothing of ours remains (home basename survives the /private symlink).
end_scn() {
  local id="$1" tag left
  [ -n "$TAP_PID" ] && kill "$TAP_PID" 2>/dev/null && TAP_PID=""
  [ -n "$SCN_HOME" ] || return 0
  tag="$(basename "$SCN_HOME")"
  BLADE_HOME="$SCN_HOME" "$CUR_BIN" stop >/dev/null 2>&1
  # Chrome helpers (network service) can outlive the browser by a few seconds: poll, then judge.
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
{ echo "# hub_live.sh 07-03  started $(date -u +%FT%TZ)  headless=$BLADE_HEADLESS  fixtures=$FIX"
  echo "# binary: $BB ($("$BB" --version 2>/dev/null | head -1))  chrome: $("$BB" version 2>/dev/null | head -1)"
  echo "# git: $(git rev-parse --short HEAD)"; } | tee -a "$EVIDENCE"

# --- helpers ------------------------------------------------------------------------------
fsize() { stat -c %s "$1" 2>/dev/null || stat -f %z "$1"; }
now_ms() { python3 -I -c 'import time;print(int(time.time()*1000))'; }
# analyse ID FILE: ffprobe + full decode; PASS/FAIL lines. Echoes nothing.
analyse() {
  local id="$1" f="$2" pj dec rc
  [ -s "$f" ] || { say "FAIL [$id] output file missing or empty: $f"; FAILS=$((FAILS + 1)); return; }
  pj=$("$FFPROBE" -v error -show_entries stream=codec_name,width,height:format=duration -of json "$f" | tr -d '\n ')
  say "[$id] ffprobe: $pj"
  dec=$("$FFMPEG" -v error -i "$f" -f null - 2>&1 >/dev/null); rc=$?
  check "$id" "full decode exit=$rc stderr_lines=$(printf '%s' "$dec" | grep -c .)" test "$rc" = 0 -a -z "$dec"
  check "$id" "codec vp9 1280x720" bash -c "echo '$pj' | grep -q '\"codec_name\":\"vp9\"' && echo '$pj' | grep -q '\"width\":1280' && echo '$pj' | grep -q '\"height\":720'"
}
# drift ID REPORTED_S WALL_MS: informational 5% line + 10% check (wall = start call to stop return)
drift() {
  local id="$1" dur="$2" wall="$3" pct
  pct=$(python3 -I -c "d,w=float('$dur'),float('$wall')/1000;print(round(abs(d-w)/w*100,1))")
  say "[$id] duration $dur s vs wall $(python3 -I -c "print(round($wall/1000,2))") s: drift ${pct}% (5% bar: $(python3 -I -c "print('met' if $pct<=5 else 'NOT met')"))"
  check "$id" "duration within 10% of wall clock (${pct}%)" python3 -I -c "import sys;sys.exit(0 if $pct<=10 else 1)"
}
saved_path() { sed -nE 's/.*recording saved: (.*) \(bytes .*/\1/p' <<<"$1" | head -1; }
num_after() { sed -nE "s/.*$2 ([0-9.]+).*/\1/p" <<<"$1" | head -1; }

# --- scenarios ----------------------------------------------------------------------------
sc_mcp() {
  say "== mcp: tests/record_hub.rs live tests (MCP stdio)"
  new_home
  local out rc
  out=$(REC_HUB_FIX="$FIX" REC_HUB_FFMPEG="$FFMPEG" REC_HUB_FFPROBE="$FFPROBE" \
        cargo test --release --test record_hub -- --ignored --test-threads=1 --nocapture 2>&1); rc=$?
  echo "$out" | grep -E "^(RESULT|test |running)|RESULT |test result" | sed 's/^/[mcp] /' | tee -a "$EVIDENCE" >/dev/null
  check mcp "six live tests pass (exit=$rc)" test "$rc" = 0
  end_scn mcp
}

sc_daemon_rec() {
  say "== daemon-rec: CLI daemon record-start, browse, record-stop"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/anim.html" >/dev/null 2>&1
  local t0 s rc_s t1 stop rc_o out path size dur
  s=$("$BB" state record-start d1 2>&1); rc_s=$?
  say "[daemon-rec] record-start rc=$rc_s: $s"
  t0=$(now_ms)
  "$BB" state open-tab "$FIX/b.html" >/dev/null 2>&1
  sleep 6
  stop=$("$BB" state record-stop 2>&1); rc_o=$?
  t1=$(now_ms)
  say "[daemon-rec] record-stop rc=$rc_o: $stop"
  check daemon-rec "start and stop exit 0" test "$rc_s" = 0 -a "$rc_o" = 0
  check daemon-rec "report says recording saved" grep -q "recording saved: " <<<"$stop"
  check daemon-rec "segments listing shows the open-tab follow (two segments)" grep -Eq 'segments \[0\.0 s tab [^,]+, [0-9.]+ s tab ' <<<"$stop"
  path=$(saved_path "$stop")
  check daemon-rec "absolute path exists" test -n "$path" -a -f "$path"
  size=$(fsize "$path"); say "[daemon-rec] file size $size, reported bytes $(num_after "$stop" bytes)"
  check daemon-rec "reported bytes equal file size" test "$(num_after "$stop" bytes)" = "$size"
  analyse daemon-rec "$path"
  drift daemon-rec "$(num_after "$stop" duration)" "$((t1 - t0))"
  unset BLADE_RECORD
  end_scn daemon-rec
}

sc_daemon_double() {
  say "== daemon-double: second record-start names the first file"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/anim.html" >/dev/null 2>&1
  local a b rc_b first
  a=$("$BB" state record-start a 2>&1)
  first=$(sed -nE 's/.*recording started: (.*) \(tab .*/\1/p' <<<"$a")
  b=$("$BB" state record-start b 2>&1); rc_b=$?
  say "[daemon-double] second start rc=$rc_b: $b"
  check daemon-double "second start exits non-zero" test "$rc_b" != 0
  check daemon-double "error names the active file" grep -qF "$first" <<<"$b"
  sleep 2
  "$BB" state record-stop >/dev/null 2>&1
  check daemon-double "exactly one file in recordings" test "$(ls "$BLADE_HOME/recordings" | grep -E '\.(webm|mp4)$' | wc -l | tr -d ' ')" = 1
  unset BLADE_RECORD
  end_scn daemon-double
}

sc_oneshot() {
  say "== oneshot: --no-daemon record-start refuses"
  new_home; export BLADE_RECORD=1
  local o rc left
  o=$("$BB" --no-daemon state record-start x 2>&1); rc=$?
  say "[oneshot] rc=$rc: $(grep -v '^\[' <<<"$o")"
  check oneshot "non-zero exit" test "$rc" != 0
  check oneshot "long-lived session text" grep -q "long-lived session" <<<"$o"
  left=$( { pgrep -fl "$(basename "$SCN_HOME")"; pgrep -fl ffmpeg; } | grep -v pgrep)
  check oneshot "no Chrome/ffmpeg left after the one-shot command (no ffmpeg spawned)" test -z "$left"
  [ -n "$left" ] && echo "$left" | tee -a "$EVIDENCE"
  check oneshot "no recordings dir" test ! -e "$BLADE_HOME/recordings"
  unset BLADE_RECORD
  end_scn oneshot
}

chrome_argv() { # normalized main-process argv of the Chrome launched under the current scratch home
  local tag; tag="$(basename "$SCN_HOME")"
  ps -ax -o args= | grep -- "--user-data-dir=" | grep "$tag" | grep -v -- "--type=" | grep -v grep \
    | sed -E "s#[^ =]*${tag}[^ ]*#SCRATCH#g; s#remote-debugging-port=[0-9]+#remote-debugging-port=PORT#g" | tr ' ' '\n' | grep -- '^--' | sort
}

ensure_base() {
  if [ -n "${HUB_BASE_BIN:-}" ] && [ -x "$HUB_BASE_BIN" ]; then BASE_BIN="$HUB_BASE_BIN"; return 0; fi
  local rev; rev=$(git rev-parse be1a3d6^) || return 1
  BASE_WT="$REPO/target/hub-base/src"
  rm -rf "$BASE_WT"; mkdir -p "$REPO/target/hub-base"
  git worktree add --detach "$BASE_WT" "$rev" >/dev/null 2>&1 || return 1
  (cd "$BASE_WT" && cargo build --release --target-dir "$REPO/target/hub-base/target" >/dev/null 2>&1) || return 1
  BASE_BIN="$REPO/target/hub-base/target/release/bladebro"
}

sc_daemon_off() {
  say "== daemon-off: BLADE_RECORD unset, nothing created, Chrome argv identical to the pre-phase binary"
  unset BLADE_RECORD
  ensure_base || { say "FAIL [daemon-off] could not build the base binary"; FAILS=$((FAILS + 1)); return; }
  say "[daemon-off] base binary: $BASE_BIN ($(git rev-parse --short be1a3d6^))"
  local rc1 rc2 rc3 rc4 o4 argv_new argv_base
  new_home
  "$BB" nav "$FIX/a.html" >/dev/null 2>&1; rc1=$?
  "$BB" see content >/dev/null 2>&1; rc2=$?
  "$BB" state tabs >/dev/null 2>&1; rc3=$?
  o4=$("$BB" state record-start x 2>&1); rc4=$?
  say "[daemon-off] record-start rc=$rc4: $o4"
  check daemon-off "nav/see/tabs exit 0 ($rc1/$rc2/$rc3)" test "$rc1" = 0 -a "$rc2" = 0 -a "$rc3" = 0
  check daemon-off "record-start exits non-zero ($rc4)" test "$rc4" != 0
  check daemon-off "record-start text names BLADE_RECORD and bladebro stop" grep -q "BLADE_RECORD.*bladebro stop" <<<"$o4"
  check daemon-off "no recordings dir" test ! -e "$BLADE_HOME/recordings"
  check daemon-off "no ffmpeg process" test -z "$(pgrep -fl ffmpeg)"
  argv_new=$(chrome_argv)
  end_scn daemon-off-new
  new_home; CUR_BIN="$BASE_BIN"
  "$BASE_BIN" nav "$FIX/a.html" >/dev/null 2>&1
  "$BASE_BIN" see content >/dev/null 2>&1
  "$BASE_BIN" state tabs >/dev/null 2>&1
  argv_base=$(chrome_argv)
  end_scn daemon-off-base
  say "[daemon-off] Chrome argv flags: new=$(echo "$argv_new" | grep -c .) base=$(echo "$argv_base" | grep -c .)"
  check daemon-off "Chrome argv identical (base vs new, scratch path/port normalized)" test -n "$argv_new" -a "$argv_new" = "$argv_base"
  [ "$argv_new" = "$argv_base" ] || { diff <(echo "$argv_base") <(echo "$argv_new") | sed 's/^/[daemon-off] argv diff: /' | tee -a "$EVIDENCE"; }
}

trace_run() { # trace_run LABEL BIN RUN -> methods file in $SCRATCH
  local label="$1" bin="$2" run="$3" log port lp i
  new_home; CUR_BIN="$bin"
  "$bin" nav "$FIX/a.html" >/dev/null 2>&1
  port=$(bb_port) || { say "FAIL [cdp-trace] no Chrome port for $label"; FAILS=$((FAILS + 1)); end_scn cdp-trace; return; }
  lp=$((19300 + RANDOM % 600))
  log="$SCRATCH/trace-$label-$run.log"; : > "$log"
  python3 -I "$RS/lib/cdp_tap.py" "$lp" "$port" "$log" &
  TAP_PID=$!
  i=0; while [ $i -lt 50 ] && ! curl -sf "http://127.0.0.1:$lp/json/version" >/dev/null 2>&1; do sleep 0.1; i=$((i + 1)); done
  : > "$log"   # drop the readiness probe
  local a=(--host 127.0.0.1 --port "$lp")
  "$bin" "${a[@]}" nav "$FIX/a.html" >/dev/null 2>&1
  "$bin" "${a[@]}" see content >/dev/null 2>&1
  "$bin" "${a[@]}" state tabs >/dev/null 2>&1
  "$bin" "${a[@]}" state cookies >/dev/null 2>&1
  "$bin" "${a[@]}" state ls >/dev/null 2>&1
  "$bin" "${a[@]}" act click Toggle >/dev/null 2>&1
  sleep 1
  kill "$TAP_PID" 2>/dev/null; TAP_PID=""
  grep '^WS ' "$log" | sed 's/^WS //' | sort > "$SCRATCH/m-$label-$run.txt"
  end_scn "cdp-trace-$label-$run"
}

sc_cdp_trace() {
  say "== cdp-trace: CDP methods the one-shot CLI sends, base vs new (BLADE_RECORD unset)"
  unset BLADE_RECORD
  ensure_base || { say "FAIL [cdp-trace] could not build the base binary"; FAILS=$((FAILS + 1)); return; }
  local lab run
  for run in 1 2; do trace_run base "$BASE_BIN" $run; trace_run new "$BB" $run; done
  for lab in base new; do
    for run in 1 2; do
      say "[cdp-trace] $lab run $run: $(wc -l < "$SCRATCH/m-$lab-$run.txt" | tr -d ' ') messages, $(sort -u "$SCRATCH/m-$lab-$run.txt" | wc -l | tr -d ' ') distinct"
    done
    cat "$SCRATCH/m-$lab-1.txt" "$SCRATCH/m-$lab-2.txt" | sort -u > "$SCRATCH/set-$lab.txt"
  done
  say "[cdp-trace] distinct methods BASE:"; sed 's/^/    /' "$SCRATCH/set-base.txt" | tee -a "$EVIDENCE" >/dev/null
  say "[cdp-trace] distinct methods NEW:";  sed 's/^/    /' "$SCRATCH/set-new.txt"  | tee -a "$EVIDENCE" >/dev/null
  for lab in base new; do
    say "[cdp-trace] $lab run 1 multiset (count method):"; sort "$SCRATCH/m-$lab-1.txt" | uniq -c | sed 's/^/    /' | tee -a "$EVIDENCE" >/dev/null
    say "[cdp-trace] $lab run 2 multiset (count method):"; sort "$SCRATCH/m-$lab-2.txt" | uniq -c | sed 's/^/    /' | tee -a "$EVIDENCE" >/dev/null
  done
  check cdp-trace "traces are non-empty" test -s "$SCRATCH/set-base.txt" -a -s "$SCRATCH/set-new.txt"
  check cdp-trace "distinct method sets identical (base vs new)" cmp -s "$SCRATCH/set-base.txt" "$SCRATCH/set-new.txt"
  check cdp-trace "no method in new absent from base" test -z "$(comm -13 "$SCRATCH/set-base.txt" "$SCRATCH/set-new.txt")"
  check cdp-trace "no Page.startScreencast / screencastFrameAck in new" test -z "$(cat "$SCRATCH"/m-new-*.txt | grep -i screencast)"
  local bmax nmax
  bmax=$(for r in 1 2; do grep -c '^Target.getTargetInfo$' "$SCRATCH/m-base-$r.txt"; done | sort -n | tail -1)
  nmax=$(for r in 1 2; do grep -c '^Target.getTargetInfo$' "$SCRATCH/m-new-$r.txt"; done | sort -n | tail -1)
  say "[cdp-trace] Target.getTargetInfo max per run: base=$bmax new=$nmax"
  check cdp-trace "Target.getTargetInfo not beyond base count" test "$nmax" -le "$bmax"
}

sc_audit() {
  say "== audit: bladebro audit (bar: 100/100 macOS baseline)"
  new_home
  local out; out=$("$BB" audit 2>&1); local rc=$?
  echo "$out" | tail -12 | sed 's/^/[audit] /' | tee -a "$EVIDENCE" >/dev/null
  say "[audit] exit=$rc"
  check audit "100/100 vectors" grep -qE "100/100" <<<"$out"
  end_scn audit
}

sc_qol() {
  say "== qol: tools/qol_probe/run.py (baseline 120/122 with 2 known failures)"
  local out rc
  out=$(python3 tools/qol_probe/run.py 2>&1); rc=$?
  echo "$out" | grep -E "^FAIL|passed, " | tail -12 | sed 's/^/[qol] /' | tee -a "$EVIDENCE" >/dev/null
  say "[qol] exit=$rc"
  check qol "only the two known failures (chord cleared the live editor; nested scroller movement)" \
    bash -c "n=\$(grep -cE '^FAIL' <<<\"\$1\"); test \"\$n\" -le 2 && ! grep -E '^FAIL' <<<\"\$1\" | grep -viE 'chord cleared the live editor|nested scroller movement is reported as such' | grep -q ." _ "$out"
  new_home; end_scn qol
}

sc_lanes() {
  say "== lanes: lane_matrix daemon oneshot mcp mcp-pipe, 5 rounds each (real lane skipped: drives the user's own browser)"
  say "[lanes] via lib/lane_matrix_local.py: tools/lane_matrix.py needs /proc (Linux) and navigates example.com; the wrapper swaps in a ps sweep and the local idle.html fixture"
  local lane out rc
  for lane in daemon oneshot mcp mcp-pipe; do
    out=$(REC_HUB_FIX="$FIX" python3 -I "$RS/lib/lane_matrix_local.py" "$lane" 5 2>&1); rc=$?
    echo "$out" | tail -12 | sed "s/^/[lanes:$lane] /" | tee -a "$EVIDENCE" >/dev/null
    check "lanes:$lane" "5 rounds clean (exit=$rc)" test "$rc" = 0
  done
  say "[lanes] real lane: skipped: drives the user's own browser"
  new_home; end_scn lanes
}

for id in mcp daemon-rec daemon-double oneshot daemon-off cdp-trace audit qol lanes; do
  want "$id" || continue
  case "$id" in
    mcp) sc_mcp ;; daemon-rec) sc_daemon_rec ;; daemon-double) sc_daemon_double ;; oneshot) sc_oneshot ;;
    daemon-off) sc_daemon_off ;; cdp-trace) sc_cdp_trace ;; audit) sc_audit ;; qol) sc_qol ;; lanes) sc_lanes ;;
  esac
done

left=$( { pgrep -fl "bladebro (mcp|daemon)"; pgrep -fl ffmpeg; pgrep -fl cdp_tap; } 2>/dev/null | grep -v pgrep)
if [ -z "$left" ]; then say "PASS [final] no bladebro daemon/mcp, ffmpeg or cdp_tap process left"
else say "FAIL [final] leftovers:"; echo "$left" | tee -a "$EVIDENCE"; FAILS=$((FAILS + 1)); fi
say "# finished $(date -u +%FT%TZ) failures=$FAILS"
exit "$FAILS"
