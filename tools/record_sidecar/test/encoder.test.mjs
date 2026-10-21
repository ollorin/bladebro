import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  even, buildFfmpegArgs, FrameQueue, buildConcatList, parseBench, jpegSize,
} from '../lib/encoder.mjs';

const base = { out: 'x.webm', width: 1280, height: 720, sizeMode: 'fixed', fps: 10 };

test('vp9 + wallclock: wallclock stamps, passthrough, libvpx-vp9, no -r, ends with -an out', () => {
  const a = buildFfmpegArgs({ ...base, codec: 'vp9', ts: 'wallclock' });
  assert.ok(a.includes('-use_wallclock_as_timestamps'));
  assert.equal(a[a.indexOf('-use_wallclock_as_timestamps') + 1], '1');
  assert.ok(a.includes('-fps_mode'));
  assert.equal(a[a.indexOf('-fps_mode') + 1], 'passthrough');
  assert.ok(a.includes('libvpx-vp9'));
  assert.equal(a[a.indexOf('-deadline') + 1], 'realtime');
  assert.equal(a.includes('-r'), false, 'wallclock must not force a frame rate');
  assert.deepEqual(a.slice(-2), ['-an', 'x.webm']);
});

// Regression: vfr dropped ~70% of frames at ~98 fps capture rate (measured, see encoder.mjs).
// Wall-clock and concat paths must never reintroduce the vfr mode.
test('regression: wallclock input timebase is 1 ms (-framerate 1000), not the 25 tbn default', () => {
  const a = buildFfmpegArgs({ ...base, codec: 'vp9', ts: 'wallclock' });
  assert.equal(a[a.indexOf('-framerate') + 1], '1000');
  assert.ok(a.indexOf('-framerate') < a.indexOf('-i'), 'must be an input option');
});

test('regression: wallclock and concat never use -fps_mode vfr (it drops frames)', () => {
  for (const ts of ['wallclock', 'concat']) {
    const a = buildFfmpegArgs({ ...base, codec: 'vp9', ts, concatList: 'list.ffconcat' });
    assert.equal(a.includes('vfr'), false, `${ts} must not use vfr`);
    assert.equal(a[a.indexOf('-fps_mode') + 1], 'passthrough', ts);
  }
});

test('cfr fps=10: framerate on input and -r on output, no wallclock stamps', () => {
  const a = buildFfmpegArgs({ ...base, codec: 'vp9', ts: 'cfr', fps: 10 });
  assert.equal(a[a.indexOf('-framerate') + 1], '10');
  assert.equal(a[a.indexOf('-r') + 1], '10');
  assert.equal(a.includes('-use_wallclock_as_timestamps'), false);
});

test('concat: reads the ffconcat list with -safe 0', () => {
  const a = buildFfmpegArgs({ ...base, codec: 'vp9', ts: 'concat', concatList: 'list.ffconcat' });
  const i = a.indexOf('-f');
  assert.equal(a[i + 1], 'concat');
  assert.deepEqual(a.slice(i, i + 6), ['-f', 'concat', '-safe', '0', '-i', 'list.ffconcat']);
});

test('fixed size 1001x641 uses even 1000:640 with letterbox pad', () => {
  const a = buildFfmpegArgs({ ...base, codec: 'vp9', ts: 'wallclock', width: 1001, height: 641 });
  const vf = a[a.indexOf('-vf') + 1];
  assert.ok(vf.includes('scale=1000:640:'), vf);
  assert.ok(vf.includes('pad=1000:640:'), vf);
});

test('size-mode even uses trunc(iw/2)*2 and no pad', () => {
  const a = buildFfmpegArgs({ ...base, codec: 'vp9', ts: 'wallclock', sizeMode: 'even' });
  const vf = a[a.indexOf('-vf') + 1];
  assert.ok(vf.includes('trunc(iw/2)*2'));
  assert.equal(vf.includes('pad='), false);
});

test('vp8 maps to libvpx (not vp9)', () => {
  const a = buildFfmpegArgs({ ...base, codec: 'vp8', ts: 'wallclock' });
  assert.ok(a.includes('libvpx'));
  assert.equal(a.includes('libvpx-vp9'), false);
});

test('x264 -> libx264 with faststart in an mp4', () => {
  const a = buildFfmpegArgs({ ...base, out: 'x.mp4', codec: 'x264', ts: 'wallclock' });
  assert.ok(a.includes('libx264'));
  assert.equal(a[a.indexOf('-movflags') + 1], '+faststart');
});

test('x264 with a .webm output throws; vp9 with .mp4 throws', () => {
  assert.throws(() => buildFfmpegArgs({ ...base, codec: 'x264', ts: 'wallclock' }), /needs a .mp4/);
  assert.throws(() => buildFfmpegArgs({ ...base, out: 'x.mp4', codec: 'vp9', ts: 'wallclock' }), /needs a .webm/);
});

test('unknown codec / ts / size-mode throw', () => {
  assert.throws(() => buildFfmpegArgs({ ...base, codec: 'h265', ts: 'wallclock' }), /unknown codec/);
  assert.throws(() => buildFfmpegArgs({ ...base, codec: 'vp9', ts: 'vfr' }), /unknown ts/);
  assert.throws(() => buildFfmpegArgs({ ...base, codec: 'vp9', ts: 'wallclock', sizeMode: 'max' }), /unknown size-mode/);
});

test('even() rounds down to an even number', () => {
  assert.equal(even(1001), 1000);
  assert.equal(even(640), 640);
});

test('FrameQueue: cap 2 drops the incoming third frame, FIFO, closed -> null', async () => {
  const q = new FrameQueue(2);
  assert.equal(q.push('a'), true);
  assert.equal(q.push('b'), true);
  assert.equal(q.push('c'), false);
  assert.equal(q.dropped, 1);
  assert.equal(q.maxSeen, 2);
  assert.equal(await q.shift(), 'a');
  assert.equal(await q.shift(), 'b');
  q.close();
  assert.equal(await q.shift(), null);
});

test('FrameQueue: a waiting shift receives the next push', async () => {
  const q = new FrameQueue(2);
  const p = q.shift();
  q.push('late');
  assert.equal(await p, 'late');
});

test('buildConcatList: durations from capture times, last frame to stop, last file repeated', () => {
  const txt = buildConcatList([
    { file: '/f/0.jpg', tMs: 0 },
    { file: '/f/1.jpg', tMs: 100 },
    { file: '/f/2.jpg', tMs: 250 },
  ], 1250);
  const lines = txt.trim().split('\n');
  assert.equal(lines[0], 'ffconcat version 1.0');
  assert.deepEqual(lines.filter((l) => l.startsWith('duration')), [
    'duration 0.100000', 'duration 0.150000', 'duration 1.000000',
  ]);
  assert.equal(lines[lines.length - 1], "file '/f/2.jpg'");
  assert.equal(lines.filter((l) => l === "file '/f/2.jpg'").length, 2);
});

test('buildConcatList: empty throws', () => {
  assert.throws(() => buildConcatList([], 1000), /no frames/);
});

test('parseBench reads the bench line and returns null on garbage', () => {
  assert.deepEqual(parseBench('frame= 3\nbench: utime=1.234s stime=0.120s rtime=10.500s\n'),
    { utime_s: 1.234, stime_s: 0.12, rtime_s: 10.5 });
  assert.equal(parseBench('no timing here'), null);
});

test('jpegSize reads SOF0 dimensions from a hand-built header; non-JPEG -> null', () => {
  // SOI, SOF0 (len 17): precision 8, h=641 (0x0281), w=1001 (0x03E9), 3 components
  const buf = Buffer.from([
    0xff, 0xd8,
    0xff, 0xc0, 0x00, 0x11, 0x08, 0x02, 0x81, 0x03, 0xe9, 0x03,
    0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01,
  ]);
  assert.deepEqual(jpegSize(buf), { w: 1001, h: 641 });
  assert.equal(jpegSize(Buffer.from('not a jpeg at all')), null);
});
