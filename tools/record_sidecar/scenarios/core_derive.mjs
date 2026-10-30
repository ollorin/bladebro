#!/usr/bin/env node
// Derives the `derived` block of results/core.json from its `runs` array. Reads the runs as
// recorded and writes the block back, so the numbers can be recomputed without repeating the
// browser sessions:  node scenarios/core_derive.mjs results/core.json
import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { stallSeconds } from '../measure.mjs';

const p95 = (x) => (x && typeof x.p95 === 'number' ? x.p95 : null);
const within = (v) => (v === null || v === undefined ? null : Math.abs(v) <= 5);

export function deriveCore(runs) {
  const by = Object.fromEntries(runs.map((r) => [r.name, r]));
  const acks = by.acks_60s;
  const slow = by.slow_consumer;
  const drift = (r, key) => (r && typeof r[key] === 'number' ? r[key] : null);
  const driftSet = (names) => Object.fromEntries(names.map(([k, n]) => [k, drift(by[n], 'drift_pct')]));
  const captureSet = (names) => Object.fromEntries(names.map(([k, n]) => [k, drift(by[n], 'drift_pct_capture')]));
  const tickNames = [['wallclock', 'drift_tick_wallclock'], ['concat', 'drift_tick_concat'], ['cfr', 'drift_tick_cfr']];
  const idleNames = [['wallclock', 'drift_wallclock'], ['concat', 'drift_concat'], ['cfr', 'drift_cfr']];
  const stall = acks ? stallSeconds(acks.fps_buckets, acks.wall_ms) : null;
  return {
    effective_fps_recv_fps_default: by.fps_default?.effective_fps_recv ?? null,
    effective_fps_recv_fps_nth2: by.fps_nth2?.effective_fps_recv ?? null,
    ack_send_us_p95_fps_default: p95(by.fps_default?.ack_send_us),
    ack_rtt_ms_p95_fps_default: p95(by.fps_default?.ack_rtt_ms),
    ack_send_us_p95_acks_60s: p95(acks?.ack_send_us),
    ack_rtt_ms_p95_acks_60s: p95(acks?.ack_rtt_ms),
    slow_consumer_drop_pct: slow && slow.frames_received ? +((slow.frames_dropped / slow.frames_received) * 100).toFixed(2) : null,
    slow_consumer_frames_received_eq_acks_sent: slow ? slow.frames_received === slow.acks_sent : null,
    frames_eq_acks_60s: acks ? acks.frames_received === acks.acks_sent && acks.acks_err === 0 : null,
    acks_ok_eq_acks_sent_60s: acks ? acks.acks_ok === acks.acks_sent : null,
    frames_received_60s: acks?.frames_received ?? null,
    // Stall = whole seconds with no frame, including trailing silence after the last frame.
    stall_seconds_60s: stall ? stall.stall_seconds : null,
    stall_interior_s_60s: stall ? stall.interior_stall_s : null,
    stall_trailing_s_60s: stall ? stall.trailing_silence_s : null,
    max_gap_ms_60s: acks?.max_gap_ms ?? null,
    // Drift vs wall_ms (plan reference; includes encoder finalize) and vs the stop request
    // (capture window only). The capture figure is the one that isolates timing.
    drift_pct_tick: driftSet(tickNames),
    drift_pct_tick_capture: captureSet(tickNames),
    within_5pct_tick: Object.fromEntries(tickNames.map(([k, n]) => [k, within(drift(by[n], 'drift_pct'))])),
    drift_pct_idle: driftSet(idleNames),
    drift_pct_idle_capture: captureSet(idleNames),
    within_5pct_idle: Object.fromEntries(idleNames.map(([k, n]) => [k, within(drift(by[n], 'drift_pct'))])),
    drift_wallclock_notail_pct: drift(by.drift_wallclock_notail, 'drift_pct'),
    notes: [
      'idle.html delivers about 1 frame per run: its drift shows only the tail hold or cfr padding, not capture timing. The tick.html group is the drift comparison.',
      'drift_wallclock_notail has 1 decoded frame: its duration is the tail hold only, so it is reported as a drift figure, not a rate.',
      'drift_pct uses wall_ms, which includes encoder finalize; drift_pct_capture uses stop_begin_ms and excludes it (only runs recorded after stop_begin_ms existed have it).',
    ],
  };
}

function main(argv) {
  const file = argv[0];
  if (!file) throw new Error('usage: core_derive.mjs results/core.json');
  const core = JSON.parse(readFileSync(file, 'utf8'));
  core.derived = deriveCore(core.runs);
  writeFileSync(file, JSON.stringify(core, null, 2) + '\n');
  process.stdout.write(JSON.stringify(core.derived) + '\n');
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) main(process.argv.slice(2));
