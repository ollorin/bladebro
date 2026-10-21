import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pickActiveFromList, shouldSwitchOnCreated, parseScript, findNewestActivePort } from '../lib/follow.mjs';

test('pickActiveFromList skips iframe, service_worker and devtools:// and returns the first page', () => {
  const list = [
    { id: 'W1', type: 'service_worker', url: 'https://x/sw.js' },
    { id: 'F1', type: 'iframe', url: 'https://x/frame' },
    { id: 'D1', type: 'page', url: 'devtools://devtools/bundled/inspector.html' },
    { id: 'P1', type: 'page', url: 'http://127.0.0.1:8765/a.html' },
    { id: 'P2', type: 'page', url: 'http://127.0.0.1:8765/b.html' },
  ];
  assert.equal(pickActiveFromList(list), 'P1');
});

test('pickActiveFromList: empty list or no real page -> null', () => {
  assert.equal(pickActiveFromList([]), null);
  assert.equal(pickActiveFromList([{ id: 'D', type: 'page', url: 'devtools://x' }]), null);
  assert.equal(pickActiveFromList(undefined), null);
});

test('shouldSwitchOnCreated: page + newest is true; page + poll is false; service_worker + newest is false', () => {
  const page = { type: 'page', url: 'http://127.0.0.1:8765/b.html' };
  assert.equal(shouldSwitchOnCreated(page, 'newest'), true);
  assert.equal(shouldSwitchOnCreated(page, 'poll'), false);
  assert.equal(shouldSwitchOnCreated({ type: 'service_worker', url: 'https://x/sw.js' }, 'newest'), false);
  assert.equal(shouldSwitchOnCreated({ type: 'page', url: 'devtools://x' }, 'newest'), false);
});

test('parseScript sorts by time and parses WxH into arg {w,h}', () => {
  const steps = parseScript('25:emulate-clear;5:emulate=1001x641;15:window=1100x700;30:minimize;40:restore');
  assert.deepEqual(steps.map((s) => s.atS), [5, 15, 25, 30, 40]);
  assert.deepEqual(steps[0], { atS: 5, action: 'emulate', arg: { w: 1001, h: 641 } });
  assert.deepEqual(steps[1], { atS: 15, action: 'window', arg: { w: 1100, h: 700 } });
  assert.deepEqual(steps[2], { atS: 25, action: 'emulate-clear', arg: null });
});

test('parseScript: an unknown action throws, empty string gives []', () => {
  assert.throws(() => parseScript('5:bogus'), /unknown script action "bogus"/);
  assert.throws(() => parseScript('5:emulate'), /needs WxH/);
  assert.throws(() => parseScript('5:minimize=1x1'), /takes no argument/);
  assert.throws(() => parseScript('minimize'), /<seconds>:<action>/);
  assert.deepEqual(parseScript(''), []);
  assert.deepEqual(parseScript(undefined), []);
});

test('findNewestActivePort picks the newest mtime and parses its port; empty -> null', () => {
  const entries = [
    { path: '/a/DevToolsActivePort', mtimeMs: 1000, text: '9111\n/devtools/browser/old\n' },
    { path: '/b/DevToolsActivePort', mtimeMs: 5000, text: '9222\n/devtools/browser/new\n' },
    { path: '/c/DevToolsActivePort', mtimeMs: 3000, text: '9333\n/devtools/browser/mid\n' },
  ];
  assert.equal(findNewestActivePort(entries), 9222);
  assert.equal(findNewestActivePort([]), null);
});
