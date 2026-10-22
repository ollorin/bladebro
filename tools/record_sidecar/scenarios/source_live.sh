#!/usr/bin/env bash
# Live screencast-source checks (06-02): real headless Chrome on local fixtures, the real
# FrameSource + Encoder (tests/record_live.rs), ffprobe + full-decode verification.
# Safety: BLADE_HEADLESS=1 always, a fresh scratch BLADE_HOME per scenario,
# BLADE_NO_UPDATE_CHECK=1, 127.0.0.1 fixtures only. Never touches a real profile or ~/.blade.
#
#   bash tools/record_sidecar/scenarios/source_live.sh [scenario ...]   (default: all)
#   scenarios: dur-idle dur-tick dur-anim resize bg bg-control   (bg-control runs inside bg)
#   06-03:     trace  (wire-level CDP method trace via a test-only WS proxy + Chrome argv before/during/after)
#              slow   (300 ms/frame consumer, queue 16, bladebro CLI commands issued mid-recording)
#              all    (= trace slow)
# Writes: $EVIDENCE (default evidence/06-02-live.txt; evidence/06-03-trace.txt when the first
#         scenario is trace, slow or all)
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../env.sh
source "$HERE/../env.sh"
export BLADE_HEADLESS=1 BLADE_NO_UPDATE_CHECK=1
# env.sh made BLADE_HOME only if unset; each scenario gets its own, so drop the shared one.
FFPROBE="${BLADE_FFPROBE:-$HOME/.local/ffmpeg/bin/ffprobe}"
FFMPEG="${BLADE_FFMPEG:-$HOME/.local/ffmpeg/bin/ffmpeg}"
case "${1:-}" in
  trace|slow|all) EV_DEFAULT=06-03-trace.txt ;;
  *) EV_DEFAULT=06-02-live.txt ;;
esac
EVIDENCE="${EVIDENCE:-$REPO/.planning/phases/06-frame-source/evidence/$EV_DEFAULT}"
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

# main_argv: the main Chrome process command line for this home (one line, helpers excluded)
main_argv() {
  local d
  for d in "$BLADE_HOME" "/private$BLADE_HOME"; do
    ps -ax -o command= | grep -- "--user-data-dir=$d" | grep -v Helper | grep -v grep | head -1
  done | grep . | head -1
}
now_ms() { python3 -I -c 'import time; print(int(time.time()*1000))'; }

scenario_trace() {
  local P T out argv_b argv_d argv_a line
  new_home
  say "=== trace: anim.html, 12s, REC_LIVE_MODE=trace (WS proxy between the Source and the page) ==="
  "$BB" nav "$FIX/anim.html" >/dev/null 2>&1
  P=$(bb_port) || { say "[trace] no chrome port"; end_home trace; return; }
  read -r T _ < <(page_targets "$P" | head -1)
  out="$SCRATCH/trace.webm"
  main_argv > "$SCRATCH/argv_before.txt"
  record trace "$P" "$T" 12 trace "$out" &
  local RP=$!
  sleep 6
  main_argv > "$SCRATCH/argv_during.txt"
  wait "$RP"
  main_argv > "$SCRATCH/argv_after.txt"
  say "[trace] argv_before: $(cat "$SCRATCH/argv_before.txt")"
  say "[trace] argv bytes before/during/after: $(wc -c < "$SCRATCH/argv_before.txt")/$(wc -c < "$SCRATCH/argv_during.txt")/$(wc -c < "$SCRATCH/argv_after.txt")"
  say "[trace] diff before vs during: $(diff "$SCRATCH/argv_before.txt" "$SCRATCH/argv_during.txt" >/dev/null && echo identical || echo DIFFERENT)"
  say "[trace] diff before vs after:  $(diff "$SCRATCH/argv_before.txt" "$SCRATCH/argv_after.txt" >/dev/null && echo identical || echo DIFFERENT)"
  [ -s "$SCRATCH/argv_before.txt" ] || say "[trace] FAIL: argv_before empty (main Chrome not found)"
  line=$(grep '^\[trace\] REC_LIVE_REPORT ' "$EVIDENCE" | tail -1)
  LAST_WALL=$(printf '%s' "${line#\[trace\] REC_LIVE_REPORT }" | python3 -I -c 'import sys,json; print(json.load(sys.stdin)["wall_ms"])')
  printf '%s' "${line#\[trace\] REC_LIVE_REPORT }" | python3 -I -c '
import sys,json
r=json.load(sys.stdin); w=r["wire_methods"]; s=r["source"]
allowed={"Page.enable","Page.startScreencast","Page.screencastFrameAck","Page.stopScreencast"}
ms=dict((m,n) for m,n in s["methods_sent"])
first=r["wire_first"]
def chk(name, ok): print("[trace] %s: %s" % ("PASS" if ok else "FAIL", name))
print("[trace] wire_methods:", json.dumps(w, sort_keys=True))
print("[trace] source methods_sent:", json.dumps(ms, sort_keys=True))
print("[trace] wire_first:", first)
chk("wire methods subset of the 4 allowed: extra=%s" % sorted(set(w)-allowed), set(w)<=allowed)
chk("Page.enable is the first wire method", first[:1]==["Page.enable"])
chk("Page.startScreencast before the first ack", "Page.startScreencast" in first and ("Page.screencastFrameAck" not in first or first.index("Page.startScreencast")<first.index("Page.screencastFrameAck")))
chk("wire acks (%s) == source acks_sent (%s)" % (w.get("Page.screencastFrameAck",0), s["acks_sent"]), w.get("Page.screencastFrameAck",0)==s["acks_sent"])
chk("wire_methods == source methods_sent", w==ms)
chk("frames_received %s == acks_sent %s, frames > 0" % (s["frames_received"], s["acks_sent"]), s["frames_received"]==s["acks_sent"] and s["frames_received"]>0)
print("[trace] frame_sizes:", json.dumps(r["frame_sizes"]))
' | tee -a "$EVIDENCE"
  [ "$(diff "$SCRATCH/argv_before.txt" "$SCRATCH/argv_during.txt" >/dev/null && diff "$SCRATCH/argv_before.txt" "$SCRATCH/argv_after.txt" >/dev/null && echo y)" = y ] \
    && say "[trace] PASS: Chrome main argv identical before/during/after" || say "[trace] FAIL: Chrome main argv changed"
  analyse trace "$out" "$LAST_WALL"
  end_home trace
}

scenario_slow() {
  local P T out RP rc el t0 t1 i cmd
  new_home
  say "=== slow: tick.html then anim.html, 25s, REC_LIVE_MODE=slow; bladebro commands at ~5/10/15s ==="
  "$BB" nav "$FIX/tick.html" >/dev/null 2>&1
  "$BB" nav "$FIX/anim.html" >/dev/null 2>&1
  P=$(bb_port) || { say "[slow] no chrome port"; end_home slow; return; }
  read -r T _ < <(page_targets "$P" | head -1)
  out="$SCRATCH/slow.webm"
  local RS0; RS0=$(now_ms)
  REC_LIVE_PORT="$P" REC_LIVE_TARGET="$T" REC_LIVE_OUT="$out" REC_LIVE_SECS=25 REC_LIVE_MODE=slow \
    "$BIN" --ignored --exact record_live --nocapture >"$SCRATCH/slow.stdout" 2>"$SCRATCH/slow.stderr" &
  RP=$!
  i=0
  for cmd in "see content" "state tabs" "nav $FIX/a.html"; do
    i=$((i + 1))
    while [ $(( $(now_ms) - RS0 )) -lt $(( 5000 * i )) ]; do sleep 0.2; done
    t0=$(now_ms)
    # shellcheck disable=SC2086
    "$BB" $cmd > "$SCRATCH/slow_cmd$i.out" 2>&1; rc=$?
    t1=$(now_ms)
    say "[slow] cmd$i '$cmd' at +$((t0 - RS0))ms rc=$rc elapsed_ms=$((t1 - t0)) out_bytes=$(wc -c < "$SCRATCH/slow_cmd$i.out") first_line: $(head -1 "$SCRATCH/slow_cmd$i.out" | cut -c1-120)"
    [ "$i" = 3 ] && NAV_AT=$((t0 - RS0))
    echo "$rc" > "$SCRATCH/slow_rc$i"
  done
  wait "$RP"
  line=$(grep '^REC_LIVE_REPORT ' "$SCRATCH/slow.stdout")
  say "[slow] $line"
  LAST_WALL=$(echo "${line#REC_LIVE_REPORT }" | python3 -I -c 'import sys,json; print(json.load(sys.stdin)["wall_ms"])')
  printf '%s' "${line#REC_LIVE_REPORT }" | NAV_AT="$NAV_AT" python3 -I -c '
import sys,json,os
r=json.load(sys.stdin); s=r["source"]; sl=r["slow_sink"]
def chk(name, ok): print("[slow] %s: %s" % ("PASS" if ok else "FAIL", name))
print("[slow] slow_sink:", json.dumps(sl), " source queue_dropped=%s forwarded=%s capped=%s" % (s["queue_dropped"], s["forwarded"], s["capped"]))
chk("max_len %s <= 16" % sl["max_len"], sl["max_len"]<=16)
chk("refused %s >= 1 and == source queue_dropped %s" % (sl["refused"], s["queue_dropped"]), sl["refused"]>=1 and sl["refused"]==s["queue_dropped"])
chk("accepted %s == source forwarded %s" % (sl["accepted"], s["forwarded"]), sl["accepted"]==s["forwarded"])
chk("frames_received %s == acks_sent %s" % (s["frames_received"], s["acks_sent"]), s["frames_received"]==s["acks_sent"] and s["frames_received"]>0)
nav=int(os.environ["NAV_AT"]); first=s["first_frame_ms"] or 0; last=s["last_frame_ms"]
print("[slow] same-tab nav at ~+%d ms (driver clock; source clock starts a few ms later); last_frame_ms=%s last_frame_age_ms=%s longest_gap_ms=%s" % (nav, last, s["last_frame_age_ms"], s["longest_gap_ms"]))
print("[slow] frames after the nav: %s" % ("YES (last frame %d ms after nav)" % (last-nav) if last is not None and last>nav else "NO (finding: quiet after same-tab nav)"))
print("[slow] frame_sizes:", json.dumps(r["frame_sizes"]))
' | tee -a "$EVIDENCE"
  [ "$(cat "$SCRATCH/slow_rc1" "$SCRATCH/slow_rc2" "$SCRATCH/slow_rc3" | tr -d '\n')" = "000" ] \
    && say "[slow] PASS: all three bladebro commands rc 0" || say "[slow] FAIL: a bladebro command failed"
  analyse slow "$out" "$LAST_WALL"
  end_home slow
}

WANT=("$@"); [ ${#WANT[@]} -eq 0 ] && WANT=(dur-idle dur-tick dur-anim resize bg)
for s in "${WANT[@]}"; do
  case "$s" in
    dur-idle) scenario_dur dur-idle idle.html 20 ;;
    dur-tick) scenario_dur dur-tick tick.html 20 ;;
    dur-anim) scenario_dur dur-anim anim.html 20 ;;
    resize)   scenario_dur resize anim.html 16 resize ;;
    bg)       scenario_bg ;;
    trace)    scenario_trace ;;
    slow)     scenario_slow ;;
    all)      scenario_trace; scenario_slow ;;
    *) echo "unknown scenario $s" >&2 ;;
  esac
done
say "# finished: $(date -u +%FT%TZ)"
