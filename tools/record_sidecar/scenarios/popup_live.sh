#!/usr/bin/env bash
# Live proof of popup following (08.1): headless, scratch BLADE_HOME per scenario, 127.0.0.1 fixtures.
#   bash tools/record_sidecar/scenarios/popup_live.sh [--only ID]    ids: spike kinds close close-opener chain burst blocked tabs per-tab exit off mcp (default all except spike)
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
# --- 08.1-02 scenarios (CLI daemon lane; `mcp` runs the MCP stdio lane) -------------------------
video_id() { "$BB" state tabs 2>&1 | sed -nE 's/^( R|\*R) +([0-9A-F]{32}).*/\2/p' | head -1; }
# video_delay O: seconds from now until the video tab (R marker) is not O; empty if never within 8 s
video_delay() {
  local o="$1" t0 v; t0=$(secs)
  while :; do
    v=$(video_id)
    if [ -n "$v" ] && [ "$v" != "$o" ]; then python3 -I -c "print(round($(secs)-$t0,2))"; return 0; fi
    [ "$(python3 -I -c "print($(secs)-$t0 > 8)")" = True ] && return 1
    sleep 0.05
  done
}
close_others() { local o="$1" id; for id in $("$BB" state tabs 2>&1 | tab_ids); do [ "$id" = "$o" ] || "$BB" state close-tab "$id" >/dev/null 2>&1; done; }
stats() { python3 -I -c "
v=sorted(float(x) for x in '$*'.split()); print('min %.2f median %.2f max %.2f' % (v[0], v[len(v)//2], v[-1]))"; }
click_text() { # click_text TEXT TAG -> echoes the popup tab id from "new tab opened — X (tab ID)"
  local o; o=$("$BB" act click "$1" 2>&1 | head -1); echo "[$2] click: $o" | tee -a "$EVIDENCE" >&2; opened_id "$o"; }
stop_rec() { local s; s=$("$BB" state record-stop 2>&1); say "[$1] stop: $s"; STOP="$s"; }
v_colors() { # v_colors STOP K...: tones at midpoints of the given segments
  local f t k out=""; f=$(saved_path "$1"); [ -n "$f" ] || f=$(grep -oE '/[^ ;]+\.webm' <<<"$1" | head -1); t=$(num_after "$1" duration); shift
  for k in "$@"; do out="$out$(tone_at "$f" "$(mid_of "$STOP" "$k" "$t")") "; done; echo "${out% }"; }

sc_kinds() {
  say "== kinds: 4 popup kinds x 3 runs, no switch-tab; follow delay = seconds after the click returns until state tabs marks the popup R"
  local kind text want O P d run delays segs lastseg nseg
  new_home; export BLADE_RECORD=1
  for spec in "blank|open blank|blue" "js|open js|blue" "window|open window|blue" "xorigin|open xorigin|green"; do
    kind=${spec%%|*}; text=$(cut -d'|' -f2 <<<"$spec"); want=${spec##*|}; delays=""
    for run in 1 2 3; do
      "$BB" nav "$FIX/popper.html" >/dev/null 2>&1; O=$(cur_id)
      "$BB" state record-start "k$kind$run" >/dev/null 2>&1; sleep 1
      P=$(click_text "$text" "kinds:$kind")
      d=$(video_delay "$O") || d=""
      say "[kinds:$kind] run $run popup=$P follow delay ${d:-NEVER}"
      [ -n "$d" ] && delays="$delays $d"
      sleep 2.5; "$BB" act press Tab >/dev/null 2>&1; sleep 1.5
      stop_rec "kinds:$kind"
      nseg=$(seg_ids "$STOP" | wc -l | tr -d ' '); lastseg=$(seg_ids "$STOP" | tail -1)
      check "kinds:$kind" "run $run: popup is a segment without switch-tab, first segment opener" test "$nseg" -ge 2 -a "$(seg_ids "$STOP" | head -1)" = "$O" -a "$(seg_ids "$STOP" | sed -n 2p)" = "$P"
      if [ "$kind" = window ]; then
        check "kinds:$kind" "run $run: window popup (own window) keeps the video until it closes (opener input does not re-front it)" test "$lastseg" = "$P"
      else
        check "kinds:$kind" "run $run: video back on the opener after act press Tab" test "$lastseg" = "$O"
      fi
      [ "$run" = 1 ] && analyse "kinds:$kind" "$(saved_path "$STOP")" "$STOP"
      [ "$run" = 1 ] && check "kinds:$kind" "popup segment colour is $want" test "$(v_colors "$STOP" 1)" = "$want"
      close_others "$O"
    done
    say "follow delay $kind: $(stats $delays) (s, 3 runs)"
  done
  unset BLADE_RECORD; end_scn kinds
}

sc_close() {
  say "== close: popup closed with state close-tab while recorded -> back on the opener"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/popper.html" >/dev/null 2>&1; local O P; O=$(cur_id)
  "$BB" state record-start pc >/dev/null 2>&1; sleep 1
  P=$(click_text "open blank" close); sleep 2.5
  "$BB" state close-tab "$P" 2>&1 | head -1 | sed 's/^/[close] close-tab: /' | tee -a "$EVIDENCE"; sleep 2.5
  say "[close] tabs: $("$BB" state tabs 2>&1 | head -2 | tr '\n' '|')"
  stop_rec close
  check close "segments O, P, O" test "$(seg_ids "$STOP" | tr '\n' ' ')" = "$O $P $O "
  check close "note: popup closed, back on opener" grep -q "popup $P closed at .*back on opener $O" <<<"$STOP"
  analyse close "$(saved_path "$STOP")" "$STOP"
  check close "colours red blue red" test "$(v_colors "$STOP" 0 1 2)" = "red blue red"
  unset BLADE_RECORD; end_scn close
}

sc_closeopener() {
  say "== close-opener: the opener is closed while the popup is recorded"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/popper.html" >/dev/null 2>&1; local O P
  O=$(cur_id)
  "$BB" state record-start co >/dev/null 2>&1; sleep 1
  P=$(click_text "open blank" close-opener); sleep 2
  "$BB" state close-tab "$O" 2>&1 | head -1 | sed 's/^/[close-opener] close-tab opener: /' | tee -a "$EVIDENCE"; sleep 2.5
  check close-opener "video stays on the popup after the opener closed" test "$(video_id)" = "$P"
  say "[close-opener] tabs: $("$BB" state tabs 2>&1 | head -3 | tr '\n' '|')"
  "$BB" state close-tab "$P" 2>&1 | head -1 | sed 's/^/[close-opener] close-tab popup: /' | tee -a "$EVIDENCE"; sleep 2.5
  stop_rec close-opener
  local f; f=$(grep -oE '/[^ ;]+\.webm' <<<"$STOP" | head -1)
  check close-opener "last tab closed: stop reports 'no tab left' and keeps the file (not a failure of the file)" grep -q "no tab left at " <<<"$STOP"
  analyse close-opener "$f" "$STOP"
  check close-opener "popup segment is blue" test "$(v_colors "$STOP" 1)" = blue
  unset BLADE_RECORD; end_scn close-opener
}

sc_chain() {
  say "== chain: popup of a popup (O -> P -> Q), Q closed -> back on P"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/popper.html" >/dev/null 2>&1; local O P Q
  O=$(cur_id)
  "$BB" state record-start ch >/dev/null 2>&1; sleep 1
  click_text "open chain" chain >/dev/null; sleep 1.5; P=$(video_id)
  "$BB" state switch-tab "$P" >/dev/null 2>&1
  Q=$(click_text "open grandchild" chain); sleep 2.5
  "$BB" state close-tab "$Q" >/dev/null 2>&1; sleep 2.5
  stop_rec chain
  check chain "segments O, P, Q, P" test "$(seg_ids "$STOP" | tr '\n' ' ')" = "$O $P $Q $P "
  analyse chain "$(saved_path "$STOP")" "$STOP"
  check chain "colours red blue green blue" test "$(v_colors "$STOP" 0 1 2 3)" = "red blue green blue"
  unset BLADE_RECORD; end_scn chain
}

sc_burst() {
  say "== burst: three popups in a row"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/popper.html" >/dev/null 2>&1; local O P1 P2 P3
  O=$(cur_id)
  "$BB" state record-start bu >/dev/null 2>&1; sleep 1
  P1=$(click_text "open blank" burst); P2=$(click_text "open blank" burst); P3=$(click_text "open blank" burst)
  sleep 3
  say "[burst] popups $P1 $P2 $P3; tabs: $("$BB" state tabs 2>&1 | head -5 | tr '\n' '|')"
  stop_rec burst
  # Timing decides whether a popup reached the front before the next click re-fronted the opener: each
  # popup is then either a segment or named in the "popup(s) not recorded (never in front)" note.
  check burst "every popup is a segment or named in the not-recorded note" bash -c "grep -q '$P1' <<<'$STOP' && grep -q '$P2' <<<'$STOP' && grep -q '$P3' <<<'$STOP'"
  say "[burst] segments: $(seg_ids "$STOP" | wc -l | tr -d ' '); not-recorded note present: $(grep -c 'not recorded (never in front)' <<<"$STOP")"
  check burst "file saved, first segment the opener" test "$(seg_ids "$STOP" | head -1)" = "$O"
  analyse burst "$(saved_path "$STOP")" "$STOP"
  unset BLADE_RECORD; end_scn burst
}

sc_blocked() {
  say "== blocked: bladebro's Chrome runs with --disable-popup-blocking, so a gesture-less window.open IS a real tab and is followed; rel=noopener link informational"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/popper.html" >/dev/null 2>&1; local O P
  O=$(cur_id)
  "$BB" state record-start bl >/dev/null 2>&1; sleep 0.5
  "$BB" nav "$FIX/blocked.html" >/dev/null 2>&1   # its script calls window.open 300 ms after load, no gesture
  sleep 3
  stop_rec blocked
  P=$(seg_ids "$STOP" | sed -n 2p)
  check blocked "popup opened without a gesture is followed (second segment present, first is the recorded tab)" test -n "$P" -a "$(seg_ids "$STOP" | head -1)" = "$O"
  analyse blocked "$(saved_path "$STOP")" "$STOP"
  close_others "$O"
  "$BB" nav "$FIX/popper.html" >/dev/null 2>&1
  "$BB" state record-start nop >/dev/null 2>&1; sleep 1
  click_text "open noopener" blocked >/dev/null; sleep 3
  stop_rec blocked-noopener
  say "[blocked-noopener] segments: $(seg_ids "$STOP" | wc -l | tr -d ' ') (informational: opener unknown for rel=noopener popups)"
  analyse blocked-noopener "$(saved_path "$STOP")" "$STOP"
  unset BLADE_RECORD; end_scn blocked
}

sc_tabs() {
  say "== tabs: markers and footer while recording"
  new_home; export BLADE_RECORD=1
  "$BB" nav "$FIX/popper.html" >/dev/null 2>&1; local O P t1 t2
  O=$(cur_id)
  "$BB" state record-start tb >/dev/null 2>&1; sleep 1
  P=$(click_text "open blank" tabs); sleep 1
  t1=$("$BB" state tabs 2>&1); say "[tabs] during the popup:"; echo "$t1" | tee -a "$EVIDENCE"
  check tabs "agent row is '* ' on the opener, video row ' R' on the popup, differ footer" bash -c "grep -q '^\*  $O' <<<'$t1' && grep -q '^ R $P' <<<'$t1' && grep -q '(\* agent, R video: differ; switch-tab to move the agent)' <<<'$t1'"
  "$BB" act press Tab >/dev/null 2>&1; sleep 1.5
  t2=$("$BB" state tabs 2>&1); say "[tabs] after act press Tab:"; echo "$t2" | tee -a "$EVIDENCE"
  check tabs "after the press: '*R' on the opener, no differ footer" bash -c "grep -q '^\*R $O' <<<'$t2' && ! grep -q 'differ' <<<'$t2'"
  stop_rec tabs
  unset BLADE_RECORD; end_scn tabs
}

sc_pertab() {
  say "== per-tab: followed popup gets its own file (BLADE_RECORD_TABS=per-tab), closed popup finalized, opener file continues"
  new_home; export BLADE_RECORD=1 BLADE_RECORD_TABS=per-tab
  "$BB" nav "$FIX/popper.html" >/dev/null 2>&1
  "$BB" state record-start pt >/dev/null 2>&1; sleep 1
  click_text "open selfclose" per-tab >/dev/null; sleep 5.5
  stop_rec per-tab
  local n=0 line path want=(red blue) total k tones
  check per-tab "header: per-tab, 2 files" grep -q "recording saved (per-tab, 2 files): " <<<"$STOP"
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    path=$(line_path "$line"); analyse per-tab "$path" "$line"
    total=$(num_after "$line" duration); tones=""
    for k in $(seq 0 $(( $(seg_ids "$line" | wc -l) - 1 ))); do tones="$tones $(tone_at "$path" "$(mid_of "$line" "$k" "$total")")"; done
    say "[per-tab] file $((n + 1)) segment colours:$tones"
    check per-tab "file $((n + 1)) shows only ${want[$n]}" test "$(echo $tones | tr ' ' '\n' | sort -u | tr '\n' ' ')" = "${want[$n]} "
    n=$((n + 1))
  done <<<"$(file_lines "$STOP")"
  check per-tab "two files" test "$n" = 2
  unset BLADE_RECORD BLADE_RECORD_TABS; end_scn per-tab
}

sc_exit() {
  say "== exit: popup segment active, then MCP stdin EOF / MCP SIGTERM / daemon stop / daemon SIGTERM"
  EXIT_EVIDENCE="$EVIDENCE" REC_HUB_FIX="$FIX" EXIT_BIN="$BB" python3 -I "$HERE/../lib/popup_exit.py"
  [ $? = 0 ] || FAILS=$((FAILS + 1))
}

# off: recording OFF, same CLI session through cdp_tap against the pre-phase binary (POPUP_BASE_BIN, a92ad77)
popup_trace() { # popup_trace LABEL BIN RUN
  local label="$1" bin="$2" run="$3" log port lp i id
  new_home; unset BLADE_RECORD; CUR_BIN="$bin"
  "$bin" nav "$FIX/a.html" >/dev/null 2>&1
  port=$(bb_port) || { say "FAIL [off] no Chrome port for $label"; FAILS=$((FAILS + 1)); end_scn off; return; }
  lp=$((19300 + RANDOM % 600)); log="$SCRATCH/trace-$label-$run.log"; : > "$log"
  python3 -I "$RS/lib/cdp_tap.py" "$lp" "$port" "$log" & TAP_PID=$!
  i=0; while [ $i -lt 50 ] && ! curl -sf "http://127.0.0.1:$lp/json/version" >/dev/null 2>&1; do sleep 0.1; i=$((i + 1)); done
  : > "$log"
  local a=(--host 127.0.0.1 --port "$lp")
  "$bin" "${a[@]}" nav "$FIX/popper.html" >/dev/null 2>&1
  id=$("$bin" "${a[@]}" act click "open blank" 2>&1 | head -1 | sed -nE 's/.*tab ([0-9A-F]{32}).*/\1/p')
  sleep 2
  "$bin" "${a[@]}" state tabs >/dev/null 2>&1
  "$bin" "${a[@]}" state switch-tab "$id" >/dev/null 2>&1
  "$bin" "${a[@]}" state close-tab "$id" >/dev/null 2>&1
  sleep 1
  kill "$TAP_PID" 2>/dev/null; TAP_PID=""
  grep '^WS ' "$log" | sed 's/^WS //' | sort > "$SCRATCH/m-$label-$run.txt"
  end_scn "off-trace-$label-$run"
}

sc_off() {
  say "== off: recording OFF: CDP method trace of nav, popup click, tabs, switch-tab, close-tab equals the a92ad77 binary"
  local base="${POPUP_BASE_BIN:-}" lab run
  [ -x "$base" ] || { say "FAIL [off] POPUP_BASE_BIN (binary built from a92ad77) missing"; FAILS=$((FAILS + 1)); return; }
  say "[off] base binary: $base (a92ad77)"
  for run in 1 2; do popup_trace base "$base" $run; popup_trace new "$BB" $run; done
  for lab in base new; do
    cat "$SCRATCH/m-$lab-1.txt" "$SCRATCH/m-$lab-2.txt" | sort -u > "$SCRATCH/set-$lab.txt"
    say "[off] $lab distinct WS methods: $(wc -l < "$SCRATCH/set-$lab.txt" | tr -d ' ')"
    for run in 1 2; do say "[off] $lab run $run: Page.bringToFront=$(grep -c '^Page.bringToFront$' "$SCRATCH/m-$lab-$run.txt") Target.getTargets=$(grep -c '^Target.getTargets$' "$SCRATCH/m-$lab-$run.txt") total=$(wc -l < "$SCRATCH/m-$lab-$run.txt" | tr -d ' ')"; done
  done
  check off "traces non-empty" test -s "$SCRATCH/set-base.txt" -a -s "$SCRATCH/set-new.txt"
  check off "distinct WS method sets equal (base vs new)" cmp -s "$SCRATCH/set-base.txt" "$SCRATCH/set-new.txt"
  local m
  for m in Page.bringToFront Target.getTargets; do
    check off "$m count per run equal (base vs new): $(for r in 1 2; do grep -c "^$m\$" "$SCRATCH/m-base-$r.txt"; done | tr '\n' ' ')vs $(for r in 1 2; do grep -c "^$m\$" "$SCRATCH/m-new-$r.txt"; done | tr '\n' ' ')" \
      test "$(grep -c "^$m\$" "$SCRATCH/m-base-1.txt") $(grep -c "^$m\$" "$SCRATCH/m-base-2.txt")" = "$(grep -c "^$m\$" "$SCRATCH/m-new-1.txt") $(grep -c "^$m\$" "$SCRATCH/m-new-2.txt")"
  done
  check off "no screencast method in new" test -z "$(cat "$SCRATCH"/m-new-*.txt | grep -i screencast)"
}

sc_mcp() {
  say "== mcp: tests/record_popup.rs live tests (MCP stdio, headless)"
  new_home
  local out rc
  out=$(REC_HUB_FIX="$FIX" REC_HUB_FFMPEG="$FFMPEG" REC_HUB_FFPROBE="$FFPROBE" cargo test --release --test record_popup -- --ignored --test-threads=1 --nocapture 2>&1); rc=$?
  echo "$out" | grep -E "^(RESULT popup .* stop|RESULT follow delay|RESULT popup colors|test |test result)|RESULT follow delay|panicked" | cut -c1-400 | sed 's/^/[mcp] /' | tee -a "$EVIDENCE" >/dev/null
  check mcp "the four live popup tests pass (exit=$rc)" test "$rc" = 0
  end_scn mcp
}

file_lines() { grep -oE '/[^ ;]+ \(bytes [^)]*\)' <<<"$1"; }
line_path() { echo "${1%% (bytes*}"; }

for id in spike kinds close close-opener chain burst blocked tabs per-tab exit off mcp; do
  want "$id" || continue
  [ "$id" = spike ] && [ "$ONLY" != spike ] && continue
  case "$id" in
    spike) sc_spike ;; kinds) sc_kinds ;; close) sc_close ;; close-opener) sc_closeopener ;; chain) sc_chain ;;
    burst) sc_burst ;; blocked) sc_blocked ;; tabs) sc_tabs ;; per-tab) sc_pertab ;; exit) sc_exit ;; off) sc_off ;; mcp) sc_mcp ;;
  esac
done
say "FAILS=$FAILS"
exit "$FAILS"
