// ffmpeg side of the sidecar: argv construction (pure), a bounded frame queue, the concat
// list writer, benchmark-line parsing, JPEG size sniffing, and the ffmpeg child wrapper.
import { spawn } from 'node:child_process';

const CODECS = new Set(['vp9', 'vp8', 'x264']);
const TIMINGS = new Set(['wallclock', 'concat', 'cfr']);
const SIZE_MODES = new Set(['fixed', 'even']);

export function even(n) {
  return n - (n % 2);
}

// Pure argv builder; no shell is involved, so paths with spaces are safe.
// Timing: wallclock stamps frames at arrival (live, but jittery); concat replays exact
// capture timestamps via the concat demuxer; cfr resamples onto a fixed tick rate.
export function buildFfmpegArgs({ codec, ts, out, width, height, sizeMode, fps, concatList }) {
  if (!CODECS.has(codec)) throw new Error(`unknown codec "${codec}" (vp9|vp8|x264)`);
  if (!TIMINGS.has(ts)) throw new Error(`unknown ts "${ts}" (wallclock|concat|cfr)`);
  if (!SIZE_MODES.has(sizeMode)) throw new Error(`unknown size-mode "${sizeMode}" (fixed|even)`);
  const webm = out.endsWith('.webm');
  const mp4 = out.endsWith('.mp4');
  if ((codec === 'vp9' || codec === 'vp8') && !webm) {
    throw new Error(`codec ${codec} needs a .webm output, got ${out}`);
  }
  if (codec === 'x264' && !mp4) throw new Error(`codec x264 needs a .mp4 output, got ${out}`);

  const args = ['-hide_banner', '-nostats', '-loglevel', 'info', '-benchmark', '-y'];

  if (ts === 'wallclock') {
    // -framerate 1000 sets the input timebase to 1 ms. The image2pipe default is 25 tbn, which
    // quantizes wall-clock stamps to 40 ms and produces non-monotonic dts when frames arrive
    // faster (measured: 232 warnings per 800 frames without it, 3 with it; all frames kept).
    args.push('-f', 'image2pipe', '-c:v', 'mjpeg', '-use_wallclock_as_timestamps', '1', '-framerate', '1000', '-i', 'pipe:0');
  } else if (ts === 'cfr') {
    args.push('-f', 'image2pipe', '-framerate', String(fps), '-c:v', 'mjpeg', '-i', 'pipe:0');
  } else {
    if (!concatList) throw new Error('ts=concat requires concatList');
    args.push('-f', 'concat', '-safe', '0', '-i', concatList);
  }

  // Fixed mode keeps one output geometry for the whole recording (letterboxed), because
  // a mid-stream resolution change breaks many players. even mode only guarantees even dims.
  const ew = even(width);
  const eh = even(height);
  let vf;
  if (sizeMode === 'fixed') {
    vf = `scale=${ew}:${eh}:force_original_aspect_ratio=decrease:force_divisible_by=2,` +
      `pad=${ew}:${eh}:(ow-iw)/2:(oh-ih)/2,format=yuv420p`;
  } else {
    vf = 'scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p';
  }
  args.push('-vf', vf);

  // wallclock/concat must keep EVERY frame at its own timestamp. `-fps_mode vfr` drops
  // frames that land within one assumed 25 fps tick: measured 234/800 frames kept at ~98 fps
  // input (ffmpeg n9.0.2, synthetic MJPEG; the live recorder showed drop=592 of 794). passthrough
  // keeps all 800. Explicit -video_track_timescale did not help.
  if (ts === 'cfr') args.push('-r', String(fps));
  else args.push('-fps_mode', 'passthrough');

  if (codec === 'vp9') {
    args.push('-c:v', 'libvpx-vp9', '-deadline', 'realtime', '-cpu-used', '8', '-row-mt', '1', '-crf', '40', '-b:v', '0');
  } else if (codec === 'vp8') {
    args.push('-c:v', 'libvpx', '-deadline', 'realtime', '-cpu-used', '8', '-crf', '10', '-b:v', '2M');
  } else {
    args.push('-c:v', 'libx264', '-preset', 'veryfast', '-crf', '28', '-movflags', '+faststart');
  }

  args.push('-an', out);
  return args;
}

// Bounded FIFO. When full, the INCOMING frame is dropped (the newest), so the queue holds
// the oldest pending frames and the capture ack path never blocks on the writer.
export class FrameQueue {
  constructor(cap) {
    this.cap = cap;
    this.items = [];
    this.waiters = [];
    this.closed = false;
    this.dropped = 0;
    this.maxSeen = 0;
  }

  get length() {
    return this.items.length;
  }

  push(item) {
    if (this.closed) return false;
    if (this.waiters.length > 0) {
      this.waiters.shift()(item);
      return true;
    }
    if (this.items.length >= this.cap) {
      this.dropped++;
      return false;
    }
    this.items.push(item);
    this.maxSeen = Math.max(this.maxSeen, this.items.length);
    return true;
  }

  shift() {
    if (this.items.length > 0) return Promise.resolve(this.items.shift());
    if (this.closed) return Promise.resolve(null);
    return new Promise((resolve) => this.waiters.push(resolve));
  }

  close() {
    this.closed = true;
    const ws = this.waiters;
    this.waiters = [];
    for (const w of ws) w(null);
  }
}

// ffconcat list. Each frame lasts until the next frame's capture time; the last frame lasts
// until stopWallMs. The final file line is repeated because the concat demuxer ignores the
// duration of the last entry otherwise.
export function buildConcatList(entries, stopWallMs) {
  if (!entries || entries.length === 0) throw new Error('buildConcatList: no frames');
  const lines = ['ffconcat version 1.0'];
  const esc = (f) => `'${String(f).replace(/'/g, `'\\''`)}'`;
  entries.forEach((e, i) => {
    lines.push(`file ${esc(e.file)}`);
    const next = entries[i + 1];
    const durMs = next ? next.tMs - e.tMs : stopWallMs - e.tMs;
    const sec = Math.max(durMs, 1) / 1000;
    lines.push(`duration ${sec.toFixed(6)}`);
  });
  lines.push(`file ${esc(entries[entries.length - 1].file)}`);
  return lines.join('\n') + '\n';
}

export function parseBench(stderrText) {
  const m = /bench:\s*utime=([\d.]+)s\s+stime=([\d.]+)s\s+rtime=([\d.]+)s/.exec(String(stderrText ?? ''));
  if (!m) return null;
  return { utime_s: Number(m[1]), stime_s: Number(m[2]), rtime_s: Number(m[3]) };
}

// Scan JPEG markers for a baseline/progressive SOF and return its dimensions.
export function jpegSize(buf) {
  if (!buf || buf.length < 4 || buf[0] !== 0xff || buf[1] !== 0xd8) return null;
  let i = 2;
  while (i + 4 <= buf.length) {
    if (buf[i] !== 0xff) { i++; continue; }
    const m = buf[i + 1];
    if (m === 0xff) { i++; continue; } // fill byte
    if (m === 0xc0 || m === 0xc1 || m === 0xc2) {
      if (i + 9 > buf.length) return null;
      return { w: buf.readUInt16BE(i + 7), h: buf.readUInt16BE(i + 5) };
    }
    // Standalone markers carry no length field.
    if (m === 0x01 || m === 0xd8 || m === 0xd9 || (m >= 0xd0 && m <= 0xd7)) { i += 2; continue; }
    if (i + 4 > buf.length) return null;
    i += 2 + buf.readUInt16BE(i + 2);
  }
  return null;
}

const STDERR_CAP = 64 * 1024;

// Spawns ffmpeg with stdin piped (frames in) and stderr captured (bench line and errors).
export function startEncoder(ffmpegPath, args) {
  const child = spawn(ffmpegPath, args, { stdio: ['pipe', 'ignore', 'pipe'] });
  let stderr = '';
  child.stderr.on('data', (d) => {
    stderr += d.toString('utf8');
    if (stderr.length > STDERR_CAP) stderr = stderr.slice(-STDERR_CAP);
  });

  let exited = null;
  let resolveExit;
  const exitP = new Promise((r) => { resolveExit = r; });
  child.on('exit', (code, signal) => {
    exited = { code, signal };
    resolveExit(exited);
  });
  child.on('error', (e) => {
    // Spawn failure (ENOENT etc.) emits 'error' and may never emit 'exit'.
    if (!exited) {
      exited = { code: null, signal: null, spawnError: e.message };
      resolveExit(exited);
    }
  });

  const stdin = child.stdin;
  let stdinErr = null;
  // EPIPE arrives when ffmpeg died early; keep the listener so it cannot crash the process.
  stdin.on('error', (e) => { stdinErr = e; });

  function write(buf) {
    if (stdinErr) return Promise.reject(stdinErr);
    if (stdin.destroyed || stdin.writableEnded) return Promise.reject(new Error('encoder stdin closed'));
    const ok = stdin.write(buf);
    if (ok) return Promise.resolve();
    // Backpressure: wait for drain, but do not hang if the pipe dies first.
    return new Promise((resolve, reject) => {
      const onDrain = () => { cleanup(); resolve(); };
      const onClose = () => { cleanup(); reject(stdinErr ?? new Error('encoder stdin closed before drain')); };
      const cleanup = () => {
        stdin.off('drain', onDrain);
        stdin.off('close', onClose);
      };
      stdin.once('drain', onDrain);
      stdin.once('close', onClose);
    });
  }

  async function finalize(timeoutMs) {
    const t0 = performance.now();
    let killed = false;
    try {
      if (!stdin.writableEnded && !stdin.destroyed) stdin.end();
    } catch { /* stream already gone */ }
    const timer = setTimeout(() => {
      if (!exited) {
        killed = true;
        child.kill('SIGKILL');
      }
    }, timeoutMs);
    const ex = await exitP;
    clearTimeout(timer);
    const stderrText = stderr;
    return {
      exit_code: ex.code,
      signal: ex.signal ?? null,
      // Only a clean exit 0 counts. A SIGKILL is a last resort and the file is not trusted.
      finalized: ex.code === 0 && !killed,
      killed,
      finalize_ms: Math.round(performance.now() - t0),
      bench: parseBench(stderrText),
      stderr_tail: stderrText.slice(-2048),
      ...(ex.spawnError ? { spawn_error: ex.spawnError } : {}),
    };
  }

  return { child, write, finalize, exited: () => exited };
}
