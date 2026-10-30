---
phase: 02-sidecar-prototype-gate
plan: 02
subsystem: testing
tags: [cdp, screencast, ffmpeg, measurement, tab-switch, encode-cost]

requires:
  - phase: 02-sidecar-prototype-gate
    provides: 02-01 recorder (sidecar.mjs, lib/encoder.mjs), smoke and baseline
provides:
  - Tab-following, attach-wait and harness-only script actions in the sidecar
  - measure.mjs (probe / merge / encode-bench) with unit tests
  - results/tabswitch.json, core.json, extra.json, stall_probe.json (every number from a run)
affects: [02-03 verdict]

tech-stack:
  added: []  # Node built-ins only; ffmpeg n9.0.2 via BLADE_FFMPEG
  patterns:
    - "Measurements are written by merge/derive scripts from run stats, never typed"
    - "Fresh BLADE_HOME and full teardown per browser session"

key-files:
  created:
    - tools/record_sidecar/lib/follow.mjs
    - tools/record_sidecar/lib/actions.mjs
    - tools/record_sidecar/lib/attach.mjs
    - tools/record_sidecar/measure.mjs
    - tools/record_sidecar/scenarios/{tabswitch,core,core_derive.mjs,extra,stall_probe}.sh|mjs
    - tools/record_sidecar/fixtures/tick.html
    - tools/record_sidecar/test/{follow,measure}.test.mjs
    - tools/record_sidecar/results/{tabswitch,core,extra,stall_probe}.json
    - tools/record_sidecar/results/tabswitch_sample.webm
  modified:
    - tools/record_sidecar/sidecar.mjs

key-decisions:
  - "Poll follow (first real page in /json/list) tracked both open-tab and switch-tab; newest tracks only open-tab (switch-tab creates no target)"
  - "Background and minimized targets deliver no frames: recording a non-foreground tab is not a viable product behavior without a separate answer"
  - "Strict decode (no error-level output) fails on every live wallclock file; decode exit is 0 and frames are all present. Left for the 02-03 verdict (see deferred-items.md)"

patterns-established:
  - "Derived blocks are computed by a script from the recorded runs (core_derive.mjs), so they can be recomputed"

requirements-completed: []  # VER-01 is verdict-level; the verdict is 02-03

duration: not tracked per task (browser sessions and reruns dominate)
completed: 2026-10-21
---

# Phase 2 Plan 02: Tab-following and Measurement Summary

**Tab-following sidecar and every Phase 2 measurement run to JSON. SC1 meets its duration, segment and drift criteria but not the strict decode criterion. SC2 and the SC3 coexistence half are met (Xvfb not measured). The strict decode check fails on every live wallclock file, and background and minimized targets stop delivering frames.**

## Performance

- **Tasks:** 4 of 4, plus follow-up reruns that fixed measurement defects found along the way
- **Commits:** 7 (listed below)
- **Browser sessions:** roughly 20 headed or headless sessions across all runs including reruns; each <= 2 min and torn down before the next
- **Files changed:** only under `tools/record_sidecar/` and `.planning/`

## Accomplishments

- Tab follow works live: `--follow poll` recorded a 31.98 s WebM across open-tab and switch-tab (segments a, b, a), 0 stale frames.
- `measure.mjs` (probe / merge / encode-bench) and 35 unit tests, all passing (`node --test tools/record_sidecar/test/*.test.mjs`).
- Core fps, ack latency, slow consumer, 60 s frames vs acks, and drift on a page that actually repaints (tick.html) were measured.
- Extra: viewport change, headless vs headed cadence, background and minimized behavior, codec CPU and size matrix, live codec runs, and bladebro coexistence (8 of 8 commands rc 0 with the sidecar running, and in the baseline).

## Derived numbers

Copied from `results/core.json` `derived` (headed, WS transport, Chrome 154.0.8037.58, ffmpeg n9.0.2, macOS 26.6.2 arm64).

| Measure | Value |
|---|---|
| Effective fps, fps_default (anim, 20 s, vp9 wallclock) | 99.31 |
| Effective fps, every-nth 2 | 49.54 |
| Ack send p95, fps_default / 60 s | 44.9 us / 40.1 us |
| Ack RTT p95, fps_default / 60 s | 0.316 ms / 0.293 ms |
| Slow consumer (200 ms writer, queue 8): drop | 94.63 % (1884 of 1991 received) |
| 60 s run: frames received vs acks sent | 5994 = 5994, acks ok 5994, err 0 |
| 60 s run: stall seconds (this run) | 0 (max gap 19 ms); see "Screencast silence" |
| Drift on tick.html, vs wall_ms: wallclock / concat / cfr | -0.355 / -0.242 / -1.902 % (all within 5 %) |
| Drift on tick.html, vs capture window: wallclock / concat / cfr | -0.107 / +0.040 / -1.775 % |
| Drift on idle.html (1 frame per run; tail hold and cfr padding only) | -0.273 / -0.143 / -1.902 % |
| Wallclock without tail | video 0.001 s for a 10 s capture (drift -99.99 %) |

From `results/extra.json`:

| Section | Result |
|---|---|
| Viewport fixed (emulate 1001x641, window 1101x701 and 1280x800) | 3363 frames, 4 script events all ok, frame sizes 1185x720 / 1124x720 / 1280x714 / 1280x713, output stays 1280x720; strict decode false (89 warning lines) |
| Viewport even | 1924 frames, output 1280x712, frame sizes change mid-stream (1280x713 x1258, 1124x720 x666); 7 warning lines |
| Headless cadence | 59.79 fps (about 60 Hz cap) vs 99.31 headed; idle.html 1 frame; tick.html 21 frames in 20 s, drift -0.463 % |
| Background: headed | 793 frames in the 8 s before the switch (99.13 fps), 0 frames in the 16 s after |
| Background: headless | 480 frames before the switch (60 fps), 0 frames after |
| Minimized (8 s to 18 s) | 3 frames in 10 s (0.3 fps); max gap 8976 ms; frames resume at restore |
| Codec CPU and size, 60 s spool, 10 fps 1280x720 | vp9 4.55 cpu-s/min, 870,778 B/min; x264 4.70 cpu-s/min, 535,374 B/min; vp8 25.74 cpu-s/min, 2,785,919 B/min |
| Codec matrix (11 rows: vp9/vp8/x264 at 5/10/15 fps, vp9 at 960x540 and 1920x1080) | see `codecs.matrix` in extra.json; vp9 1080p 6.18 cpu-s/min (an upscale from the ~1185x720 capture) |
| Live 30 s, wallclock, headed | vp9 2994 frames, 0 dropped, CPU 9.8 + 2.2 s; vp8 2989 frames, 55 dropped, CPU 44.5 + 0.9 s; x264 2990 frames, 0 dropped, CPU 10.8 + 2.0 s |
| Coexistence (sidecar running vs baseline) | Both runs: 8 of 8 commands rc 0, eval readback "clicked", click verdict "outcome: dom-changed (content-only) via mouse". Command elapsed times agree within about 0.15 s between runs (click 0.78 s vs 0.63 s; the rest within 0.03 s). Sidecar: 5987 frames over 60 s, 0 stale, 1 ack error (in flight at stop). fps 92.5 in the 2 s before a same-tab nav, 102 after |

Tab switch (`results/tabswitch.json`, headless, `poll` run):

| Measure | Value |
|---|---|
| Duration | 31.98 s (wall_ms 32061) |
| Segments (frames, switch_ms) | a.html 599 / b.html 615 (switch 7 ms) / a.html 701 (switch 10 ms) |
| Drift vs wall / vs capture window | -0.246 % / -0.144 % |
| Stale frames, acks | 0, 1915 sent / 1915 ok / 0 err |
| Decode exit / warning lines | 0 / 65 (strict "no errors" check: FAIL) |

## follow_finding (copied verbatim from tabswitch.json)

`poll: open-tab tracked=true, switch-tab tracked=true, segment urls=a.html>b.html>a.html | newest: open-tab tracked=true, switch-tab tracked=false, segment urls=a.html>b.html`

Interpretation: poll tracked both transitions, because the active page is listed first in `/json/list` while it is current (observed in both runs). newest tracks only `open-tab`, because `switch-tab` creates no target and so emits no targetCreated event. This is a limit of newest and the reason poll is the candidate follow mode for 02-03.

## Success criteria status

- **SC1 (tabswitch)**: met on duration (31.98 s >= 30), segments (3, all non-zero, poll), drift (-0.25 % <= 5 %), and ffprobe readable. **Not met** on the strict "full decode with no errors" wording: decode exit is 0, but 65 warning lines remain. Logged in deferred-items.md. This is the main question for 02-03.
- **SC2 (core)**: effective fps, ack latency, drops under slow consumer, 60 s frames vs acks, and drift all recorded. Xvfb: `not measured because macOS host has no Xvfb`. The 60 s stall is recorded (see below).
- **SC2 (extra)**: viewport change, headless vs headed cadence, background and minimized frames, and codec CPU and size all recorded.
- **SC3 (coexistence half)**: `kept_working` = true in both runs.
- `node --test tools/record_sidecar/test/*.test.mjs`: 35 of 35 pass.

## Screencast silence (watched item from 02-01)

- One of 7 recordings of 60 s or more on anim.html stalled: 2246 frames, then none for 37 s, with 0 ack errors and the sidecar still attached. The stats for that run were overwritten when core.sh was rerun; this summary is the record. The 02-01 smoke also had a one-frame stall; not reproduced here.
- Two 60 s probe runs, with the page's own counter sampled every 5 s, did not reproduce it (5993 and 5994 frames). Cause not established. Logged in deferred-items.md.
- The silence counter was initially wrong: it counted gaps between frames, not silence after the last frame. Fixed in `stallSeconds` (test added).

## Deviations from the plan

1. **vfr vs passthrough.** The plan text says `-fps_mode vfr` for wallclock and concat. The 02-01 override replaced it with `-fps_mode passthrough`, and every 02-02 run used passthrough. Both are reported here: plan said vfr, run used passthrough.
2. **Headless for tabswitch.** The plan said headed. Per the user's preference, tabswitch ran headless (`BB_HEADED=1` reruns it headed). Core ran headed, as the plan's reference cadence. Extra ran headed except the headless cadence and background sections.
3. **Discovery always on.** `Target.setDiscoverTargets` is sent in every mode, not only when follow is on. A follow-off run needs targetDestroyed to learn that its page closed.
4. **Actions in lib/actions.mjs.** The plan's verify greps `sidecar.mjs` for `setWindowBounds`. The plan's own text puts the action runner in `lib/actions.mjs`, which is where it is.
5. **Idle drift replaced by tick.** idle.html delivers about one frame per run, so its drift only shows tail hold and cfr padding. tick.html (one repaint per second) gives real frames over time, and the drift comparison uses it. The idle runs are kept and labelled.
6. **Encode count.** The plan text says 12 encodes; the listed matrix is 11 (3 codecs x 3 fps + 2 sizes). 11 were run.
7. **Two timing bases.** `wall_ms` includes encoder finalize. The plan's drift is kept against wall_ms, and `drift_pct_capture` is added (against stop_begin_ms, capture only). It matters: under backpressure the slow-consumer run reads +7.07 % on the capture basis, because wallclock stamps are taken when ffmpeg reads a frame, and the queue drained after stop adds about 1.4 s.
8. **Background and minimized behavior** were not in the plan's expected findings. They are measured and reported (see Accomplishments and deferred-items.md).

## Defects found and fixed during execution (each rerun from scratch)

- **Shared BLADE_HOME leaked a browser between runs.** The first newest pass showed a second b.html target. Fixed: a fresh BLADE_HOME and a full teardown per session. The reported tabswitch.json is from the fixed runs.
- **Trailing silence was not counted** in the derived stall figure (fixed; test added).
- **Vision artifact was checked after teardown** (the file was already removed), so existence read false. Fixed: checked inside the step. Both runs now read true.
- **Background seconds-after were counted from the bucket array** (read 0 instead of 16). Fixed to wall time.
- **Coexistence fps window divided by 5 when only 2 s existed.** Fixed; averages real seconds only.
- **Parser for `act eval`** expected bare JSON but the output is `result: "<json>"`. Fixed in stall_probe.sh; its first output was discarded.
- **A bash quoting error** (an apostrophe in a comment inside a `node -e` block) stopped one run before any command ran; corrected and rerun.

## Task Commits

1. Task 1, sidecar tab-following, attach-wait, script actions: `13b2e37` (feat)
2. Task 2, follow/measure tests, measure.mjs, tabswitch scenario: `be8f86c` (test)
3. Task 3, core measurements (+ stop_begin, derive, tick fixture): `2106508` (feat)
4. Task 4, extra measurements: `223d345` (feat); corrected in `0c97ea5` (fix)
5. Stall probe: `95183a5` (test)
6. SC1 rerun with capture drift: `e332255` (test)
7. This summary and deferred-items.md: see the metadata commit

## Open for 02-03

- Strict decode on wallclock output (dts warnings at ~100 fps). The concat path at a fixed fps (`encode-bench`, 11 rows) decodes with no error output (playable true on every row), so the warnings come from live wallclock stamping, not from the frames.
- Background and minimized targets produce no frames, so a session recorder cannot follow a tab that is not in the foreground without a different capture approach.
- vp8 cannot keep up live at ~100 fps (55 drops in 30 s, 44.5 s CPU) and costs 25 cpu-s/min offline. vp9 and x264 cost about 4.6 to 4.7 cpu-s/min at 10 fps 1280x720 and drop nothing live.
- vp8 offline CPU at 5 fps varied between runs (7.7, 12.6, 13.4 cpu-s/min) because each run captured a new spool. The vp9 and x264 rows were stable (within about 0.1 cpu-s/min at 10 fps).

## Telemetry

context_pressure=0.6, instructions_not_followed=0, ambiguities=4 (vfr vs passthrough wording, headed vs headless for tabswitch, 12 vs 11 encodes, idle drift), tool_errors_swallowed=0 (every error was surfaced and fixed or rerun; see "Defects found and fixed").

## Self-Check

- Files: all created files listed in key-files exist (checked by `git status` and the commits above).
- Commits: 13b2e37, be8f86c, 2106508, 223d345, 95183a5, e332255, 0c97ea5 present in `git log`.
- No processes left: no `bladebro daemon`, no scratch Chrome, no ffmpeg from this plan after the last session (verified after each run).
- No changes to `src/`, `tests/`, `npm/`, `Cargo.*`, `CHANGELOG.md`, `.github/`, `ROADMAP.md`, `STATE.md`. `.planning/EXECUTION_LOG.md` not committed.
