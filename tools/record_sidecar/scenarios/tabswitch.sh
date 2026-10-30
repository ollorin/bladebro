#!/usr/bin/env bash
# SC1: a >= 30 s WebM recorded by the sidecar across `state open-tab` + `state switch-tab`,
# against a bladebro-driven Chrome (WS transport, scratch BLADE_HOME, local fixtures only).
# Runs --follow poll (primary) and --follow newest (second pass, to attribute the signals).
# Headless by default (user preference); BB_HEADED=1 runs headed (plan's original wording).
set -u
source "$(dirname "$0")/../env.sh"
trap bb_cleanup EXIT
fixture_up || exit 1
if [ "${BB_HEADED:-0}" = "1" ]; then unset BLADE_CHROME_FLAGS; else export BLADE_CHROME_FLAGS="--headless=new"; fi

one_run() {   # $1 = follow mode, $2 = stats/out tag
  local mode="$1" tag="$2"
  # Fresh BLADE_HOME per run: a shared one let the previous run's browser leak into the next
  # run's /json/list (the newest pass showed a second b.html). Torn down at the end of each run.
  export BLADE_HOME="$(mktemp -d -t bbrec)"
  $BB nav "$FIX/a.html" >/dev/null 2>&1 || { echo "nav failed" >&2; return 1; }
  local P; P=$(bb_port) || { echo "no port" >&2; return 1; }
  node "$RS/sidecar.mjs" --port "$P" --follow "$mode" --duration 32 --codec vp9 --ts wallclock \
    --out "$SCRATCH/$tag.webm" --stats "$SCRATCH/$tag.stats.json" 2>"$SCRATCH/$tag.err" &
  local SC=$!
  sleep 10
  $BB state open-tab "$FIX/b.html" >/dev/null 2>&1
  sleep 4
  # /json/list order while B is current: recorded to check whether list order is MRU.
  curl -s "http://127.0.0.1:$P/json/list" > "$SCRATCH/$tag.list_after_open.json"
  sleep 6
  local A_ID
  A_ID=$($BB state tabs | grep 'a\.html' | grep -o '[0-9A-F]\{32\}' | head -1)
  $BB state switch-tab "$A_ID" >/dev/null 2>&1
  wait $SC
  local rc=$?
  node "$RS/measure.mjs" merge "tabswitch_$mode" "$SCRATCH/$tag.stats.json" "$SCRATCH/$tag.webm" > "$SCRATCH/$tag.merged.json" \
    || echo "merge failed for $mode" >&2
  echo "run $mode: sidecar exit=$rc a_id=$A_ID" >&2
  teardown_home
  return 0
}

teardown_home() {
  $BB stop >/dev/null 2>&1
  pkill -f "$BLADE_HOME" 2>/dev/null
  sleep 1
  rm -rf "$BLADE_HOME"
}

one_run poll poll
one_run newest newest

node -e '
const fs = require("fs");
const [dir, out] = process.argv.slice(1);
const load = (f) => JSON.parse(fs.readFileSync(f, "utf8"));
const runs = {};
for (const m of ["poll", "newest"]) {
  const r = load(`${dir}/${m}.merged.json`);
  const list = load(`${dir}/${m}.list_after_open.json`).map((t) => ({ id: t.id, type: t.type, url: t.url }));
  runs[m] = { ...r, json_list_while_b_current: list };
}
const urlsOf = (r) => (r.segments || []).map((s) => (s.url || "").split("/").pop());
const finding = (m) => {
  const u = urlsOf(runs[m]);
  const openTracked = u.includes("b.html");
  const switchTracked = u.length >= 3 && u[u.length - 1] === "a.html" && u.indexOf("b.html") < u.length - 1;
  return `${m}: open-tab tracked=${openTracked}, switch-tab tracked=${switchTracked}, segment urls=${u.join(">")}`;
};
const result = {
  scenario: "tabswitch",
  host: { os: "macOS arm64", ffmpeg: "n9.0.2", transport: "ws", headless: process.env.BB_HEADED !== "1" },
  poll: runs.poll,
  newest: runs.newest,
  follow_finding: [finding("poll"), finding("newest")].join(" | "),
};
fs.writeFileSync(out, JSON.stringify(result, null, 2) + "\n");
console.log(result.follow_finding);
' "$SCRATCH" "$RS/results/tabswitch.json"

# Keep the sample only if it is small enough for the repo (< 2 MB).
if [ -f "$SCRATCH/poll.webm" ] && [ "$(wc -c < "$SCRATCH/poll.webm")" -lt 2000000 ]; then
  cp "$SCRATCH/poll.webm" "$RS/results/tabswitch_sample.webm"
fi
echo "done: $RS/results/tabswitch.json"
