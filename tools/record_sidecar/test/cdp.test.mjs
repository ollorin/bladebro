import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseDevToolsActivePort, percentile } from '../lib/cdp.mjs';

test('parseDevToolsActivePort: port + browser path', () => {
  assert.deepEqual(parseDevToolsActivePort('9333\n/devtools/browser/abc\n'),
    { port: 9333, browserPath: '/devtools/browser/abc' });
});

test('parseDevToolsActivePort: empty and malformed throw', () => {
  assert.throws(() => parseDevToolsActivePort(''));
  assert.throws(() => parseDevToolsActivePort('x\n'));
  assert.throws(() => parseDevToolsActivePort('9333\n/not/browser\n'));
});

test('percentile: nearest rank, empty -> null', () => {
  assert.equal(percentile([1, 2, 3, 4], 50), 2);
  assert.equal(percentile(Array.from({ length: 100 }, (_, i) => i + 1), 95), 95);
  assert.equal(percentile([], 50), null);
});
