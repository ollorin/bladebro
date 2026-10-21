# Phase 2 sidecar results

Date 2026-10-21. Host macOS 26.6.2 arm64 (Apple M4 Pro, headed Chrome unless stated). Chrome 154.0.8037.58
(the roadmap and AGENTS.md name Chrome 151; every number below was measured on 154, not 151). ffmpeg n9.0.2
(`~/.local/ffmpeg`), bladebro v4.6.5 (source build), WebSocket transport only, a fresh scratch `BLADE_HOME`
per session, local fixtures only. The sidecar is `sidecar.mjs`: a second, independent CDP client attached to the
page through the browser WebSocket (not the daemon's own connection).

Deviations from the plan text that override it (all recorded in 02-01/02-02 summaries):
- `-fps_mode passthrough` (not `vfr`) for wallclock and concat: vfr kept only 234 of 800 frames in an isolated test.
- Endpoint discovery reads `--remote-debugging-port=N` from the main Chrome process line; `DevToolsActivePort` is not
  written on the agent lane (results/discovery.txt). The same holds for `bladebro audit`'s own browser.
- `bladebro audit` prints `SCORE: 100/100` on this platform, not the 111 in AGENTS.md (results/baseline_audit.txt).
- An idle page yields about 1 frame, so drift was measured on `tick.html` (one repaint per second) as well.

## Measurements

Sources are under `tools/record_sidecar/results/`. "cap" = drift measured against the capture window.

| metric | value | source (file:key) | note |
|---|---|---|---|
| effective fps, anim, defaults | 99.31 fps (1992 frames / 20 s) | core.json:derived.effective_fps_recv_fps_default | headed |
| fps with everyNthFrame 2 | 49.54 fps | core.json:derived.effective_fps_recv_fps_nth2 | exactly half |
| ack send time p95 | 44.9 us (60 s run: 40.1 us) | core.json:derived.ack_send_us_p95_* | |
| ack RTT p95 | 0.316 ms (60 s run: 0.293 ms) | core.json:derived.ack_rtt_ms_p95_* | |
| capture-to-receipt p95 | 2.93 ms (p50 2.46) | core.json:runs[fps_default].capture_to_recv_ms | from frame metadata.timestamp |
| drops, slow consumer (200 ms writer, queue 8) | 94.63 % (1884 of 1991 dropped at the queue) | core.json:derived.slow_consumer_drop_pct | frames received == acks sent (1991 = 1991): drops happen after the ack, never before |
| frames vs acks, 60 s | 5994 = 5994, acks_ok 5994, err 0 | core.json:derived.frames_eq_acks_60s | max gap 19 ms, stall 0 s in this run |
| screencast silence (watched item) | 1 of 7 runs of 60 s or more: 37 s of silence after 23 s, 2246 frames, 0 ack errors | deferred-items.md; stall_probe.json | the run's metrics were overwritten; two later 60 s probes did not reproduce it (5993 and 5994 frames). Cause not established |
| drift on idle page (idle.html, 1 frame) wallclock / concat / cfr | -0.273 / -0.143 / -1.902 % | core.json:derived.drift_pct_idle | measures tail hold and cfr padding only |
| drift on repainting page (tick.html) wallclock / concat / cfr | -0.355 / -0.242 / -1.902 % (cap: -0.107 / +0.040 / -1.775 %) | core.json:derived.drift_pct_tick(_capture) | all within 5 % |
| wallclock without tail | video 0.001 s for a 10 s capture (-99.99 %) | core.json:derived.drift_wallclock_notail_pct | the tail hold is required |
| 30 s tab-switch WebM | 31.982 s, 1916 frames, 3 segments (599 / 615 / 701 frames), 0 stale frames, drift -0.246 % (cap -0.144 %) | tabswitch.json:poll | headless; ack 1915 = 1915, err 0 |
| tab-switch decode | ffprobe readable, `decode_exit` 0, all 1916 frames read; strict "no error output" check FAILED: 65 warning lines | tabswitch.json:poll.probe; sample_timestamps.json | see SC1 note below |
| follow signal that worked | `poll` (first real page in /json/list): open-tab and switch-tab both tracked. `newest` (Target.targetCreated): open-tab only, switch-tab creates no target | tabswitch.json:follow_finding | |
| viewport change, fixed size mode | emulate 1001x641, window 1101x701 and 1280x800 mid-run: frame sizes 1185x720, 1124x720, 1280x714, 1280x713; output stays 1280x720; 3363 frames, 0 dropped | extra.json:viewport.fixed | strict decode failed (89 lines) |
| viewport change, even size mode | output 1280x712; frame sizes change mid-stream (1280x713 x1258, 1124x720 x666) | extra.json:viewport.even | strict decode failed (7 lines) |
| headed vs headless fps | headed 99.31 fps; headless 59.79 fps (about a 60 Hz cap; frame size 756x469) | core.json; extra.json:headless.fps_headless | |
| Xvfb | not measured because macOS host has no Xvfb; Linux Xvfb cadence must be measured on a Linux host before Phase 6 | extra.json:xvfb | |
| background tab, headed | 793 frames in the 8 s before the switch (99.13 fps), 0 frames in the following 16 s | extra.json:background.headed | |
| background tab, headless | 480 frames before (60 fps), 0 after | extra.json:background.headless | |
| minimized window | 3 frames in the 10 s minimized (0.3 fps), max gap 8976 ms; resumes about 1 s after restore | extra.json:minimized | |
| Page.enable needed? | no: the `--no-page-enable` smoke run delivered 792 frames on a settled page | smoke.json:vp9_nopage | but see the audit row: on audit's browser while it was still starting, attaching without Page.enable often failed (-32000); see the note. Send Page.enable |
| bladebro commands during capture | 8 of 8 commands rc 0 with the sidecar recording, and 8 of 8 in the baseline; click verdict, eval readback, see delta, tabs, vision (artifact written) and nav all worked; per-command time within about 0.15 s of baseline | extra.json:coexistence | click 0.78 s baseline vs 0.63 s; others within 0.03 s |
| CDP events during capture | nav completed, click verdict `dom-changed (content-only) via mouse`, see reflected the change | extra.json:coexistence.*.events_flowing | sidecar: 5987 frames, 0 stale, 1 ack error (in flight at stop) |
| codec CPU, 10 fps 1280x720, per minute | vp9 4.55 cpu-s, 870,778 B; x264 4.70 cpu-s, 535,374 B; vp8 25.74 cpu-s, 2,785,919 B | extra.json:codecs.matrix | offline re-encode of one 60 s spool |
| live 30 s at ~100 fps in, wallclock | vp9 2994 frames, 0 dropped; x264 0 dropped; vp8 2989 frames, 55 dropped, 44.5 cpu-s | extra.json:codecs.live_30s_wallclock | vp8 drops at the queue; vp9 and x264 do not |
| size per minute, vp9 | 5 fps 396,855 B; 10 fps 870,778 B; 15 fps 1,150,851 B; 960x540 at 10 fps 587,902 B; 1920x1080 at 10 fps 1,124,821 B (6.18 cpu-s) | extra.json:codecs.matrix | 1080p is an upscale from a ~1185x720 capture |
| size per minute, x264 | 5 / 10 / 15 fps: 321,869 / 535,374 / 644,429 B | extra.json:codecs.matrix | |
| CPU per minute, vp9 | 5 / 10 / 15 fps: 3.64 / 4.55 / 5.40 cpu-s | extra.json:codecs.matrix | vp8 at 5 fps varied 7.7 to 13.4 cpu-s between runs |
| audit without screencast | SCORE 100/100, exit 0 | stealth.json:audit_without | baseline 02-01 also 100/100 |
| audit with screencast attached to audit's own browser | SCORE 100/100, exit 0; sidecar received 5 frames, 5 acks ok, 0 ack errors, first frame 17 ms, attached for 5.2 s (the whole audit run) | stealth.json:audit_with_screencast | attached on attempt 1; see audit note |
| audit with screencast, no Page.enable | SCORE 100/100, exit 0; 6 frames, 6 acks ok | stealth.json:audit_with_screencast_no_page_enable | needed 2 attempts in the final run (see note) |
| audit changed by screencast | false | stealth.json:audit_changed | |
| diff_oracle | did not run: exit 2 "no Chrome/Chromium found (set CHROME_PATH)"; with CHROME_PATH set, exit 1 `ModuleNotFoundError: No module named 'websockets'` (the oracle also requires Linux + Xvfb per its README) | stealth.json:diff_oracle; diff_oracle_attempt.txt | nothing installed |
| battery A/B (substitute) | 99 keys; divergent 0; noise 1 key (`voices`, differs between two runs without any screencast) | stealth.json:battery_* | A, then B with the screencast attached, then A2. During B the sidecar received 296 to 299 frames with acks_ok equal |

Notes:
- Audit evidence is real but thin: the audit page is mostly static, so the screencast delivered 5 to 6 frames over the
  ~5 s run. It proves that attaching, `Page.enable` and `Page.startScreencast` during the audit do not change the
  vectors; it does not prove anything about a long recording during an audit. The battery check (299 frames at ~100
  fps during the probes, divergent 0) covers the high-frame-rate case for the oracle battery.
- Attaching without Page.enable during audit start: the `--no-page-enable` sidecar often exited non-zero with
  "Not attached to an active page (code -32000)". Across five scenario runs (retry loop, fresh audit per attempt) the
  console showed: succeeded on attempt 3, never within 4, never within 8, never within 8, succeeded on attempt 2
  (the stored final run). With Page.enable the first attempt succeeded in every run. Only the final run's result
  is stored (stealth.json attempts: 2); the other counts are from console output and are not in a file.
- Per-frame JPEG size: not measured because the probe records only frame dimensions, not byte size. Whether a frame
  fits under 16 KiB (see below) was therefore not measured for each fixture.

### SC1 decode check: only partial

The plan's strict decode criterion (full decode with no error output) is NOT met by any live wallclock file: the
tab-switch file has 65 warning lines (`decode_exit` 0, all 1916 frames read), core fps_default 47, acks_60s 54, live 30 s
runs 36 (vp9) / 45 (vp8) / 9 (x264). results/sample_timestamps.json explains them: the file holds 1916 packets of which 9 share
a timestamp with their neighbor (9 non-monotonic steps); every one of the 65 lines is an ffmpeg "non monotonically
increasing dts" message (plus "repeated" lines), none is a decode or corruption error. Evidence that it is a
capture-rate artifact, not a data problem: pages that repaint rarely (idle, tick, headless idle) produce 0 warning
lines for wallclock, concat and cfr, and the concat spool re-encoded at fixed 5/10/15 fps decoded with no error output
on all 11 matrix rows (playable true). What was not measured: a live capped-fps (10 fps) wallclock file on a busy page.
The verdict below counts this as a caveat, not a pass.

## In-tree approach

Facts read from `src/` (read-only):
- `src/cdp/client.rs`: `MAX_EVENT_BYTES = 16 * 1024`. In the reader, any event whose text exceeds that goes through
  `shrink_oversized_event` before reaching the broadcast bus. That function keeps identity fields for a handful of
  methods and, for every other method (the `_ => {}` arm), replaces `params` with an empty object. A
  `Page.screencastFrame` over 16 KiB (any non-trivial 1280x720 JPEG once base64 encoded) would arrive with its `data`
  and its `sessionId` stripped, which defeats both recording and the ack. This applies to ANY `CdpClient`, so a second
  client in-tree has the same problem as the shared one. ROADMAP Phase 6 SC3 ("a 2 MB frame passes through
  `shrink_oversized_event` unchanged") is therefore a real requirement, not a formality: the screencast method needs an
  exemption (with its own size cap) or must be taken before the shrink in the reader.
- `EVENT_BUS_CAPACITY = 16384` on a `tokio::sync::broadcast`. At the measured ~99 fps (headed) that is about 165 s of
  frames, but each slot would hold a full frame; slow subscribers get `Lagged` and skip. The comment on the constant
  says a dropped attach event leaves a target paused forever. Putting 100 fps of large frames on the bus that
  attach/dialog/download/network consumers share is the wrong place to absorb a slow encoder.
- `src/cdp/session.rs`: WS mode uses a direct page WebSocket per `Page` (root session, no filtering); pipe mode uses
  flat child sessions of the browser connection. A "second client" means a second WS connection in WS mode, but in pipe
  mode only a second flat session on the same connection is possible (the sidecar probe itself needs the browser
  WebSocket and so cannot run on `BLADE_TRANSPORT=pipe`).
- `src/page/tabs.rs`: `switch_tab` builds a new `Page` via `attach_tab_target`, activates it, then assigns
  `*self = new_page`; the old `Page` is dropped and its background tasks aborted. Anything stored inside `Page` is lost
  at every switch.

Choice: do NOT hang the recorder on the existing `Page` session or its event bus. Run it as a separate recorder
task that owns its own CDP attachment (a second WS client in WS mode; a second flat child session in pipe mode, both
built the same way `attach_tab_target` builds one), lives outside `Page`, and re-binds by target id when a switch
or open-tab is observed. Measurements support this: the independent sidecar ran next to the daemon with 8 of 8
commands unchanged, acks 5994 = 5994, and `poll` follow re-bound 3 segments with a 7 to 10 ms switch (tabswitch.json).
It contradicts the roadmap's stated preference for the existing `Page` session on three counts: the 16 KiB strip,
the shared bus, and `switch_tab` replacing `Page`. Either way, `shrink_oversized_event` needs an exemption for
`Page.screencastFrame` in the recorder's reader (an src change for Phase 6). Not measured: any in-tree
implementation; this is reasoning from the code plus the out-of-process probe.

## Recommended defaults for Phase 3

| setting | value | justification |
|---|---|---|
| codec | WebM VP9 (default); x264/MP4 opt-in; VP8 not offered | vp9 4.55 cpu-s/min and 870,778 B/min at 10 fps 1280x720, 0 live drops; x264 4.70 cpu-s, 535,374 B, 0 drops; vp8 25.74 cpu-s, 55 live drops. Resolves D-04 (VP8 fallback dropped) |
| fps cap | 10 fps (range 5 to 15) | vp9 3.64 / 4.55 / 5.40 cpu-s/min at 5/10/15 fps; Chrome delivers ~100 fps headed, ~60 headless, so the cap is applied by dropping frames in the recorder (cfr run: 793 received, 79 written at 10 fps). Resolves D-05 fps |
| max frame size | 1280 x 720 (maxWidth x maxHeight), fixed output size | 1080p costs 6.18 cpu-s/min and 1,124,821 B vs 870,778 B at 1280x720; 960x540 587,902 B. Fixed mode kept one output size through four viewport changes; even mode changes output height |
| JPEG quality | 70 | the quality used in every run here; q60 to 80 was not compared, so this is unproven |
| timestamp strategy | frame durations from `metadata.timestamp` at the capped fps (concat) with a held final frame; wallclock as fallback | all three within 5 % (concat -0.242 % / +0.040 % cap, wallclock -0.355 %, cfr -1.902 %); concat spool decoded clean on all 11 rows; wallclock gave dts warnings at ~100 fps input; vfr rejected (234 of 800 frames kept) and passthrough used. Resolves D-06. Needs a disk spool (concat finalize was the slowest, 562 ms) |
| tail hold | keep the final frame to the stop time | without it a 10 s capture became 0.001 s |
| queue length | 16 (8 under test) with drop-at-queue, ack always first | acks 5994 = 5994 under 94.63 % queue drops; first-frame 8 to 43 ms |
| size mode | fixed | see max frame size |
| follow signal | poll the page list (first real page) plus the recorder's own bookkeeping, not `Target.targetCreated` alone | `newest` misses switch-tab |
| Page.enable | send it | attach without it failed on a starting page |

## Gate verdict

PASS-WITH-CAVEATS

Rule applied (hard criteria, all must hold): tab-switch WebM readable, >= 30 s and >= 2 non-empty segments (31.98 s,
3 segments, 1916 frames, decode exit 0); frames == acks over 60 s with 0 ack errors (5994 = 5994); at least one
timestamp strategy within 5 % on idle (concat -0.143 %, wallclock -0.273 %); audit unchanged with an attached,
frame-producing screencast (100/100 both, 5 to 6 frames); battery divergent == 0 (0 of 99 keys); bladebro commands kept working
(8 of 8). All hold, so not FAIL. Not a plain PASS because of these caveats:
1. SC1 strict decode: 65 dts warning lines on the tab-switch file and on every live wallclock file at ~100 fps (see SC1
   note); the file plays and all frames are read, but the strict wording of the plan is not met.
2. diff_oracle did not run on macOS (no Chrome path by default, `websockets` missing, Linux + Xvfb required); a
   substitute battery diff was run instead. The real oracle must run on Linux CI before Phase 6 (`divergent: 0`).
3. Xvfb cadence not measured (macOS host). Re-measure on Linux before Phase 6.
4. Background tabs and minimized windows deliver no frames (0 frames in 16 s; 0.3 fps minimized). Recording follows the
   foreground tab only.
5. One unexplained 37 s screencast silence in 1 of 7 long runs; not reproduced in 2 probes (cause unknown).
6. Audit-with-screencast evidence is 5 to 6 frames; the 111-vector figure in AGENTS.md is 100 on this platform.
7. Chrome 154 baseline, not 151.

## Open items for later phases

- Phase 6: exempt `Page.screencastFrame` from `shrink_oversized_event` with its own cap; test a 2 MB frame.
- Phase 6: add a no-frames watchdog; the 37 s silence shows a live recording can go quiet with acks healthy.
- Re-run `diff_oracle` (agent and real lanes), the Xvfb cadence and audit on Linux CI; Windows not measured.
- Choose and test the capped-fps path live: concat spool vs capped wallclock on a busy page, strict decode.
- Switch handling: `poll` works in the probe; the in-tree rebind rule (page list order) needs a real definition.
- Decide the product behavior for backgrounded or minimized recorded tabs (frozen content, honest note).
- Measure per-frame JPEG bytes and Chrome-side CPU of screencast at the capped rate (not measured here).
- Compare JPEG q60/70/80 (only 70 was run).
- Concat spool lives on disk: location, 0700 permissions and cleanup belong in Phase 3.
- The 111 vs 100 audit count difference is unexplained here (platform-specific vectors presumed, not verified).
