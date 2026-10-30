#!/usr/bin/env bash
# SC2 core measurements: fps, ack latency, slow consumer, 60 s frames vs acks, drift on idle and
# tick fixtures. Headed by default (the plan's reference cadence, matching the 02-01 smoke);
# BB_HEADLESS=1 runs headless. Each group is its own browser session of <= 2 min; each session
# is torn down (daemon, scratch Chrome, BLADE_HOME) before the next one starts.
# The derived block is computed by scenarios/core_derive.mjs from the recorded runs.
set -u
source "$(dirname "$0")/../env.sh"
trap bb_cleanup EXIT
fixture_up || exit 1
OUT="$RS/results/core.json"
RUNS_DIR="$SCRATCH/runs"; mkdir -p "$RUNS_DIR"

session() {   # $1 = fixture file. Sets P (endpoint port) for rec().
  export BLADE_HOME="$(mktemp -d -t bbrec)"
  if [ "${BB_HEADLESS:-0}" = "1" ]; then export BLADE_CHROME_FLAGS="--headless=new"; else unset BLADE_CHROME_FLAGS; fi
  $BB nav "$FIX/$1" >/dev/null 2>&1 || { echo "nav $1 failed" >&2; return 1; }
  P=$(bb_port) || { echo "no port for $1" >&2; return 1; }
  sleep 2
}

teardown_session() {
  $BB stop >/dev/null 2>&1
  pkill -f "$BLADE_HOME" 2>/dev/null
  sleep 1
  rm -rf "$BLADE_HOME"
}

rec() {       # $1 = run name, $2 = duration seconds, rest = sidecar flags
  local name="$1" dur="$2"; shift 2
  local st="$RUNS_DIR/$name.stats.json" vid="$RUNS_DIR/$name.webm"
  node "$RS/sidecar.mjs" --port "$P" --duration "$dur" "$@" --out "$vid" --stats "$st" 2>"$RUNS_DIR/$name.err"
  local rc=$?
  if [ -f "$st" ] && node "$RS/measure.mjs" merge "$name" "$st" "$vid" > "$RUNS_DIR/$name.merged.json" 2>"$RUNS_DIR/$name.merge.err"; then
    echo "$name: rc=$rc ok" >&2
  else
    # A failed run is kept, not dropped: exit code, encoder tail and error text go in the entry.
    node -e '
      const fs = require("fs");
      const [name, rc, st, err, outp] = process.argv.slice(1);
      let stats = null; try { stats = JSON.parse(fs.readFileSync(st, "utf8")); } catch {}
      let errText = ""; try { errText = fs.readFileSync(err, "utf8"); } catch {}
      fs.writeFileSync(outp, JSON.stringify({ name, exit_code: Number(rc), error: (errText.trim() || "stats missing").slice(-600), encoder: { stderr_tail: stats?.encoder?.stderr_tail ?? null } }, null, 2));
    ' "$name" "$rc" "$st" "$RUNS_DIR/$name.merge.err" "$RUNS_DIR/$name.merged.json"
    echo "$name: rc=$rc FAILED (kept as failed entry)" >&2
  fi
  rm -f "$vid"   # the videos are large and not evidence; the stats and merged metrics are
}

# --- Group A: anim fixture, fps / every-nth / slow consumer --------------------------------
session anim.html || exit 1
rec fps_default 20 --codec vp9 --ts wallclock
rec fps_nth2 15 --codec vp9 --ts wallclock --every-nth 2
rec slow_consumer 20 --codec vp9 --ts wallclock --slow-writer-ms 200 --queue 8
teardown_session

# --- Group B: anim fixture, 60 s frames vs acks -------------------------------------------
session anim.html || exit 1
rec acks_60s 60 --codec vp9 --ts wallclock
teardown_session

# --- Group C: idle fixture. idle.html repaints almost never (about 1 frame per run), so these
# drift numbers show only the tail hold and cfr padding. Two sessions keep each under 2 min.
session idle.html || exit 1
rec drift_wallclock 30 --codec vp9 --ts wallclock
rec drift_concat 30 --codec vp9 --ts concat
teardown_session

session idle.html || exit 1
rec drift_cfr 30 --codec vp9 --ts cfr --fps 10
rec drift_wallclock_notail 10 --codec vp9 --ts wallclock --no-tail
teardown_session

# --- Group D: tick fixture (one repaint per second): the drift comparison that matters.
session tick.html || exit 1
rec drift_tick_wallclock 30 --codec vp9 --ts wallclock
rec drift_tick_concat 30 --codec vp9 --ts concat
teardown_session

session tick.html || exit 1
rec drift_tick_cfr 30 --codec vp9 --ts cfr --fps 10
teardown_session

# --- Assemble core.json: runs from the merged metrics (failed runs kept), then the derived block.
node -e '
const fs = require("fs");
const [runsDir, out, os] = process.argv.slice(1);
const names = ["fps_default","fps_nth2","slow_consumer","acks_60s","drift_wallclock","drift_concat","drift_cfr","drift_wallclock_notail","drift_tick_wallclock","drift_tick_concat","drift_tick_cfr"];
// A run whose merged file is missing or unparseable is recorded as a failed entry, never dropped.
const load = (n) => {
  try { return JSON.parse(fs.readFileSync(`${runsDir}/${n}.merged.json`, "utf8")); }
  catch { return { name: n, error: "no merged metrics" }; }
};
const runs = names.map(load);
// A run with fewer than 2 decoded frames has no rate: its duration is only the tail hold, so a
// bytes-per-minute figure from it is meaningless. Null it and say why, do not report it.
for (const r of runs) {
  const nb = r.probe?.nb_read_frames;
  if (typeof nb === "number" && nb < 2) {
    r.bytes_per_min = null;
    r.bytes_per_min_not_measured_because = `only ${nb} decoded frame(s): duration is the tail hold, no rate`;
  }
}
const chrome = runs.find((r) => r.chrome)?.chrome ?? null;
const result = {
  scenario: "core",
  host: { os, chrome, ffmpeg: "n9.0.2", transport: "ws", headless: process.env.BB_HEADLESS === "1", fixtures: ["anim.html", "idle.html", "tick.html"] },
  runs,
};
fs.writeFileSync(out, JSON.stringify(result, null, 2) + "\n");
' "$RUNS_DIR" "$OUT" "macOS $(sw_vers -productVersion 2>/dev/null) $(uname -m)"

node "$RS/scenarios/core_derive.mjs" "$OUT" >/dev/null && echo "derived: $OUT"
echo "done: $OUT"
