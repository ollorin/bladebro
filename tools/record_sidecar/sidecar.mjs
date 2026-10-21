#!/usr/bin/env node
// Standalone CDP screencast -> ffmpeg recorder. Probe for the Phase 2 gate only: NOT shipped
// in the bladebro binary and not used by src/. Attaches a NEW flattened session to a page on a
// bladebro Chrome (WS transport), so it can run next to the daemon's own connection.
//
// Harness-only features (--follow, --script, --attach-wait) exist to measure tab switching,
// viewport changes and minimize; they are not product recorder behavior.
import { writeFileSync, mkdirSync, rmSync, statSync, existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { getJson, connect, percentile } from './lib/cdp.mjs';
import { buildFfmpegArgs, buildConcatList, FrameQueue, startEncoder, jpegSize } from './lib/encoder.mjs';
import { pickActiveFromList, shouldSwitchOnCreated, parseScript } from './lib/follow.mjs';
import { runScriptAction } from './lib/actions.mjs';
import { waitForBrowserPort, waitForPage } from './lib/attach.mjs';

const USAGE = 'usage: sidecar.mjs (--port N | --attach-wait S --blade-home DIR) --out FILE [--duration S] [--codec vp9|vp8|x264] [--ts wallclock|concat|cfr] [--fps N] [--quality N] [--max-width N] [--max-height N] [--every-nth N] [--queue N] [--slow-writer-ms N] [--size-mode fixed|even] [--target ID] [--follow off|newest|poll] [--poll-ms N] [--script SPEC] [--no-page-enable] [--no-tail] [--stats FILE] [--finalize-timeout MS] [--keep-frames] [--ffmpeg PATH]';

function usage(msg) {
  process.stderr.write(`sidecar: ${msg}\n${USAGE}\n`);
  process.exit(2);
}

const BOOL_FLAGS = new Set(['--no-page-enable', '--no-tail', '--keep-frames']);
const VALUE_FLAGS = new Set([
  '--port', '--target', '--out', '--duration', '--codec', '--ts', '--fps', '--quality',
  '--max-width', '--max-height', '--every-nth', '--queue', '--slow-writer-ms', '--size-mode',
  '--stats', '--finalize-timeout', '--ffmpeg', '--follow', '--poll-ms', '--script',
  '--attach-wait', '--blade-home',
]);

function parseArgs(argv) {
  const raw = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (BOOL_FLAGS.has(a)) { raw[a] = true; continue; }
    if (VALUE_FLAGS.has(a)) {
      if (i + 1 >= argv.length) usage(`${a} needs a value`);
      raw[a] = argv[++i];
      continue;
    }
    usage(`unknown argument ${a}`);
  }
  const int = (flag, def, min = 0) => {
    if (raw[flag] === undefined) return def;
    const n = Number(raw[flag]);
    if (!Number.isInteger(n) || n < min) usage(`${flag} must be an integer >= ${min}`);
    return n;
  };
  if (raw['--out'] === undefined) usage('--out is required');
  if (raw['--port'] === undefined && raw['--attach-wait'] === undefined) usage('--port or --attach-wait is required');
  if (raw['--attach-wait'] !== undefined && raw['--blade-home'] === undefined) usage('--attach-wait needs --blade-home DIR');
  const a = {
    port: raw['--port'] === undefined ? null : int('--port', 0, 1),
    attachWaitS: raw['--attach-wait'] === undefined ? null : Number(raw['--attach-wait']),
    bladeHome: raw['--blade-home'] ? resolve(raw['--blade-home']) : null,
    target: raw['--target'] ?? null,
    out: resolve(raw['--out']),
    duration: raw['--duration'] === undefined ? null : Number(raw['--duration']),
    codec: raw['--codec'] ?? 'vp9',
    ts: raw['--ts'] ?? 'wallclock',
    fps: int('--fps', 10, 1),
    quality: int('--quality', 70, 1),
    maxWidth: int('--max-width', 1280, 1),
    maxHeight: int('--max-height', 720, 1),
    everyNth: int('--every-nth', 1, 1),
    queue: int('--queue', 16, 1),
    slowWriterMs: int('--slow-writer-ms', 0, 0),
    sizeMode: raw['--size-mode'] ?? 'fixed',
    follow: raw['--follow'] ?? 'off',
    pollMs: int('--poll-ms', 250, 10),
    script: raw['--script'] ?? null,
    pageEnable: !raw['--no-page-enable'],
    tail: !raw['--no-tail'],
    statsPath: raw['--stats'] ? resolve(raw['--stats']) : `${resolve(raw['--out'])}.stats.json`,
    finalizeTimeout: int('--finalize-timeout', 10000, 1),
    keepFrames: !!raw['--keep-frames'],
    ffmpeg: raw['--ffmpeg'] ?? process.env.BLADE_FFMPEG ?? 'ffmpeg',
  };
  if (a.duration !== null && (!Number.isFinite(a.duration) || a.duration <= 0)) usage('--duration must be > 0');
  if (a.attachWaitS !== null && (!Number.isFinite(a.attachWaitS) || a.attachWaitS <= 0)) usage('--attach-wait must be > 0 seconds');
  if (!['vp9', 'vp8', 'x264'].includes(a.codec)) usage(`--codec must be vp9|vp8|x264, got ${a.codec}`);
  if (!['wallclock', 'concat', 'cfr'].includes(a.ts)) usage(`--ts must be wallclock|concat|cfr, got ${a.ts}`);
  if (!['fixed', 'even'].includes(a.sizeMode)) usage(`--size-mode must be fixed|even, got ${a.sizeMode}`);
  if (!['off', 'newest', 'poll'].includes(a.follow)) usage(`--follow must be off|newest|poll, got ${a.follow}`);
  try { a.steps = parseScript(a.script); } catch (e) { usage(e.message); }
  return a;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const withTimeout = (p, ms) => Promise.race([
  p,
  new Promise((_, rej) => setTimeout(() => rej(new Error(`timeout after ${ms}ms`)), ms)),
]);

function summarize(arr) {
  const s = [...arr].sort((x, y) => x - y);
  return { p50: percentile(s, 50), p95: percentile(s, 95), max: s.length ? s[s.length - 1] : null };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const startedMs = Date.now();
  const stats = {
    version: 2,
    args,
    chrome: null,
    ffmpeg_path: args.ffmpeg,
    started_at: new Date(startedMs).toISOString(),
    wall_ms: null,
    follow: args.follow,
    stop_reason: null,
    frames_received: 0,
    stale_frames: 0,
    acks_sent: 0,
    acks_ok: 0,
    acks_err: 0,
    frames_written: 0,
    dup_frames_written: 0,
    frames_dropped: 0,
    queue_max: 0,
    ack_send_us: null,
    ack_rtt_ms: null,
    capture_to_recv_ms: null,
    effective_fps_recv: null,
    first_frame_ms: null,
    max_gap_ms: null,
    fps_buckets: [],
    frame_sizes: {},
    concat_clock: null,
    segments: [],
    events: [],
    encoder: null,
    out: args.out,
    out_bytes: null,
  };

  mkdirSync(dirname(args.out), { recursive: true });

  // --- endpoint and page discovery -------------------------------------------------------
  let port = args.port;
  if (args.attachWaitS !== null) {
    try {
      port = await waitForBrowserPort({ dir: args.bladeHome, timeoutS: args.attachWaitS, sinceMs: startedMs });
    } catch (e) {
      process.stderr.write(`sidecar: ${e.message}\n`);
      process.exit(3);
    }
  }
  const ver = await getJson(port, '/json/version');
  stats.chrome = ver.Browser;
  const browser = await connect(ver.webSocketDebuggerUrl);

  let page;
  if (args.target) {
    const list = await getJson(port, '/json/list');
    page = list.find((t) => t.id === args.target);
    if (!page) throw new Error(`no target ${args.target} in /json/list`);
  } else if (args.attachWaitS !== null) {
    page = await waitForPage(port, pickActiveFromList);
  } else {
    const list = await getJson(port, '/json/list');
    const id = pickActiveFromList(list);
    if (!id) throw new Error('no page target in /json/list');
    page = list.find((t) => t.id === id);
  }

  // Target.setDiscoverTargets is sent for every mode (not only follow): targetDestroyed on the
  // current page is how a follow-off run learns the page was closed, and newest/poll need it.
  await browser.send('Target.setDiscoverTargets', { discover: true });

  // --- capture state ---------------------------------------------------------------------
  let S = null;                 // current screencast sessionId; frames from any other session are stale
  let curTarget = page.id;
  const failedTargets = new Set();
  const screencastParams = () => ({
    format: 'jpeg',
    quality: args.quality,
    maxWidth: args.maxWidth,
    maxHeight: args.maxHeight,
    everyNthFrame: args.everyNth,
  });

  const attachTo = async (id) => {
    const { sessionId } = await browser.send('Target.attachToTarget', { targetId: id, flatten: true });
    return sessionId;
  };
  S = await attachTo(page.id);
  stats.segments.push({ t_ms: 0, targetId: page.id, url: page.url, frames: 0, switch_ms: null });
  if (args.pageEnable) await browser.send('Page.enable', {}, S);
  await browser.send('Page.startScreencast', screencastParams(), S);
  const tStartCapture = performance.now();

  const framesDir = `${args.out}.frames`;
  if (args.ts === 'concat') mkdirSync(framesDir, { recursive: true });

  const queue = new FrameQueue(args.queue);
  let enc = null;
  let writerP = null;
  let writerErr = null;
  const ackSendUs = [];
  const ackRttMs = [];
  const captureToRecvMs = [];
  const concatEntries = [];
  let latest = null;
  let lastTick = null;
  let lastBuf = null;
  let lastRecvWall = null;
  let lastRecvDateMs = Date.now();
  let firstFrameMs = null;
  let maxGapMs = null;
  let seq = 0;
  let ivl = null;
  let stopping = false;
  let stopPromise = null;
  let pendingSwitchT0 = null;   // decision time of a switch whose first frame has not arrived yet

  if (args.ts !== 'concat') {
    const argv = buildFfmpegArgs({
      codec: args.codec, ts: args.ts, out: args.out, width: args.maxWidth, height: args.maxHeight,
      sizeMode: args.sizeMode, fps: args.fps,
    });
    enc = startEncoder(args.ffmpeg, argv);
    writerP = (async () => {
      let f;
      while ((f = await queue.shift()) !== null) {
        if (args.slowWriterMs) await sleep(args.slowWriterMs);
        await enc.write(f.buf);
        stats.frames_written++;
        if (f.dup) stats.dup_frames_written++;
      }
    })().catch((e) => { writerErr = e; });
  }

  browser.on('Page.screencastFrame', (params, sid) => {
    // Ack FIRST and synchronously, on the session the frame came from: Chrome stops sending
    // frames until it is acked, so nothing may sit between the event and this send. Stale
    // frames (an old session after a switch) must be acked too, or Chrome stalls that session.
    const t0 = performance.now();
    const ack = browser.sendNoWait('Page.screencastFrameAck', { sessionId: params.sessionId }, sid);
    ackSendUs.push((performance.now() - t0) * 1000);
    stats.acks_sent++;
    ack.done.then(
      () => { stats.acks_ok++; ackRttMs.push(performance.now() - ack.sentAt); },
      () => { stats.acks_err++; },
    );
    if (sid !== S) { stats.stale_frames++; return; }

    const now = performance.now();
    stats.frames_received++;
    if (firstFrameMs === null) firstFrameMs = Math.round(now - tStartCapture);
    if (lastRecvWall !== null) {
      const gap = now - lastRecvWall;
      maxGapMs = maxGapMs === null ? gap : Math.max(maxGapMs, gap);
    }
    lastRecvWall = now;
    lastRecvDateMs = Date.now();
    const bucket = Math.floor((now - tStartCapture) / 1000);
    stats.fps_buckets[bucket] = (stats.fps_buckets[bucket] ?? 0) + 1;

    const buf = Buffer.from(params.data, 'base64');
    const sz = jpegSize(buf);
    if (sz) {
      const key = `${sz.w}x${sz.h}`;
      stats.frame_sizes[key] = (stats.frame_sizes[key] ?? 0) + 1;
    }
    const meta = params.metadata ?? {};
    if (typeof meta.timestamp === 'number') {
      captureToRecvMs.push(Date.now() - meta.timestamp * 1000);
    }
    const seg = stats.segments[stats.segments.length - 1];
    if (seg.frames === 0 && pendingSwitchT0 !== null) {
      seg.switch_ms = Math.round(now - pendingSwitchT0);
      pendingSwitchT0 = null;
    }
    seg.frames++;

    if (args.ts === 'wallclock') {
      lastBuf = buf;
      const ok = queue.push({ buf });
      if (!ok) stats.frames_dropped = queue.dropped;
    } else if (args.ts === 'cfr') {
      latest = { buf };
    } else {
      // concat: real capture timestamps are the timeline; frames go to disk, not to ffmpeg stdin.
      const file = resolve(framesDir, `f${String(seq++).padStart(6, '0')}.jpg`);
      writeFileSync(file, buf);
      const tMs = typeof meta.timestamp === 'number' ? meta.timestamp * 1000 : Date.now();
      stats.concat_clock = typeof meta.timestamp === 'number' ? 'capture_metadata' : 'wall';
      concatEntries.push({ file, tMs });
    }
    stats.queue_max = Math.max(stats.queue_max, queue.maxSeen);
  });


  // --- tab following ---------------------------------------------------------------------
  // Switches are serialized: at most one stopScreencast/attach/startScreencast sequence runs
  // at a time, and the one encoder keeps running across every switch.
  let switchChain = Promise.resolve();
  const doSwitch = async (id, url) => {
    if (stopping || id === curTarget) return;
    const decided = performance.now();
    const old = S;
    await withTimeout(browser.send('Page.stopScreencast', {}, old), 2000).catch(() => {});
    await withTimeout(browser.send('Target.detachFromTarget', { sessionId: old }), 2000).catch(() => {});
    const newS = await attachTo(id);
    // The segment is pushed and S is switched BEFORE startScreencast: frames can only start
    // after it, and the handler must see the new segment and the new session at that point.
    pendingSwitchT0 = decided;
    stats.segments.push({ t_ms: Date.now() - startedMs, targetId: id, url, frames: 0, switch_ms: null });
    S = newS;
    curTarget = id;
    if (args.pageEnable) await browser.send('Page.enable', {}, S);
    await browser.send('Page.startScreencast', screencastParams(), S);
  };
  const switchTo = (id, url) => {
    if (failedTargets.has(id)) return switchChain;
    switchChain = switchChain.then(() => doSwitch(id, url)).catch((e) => {
      failedTargets.add(id);
      stats.events.push({ t_ms: Date.now() - startedMs, type: 'switch_error', target: id, message: String(e?.message ?? e) });
    });
    return switchChain;
  };

  let pollTimer = null;
  let pollBusy = false;
  const pollOnce = async () => {
    if (pollBusy || stopping) return;
    pollBusy = true;
    try {
      const list = await getJson(port, '/json/list');
      const id = pickActiveFromList(list);
      if (id && id !== curTarget) await switchTo(id, list.find((t) => t.id === id)?.url ?? null);
    } catch {
      // A transient /json/list failure is retried on the next tick; recording is unaffected.
    } finally {
      pollBusy = false;
    }
  };
  if (args.follow === 'newest') {
    browser.on('Target.targetCreated', (p) => {
      const info = p.targetInfo ?? {};
      if (shouldSwitchOnCreated(info, args.follow)) switchTo(info.targetId, info.url ?? null);
    });
  }
  if (args.follow === 'poll') {
    pollTimer = setInterval(pollOnce, args.pollMs);
  }
  browser.on('Target.targetDestroyed', (p) => {
    if (p.targetId !== curTarget || stopping) return;
    if (args.follow === 'poll') pollOnce();
    else if (args.follow === 'off') stop('browser-gone').then((c) => process.exit(c));
  });

  // --- harness script (viewport / window), harness-only, see lib/actions.mjs ---------------
  const scriptTimers = [];
  const scriptCtx = { browser, session: () => S, target: () => curTarget };
  for (const step of args.steps) {
    const label = step.arg ? `${step.action}=${step.arg.w}x${step.arg.h}` : step.action;
    scriptTimers.push(setTimeout(() => {
      const t_ms = Math.round(performance.now() - tStartCapture);
      runScriptAction(scriptCtx, step).then(
        () => stats.events.push({ t_ms, action: label, ok: true, error: null }),
        (e) => stats.events.push({ t_ms, action: label, ok: false, error: String(e?.message ?? e) }),
      );
    }, step.atS * 1000));
  }

  if (args.ts === 'cfr') {
    ivl = setInterval(() => {
      if (!latest) return; // no tick before the first frame
      const dup = latest === lastTick;
      lastTick = latest;
      if (!queue.push({ buf: latest.buf, dup })) stats.frames_dropped = queue.dropped;
    }, 1000 / args.fps);
  }

  const stop = (reason) => {
    if (stopPromise) return stopPromise;
    stopPromise = (async () => {
      stopping = true;
      stats.stop_reason = reason;
      if (pollTimer) clearInterval(pollTimer);
      for (const t of scriptTimers) clearTimeout(t);
      await withTimeout(switchChain, 5000).catch(() => {});
      await withTimeout(browser.send('Page.stopScreencast', {}, S), 2000).catch(() => {});
      await withTimeout(browser.send('Target.detachFromTarget', { sessionId: S }), 2000).catch(() => {});
      if (ivl) clearInterval(ivl);
      if (args.ts === 'wallclock' && args.tail && lastBuf) {
        // Hold the last frame to the stop time, otherwise the video ends at the last change.
        queue.push({ buf: lastBuf });
      }
      queue.close();
      if (writerP) {
        const drained = await Promise.race([writerP.then(() => true), sleep(5000).then(() => false)]);
        if (!drained) stats.events.push({ t_ms: Date.now() - startedMs, type: 'writer_drain_timeout' });
      }
      stats.frames_dropped = queue.dropped;
      stats.queue_max = Math.max(stats.queue_max, queue.maxSeen);

      let encRes;
      if (args.ts === 'concat') {
        if (concatEntries.length === 0) {
          encRes = { exit_code: null, signal: null, finalized: false, killed: false, finalize_ms: 0, bench: null, stderr_tail: 'no frames' };
        } else {
          // Stop time on the same clock as the frame timestamps: extrapolate the last frame's
          // clock by the wall time elapsed since it arrived.
          const last = concatEntries[concatEntries.length - 1];
          const stopClock = stats.concat_clock === 'capture_metadata'
            ? last.tMs + (Date.now() - lastRecvDateMs)
            : Date.now();
          const listPath = resolve(framesDir, 'list.ffconcat');
          writeFileSync(listPath, buildConcatList(concatEntries, stopClock));
          const argv = buildFfmpegArgs({
            codec: args.codec, ts: 'concat', out: args.out, width: args.maxWidth, height: args.maxHeight,
            sizeMode: args.sizeMode, fps: args.fps, concatList: listPath,
          });
          const cenc = startEncoder(args.ffmpeg, argv);
          encRes = await cenc.finalize(Math.max(args.finalizeTimeout, 60000));
        }
      } else {
        encRes = enc ? await enc.finalize(args.finalizeTimeout) : null;
        if (writerErr) stats.events.push({ t_ms: Date.now() - startedMs, type: 'writer_error', message: String(writerErr.message ?? writerErr) });
      }
      stats.encoder = encRes;

      if (args.ts === 'concat' && !args.keepFrames) rmSync(framesDir, { recursive: true, force: true });

      const wallMs = Date.now() - startedMs;
      stats.wall_ms = wallMs;
      stats.ack_send_us = summarize(ackSendUs);
      stats.ack_rtt_ms = summarize(ackRttMs);
      stats.capture_to_recv_ms = summarize(captureToRecvMs);
      stats.effective_fps_recv = stats.frames_received > 0 ? +(stats.frames_received / (wallMs / 1000)).toFixed(2) : null;
      stats.first_frame_ms = firstFrameMs;
      stats.max_gap_ms = maxGapMs === null ? null : Math.round(maxGapMs);
      stats.frames_dropped = queue.dropped;
      if (existsSync(args.out)) stats.out_bytes = statSync(args.out).size;
      writeFileSync(args.statsPath, JSON.stringify(stats, null, 2) + '\n');

      await browser.close().catch(() => {});
      const ok = !!(stats.encoder && stats.encoder.finalized) && stats.frames_received > 0;
      const code = stats.frames_received === 0 ? 3 : ok ? 0 : 1;
      process.stderr.write(`sidecar: stopped (${reason}) frames=${stats.frames_received} stale=${stats.stale_frames} written=${stats.frames_written} dropped=${stats.frames_dropped} segments=${stats.segments.length} encoder_finalized=${stats.encoder?.finalized ?? null} exit=${code}\n`);
      return code;
    })();
    return stopPromise;
  };

  browser.closed.then(() => { if (!stopping) stop('browser-gone').then((c) => process.exit(c)); });
  process.on('SIGINT', () => stop('signal').then((c) => process.exit(c)));
  process.on('SIGTERM', () => stop('signal').then((c) => process.exit(c)));
  if (args.duration !== null) {
    setTimeout(() => stop('duration').then((c) => process.exit(c)), args.duration * 1000);
  }
  process.stderr.write(`sidecar: attached to ${page.url} (${page.id}); follow=${args.follow}; recording to ${args.out}\n`);
}

main().catch((e) => {
  process.stderr.write(`sidecar: ${e?.message ?? e}\n`);
  process.exit(1);
});
