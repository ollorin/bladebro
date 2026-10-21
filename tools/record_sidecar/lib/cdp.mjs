// Minimal CDP WebSocket client + discovery helpers. Node built-ins only (global WebSocket
// and fetch, Node >= 22). Deliberately tiny: this is a probe, not a general CDP library.

// The agent lane arms a FIXED --remote-debugging-port, and Chrome writes DevToolsActivePort
// only for port=0, so the browser path is normally taken from /json/version instead. This
// parser exists for the port=0 / real-lane file format (line 1 port, line 2 browser path).
export function parseDevToolsActivePort(text) {
  const lines = String(text ?? '').split(/\r?\n/);
  const port = Number(lines[0]);
  const browserPath = (lines[1] ?? '').trim();
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error(`DevToolsActivePort: bad port line ${JSON.stringify(lines[0])}`);
  }
  if (!browserPath.startsWith('/devtools/browser/')) {
    throw new Error(`DevToolsActivePort: bad browser path ${JSON.stringify(browserPath)}`);
  }
  return { port, browserPath };
}

export async function getJson(port, path) {
  const res = await fetch(`http://127.0.0.1:${port}${path}`);
  if (!res.ok) throw new Error(`GET ${path} on :${port} -> HTTP ${res.status}`);
  return res.json();
}

// connect() resolves once the socket is open. Requests are matched by id; everything else
// with a `method` is an event routed to on() handlers as (params, sessionId).
export function connect(wsUrl) {
  return new Promise((resolveOpen, rejectOpen) => {
    const ws = new WebSocket(wsUrl);
    let nextId = 1;
    const pending = new Map();
    const handlers = new Map();
    const anyHandlers = [];
    let opened = false;
    let resolveClosed;
    const closed = new Promise((r) => { resolveClosed = r; });

    const failAll = (err) => {
      for (const p of pending.values()) p.reject(err);
      pending.clear();
    };

    ws.onmessage = (ev) => {
      const raw = typeof ev.data === 'string' ? ev.data : Buffer.from(ev.data).toString('utf8');
      const msg = JSON.parse(raw);
      if (msg.id !== undefined && pending.has(msg.id)) {
        const p = pending.get(msg.id);
        pending.delete(msg.id);
        if (msg.error) p.reject(new Error(`${msg.error.message} (code ${msg.error.code})`));
        else p.resolve(msg.result ?? {});
        return;
      }
      if (msg.method) {
        for (const fn of anyHandlers) fn(msg);
        const hs = handlers.get(msg.method);
        if (hs) for (const h of hs) h(msg.params ?? {}, msg.sessionId);
      }
    };

    ws.onopen = () => {
      opened = true;
      resolveOpen({
        send(method, params = {}, sessionId) {
          return sendRaw(method, params, sessionId).done;
        },
        // Fire-and-forget request whose send happens synchronously. Used for screencast acks:
        // the frame must be acknowledged before anything else is done with it.
        sendNoWait(method, params = {}, sessionId) {
          return sendRaw(method, params, sessionId);
        },
        on(method, fn) {
          if (!handlers.has(method)) handlers.set(method, []);
          handlers.get(method).push(fn);
          return () => {
            const arr = handlers.get(method) ?? [];
            const i = arr.indexOf(fn);
            if (i >= 0) arr.splice(i, 1);
          };
        },
        onAny(fn) {
          anyHandlers.push(fn);
        },
        close() {
          try { ws.close(); } catch { /* already closed */ }
          return closed;
        },
        closed,
      });
    };

    function sendRaw(method, params, sessionId) {
      const id = nextId++;
      const body = { id, method, params };
      if (sessionId) body.sessionId = sessionId;
      const sentAt = performance.now();
      const done = new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
      });
      try {
        ws.send(JSON.stringify(body));
      } catch (e) {
        pending.delete(id);
        return { id, sentAt, done: Promise.reject(e) };
      }
      return { id, sentAt, done };
    }

    ws.onerror = (ev) => {
      const err = new Error(`websocket error on ${wsUrl}: ${ev?.message ?? 'unknown'}`);
      if (!opened) rejectOpen(err);
      failAll(err);
    };
    ws.onclose = () => {
      const err = new Error(`websocket closed: ${wsUrl}`);
      if (!opened) rejectOpen(err);
      failAll(err);
      resolveClosed();
    };
  });
}

// Nearest-rank percentile. p in 0..100; sortedArr must already be ascending.
export function percentile(sortedArr, p) {
  if (!sortedArr || sortedArr.length === 0) return null;
  const rank = Math.ceil((p / 100) * sortedArr.length);
  const idx = Math.min(sortedArr.length - 1, Math.max(0, rank - 1));
  return sortedArr[idx];
}
