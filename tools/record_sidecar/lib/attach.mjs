// Discovery of a browser we did not launch (--attach-wait). Harness-only.
// Chrome writes DevToolsActivePort only for --remote-debugging-port=0, so this path is used
// for a browser started with a dynamic port (bladebro audit / real-lane style). It walks the
// given data dir a few levels deep, because the file sits at different depths per lane.
import { readdirSync, statSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { findNewestActivePort } from './follow.mjs';
import { getJson } from './cdp.mjs';

export function findDevToolsFiles(dir, maxDepth = 4, depth = 0, out = []) {
  let names;
  try { names = readdirSync(dir); } catch { return out; }
  for (const name of names) {
    const p = join(dir, name);
    if (name === 'DevToolsActivePort') {
      try {
        const st = statSync(p);
        out.push({ path: p, mtimeMs: st.mtimeMs, text: readFileSync(p, 'utf8') });
      } catch { /* file vanished between readdir and stat */ }
      continue;
    }
    if (depth >= maxDepth) continue;
    let st;
    try { st = statSync(p); } catch { continue; }
    if (st.isDirectory()) findDevToolsFiles(p, maxDepth, depth + 1, out);
  }
  return out;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Polls every 50 ms until a DevToolsActivePort newer than (sinceMs - 2000) appears, then
// returns its port. Throws with the exact string the caller maps to exit code 3.
export async function waitForBrowserPort({ dir, timeoutS, sinceMs, pollMs = 50 }) {
  const deadline = Date.now() + timeoutS * 1000;
  for (;;) {
    const fresh = findDevToolsFiles(dir).filter((e) => e.mtimeMs >= sinceMs - 2000);
    if (fresh.length > 0) {
      try {
        const port = findNewestActivePort(fresh);
        if (port) return port;
      } catch { /* half-written file: retry */ }
    }
    if (Date.now() >= deadline) throw new Error(`no browser appeared under ${dir} within ${timeoutS}s`);
    await sleep(pollMs);
  }
}

// After the endpoint exists, Chrome still needs a moment before /json/list has a page.
export async function waitForPage(port, pickFn, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    try {
      const list = await getJson(port, '/json/list');
      const id = pickFn(list);
      if (id) return list.find((t) => t.id === id);
    } catch { /* endpoint not answering yet */ }
    if (Date.now() >= deadline) throw new Error(`no page target on :${port} within ${timeoutMs}ms`);
    await sleep(100);
  }
}
