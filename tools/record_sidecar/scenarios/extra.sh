#!/usr/bin/env bash
# SC2 extra + SC3 (coexistence half): viewport change, headed vs headless cadence, background
# and minimized windows, codec CPU/size matrix, and whether bladebro's own commands keep working
# while the sidecar records. Raw artifacts go to $SCRATCH/sec; the assembler at the end builds
# results/extra.json from them, so no number is typed by hand.
#
# Order matters: every browser session runs first, the offline codec encodes run last with
# nothing else active, so CPU figures are not inflated by a running Chrome or recorder.
set -u
source "$(dirname "$0")/../env.sh"
trap bb_cleanup EXIT
fixture_up || exit 1
SEC="$SCRATCH/sec"; mkdir -p "$SEC/runs" "$SEC/meta" "$SEC/seq" "$SEC/bench"
OUT="$RS/results/extra.json"

now() { python3 -c 'import time;print(f"{time.time():.3f}")'; }

newhome() { export BLADE_HOME="$(mktemp -d -t bbrec)"; }
headed() { unset BLADE_CHROME_FLAGS; }
headless() { export BLADE_CHROME_FLAGS="--headless=new"; }
teardown() {
  $BB stop >/dev/null 2>&1
  pkill -f "$BLADE_HOME" 2>/dev/null
  sleep 1
  rm -rf "$BLADE_HOME"
}
# Starts a fresh browser on $1 and sets P (endpoint port).
open_page() {
  $BB nav "$FIX/$1" >/dev/null 2>&1 || { echo "nav $1 failed" >&2; return 1; }
  P=$(bb_port) || { echo "no port" >&2; return 1; }
  sleep 2
}

# Turns the finished run into runs/<name>.merged.json, or a failed entry that keeps exit code,
# encoder tail and error text. Nothing is dropped.
finish_run() {   # name rc stats vid
  local name="$1" rc="$2" st="$3" vid="$4"
  if [ -f "$st" ] && node "$RS/measure.mjs" merge "$name" "$st" "$vid" > "$SEC/runs/$name.merged.json" 2>"$SEC/runs/$name.merge.err"; then
    echo "$name: rc=$rc ok" >&2
  else
    node -e '
      const fs = require("fs");
      const [name, rc, st, err, outp] = process.argv.slice(1);
      let stats = null; try { stats = JSON.parse(fs.readFileSync(st, "utf8")); } catch {}
      let errText = ""; try { errText = fs.readFileSync(err, "utf8"); } catch {}
      fs.writeFileSync(outp, JSON.stringify({ name, exit_code: Number(rc), error: (errText.trim() || "stats missing").slice(-600), encoder: { stderr_tail: stats?.encoder?.stderr_tail ?? null } }, null, 2));
    ' "$name" "$rc" "$st" "$SEC/runs/$name.err" "$SEC/runs/$name.merged.json"
    echo "$name: rc=$rc FAILED (kept as failed entry)" >&2
  fi
  rm -f "$vid"
}

rec() {   # name duration flags...
  local name="$1" dur="$2"; shift 2
  local st="$SEC/runs/$name.stats.json" vid="$SEC/runs/$name.webm"
  node "$RS/sidecar.mjs" --port "$P" --duration "$dur" "$@" --out "$vid" --stats "$st" 2>"$SEC/runs/$name.err"
  finish_run "$name" $? "$st" "$vid"
}

# Background-window run: the sidecar records --target <anim page>; after $sw seconds the tab is
# switched away with open-tab, so the recorded page is in the background.
rec_bg() {   # name duration switch_after flags...
  local name="$1" dur="$2" sw="$3"; shift 3
  local anim_id
  anim_id=$(curl -s "http://127.0.0.1:$P/json/list" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const l=JSON.parse(s).filter(t=>t.type==="page"&&!t.url.startsWith("devtools://"));process.stdout.write(l[0].id)})')
  local st="$SEC/runs/$name.stats.json" vid="$SEC/runs/$name.webm"
  local t_start; t_start=$(now); echo "$t_start" > "$SEC/meta/$name.start"
  node "$RS/sidecar.mjs" --port "$P" --target "$anim_id" --follow off --duration "$dur" "$@" --out "$vid" --stats "$st" 2>"$SEC/runs/$name.err" &
  local SC=$!
  sleep "$sw"
  local t_open; t_open=$(now)
  $BB state open-tab "$FIX/b.html" >/dev/null 2>&1
  python3 -c "print(round($t_open-$t_start,3))" > "$SEC/meta/$name.open_offset"
  wait $SC
  finish_run "$name" $? "$st" "$vid"
}

# One bladebro command, timed. Appends one JSON line to the sequence file.
step() {   # seqfile step_name cmd...
  local f="$1" name="$2"; shift 2
  local t0 t1 o rc
  t0=$(now)
  o=$("$@" 2>&1); rc=$?
  t1=$(now)
  python3 -c '
import json, os, re, sys
seqf, name, rc, t0, t1, out = sys.argv[1:7]
first = out.strip().split("\n")[0] if out.strip() else ""
rec = {"step": name, "rc": int(rc), "start": float(t0), "elapsed_s": round(float(t1) - float(t0), 3), "first_line": first[:200], "output": out[:4000]}
# An artifact path is checked HERE, while the session home still exists (teardown removes it).
m = re.search(r"(/[^\s\"\x27]+\.(?:png|jpe?g|webp))", out)
if m:
    rec["artifact_path"] = m.group(1)
    rec["artifact_exists_at_step"] = os.path.isfile(m.group(1))
open(seqf, "a").write(json.dumps(rec) + "\n")
' "$f" "$name" "$rc" "$t0" "$t1" "$o"
}

# The coexistence sequence. Same commands in both runs; the only difference is the sidecar.
bb_seq() {   # label
  local f="$SEC/seq/$1.jsonl"; : > "$f"
  step "$f" nav_a $BB nav "$FIX/a.html"
  step "$f" see_before $BB see content
  step "$f" click $BB act click Toggle
  step "$f" eval $BB act eval "document.getElementById('s').textContent"
  step "$f" see_after $BB see content
  step "$f" tabs $BB state tabs
  step "$f" vision $BB vision
  step "$f" nav_anim $BB nav "$FIX/anim.html"
}

# ---------------------------------------------------------------- 1. viewport (headed)
newhome; headed
open_page anim.html || exit 1
rec vp_fixed 35 --codec vp9 --ts wallclock --size-mode fixed \
  --script "5:emulate=1001x641;12:emulate-clear;18:window=1101x701;26:window=1280x800"
rec vp_even 20 --codec vp9 --ts wallclock --size-mode even \
  --script "5:emulate=1001x641;12:emulate-clear"
teardown

# ---------------------------------------------------------------- 2. headless (cadence, drift, background)
newhome; headless
open_page anim.html || exit 1
$BB state tabs >/dev/null 2>&1
ps -ax -o command= | grep -- "--user-data-dir=$BLADE_HOME" | grep -v Helper | grep -v grep | grep -c -- "--headless=new" > "$SEC/meta/headless_procs.txt"
rec fps_headless 20 --codec vp9 --ts wallclock
open_page idle.html && rec idle_headless_drift 20 --codec vp9 --ts wallclock
open_page tick.html && rec tick_headless_drift 20 --codec vp9 --ts wallclock
open_page anim.html && rec_bg bg_headless 25 8 --codec vp9 --ts wallclock
teardown

# ---------------------------------------------------------------- 3. headed background + minimized
newhome; headed
open_page anim.html || exit 1
rec_bg bg_headed 25 8 --codec vp9 --ts wallclock
open_page anim.html || exit 1
echo "$(now)" > "$SEC/meta/minimized.start"
rec minimized 30 --codec vp9 --ts wallclock --script "8:minimize;18:restore"
teardown

# ---------------------------------------------------------------- 4. codec spool (headed, 60 s, kept frames)
newhome; headed
open_page anim.html || exit 1
mkdir -p "$SCRATCH/spool"
node "$RS/sidecar.mjs" --port "$P" --duration 60 --codec vp9 --ts concat --keep-frames \
  --out "$SCRATCH/spool/spool.webm" --stats "$SEC/runs/spool.stats.json" 2>"$SEC/runs/spool.err"
echo "spool: rc=$? frames=$(ls "$SCRATCH/spool/spool.webm.frames" 2>/dev/null | wc -l | tr -d ' ')" >&2
teardown

# ---------------------------------------------------------------- 5. live codec runs (headed, 30 s each)
newhome; headed
open_page anim.html || exit 1
rec live_vp9 30 --codec vp9 --ts wallclock
node "$RS/sidecar.mjs" --port "$P" --duration 30 --codec vp8 --ts wallclock --out "$SEC/runs/live_vp8.webm" --stats "$SEC/runs/live_vp8.stats.json" 2>"$SEC/runs/live_vp8.err"
finish_run live_vp8 $? "$SEC/runs/live_vp8.stats.json" "$SEC/runs/live_vp8.webm"
node "$RS/sidecar.mjs" --port "$P" --duration 30 --codec x264 --ts wallclock --out "$SEC/runs/live_x264.mp4" --stats "$SEC/runs/live_x264.stats.json" 2>"$SEC/runs/live_x264.err"
finish_run live_x264 $? "$SEC/runs/live_x264.stats.json" "$SEC/runs/live_x264.mp4"
teardown

# ---------------------------------------------------------------- 6. coexistence (headed)
# Baseline first (no sidecar), then the same sequence with a sidecar recording the page.
newhome; headed
open_page a.html || exit 1
bb_seq baseline
open_page a.html || exit 1
echo "$(now)" > "$SEC/meta/seq_sidecar.start"
node "$RS/sidecar.mjs" --port "$P" --follow poll --duration 60 --codec vp9 --ts wallclock \
  --out "$SEC/runs/coex_sidecar.webm" --stats "$SEC/runs/coex_sidecar.stats.json" 2>"$SEC/runs/coex_sidecar.err" &
SC=$!
sleep 1
bb_seq sidecar
wait $SC
finish_run coex_sidecar $? "$SEC/runs/coex_sidecar.stats.json" "$SEC/runs/coex_sidecar.webm"
teardown

# ---------------------------------------------------------------- 7. codec encodes (offline, nothing else running)
# Same captured frame set for every encode: the 60 s spool from section 4.
if [ -f "$SCRATCH/spool/spool.webm.frames/list.ffconcat" ]; then
  for spec in "vp9 5 1280x720" "vp9 10 1280x720" "vp9 15 1280x720" \
              "vp8 5 1280x720" "vp8 10 1280x720" "vp8 15 1280x720" \
              "x264 5 1280x720" "x264 10 1280x720" "x264 15 1280x720" \
              "vp9 10 960x540" "vp9 10 1920x1080"; do
    set -- $spec
    codec=$1; fps=$2; size=$3
    ext=webm; [ "$codec" = "x264" ] && ext=mp4
    out="$SCRATCH/spool/bench_${codec}_${fps}_${size}.$ext"
    node "$RS/measure.mjs" encode-bench "$SCRATCH/spool/spool.webm.frames" "$codec" "$fps" "$size" "$out" \
      > "$SEC/bench/${codec}_${fps}_${size}.json" 2>"$SEC/bench/${codec}_${fps}_${size}.err"
    echo "bench $codec $fps $size rc=$?" >&2
    rm -f "$out"
  done
  rm -rf "$SCRATCH/spool"
else
  echo "no spool frames: codec encodes not measured" >&2
fi

# ---------------------------------------------------------------- assemble results/extra.json
node -e '
const fs = require("fs");
const path = require("path");
const [sec, out, coreFile] = process.argv.slice(1);
const readJson = (f) => { try { return JSON.parse(fs.readFileSync(f, "utf8")); } catch { return null; } };
const readText = (f) => { try { return fs.readFileSync(f, "utf8").trim(); } catch { return null; } };
const run = (n) => readJson(path.join(sec, "runs", n + ".merged.json"));
const brief = (r) => {
  if (!r) return { not_measured_because: "run did not produce metrics" };
  if (r.exit_code !== undefined && !r.frames_received) return { failed: true, exit_code: r.exit_code, error: r.error, encoder_stderr_tail: r.encoder?.stderr_tail ?? null };
  return {
    stop_reason: r.stop_reason, wall_ms: r.wall_ms, frames_received: r.frames_received, stale_frames: r.stale_frames,
    acks_sent: r.acks_sent, acks_ok: r.acks_ok, acks_err: r.acks_err, frames_dropped: r.frames_dropped,
    effective_fps_recv: r.effective_fps_recv, max_gap_ms: r.max_gap_ms, frame_sizes: r.frame_sizes,
    fps_buckets: r.fps_buckets, events: r.events,
    probe: { width: r.probe?.width, height: r.probe?.height, duration_s: r.probe?.duration_s, nb_read_frames: r.probe?.nb_read_frames, playable: r.probe?.playable, decode_error_lines: r.probe?.decode_error_lines },
    encoder: { finalized: r.encoder?.finalized ?? null, bench: r.encoder?.bench ?? null },
    drift_pct: r.drift_pct, bytes_per_min: r.bytes_per_min ?? null,
  };
};
const sumRange = (b, lo, hi) => { let s = 0; for (let i = Math.max(0, lo); i < Math.min(hi, (b || []).length); i++) s += b[i] || 0; return s; };
const endOf = (r) => (r && r.fps_buckets ? r.fps_buckets.length : 0);
const meta = (n) => readText(path.join(sec, "meta", n));
const core = readJson(coreFile);
const out_ = {};

out_.viewport = {
  fixed: { ...brief(run("vp_fixed")), note: "35 s; emulate 1001x641 at 5 s, clear at 12 s, window 1101x701 at 18 s, 1280x800 at 26 s" },
  even: { ...brief(run("vp_even")), note: "20 s; emulate 1001x641 at 5 s, clear at 12 s; even size mode" },
};

const hl = run("fps_headless"), idleHl = run("idle_headless_drift"), tickHl = run("tick_headless_drift");
out_.headless = {
  headed_reference: { source: "core.json runs.fps_default (same fixture, same 20 s)", effective_fps_recv: core?.runs?.find((r) => r.name === "fps_default")?.effective_fps_recv ?? null },
  chrome_headless_processes_seen: Number(meta("headless_procs.txt") ?? NaN) || null,
  fps_headless: brief(hl),
  idle_headless_drift: { ...brief(idleHl), note: "idle.html delivers ~1 frame per run: drift only shows the tail hold" },
  tick_headless_drift: brief(tickHl),
};

const bgSummary = (name) => {
  const r = run(name); const off = Number(meta(name + ".open_offset"));
  if (!r || !r.fps_buckets) return { ...brief(r), not_measured_because: "no buckets" };
  // Seconds come from the wall time of the run, not from the bucket array: a run whose frames stop
  // early has a short array but a full wall time, and the silent seconds are the finding.
  const b = r.fps_buckets; const wallSec = Math.floor(r.wall_ms / 1000);
  const cut = Math.floor(off);
  const before = sumRange(b, 0, cut); const after = sumRange(b, cut + 1, wallSec);
  const secAfter = Math.max(0, wallSec - cut - 1);
  return { ...brief(r), open_tab_offset_s: off, frames_before_switch: before, seconds_before_switch: cut, fps_before_switch: cut > 0 ? +(before / cut).toFixed(2) : null,
    frames_after_switch: after, seconds_after_switch: secAfter, fps_after_switch: secAfter > 0 ? +(after / secAfter).toFixed(2) : null };
};
out_.background = {
  headed: bgSummary("bg_headed"),
  headless: bgSummary("bg_headless"),
  note: "the recorded anim page is backgrounded by open-tab at ~8 s; window stays visible in headed mode",
};

const mr = run("minimized"); const mb = mr?.fps_buckets || [];
const inWin = (from, to) => { let s = 0, n = 0; for (let i = from; i < to; i++) { s += mb[i] || 0; n++; } return { frames: s, seconds: n, fps: n ? +(s / n).toFixed(2) : null }; };
out_.minimized = {
  ...brief(mr),
  window_8s_to_18s: inWin(8, 18),
  outside_window: { frames: (mr?.frames_received ?? 0) - inWin(8, 18).frames, seconds: Math.max(0, Math.floor((mr?.wall_ms ?? 0) / 1000) - 10) },
  note: "minimize at 8 s, restore at 18 s",
};

out_.xvfb = { not_measured_because: "macOS host has no Xvfb; Linux Xvfb cadence must be measured on a Linux host before Phase 6" };

// codecs
const benchFiles = fs.existsSync(path.join(sec, "bench")) ? fs.readdirSync(path.join(sec, "bench")).filter((f) => f.endsWith(".json")) : [];
const matrix = benchFiles.map((f) => {
  const j = readJson(path.join(sec, "bench", f));
  if (!j) return { file: f, not_measured_because: "encode-bench produced no JSON (see .err)", stderr_tail: readText(path.join(sec, "bench", f.replace(".json", ".err")))?.slice(-400) ?? null };
  return j;
}).sort((a, b) => (a.codec + a.size + a.fps).localeCompare(b.codec + b.size + b.fps));
const live = {};
for (const c of ["live_vp9", "live_vp8", "live_x264"]) live[c.replace("live_", "")] = brief(run(c));
out_.codecs = {
  method: "encode-bench re-encodes one 60 s concat spool (same captured frames) per row; cpu_s_per_min = (utime+stime)/duration*60; 1920x1080 is an upscale from the ~1185x720 capture",
  matrix,
  live_30s_wallclock: live,
  note_row_count: `${matrix.length} encode rows (plan text said 12; the listed matrix is 11)`,
};

const coexSteps = (label) => {
  const f = path.join(sec, "seq", label + ".jsonl");
  const t = readText(f); if (!t) return null;
  return t.split("\n").map((l) => JSON.parse(l));
};
const judge = (steps) => {
  if (!steps) return { not_measured_because: "sequence did not run" };
  const by = Object.fromEntries(steps.map((s) => [s.step, s]));
  const rcOk = steps.every((s) => s.rc === 0);
  const evalOk = !!by.eval && by.eval.output.includes("clicked");
  const seeOk = !!by.see_after && by.see_after.output.includes("clicked");
  // Existence was checked inside the step, before the session home was removed.
  const visionPath = by.vision?.artifact_path ?? null;
  const visionExists = by.vision ? (by.vision.artifact_exists_at_step ?? null) : null;
  return {
    steps: steps.map((s) => ({ step: s.step, rc: s.rc, elapsed_s: s.elapsed_s, first_line: s.first_line })),
    kept_working: rcOk && evalOk,
    events_flowing: {
      nav_completed: !!by.nav_a && by.nav_a.rc === 0 && !!by.nav_anim && by.nav_anim.rc === 0,
      click_verdict_line: by.click ? by.click.first_line : null,
      see_reflects_change: seeOk,
      eval_readback: by.eval ? by.eval.output.trim().slice(0, 120) : null,
    },
    vision_image_path: visionPath, vision_image_exists: visionExists,
  };
};
const base = judge(coexSteps("baseline"));
const withS = judge(coexSteps("sidecar"));
const cr = run("coex_sidecar");
let navOffset = null;
{
  const steps = coexSteps("sidecar") || [];
  const na = steps.find((s) => s.step === "nav_anim");
  const ss = Number(meta("seq_sidecar.start"));
  if (na && ss) navOffset = +(na.start - ss).toFixed(3);
}
const cb = cr?.fps_buckets || [];
const around = navOffset !== null ? Math.floor(navOffset) : null;
// Average over the seconds that actually exist inside the recording: a window that starts before
// the sidecar attached has fewer than 5 real seconds and must not be divided by 5.
const fpsWin = (lo, hi) => {
  const a = Math.max(0, lo), b = Math.min(hi, Math.floor((cr?.wall_ms ?? 0) / 1000));
  return b > a ? +(sumRange(cb, a, b) / (b - a)).toFixed(2) : null;
};
out_.coexistence = {
  baseline: base,
  with_sidecar: { ...withS, sidecar: brief(cr), nav_anim_offset_s: navOffset,
    seconds_available_before_same_tab_nav: around !== null ? Math.min(5, around) : null,
    fps_5s_before_same_tab_nav: around !== null ? fpsWin(around - 5, around) : null,
    fps_5s_after_same_tab_nav: around !== null ? fpsWin(around + 1, around + 6) : null,
    segments: cr?.segments ?? null },
  kept_working: base.kept_working === true && withS.kept_working === true,
  note: "kept_working requires every command rc 0 and the eval readback clicked, in both runs",
};

const result = { scenario: "extra", host: { os: "macOS arm64", ffmpeg: "n9.0.2", transport: "ws" }, ...out_ };
fs.writeFileSync(out, JSON.stringify(result, null, 2) + "\n");

// Ten-line digest, computed from the same data.
const cheapest10 = matrix.filter((r) => r.fps === 10 && r.size === "1280x720" && r.cpu_s_per_min != null).sort((a, b) => a.cpu_s_per_min - b.cpu_s_per_min)[0];
const lines = [
  `headless fps_headless=${out_.headless.fps_headless.effective_fps_recv} vs headed=${out_.headless.headed_reference.effective_fps_recv}`,
  `background headed: fps before=${out_.background.headed.fps_before_switch} after=${out_.background.headed.fps_after_switch}`,
  `background headless: fps before=${out_.background.headless.fps_before_switch} after=${out_.background.headless.fps_after_switch}`,
  `minimized: fps in 8-18s=${out_.minimized.window_8s_to_18s.fps} frames=${out_.minimized.window_8s_to_18s.frames}`,
  `viewport fixed: frame_sizes=${JSON.stringify(out_.viewport.fixed.frame_sizes ?? null)} even: ${out_.viewport.even.failed ? "FAILED" : JSON.stringify(out_.viewport.even.frame_sizes ?? null)}`,
  `cheapest codec at 10 fps 1280x720: ${cheapest10 ? `${cheapest10.codec} cpu_s_per_min=${cheapest10.cpu_s_per_min}` : "not measured"}`,
  `bytes_per_min (vp9 10fps 1280x720): ${(matrix.find((r) => r.codec === "vp9" && r.fps === 10 && r.size === "1280x720") || {}).bytes_per_min ?? "not measured"}`,
  `live 30 s wallclock frames_dropped vp9=${live.vp9?.frames_dropped ?? "n/a"} vp8=${live.vp8?.frames_dropped ?? "n/a"} x264=${live.x264?.frames_dropped ?? "n/a"}`,
  `coexistence kept_working=${out_.coexistence.kept_working} (baseline=${base.kept_working}, with_sidecar=${withS.kept_working})`,
  `coexistence events_flowing: nav_completed=${withS.events_flowing?.nav_completed} see_reflects_change=${withS.events_flowing?.see_reflects_change}`,
];
console.log(lines.join("\n"));
' "$SEC" "$OUT" "$RS/results/core.json" 2>"$SEC/assemble.err"
echo "done: $OUT"
