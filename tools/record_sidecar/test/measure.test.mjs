import { test } from 'node:test';
import assert from 'node:assert/strict';
import { driftPct, parseProbe, bytesPerMinute, summarizeSegments, countDecodeErrorLines } from '../measure.mjs';

test('countDecodeErrorLines: counts non-empty stderr lines, empty -> 0', () => {
  assert.equal(countDecodeErrorLines(''), 0);
  assert.equal(countDecodeErrorLines('\n  \n'), 0);
  const warn = '[null @ 0x1] Application provided invalid, non monotonically increasing dts to muxer in stream 0: 217 >= 217\n';
  assert.equal(countDecodeErrorLines(warn + warn + '\n'), 2);
});

test('driftPct: 30.0 s video vs 30000 ms wall -> 0; 28.5 s -> -5', () => {
  assert.equal(driftPct(30.0, 30000), 0);
  assert.equal(driftPct(28.5, 30000), -5);
});

test('driftPct: wallMs 0 throws', () => {
  assert.throws(() => driftPct(10, 0));
});

test('parseProbe: canned ffprobe JSON -> numbers, not strings', () => {
  const canned = JSON.stringify({
    streams: [{ codec_name: 'vp9', width: 1280, height: 720, nb_read_frames: '794' }],
    format: { duration: '7.990000', size: '784116' },
  });
  assert.deepEqual(parseProbe(canned), {
    codec: 'vp9', width: 1280, height: 720, duration_s: 7.99, nb_read_frames: 794, size_bytes: 784116,
  });
});

test('bytesPerMinute: (6e6 bytes, 60 s) -> 6e6; zero duration -> null', () => {
  assert.equal(bytesPerMinute(6e6, 60), 6e6);
  assert.equal(bytesPerMinute(6e6, 0), null);
});

test('summarizeSegments: counts, per-segment frames, all_nonzero', () => {
  assert.deepEqual(
    summarizeSegments([{ frames: 240 }, { frames: 258 }, { frames: 328 }]),
    { count: 3, frames_per_segment: [240, 258, 328], all_nonzero: true },
  );
  assert.equal(summarizeSegments([{ frames: 10 }, { frames: 0 }]).all_nonzero, false);
  assert.equal(summarizeSegments([]).all_nonzero, false);
});
