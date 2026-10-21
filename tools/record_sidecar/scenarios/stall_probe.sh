#!/usr/bin/env bash
# Diagnoses the screencast silence seen in the core acks_60s run (frames for 23 s, then none for
# 37 s, with zero ack errors). Two 60 s headed sessions on anim.html. During each recording the
# page's own frame counter (anim.html's top-level `frames`) and visibility are sampled every 5 s
# through bladebro. If the counter keeps climbing while the screencast is silent, the page was
# still painting and the silence is on the capture side; if the counter stops too, the page
# stopped painting (throttling, occlusion or display state).
set -u
source "$(dirname "$0")/../env.sh"
trap bb_cleanup EXIT
fixture_up || exit 1
SEC="$SCRATCH/probe"; mkdir -p "$SEC"
OUT="$RS/results/stall_probe.json"

now() { python3 -c 'import time;print(f"{time.time():.3f}")'; }

for i in 1 2; do
  export BLADE_HOME="$(mktemp -d -t bbrec)"
  unset BLADE_CHROME_FLAGS   # headed, same as core
  $BB nav "$FIX/anim.html" >/dev/null 2>&1 || { echo "nav failed" >&2; exit 1; }
  P=$(bb_port) || { echo "no port" >&2; exit 1; }
  sleep 2
  st="$SEC/run$i.stats.json"; vid="$SEC/run$i.webm"; samples="$SEC/run$i.samples.jsonl"; : > "$samples"
  node "$RS/sidecar.mjs" --port "$P" --duration 60 --codec vp9 --ts wallclock --out "$vid" --stats "$st" 2>"$SEC/run$i.err" &
  SC=$!
  t0=$(now)
  for k in $(seq 1 12); do
    sleep 5
    v=$($BB act eval "JSON.stringify({frames: typeof frames === 'number' ? frames : null, vis: document.visibilityState, focus: document.hasFocus()})" 2>&1 | head -1)
    python3 -c 'import json,sys; open(sys.argv[1],"a").write(json.dumps({"t_s": round(float(sys.argv[2])-float(sys.argv[3]),3), "raw": sys.argv[4][:300]})+"\n")' \
      "$samples" "$(now)" "$t0" "$v"
  done
  wait $SC
  rc=$?
  node "$RS/measure.mjs" merge "stall_probe_run$i" "$st" "$vid" > "$SEC/run$i.merged.json" 2>"$SEC/run$i.merge.err" || echo "merge run$i failed" >&2
  echo "run $i: sidecar rc=$rc" >&2
  rm -f "$vid"
  $BB stop >/dev/null 2>&1; pkill -f "$BLADE_HOME" 2>/dev/null; sleep 1; rm -rf "$BLADE_HOME"
done

node --input-type=module -e '
import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
const [sec, out, measureUrl] = process.argv.slice(1);
const { stallSeconds } = await import(measureUrl);
const runs = [1, 2].map((i) => {
  let m; try { m = JSON.parse(readFileSync(`${sec}/run${i}.merged.json`, "utf8")); } catch { return { run: i, error: "no merged metrics" }; }
  const samples = readFileSync(`${sec}/run${i}.samples.jsonl`, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
  const stall = stallSeconds(m.fps_buckets, m.wall_ms);
  // bladebro act eval prints `result: "<json string>"`: strip the prefix, then decode the string
  // literal, then the JSON object inside it. Anything else is kept raw, never guessed at.
  const decode = (raw) => {
    const m = /^result:\s*(.*)$/.exec(raw);
    let v = m ? m[1] : raw;
    try { v = JSON.parse(v); if (typeof v === "string") v = JSON.parse(v); } catch { return null; }
    return v && typeof v === "object" ? v : null;
  };
  const parsed = samples.map((s) => { const v = decode(s.raw); return { t_s: s.t_s, ...(v ?? { raw: s.raw }) }; });
  const counters = parsed.map((p) => p.frames).filter((x) => typeof x === "number");
  const lastFrameS = m.fps_buckets ? m.fps_buckets.length : 0;
  // Counter growth after the screencast went silent: the page kept painting if it still rises.
  const afterSilence = parsed.filter((p) => p.t_s >= lastFrameS && typeof p.frames === "number");
  const growthAfter = afterSilence.length >= 2 ? afterSilence[afterSilence.length - 1].frames - afterSilence[0].frames : null;
  return {
    run: i, name: m.name, wall_ms: m.wall_ms, frames_received: m.frames_received, acks_sent: m.acks_sent, acks_err: m.acks_err,
    fps_buckets: m.fps_buckets, stall: stall, last_frame_bucket_s: lastFrameS - 1,
    page_counter_samples: parsed, page_counter_growth_after_last_frame: growthAfter,
    reading: stall.stall_seconds === 0 ? "no screencast silence in this run (frames through the end of the run)"
      : growthAfter === null ? "not measured: too few counter samples after the silence"
      : growthAfter > 0 ? "page kept painting while the screencast was silent (capture-side silence)"
      : "page counter also stopped (page-side: no painting)",
  };
});
writeFileSync(out, JSON.stringify({ scenario: "stall_probe", purpose: "diagnose screencast silence seen in core acks_60s", runs }, null, 2) + "\n");
console.log(JSON.stringify(runs.map((r) => ({ run: r.run, frames: r.frames_received, stall: r.stall, growth: r.page_counter_growth_after_last_frame, reading: r.reading }))));
' "$SEC" "$OUT" "file://$RS/measure.mjs"
echo "done: $OUT"
