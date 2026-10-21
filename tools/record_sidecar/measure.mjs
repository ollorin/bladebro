#!/usr/bin/env node
// Measurement helpers for the Phase 2 gate. Harness-only: NOT shipped and not used by src/.
//   node measure.mjs probe <video>                       -> playability JSON for one file
//   node measure.mjs merge <name> <stats.json> <video>   -> one metrics object per run
//   node measure.mjs encode-bench <framesDir> <codec> <fps> <WxH> <out>
// Every number printed here is computed from a run's stats/ffprobe/ffmpeg output, never typed.
import { readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { buildFfmpegArgs, parseBench } from './lib/encoder.mjs';

const FFPROBE = process.env.BLADE_FFPROBE ?? 'ffprobe';
const FFMPEG = process.env.BLADE_FFMPEG ?? 'ffmpeg';

// Wall-clock recording length vs the encoded duration, in percent. Negative = video shorter.
export function driftPct(durationS, wallMs) {
  if (!(wallMs > 0)) throw new Error(`driftPct: wallMs must be > 0, got ${wallMs}`);
  return +(((durationS * 1000 - wallMs) / wallMs) * 100).toFixed(3);
}

// Parses ffprobe -of json output (format + first video stream). Numbers, not strings.
export function parseProbe(json) {
  const j = typeof json === 'string' ? JSON.parse(json) : json;
  const s = (j.streams ?? [])[0] ?? {};
  const f = j.format ?? {};
  return {
    codec: s.codec_name ?? null,
    width: s.width ?? null,
    height: s.height ?? null,
    duration_s: f.duration === undefined ? null : Number(f.duration),
    nb_read_frames: s.nb_read_frames === undefined ? null : Number(s.nb_read_frames),
    size_bytes: f.size === undefined ? null : Number(f.size),
  };
}

export function bytesPerMinute(sizeBytes, durationS) {
  if (!(durationS > 0)) return null;
  return +((sizeBytes / durationS) * 60).toFixed(1);
}

export function summarizeSegments(segments) {
  const frames = (segments ?? []).map((s) => s.frames);
  return {
    count: frames.length,
    frames_per_segment: frames,
    all_nonzero: frames.length > 0 && frames.every((f) => f > 0),
  };
}

// Count of non-empty lines in ffmpeg's error-level stderr. Kept separate from the 500-char
// excerpt so a long warning storm is counted in full, not truncated.
export function countDecodeErrorLines(stderr) {
  return String(stderr ?? '').split('\n').filter((l) => l.trim() !== '').length;
}

export function probeFile(video) {
  const r = spawnSync(FFPROBE, [
    '-v', 'error', '-count_frames', '-select_streams', 'v:0',
    '-show_entries', 'format=duration,size:stream=codec_name,width,height,nb_read_frames',
    '-of', 'json', video,
  ], { encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`ffprobe failed (${r.status}): ${String(r.stderr).slice(0, 500)}`);
  const parsed = parseProbe(r.stdout);
  const dec = spawnSync(FFMPEG, ['-v', 'error', '-i', video, '-f', 'null', '-'], { encoding: 'utf8' });
  const stderr = `${dec.stderr ?? ''}`;
  const decodeErrors = stderr.trim().slice(0, 500);
  const lines = countDecodeErrorLines(stderr);
  return {
    ...parsed,
    decode_exit: dec.status,
    decode_error_lines: lines,
    decode_errors: decodeErrors,
    // Strict: no error-level output at all (the plan's SC1 wording). decode_exit 0 with warnings
    // is reported separately as decodes_clean_exit so the two are never conflated.
    playable: decodeErrors === '' && (parsed.nb_read_frames ?? 0) > 0,
    decodes_clean_exit: dec.status === 0 && (parsed.nb_read_frames ?? 0) > 0,
  };
}

function cmdProbe(video) {
  process.stdout.write(JSON.stringify(probeFile(resolve(video)), null, 2) + '\n');
}

function cmdMerge(name, statsPath, video) {
  const st = JSON.parse(readFileSync(statsPath, 'utf8'));
  const probe = probeFile(resolve(video));
  const out = {
    name,
    chrome: st.chrome,
    follow: st.follow,
    stop_reason: st.stop_reason,
    wall_ms: st.wall_ms,
    frames_received: st.frames_received,
    stale_frames: st.stale_frames,
    acks_sent: st.acks_sent,
    acks_ok: st.acks_ok,
    acks_err: st.acks_err,
    frames_written: st.frames_written,
    frames_dropped: st.frames_dropped,
    dup_frames_written: st.dup_frames_written,
    queue_max: st.queue_max,
    ack_send_us: st.ack_send_us,
    ack_rtt_ms: st.ack_rtt_ms,
    capture_to_recv_ms: st.capture_to_recv_ms,
    effective_fps_recv: st.effective_fps_recv,
    first_frame_ms: st.first_frame_ms,
    max_gap_ms: st.max_gap_ms,
    fps_buckets: st.fps_buckets,
    frame_sizes: st.frame_sizes,
    segments: st.segments,
    segment_summary: summarizeSegments(st.segments),
    events: st.events,
    encoder: st.encoder ? { finalized: st.encoder.finalized, bench: st.encoder.bench, exit_code: st.encoder.exit_code } : null,
    probe,
    drift_pct: probe.duration_s && st.wall_ms ? driftPct(probe.duration_s, st.wall_ms) : null,
    bytes_per_min: bytesPerMinute(probe.size_bytes ?? statSync(resolve(video)).size, probe.duration_s),
  };
  if (st.encoder && st.encoder.finalized === false) {
    out.encoder.stderr_tail = String(st.encoder.stderr_tail ?? '').slice(-600);
  }
  process.stdout.write(JSON.stringify(out, null, 2) + '\n');
}

// Re-encodes the frames captured by a --ts concat --keep-frames run at a fixed fps and size.
// The fps filter is inserted in front of the scale chain so the rate change happens first.
function cmdEncodeBench(framesDir, codec, fpsStr, size, outPath) {
  const [w, h] = size.split('x').map(Number);
  if (!w || !h) throw new Error(`encode-bench: size must be WxH, got ${size}`);
  const fps = Number(fpsStr);
  const argv = buildFfmpegArgs({
    codec, ts: 'concat', out: resolve(outPath), width: w, height: h, sizeMode: 'fixed', fps,
    concatList: resolve(framesDir, 'list.ffconcat'),
  });
  const vfAt = argv.indexOf('-vf') + 1;
  argv[vfAt] = `fps=${fps},${argv[vfAt]}`;
  const r = spawnSync(FFMPEG, argv, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  if (r.status !== 0) throw new Error(`encode-bench ffmpeg exit ${r.status}: ${String(r.stderr).slice(-600)}`);
  const bench = parseBench(r.stderr);
  const probe = probeFile(resolve(outPath));
  const outBytes = statSync(resolve(outPath)).size;
  const cpu = bench ? bench.utime_s + bench.stime_s : null;
  const dur = probe.duration_s;
  process.stdout.write(JSON.stringify({
    codec, fps, size, utime_s: bench?.utime_s ?? null, stime_s: bench?.stime_s ?? null,
    rtime_s: bench?.rtime_s ?? null, out_bytes: outBytes, duration_s: dur,
    cpu_s_per_min: cpu !== null && dur > 0 ? +((cpu / dur) * 60).toFixed(3) : null,
    bytes_per_min: bytesPerMinute(outBytes, dur),
    playable: probe.playable,
  }) + '\n');
}

function main(argv) {
  const [cmd, ...rest] = argv;
  if (cmd === 'probe' && rest.length === 1) return cmdProbe(rest[0]);
  if (cmd === 'merge' && rest.length === 3) return cmdMerge(rest[0], rest[1], rest[2]);
  if (cmd === 'encode-bench' && rest.length === 5) return cmdEncodeBench(...rest);
  process.stderr.write('usage: measure.mjs probe <video> | merge <name> <stats.json> <video> | encode-bench <framesDir> <codec> <fps> <WxH> <out>\n');
  process.exit(2);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { main(process.argv.slice(2)); } catch (e) {
    process.stderr.write(`measure: ${e.message}\n`);
    process.exit(1);
  }
}
