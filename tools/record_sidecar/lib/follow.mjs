// Harness-only policy helpers for the sidecar: which page is "active", how a --script spec is
// parsed, and which DevToolsActivePort file is the newest. Pure functions, no I/O, so the
// unit tests can pin them down without a browser. Not used by src/ and not shipped.
import { parseDevToolsActivePort } from './cdp.mjs';

const isRealPage = (t) => t && t.type === 'page' && !String(t.url ?? '').startsWith('devtools://');

// /json/list entries for a page-only view. Chrome lists iframes, workers and devtools
// front-ends alongside pages; only a real page can host the screencast.
export function pickActiveFromList(list) {
  if (!Array.isArray(list)) return null;
  const hit = list.find(isRealPage);
  return hit ? hit.id : null;
}

// Newest-mode policy: a freshly created real page becomes the recording target. Other target
// types (service workers, iframes, devtools) never steal the recording.
export function shouldSwitchOnCreated(targetInfo, mode) {
  if (mode !== 'newest') return false;
  return isRealPage(targetInfo);
}

const ACTIONS_NO_ARG = new Set(['emulate-clear', 'minimize', 'restore']);
const ACTIONS_SIZE = new Set(['emulate', 'window']);

// Spec: "5:emulate=1001x641;15:window=1100x700;25:emulate-clear;30:minimize;40:restore".
// Returns steps sorted by time. Anything outside the vocabulary throws, so a typo cannot
// silently record a clean run that was supposed to exercise a resize.
export function parseScript(str) {
  if (str === undefined || str === null) return [];
  const steps = [];
  for (const raw of String(str).split(';')) {
    const part = raw.trim();
    if (part === '') continue;
    const m = /^(\d+(?:\.\d+)?):(.+)$/.exec(part);
    if (!m) throw new Error(`script step "${part}" must be "<seconds>:<action>"`);
    const atS = Number(m[1]);
    const body = m[2].trim();
    const eq = body.indexOf('=');
    const name = eq >= 0 ? body.slice(0, eq) : body;
    if (ACTIONS_NO_ARG.has(name)) {
      if (eq >= 0) throw new Error(`script action "${name}" takes no argument, got "${body}"`);
      steps.push({ atS, action: name, arg: null });
    } else if (ACTIONS_SIZE.has(name)) {
      const sm = /^(\d+)x(\d+)$/.exec(body.slice(eq + 1));
      if (eq < 0 || !sm) throw new Error(`script action "${name}" needs WxH, got "${body}"`);
      const w = Number(sm[1]);
      const h = Number(sm[2]);
      if (w < 1 || h < 1) throw new Error(`script action "${name}" needs positive WxH, got "${body}"`);
      steps.push({ atS, action: name, arg: { w, h } });
    } else {
      throw new Error(`unknown script action "${name}" (allowed: emulate=WxH, emulate-clear, window=WxH, minimize, restore)`);
    }
  }
  return steps.sort((a, b) => a.atS - b.atS);
}

// Entries are {path, mtimeMs, text}. The newest DevToolsActivePort wins: a stale file left by
// an earlier browser has an older mtime than the one the caller just launched.
export function findNewestActivePort(entries) {
  if (!Array.isArray(entries) || entries.length === 0) return null;
  let best = entries[0];
  for (const e of entries) if (e.mtimeMs > best.mtimeMs) best = e;
  return parseDevToolsActivePort(best.text).port;
}
