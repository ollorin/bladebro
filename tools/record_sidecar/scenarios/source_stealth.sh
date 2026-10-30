#!/usr/bin/env bash
# Stealth instruments with the PRODUCT FrameSource attached (06-03), the product-recorder twin of
# stealth.sh (which used the Node sidecar):
#   A  `bladebro audit` without a recorder (the baseline, must itself be 100/100 on macOS)
#   B  `bladebro audit` with the recorder (tests/record_live.rs) attached to the audit's own browser
#   C  diff_oracle: NOT run (python `websockets` missing, installs forbidden) - stated, never claimed
#   D  Phase 2 substitute battery A / B (recorder attached, frames flowing) / A2
# Safety: BLADE_HEADLESS=1, a fresh scratch BLADE_HOME per part, BLADE_NO_UPDATE_CHECK=1, 127.0.0.1
# fixtures only. Runtime several minutes.
#   bash tools/record_sidecar/scenarios/source_stealth.sh
# Writes: $EVIDENCE (default .planning/phases/06-frame-source/evidence/06-03-stealth.txt)
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../env.sh
source "$HERE/../env.sh"
export BLADE_HEADLESS=1 BLADE_NO_UPDATE_CHECK=1
EVIDENCE="${EVIDENCE:-$REPO/.planning/phases/06-frame-source/evidence/06-03-stealth.txt}"
mkdir -p "$(dirname "$EVIDENCE")"
: > "$EVIDENCE"
trap 'bb_cleanup' EXIT
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
say() { echo "$*" | tee -a "$EVIDENCE"; }
say "# started: $(date -u +%FT%TZ)  headless=$BLADE_HEADLESS  chrome: $("$BB" version 2>/dev/null | head -1)"

home() { mktemp -d -t bbrecst; }
teardown() { BLADE_HOME="$1" $BB stop >/dev/null 2>&1; pkill -f "$1" 2>/dev/null; sleep 1; rm -rf "$1"; }
score() { sed -nE 's/.*SCORE:[[:space:]]*([0-9]+\/[0-9]+).*/\1/p' "$1" | head -1; }

# ---------- Part A: audit without a recorder ----------
HA=$(home)
BLADE_HOME=$HA $BB audit > "$SCRATCH/audit_A.txt" 2>&1; A_EXIT=$?
A_SCORE=$(score "$SCRATCH/audit_A.txt")
say "[A] audit without recorder: SCORE $A_SCORE exit=$A_EXIT"
teardown "$HA"

# ---------- Part B: audit with the recorder attached to the audit's own browser ----------
B_OK=0; B_ATT=0; B_SCORE=""; B_EXIT=""; B_LINE=""
while [ "$B_ATT" -lt 8 ]; do
  B_ATT=$((B_ATT + 1))
  H=$(home); export BLADE_HOME="$H"
  BLADE_HOME=$H $BB audit > "$SCRATCH/audit_B.txt" 2>&1 &
  AU=$!
  # audit's browser uses a fixed port (no DevToolsActivePort): poll the process command line.
  P=""; i=0
  while [ -z "$P" ] && [ $i -lt 300 ]; do P=$(bb_port) || P=""; [ -z "$P" ] && sleep 0.1; i=$((i + 1)); done
  if [ -z "$P" ]; then
    say "[B] attempt $B_ATT: no audit browser port within 30 s"
    wait $AU; teardown "$H"; continue
  fi
  REC_LIVE_PORT="$P" REC_LIVE_OUT="$SCRATCH/audit_rec.webm" REC_LIVE_SECS=150 REC_LIVE_MODE=plain \
    "$BIN" --ignored --exact record_live --nocapture >"$SCRATCH/rec_B.out" 2>"$SCRATCH/rec_B.err" &
  RP=$!
  wait $AU; B_EXIT=$?
  sleep 1
  kill -INT $RP 2>/dev/null
  # bounded wait for the recorder to report (SIGINT can be ignored by a background job)
  j=0; while kill -0 $RP 2>/dev/null && [ $j -lt 150 ]; do sleep 0.2; j=$((j + 1)); done
  kill -0 $RP 2>/dev/null && { say "[B] recorder pid $RP did not exit after SIGINT; killing it (own process)"; kill -9 $RP; }
  wait $RP 2>/dev/null
  B_SCORE=$(score "$SCRATCH/audit_B.txt")
  B_LINE=$(grep '^REC_LIVE_REPORT ' "$SCRATCH/rec_B.out" | tail -1)
  if [ -n "$B_LINE" ]; then
    FR=$(printf '%s' "${B_LINE#REC_LIVE_REPORT }" | python3 -I -c 'import sys,json; print(json.load(sys.stdin)["source"]["frames_received"])')
  else
    FR=0; say "[B] attempt $B_ATT: no REC_LIVE_REPORT; stderr tail: $(tail -2 "$SCRATCH/rec_B.err" | tr '\n' ' ')"
  fi
  say "[B] attempt $B_ATT: audit SCORE $B_SCORE exit=$B_EXIT recorder frames_received=$FR"
  teardown "$H"
  if [ "${FR:-0}" -gt 0 ]; then B_OK=1; break; fi
done
say "[B] attempts: $B_ATT"
[ -n "$B_LINE" ] && say "[B] $B_LINE"
printf '%s' "${B_LINE#REC_LIVE_REPORT }" | python3 -I -c '
import sys,json
try: r=json.load(sys.stdin)
except Exception: print("[B] FAIL: no recorder report"); sys.exit()
s=r["source"]
print("[B] recorder: outcome=%s frames_received=%s acks_sent=%s forwarded=%s lost=%s sizes=%s" % (s["outcome"], s["frames_received"], s["acks_sent"], s["forwarded"], s["lost"], r["frame_sizes"]))
print("[B] %s: frames > 0 and frames == acks" % ("PASS" if s["frames_received"]>0 and s["frames_received"]==s["acks_sent"] else "FAIL"))
' | tee -a "$EVIDENCE"
if [ "$A_SCORE" = "100/100" ]; then say "[A] PASS: baseline audit is 100/100"; else say "[A] FAIL: baseline audit is $A_SCORE"; fi
if [ "$B_OK" = 1 ] && [ "$B_SCORE" = "$A_SCORE" ]; then say "[B] PASS: audit with recorder SCORE $B_SCORE equals baseline $A_SCORE"; else say "[B] FAIL: audit with recorder SCORE '$B_SCORE' vs baseline '$A_SCORE' (attached=$B_OK)"; fi

# ---------- Part C: diff_oracle ----------
say "[C] diff_oracle: NOT RUN. tools/diff_oracle/oracle.py needs the python 'websockets' module, which is not installed here, and installs are forbidden. The battery A/B below is the Phase 2 substitute. Nothing is claimed for diff_oracle."

# ---------- Part D: battery A / B (recorder attached) / A2 ----------
HD=$(home); export BLADE_HOME="$HD"
"$BB" nav "$FIX/anim.html" >/dev/null 2>&1 || say "[D] nav failed"
sleep 2
python3 -I "$RS/battery_probe.py" run "$SCRATCH/A.json" | tee -a "$EVIDENCE"
P=$(bb_port)
REC_LIVE_PORT="$P" REC_LIVE_OUT="$SCRATCH/bat.webm" REC_LIVE_SECS=180 REC_LIVE_MODE=plain \
  "$BIN" --ignored --exact record_live --nocapture >"$SCRATCH/rec_D.out" 2>"$SCRATCH/rec_D.err" &
RP=$!
sleep 3
python3 -I "$RS/battery_probe.py" run "$SCRATCH/B.json" | tee -a "$EVIDENCE"
kill -INT $RP 2>/dev/null
j=0; while kill -0 $RP 2>/dev/null && [ $j -lt 150 ]; do sleep 0.2; j=$((j + 1)); done
kill -0 $RP 2>/dev/null && { say "[D] recorder pid $RP did not exit after SIGINT; killing it (own process)"; kill -9 $RP; }
wait $RP 2>/dev/null
DL=$(grep '^REC_LIVE_REPORT ' "$SCRATCH/rec_D.out" | tail -1)
[ -n "$DL" ] && say "[D] $DL"
printf '%s' "${DL#REC_LIVE_REPORT }" | python3 -I -c '
import sys,json
try: r=json.load(sys.stdin)
except Exception: print("[D] FAIL: no recorder report"); sys.exit()
s=r["source"]
print("[D] recorder during B: outcome=%s frames_received=%s acks_sent=%s forwarded=%s sizes=%s" % (s["outcome"], s["frames_received"], s["acks_sent"], s["forwarded"], r["frame_sizes"]))
print("[D] %s: frames > 0 and frames == acks" % ("PASS" if s["frames_received"]>0 and s["frames_received"]==s["acks_sent"] else "FAIL"))
' | tee -a "$EVIDENCE"
python3 -I "$RS/battery_probe.py" run "$SCRATCH/A2.json" | tee -a "$EVIDENCE"
python3 -I "$RS/battery_probe.py" classify "$SCRATCH/A.json" "$SCRATCH/B.json" "$SCRATCH/A2.json" > "$SCRATCH/classify.json"
say "[D] classify: $(tr -d '\n' < "$SCRATCH/classify.json")"
python3 -I -c '
import json,sys
c=json.load(open(sys.argv[1]))
d=c.get("divergent")
print("[D] %s: divergent=%d keys=%s %s noise_keys=%s (battery B vs A, run-to-run noise excluded)" % ("PASS" if not d else "FAIL", len(d or []), c.get("keys"), list(d) if d else "", sorted(c.get("noise") or [])))
' "$SCRATCH/classify.json" | tee -a "$EVIDENCE"
teardown "$HD"
n=$( { pgrep -fl bbrecst; pgrep -fl record_live; pgrep -fl ffmpeg; } 2>/dev/null | grep -v pgrep | grep -c . )
say "[end] leftover processes: $n"
say "# finished: $(date -u +%FT%TZ)"
