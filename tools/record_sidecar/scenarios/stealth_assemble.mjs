// Builds results/stealth.json from the audit files, sidecar stats and battery classification.
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
const RS = process.env.RS, S = process.env.SCRATCH, R = `${RS}/results`;
const rd = (p) => (existsSync(p) ? readFileSync(p, 'utf8') : null);
const audit = (p) => {
  const t = rd(p);
  if (t === null) return { not_measured_because: `${p} missing` };
  const m = t.match(/SCORE:\s*(\d+)\/(\d+)/), e = t.match(/audit_exit=(\d+)/), v = t.match(/VERDICT:\s*(\w+)/);
  return { score: m ? `${m[1]}/${m[2]}` : null, exit: e ? Number(e[1]) : null, verdict: v ? v[1] : null };
};
const side = (tag) => {
  const t = rd(`${S}/audit_${tag}.stats.json`);
  if (t === null) return null;
  const s = JSON.parse(t);
  const buckets = s.fps_buckets || [];
  return {
    frames_received: s.frames_received, acks_sent: s.acks_sent, acks_ok: s.acks_ok, acks_err: s.acks_err,
    first_frame_ms: s.first_frame_ms, stop_reason: s.stop_reason, wall_ms: s.wall_ms,
    segments: (s.segments || []).length, nonempty_buckets: buckets.filter((b) => b > 0).length, bucket_count: buckets.length,
  };
};
const without = audit(`${R}/audit_without_screencast.txt`);
const att = (t) => Number((rd(`${S}/attempts_${t}`) || '0').trim());
const withP = { ...audit(`${R}/audit_with_screencast.txt`), attempts: att('page'), sidecar: side('page') };
const withN = existsSync(`${R}/audit_with_screencast_no_page_enable.txt`)
  ? { ...audit(`${R}/audit_with_screencast_no_page_enable.txt`), attempts: att('nopage'), sidecar: side('nopage'), last_sidecar_error: side('nopage') ? null : lastErr('nopage') }
  : { not_measured_because: 'scenario did not produce the file' };
const lastErr = (t) => (rd(`${S}/last_err_${t}.txt`) || '').trim().split('\n').pop();
const oracleTxt = rd(`${R}/diff_oracle_attempt.txt`) || '';
const oexit = (oracleTxt.match(/exit=(\d+)\s*$/) || [])[1];
const first = (oracleTxt.split('\n').find((l) => /ERROR/.test(l)) || '').trim();
const errs = oracleTxt.split('\n').filter((l) => /ERROR|ModuleNotFoundError/.test(l)).map((l) => l.trim());
const reason = errs.join(' | ');
const ran = /DIVERGENT:|divergent:|expected:/i.test(oracleTxt) && !/Traceback|ERROR/.test(oracleTxt);
const exits = [...oracleTxt.matchAll(/^exit=(\d+)/gm)].map((m) => Number(m[1]));
const cls = JSON.parse(rd(`${S}/classify.json`));
const proven = withP.sidecar && withP.sidecar.frames_received > 0;
writeFileSync(`${R}/stealth.json`, JSON.stringify({
  audit_baseline_02_01: audit(`${R}/baseline_audit.txt`),
  audit_without: without,
  audit_with_screencast: withP,
  audit_with_screencast_no_page_enable: withN,
  screencast_proven_active_during_audit: !!proven,
  audit_changed: withP.score !== without.score || withP.exit !== without.exit,
  diff_oracle: { ran, exits, reason },
  battery_page: 'anim.html', battery_sidecar_during_B: (() => { const t = rd(`${S}/bat.stats.json`); if (!t) return null; const x = JSON.parse(t); return { frames_received: x.frames_received, acks_ok: x.acks_ok, acks_err: x.acks_err, stop_reason: x.stop_reason, wall_ms: x.wall_ms }; })(),
  battery_keys: cls.keys, battery_divergent: cls.divergent, battery_noise: cls.noise,
}, null, 2));
