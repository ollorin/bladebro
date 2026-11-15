// Read-only page audit. Expression form: `(CFG) => Promise<result>`, called by
// audit.rs with {min_target, links, max_links}. Dependency-free; mutates nothing.
async (CFG) => {
  const MAXN = 6000, CAP = 500;
  const doc = document, de = doc.documentElement;
  const all = [...doc.querySelectorAll('*')].slice(0, MAXN);
  const cs = (e) => getComputedStyle(e);
  const visible = (e) => {
    const s = cs(e);
    if (s.display === 'none' || s.visibility === 'hidden' || s.visibility === 'collapse') return false;
    const r = e.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  };
  const esc = (v) => (window.CSS && CSS.escape ? CSS.escape(v) : String(v).replace(/[^\w-]/g, '\\$&'));
  const sel = (e) => {
    if (e.id && doc.querySelectorAll('#' + esc(e.id)).length === 1) return '#' + esc(e.id);
    const parts = [];
    for (let n = e, d = 0; n && n.nodeType === 1 && n !== de && d < 4; n = n.parentElement, d++) {
      let p = n.localName;
      if (n.id && doc.querySelectorAll('#' + esc(n.id)).length === 1) { parts.unshift('#' + esc(n.id)); break; }
      const cls = [...n.classList].slice(0, 2).map((c) => '.' + esc(c)).join('');
      p += cls;
      const sib = n.parentElement ? [...n.parentElement.children].filter((x) => x.localName === n.localName) : [];
      if (sib.length > 1) p += ':nth-of-type(' + (sib.indexOf(n) + 1) + ')';
      parts.unshift(p);
    }
    return parts.join(' > ');
  };
  const out = { contrast: [], names: [], duplicate_ids: [], overflow: [], targets: [] };
  const counts = { contrast: 0, names: 0, duplicate_ids: 0, overflow: 0, targets: 0 };
  const add = (k, f) => { counts[k]++; if (out[k].length < CAP) out[k].push(f); };

  // ---- contrast
  const parseC = (s) => {
    const m = /^rgba?\(([^)]+)\)$/.exec((s || '').trim());
    if (!m) return null;
    const p = m[1].split(/[\s,\/]+/).filter(Boolean).map(parseFloat);
    if (p.length < 3 || p.some((x) => isNaN(x))) return null;
    return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 };
  };
  const over = (f, b) => ({ r: f.r * f.a + b.r * (1 - f.a), g: f.g * f.a + b.g * (1 - f.a), b: f.b * f.a + b.b * (1 - f.a), a: 1 });
  const lum = (c) => {
    const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
    return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
  };
  const hex = (c) => '#' + [c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');
  const bgOf = (e) => {
    const layers = [];
    for (let n = e; n && n.nodeType === 1; n = n.parentElement) {
      const s = cs(n);
      if (s.backgroundImage !== 'none') return null;
      const c = parseC(s.backgroundColor);
      if (c && c.a > 0) { layers.push(c); if (c.a >= 1) break; }
    }
    let acc = { r: 255, g: 255, b: 255, a: 1 };
    for (let i = layers.length - 1; i >= 0; i--) acc = over(layers[i], acc);
    return acc;
  };
  let checked = 0;
  for (const e of all) {
    if (checked > 3000) break;
    let has = false;
    for (const n of e.childNodes) if (n.nodeType === 3 && n.nodeValue.trim()) { has = true; break; }
    if (!has || !visible(e)) continue;
    const s = cs(e);
    const fg0 = parseC(s.color), bg = bgOf(e);
    if (!fg0 || !bg || fg0.a === 0) continue;
    checked++;
    const fg = over(fg0, bg);
    const l1 = lum(fg), l2 = lum(bg);
    const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    const px = parseFloat(s.fontSize), bold = parseInt(s.fontWeight, 10) >= 700;
    const large = px >= 24 || (px >= 18.66 && bold);
    const need = large ? 3 : 4.5;
    if (ratio < need) add('contrast', { selector: sel(e), reason: 'contrast ' + ratio.toFixed(2) + ':1 < ' + need + ':1 (' + hex(fg) + ' on ' + hex(bg) + ', ' + px + 'px)' });
  }

  // ---- names
  const txt = (e) => (e.textContent || '').replace(/\s+/g, ' ').trim();
  const accName = (e) => {
    const lb = e.getAttribute('aria-labelledby');
    if (lb) { const t = lb.split(/\s+/).map((i) => { const x = doc.getElementById(i); return x ? txt(x) : ''; }).join(' ').trim(); if (t) return t; }
    const al = (e.getAttribute('aria-label') || '').trim();
    if (al) return al;
    const tag = e.localName, type = (e.getAttribute('type') || '').toLowerCase();
    if (tag === 'input' && ['submit', 'button', 'reset'].includes(type)) return (e.value || (type === 'submit' ? 'Submit' : type === 'reset' ? 'Reset' : '')).trim();
    if (tag === 'input' && type === 'image') return (e.getAttribute('alt') || '').trim();
    if (['input', 'select', 'textarea'].includes(tag)) {
      const t = [...(e.labels || [])].map(txt).join(' ').trim();
      if (t) return t;
    } else {
      const t = txt(e);
      if (t) return t;
      const im = e.querySelector('img[alt]:not([alt=""])'); if (im) return im.getAttribute('alt').trim();
      const sv = e.querySelector('svg title'); if (sv && txt(sv)) return txt(sv);
      const ia = e.querySelector('[aria-label]'); if (ia) return ia.getAttribute('aria-label').trim();
    }
    return (e.getAttribute('title') || '').trim();
  };
  if (!(de.getAttribute('lang') || '').trim()) add('names', { selector: 'html', reason: 'missing lang attribute' });
  if (!doc.title.trim()) add('names', { selector: 'head', reason: 'document has no <title>' });
  for (const e of all) {
    const tag = e.localName, role = (e.getAttribute('role') || '').toLowerCase();
    if (e.closest('[aria-hidden="true"]')) continue;
    if (tag === 'img' && !e.hasAttribute('alt') && role !== 'presentation' && role !== 'none') {
      if (visible(e)) add('names', { selector: sel(e), reason: 'img missing alt attribute' });
      continue;
    }
    const widget = tag === 'button' || (tag === 'a' && e.hasAttribute('href')) || tag === 'select' || tag === 'textarea' ||
      (tag === 'input' && (e.getAttribute('type') || '').toLowerCase() !== 'hidden') ||
      ['button', 'link', 'checkbox', 'radio', 'tab', 'menuitem', 'switch', 'textbox', 'combobox', 'img'].includes(role);
    if (!widget || !visible(e)) continue;
    if (!accName(e)) {
      const ph = tag === 'input' || tag === 'textarea' ? (e.getAttribute('placeholder') || '').trim() : '';
      add('names', { selector: sel(e), reason: (role || tag) + ' has no accessible name' + (ph ? ' (placeholder "' + ph.slice(0, 30) + '" is not a label)' : '') });
    }
  }

  // ---- duplicate ids
  const seen = new Map();
  for (const e of doc.querySelectorAll('[id]')) if (e.id) seen.set(e.id, (seen.get(e.id) || 0) + 1);
  for (const [id, n] of seen) if (n > 1) add('duplicate_ids', { selector: '#' + esc(id), reason: 'id used ' + n + ' times' });

  // ---- horizontal overflow
  const cw = de.clientWidth, sw = de.scrollWidth;
  const viewport = { width: innerWidth, height: innerHeight, client_width: cw, scroll_width: sw, dpr: devicePixelRatio };
  const doc_overflow = sw > cw + 1;
  const off = new Set();
  const clipped = (e) => {
    for (let n = e.parentElement; n && n !== doc.body && n !== de; n = n.parentElement) {
      const o = cs(n).overflowX;
      if (o !== 'visible') return true;
    }
    return false;
  };
  const cand = [];
  for (const e of all) {
    if (e === doc.body) continue;
    const s = cs(e);
    if (s.position === 'fixed' || s.display === 'none' || s.visibility === 'hidden') continue;
    const r = e.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    if ((r.right > cw + 1 || r.left < -1) && !clipped(e)) { off.add(e); cand.push([e, r]); }
  }
  const offenders = cand.filter(([e]) => !(e.parentElement && off.has(e.parentElement)))
    .map(([e, r]) => ({ e, by: Math.max(r.right - cw, -r.left) })).sort((a, b) => b.by - a.by);
  if (doc_overflow || offenders.length) {
    counts.overflow = offenders.length + (doc_overflow ? 1 : 0);
    if (doc_overflow) out.overflow.push({ selector: 'html', reason: 'document scrollWidth ' + sw + ' > viewport width ' + cw + ' (horizontal scroll)' });
    for (const o of offenders.slice(0, CAP)) out.overflow.push({ selector: sel(o.e), reason: 'extends ' + Math.round(o.by) + 'px past the viewport edge (' + Math.round(o.e.getBoundingClientRect().width) + 'px wide)' });
  }

  // ---- touch targets
  const min = CFG.min_target;
  const TSEL = 'a[href],button,input:not([type=hidden]),select,textarea,summary,[role=button],[role=link],[role=checkbox],[role=radio],[role=tab],[role=switch],[role=menuitem],[onclick],[tabindex]:not([tabindex="-1"])';
  const tgs = [...doc.querySelectorAll(TSEL)].filter((e) => !e.disabled && visible(e)).slice(0, 2000);
  const rects = tgs.map((e) => e.getBoundingClientRect());
  tgs.forEach((e, i) => {
    const r = rects[i];
    if (r.width >= min && r.height >= min) return;
    const s = cs(e);
    if (e.localName === 'a' && s.display === 'inline' && e.parentElement && txt(e.parentElement).length > txt(e).length + 8) return; // WCAG 2.5.8 inline exception
    if (e.localName === 'input' && ['checkbox', 'radio'].includes(e.type) && [...(e.labels || [])].some((l) => { const lr = l.getBoundingClientRect(); return lr.width >= min && lr.height >= min; })) return;
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2, rad = min / 2;
    const crowded = rects.some((o, j) => {
      if (j === i) return false;
      const dx = Math.max(o.left - cx, 0, cx - o.right), dy = Math.max(o.top - cy, 0, cy - o.bottom);
      return Math.hypot(dx, dy) < rad;
    });
    if (!crowded) return; // WCAG 2.5.8 spacing exception: a clear min-size circle around the target
    add('targets', { selector: sel(e), reason: Math.round(r.width) + 'x' + Math.round(r.height) + 'px < ' + min + 'px, other targets within ' + rad + 'px' });
  });

  // ---- links
  const links = { mode: CFG.links, checked: 0, broken: [], skipped: 0, cross_origin: [], note: '' };
  if (CFG.links !== 'off') {
    if (location.protocol !== 'http:' && location.protocol !== 'https:') {
      links.note = 'link check needs an http(s) page (this is ' + location.protocol + ')';
    } else {
      const seenU = new Set(), same = [], cross = [];
      for (const a of doc.querySelectorAll('a[href]')) {
        let u; try { u = new URL(a.getAttribute('href'), location.href); } catch (_) { continue; }
        if (u.protocol !== 'http:' && u.protocol !== 'https:') continue;
        u.hash = '';
        if (seenU.has(u.href)) continue;
        seenU.add(u.href);
        (u.origin === location.origin ? same : cross).push({ url: u.href, selector: sel(a) });
      }
      const pick = same.slice(0, CFG.max_links);
      links.skipped = same.length - pick.length;
      if (CFG.links === 'all') links.cross_origin = cross.slice(0, CFG.max_links);
      const probe = async (l) => {
        const run = async (method) => {
          const ac = new AbortController(); const t = setTimeout(() => ac.abort(), 6000);
          try { return await fetch(l.url, { method, redirect: 'follow', signal: ac.signal, credentials: 'same-origin' }); } finally { clearTimeout(t); }
        };
        try {
          let r = await run('HEAD');
          if (r.status === 405 || r.status === 501) r = await run('GET');
          links.checked++;
          if (r.status >= 400) links.broken.push({ url: l.url, selector: l.selector, status: r.status });
        } catch (err) {
          links.checked++;
          links.broken.push({ url: l.url, selector: l.selector, status: 0, error: String((err && err.name) || err) });
        }
      };
      const q = pick.slice();
      await Promise.all(Array.from({ length: 5 }, async () => { while (q.length) await probe(q.shift()); }));
    }
  }
  return { url: location.href, viewport, counts, findings: out, links };
}
