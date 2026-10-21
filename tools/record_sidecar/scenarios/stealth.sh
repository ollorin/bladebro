#!/usr/bin/env bash
# SC3 stealth half: `bladebro audit` without vs with the sidecar screencast attached to audit's
# own browser, the verbatim diff_oracle attempt, and the oracle battery diffed in the bladebro
# browser with vs without a screencast (A, B, A2; A-vs-A2 is run-to-run noise).
# Runtime about 6-8 minutes. Writes results/stealth.json and results/audit_*.txt,
# results/diff_oracle_attempt.txt.
set -u
source "$(dirname "$0")/../env.sh"
trap bb_cleanup EXIT
fixture_up || exit 1
cd "$REPO" || exit 1
R="$RS/results"
rm -f $R/stats/audit_page.stats.json $R/stats/audit_nopage.stats.json $R/stats/battery_screencast.stats.json
home() { mktemp -d -t bbrec; }
teardown() { BLADE_HOME="$1" $BB stop >/dev/null 2>&1; pkill -f "$1" 2>/dev/null; sleep 1; rm -rf "$1"; }

# Part A: audit without screencast
HA=$(home)
BLADE_HOME=$HA $BB audit > $R/audit_without_screencast.txt 2>&1
echo "audit_exit=$?" >> $R/audit_without_screencast.txt
teardown "$HA"

# Part B: audit with the sidecar attached to audit's own browser
audit_with() {   # tag extra-sidecar-flags...
  local tag="$1"; shift
  local H; H=$(home)
  local OUTF=$R/audit_with_screencast.txt
  [ "$tag" = nopage ] && OUTF=$R/audit_with_screencast_no_page_enable.txt
  BLADE_HOME=$H $BB audit > $OUTF 2>&1 &
  local AU=$!
  # audit's browser uses a fixed port under $H/profiles/sess-N (no DevToolsActivePort), so poll
  # the process command line (bb_port) as soon as it appears and attach right away.
  local P="" i=0
  while [ -z "$P" ] && [ $i -lt 300 ]; do
    P=$(BLADE_HOME=$H bb_port) || P=""
    [ -z "$P" ] && sleep 0.1
    i=$((i + 1))
  done
  if [ -z "$P" ]; then echo "no audit browser found ($tag)" >&2; wait $AU; echo "audit_exit=$?" >> $OUTF; teardown "$H"; return; fi
  node $RS/sidecar.mjs --port "$P" --follow poll --duration 150 --codec vp9 --ts wallclock \
       "$@" --out $SCRATCH/audit_$tag.webm --stats $SCRATCH/audit_$tag.stats.json 2>$SCRATCH/audit_$tag.err &
  local SC=$!
  wait $AU
  echo "audit_exit=$?" >> $OUTF
  wait $SC
  echo "sidecar_exit=$? tag=$tag" >&2
  tail -3 $SCRATCH/audit_$tag.err >&2
  cp $SCRATCH/audit_$tag.stats.json $R/stats/audit_$tag.stats.json 2>/dev/null
  tail -2 $SCRATCH/audit_$tag.err > $SCRATCH/last_err_$tag.txt
  teardown "$H"
}
# The attach races the audit's own startup (the sidecar can hit a not-yet-active page, exit 1).
# Retry up to 8 fresh audits until the sidecar exits 0 with frames; attempts are recorded.
for TAG in page nopage; do
  FLAGS=""; [ "$TAG" = nopage ] && FLAGS="--no-page-enable"
  n=0
  while [ $n -lt 8 ]; do
    n=$((n + 1))
    audit_with $TAG $FLAGS
    if node -e 'const s=JSON.parse(require("fs").readFileSync(process.argv[1]));process.exit(s.frames_received>0?0:1)' $SCRATCH/audit_$TAG.stats.json 2>/dev/null; then break; fi
  done
  echo "$n" > $SCRATCH/attempts_$TAG
done

# Part C: diff_oracle verbatim, then the substitute
python3 tools/diff_oracle/oracle.py --url "$FIX/idle.html" > $R/diff_oracle_attempt.txt 2>&1
echo "exit=$?" >> $R/diff_oracle_attempt.txt
# The first refusal is only "no Chrome found" (CHROME_PATH unset); record the next gate too.
echo "--- second attempt with CHROME_PATH set to the system Chrome ---" >> $R/diff_oracle_attempt.txt
CHROME_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" python3 tools/diff_oracle/oracle.py --url "$FIX/idle.html" >> $R/diff_oracle_attempt.txt 2>&1
echo "exit=$?" >> $R/diff_oracle_attempt.txt

HC=$(home); export BLADE_HOME=$HC
# anim.html repaints at ~100 fps, so the screencast is genuinely delivering frames during B
$BB nav "$FIX/anim.html" >/dev/null 2>&1 || echo "nav failed" >&2
sleep 2
python3 $RS/battery_probe.py run $SCRATCH/A.json
P=$(bb_port)
node $RS/sidecar.mjs --port "$P" --duration 60 --codec vp9 --ts wallclock --out $SCRATCH/bat.webm --stats $SCRATCH/bat.stats.json 2>$SCRATCH/bat.err &
SC=$!
sleep 3
python3 $RS/battery_probe.py run $SCRATCH/B.json
kill -INT $SC; wait $SC
cp $SCRATCH/bat.stats.json $R/stats/battery_screencast.stats.json 2>/dev/null
python3 $RS/battery_probe.py run $SCRATCH/A2.json
python3 $RS/battery_probe.py classify $SCRATCH/A.json $SCRATCH/B.json $SCRATCH/A2.json > $SCRATCH/classify.json
teardown "$HC"

# Assemble stealth.json (scores regex-parsed from the audit files; nothing typed by hand)
SCRATCH="$SCRATCH" RS="$RS" node $RS/scenarios/stealth_assemble.mjs
