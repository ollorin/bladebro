---
phase: 02-sidecar-prototype-gate
plan: 01
subsystem: testing
tags: [cdp, screencast, ffmpeg, node, probe, recording]

requires:
  - phase: 01-decisions-and-upstream-issue
    provides: DECISIONS.md D-03..D-06 (the recording path under test)
provides:
  - Standalone CDP screencast -> ffmpeg recorder (tools/record_sidecar/sidecar.mjs), zero npm deps
  - Pre-sidecar stealth baseline audit (SCORE 100/100, exit 0)
  - Measured CDP endpoint discovery for the agent lane
  - Live smoke across vp9/vp8/x264, wallclock/concat/cfr, fixed/even size, page-enable on/off
affects: [02-02 measurements, 02-03 verdict]

tech-stack:
  added: []  # Node built-ins only (global WebSocket, fetch, node:test); ffmpeg n9.0.2 via BLADE_FFMPEG
  patterns:
    - "Ack each Page.screencastFrame synchronously before any queueing"
    - "Bounded FrameQueue drops the incoming frame, never blocks the ack path"
    - "ffmpeg -fps_mode passthrough plus 1 ms input timebase for wall-clock capture"

key-files:
  created:
    - tools/record_sidecar/sidecar.mjs
    - tools/record_sidecar/lib/cdp.mjs
    - tools/record_sidecar/lib/encoder.mjs
    - tools/record_sidecar/env.sh
    - tools/record_sidecar/README.md
    - tools/record_sidecar/test/encoder.test.mjs
    - tools/record_sidecar/test/cdp.test.mjs
    - tools/record_sidecar/fixtures/{anim,idle,a,b}.html
    - tools/record_sidecar/results/baseline_audit.txt
    - tools/record_sidecar/results/discovery.txt
    - tools/record_sidecar/results/smoke.json
    - tools/record_sidecar/results/stats/*.stats.json
  modified: []

key-decisions:
  - "Endpoint is taken from the main Chrome process line (--remote-debugging-port=N), not DevToolsActivePort: the agent lane arms a fixed port and Chrome writes the file only for port=0 (src/browser/launch.rs)"
  - "wallclock and concat use -fps_mode passthrough, not vfr: vfr kept only 234/800 frames in isolation"
  - "wallclock input uses -framerate 1000: the 25 tbn default produced 232 non-monotonic dts warnings per 800 frames; 1 ms cut that to 3"
  - "Page.enable is not required for screencast frames in this probe (vp9_nopage delivered 792 frames); recorded for the 02-03 verdict"

patterns-established:
  - "Probe-only tooling under tools/record_sidecar/, never shipped in the binary or used by src/"
  - "Every smoke number is copied from a run's stats/ffprobe output by a merge script, not typed"

requirements-completed: []  # VER-01 is verdict-level; this plan only builds the instrument and records the baseline. Verdict is 02-03.

duration: ~15min
completed: 2026-10-21
---

# Phase 2 Plan 01: Sidecar Recorder and Baseline Summary

**Zero-dependency CDP screencast recorder that acks every frame and pipes it into ffmpeg; all seven codec/timing/size/page-enable smoke runs exit 0 and decode, with two encoder-timing bugs found and fixed along the way**

## Performance

- **Duration:** about 15 min (baseline audit timestamp 10:28Z, final smoke 10:42Z)
- **Tasks:** 3 of 3
- **Files created:** 22 (see key-files); no files modified outside tools/record_sidecar/ and .planning/

## Accomplishments

- Baseline `bladebro audit` recorded before any sidecar code ran: `SCORE: 100/100 pass`, `audit_exit=0`, Chrome 154.0.8037.58, bladebro v4.6.5.
- Agent-lane CDP endpoint discovered and recorded (see Discovery below). Browser WebSocket `ws://127.0.0.1:<port>/devtools/browser/<uuid>` answers `/json/version`; the page target is listed in `/json/list`; the main Chrome process carries `--remote-debugging-port=<port>` and zero `--remote-debugging-pipe`.
- Recorder attaches a flattened session, sends `Page.enable` (optional), `Page.startScreencast`, and acks every `Page.screencastFrame` synchronously before enqueueing.
- Unit tests: 21 of 21 pass (`node --test tools/record_sidecar/test/*.test.mjs`).
- Live smoke: 7 of 7 runs exit 0, `frames_received == acks_sent == acks_ok` in every run, every output is ffprobe-readable, and `nb_read_frames` tracks `frames_written` for every run except vp8's queue drops.

## Discovery (SC3 endpoint method)

- The planned route (read `DevToolsActivePort` under `BLADE_HOME`) finds nothing on the agent lane. Verified: no `DevToolsActivePort` exists under the profile directory while the daemon is running.
- Working method: the port comes from the main Chrome process command line (`--remote-debugging-port=<N>`), then `/json/version` gives `webSocketDebuggerUrl`. `bb_port` in `env.sh` implements this and falls back to `DevToolsActivePort` only if a file exists.
- Pipe check: `--remote-debugging-pipe` count on the main Chrome process = 0. An earlier count of 3 was a false positive from the grep's own shell command line; the re-run uses a check that cannot match itself.

## Smoke results

Source: `tools/record_sidecar/results/smoke.json` (final clean run). Chrome 154.0.8037.58, fixture `anim.html`, 8 s per run, capture at about 98 fps on this display.

| Run | Codec / ts / size / page | Exit | Recv = acks | Written | Queue drops | Output frames (ffprobe) | Encoder finalized | Output |
|---|---|---|---|---|---|---|---|---|
| vp9_wall | vp9 / wallclock / fixed / on | 0 | 793 = 793 | 794 | 0 | 794 | yes | 1280x720, 7.99 s |
| vp8_wall | vp8 / wallclock / fixed / on | 0 | 792 = 792 | 657 | 136 | 657 | yes | 1280x720, 7.99 s |
| x264_wall | x264 / wallclock / fixed / on | 0 | 793 = 793 | 794 | 0 | 794 | yes | h264 1280x720, 7.99 s |
| vp9_concat | vp9 / concat / fixed / on | 0 | 793 = 793 | n/a | 0 | 794 | yes | 1280x720, 8.04 s |
| vp9_cfr | vp9 / cfr 10 fps / fixed / on | 0 | 793 = 793 | 79 | 0 | 79 | yes | 1280x720, 7.90 s |
| vp9_even | vp9 / wallclock / even / on | 0 | 792 = 792 | 793 | 0 | 793 | yes | 1184x720, 7.99 s |
| vp9_nopage | vp9 / wallclock / fixed / **off** | 0 | 792 = 792 | 793 | 0 | 793 | yes | 1280x720, 7.99 s |

Notes on the numbers (all from `results/stats/*.stats.json`):
- Ack round trip p50 about 0.27 ms, p95 about 0.35 ms in the vp9_wall run; ack send p50 about 36 us.
- Capture-to-receive p50 about 2.4 ms in vp9_wall (from the frame `metadata.timestamp`).
- `first_frame_ms` 8 to 13 ms; `max_gap_ms` 18 to 36 ms.
- `Page.enable` is not required for frames to arrive in this probe (vp9_nopage: 792 frames). This goes into the 02-03 verdict as a fact to weigh, not a conclusion.
- vp8 queue: the writer could not keep up, so the bounded queue dropped 136 of 792 frames. Drops are counted, not hidden.
- Output bytes for 8 s: vp9_wall 784,116; vp8_wall 1,488,477; x264_wall 300,523; vp9_cfr (10 fps) 109,998; vp9_concat 803,358.
- Encoder finalize time: 10 to 562 ms (concat is the slowest, 562 ms; vp8 is the fastest, 10 ms).
- `first_frame_ms` 8 to 13 across all seven runs; `max_gap_ms` 18 to 36.

## Baseline audit

`results/baseline_audit.txt`: `SCORE: 100/100 pass`, `audit_exit=0`, `VERDICT: PASS`. AGENTS.md says 111 vectors; this build prints 100 on this platform. Recorded as printed; not rerun to change it.

## Task Commits

1. **Task 1: baseline audit, env.sh, fixtures, discovery** - `760ebfd` (chore)
2. **Task 2: cdp.mjs, encoder.mjs, sidecar.mjs** - `aa0ac06` (feat)
3. **Task 3 (fix): every frame kept in wall-clock and concat** - `f14fd50` (fix; TDD regression tests in the same commit)
4. **Task 3 (smoke + README): smoke.json, stats, README skeleton** - `d21aed9` (test)

**Plan metadata:** committed after this file (docs: complete plan 02-01).

## Files Created/Modified

- `tools/record_sidecar/sidecar.mjs` - CLI recorder: attach, screencast, ack, bounded queue, ffmpeg writer, timed stop, stats JSON
- `tools/record_sidecar/lib/cdp.mjs` - minimal CDP WebSocket client, `getJson`, `percentile`, DevToolsActivePort parser
- `tools/record_sidecar/lib/encoder.mjs` - `buildFfmpegArgs`, `FrameQueue`, `buildConcatList`, `parseBench`, `jpegSize`, `startEncoder`
- `tools/record_sidecar/env.sh` - scratch BLADE_HOME, fixture server, `bb_port`, `bb_cleanup`
- `tools/record_sidecar/test/*.test.mjs` - 21 unit tests
- `tools/record_sidecar/fixtures/{anim,idle,a,b}.html` - static local fixtures
- `tools/record_sidecar/results/*` - baseline audit, discovery, smoke.json, stats copies
- `tools/record_sidecar/README.md` - purpose, requirements, safety, usage, skeleton status

## Decisions Made

- Endpoint discovery uses the process command line, because the agent lane arms a fixed port and Chrome does not write `DevToolsActivePort` for it (plan premise corrected).
- wallclock and concat use `-fps_mode passthrough` instead of `vfr`. Isolated experiment, synthetic MJPEG at 10 ms spacing, 800 frames: vfr kept 234 (`drop=567`), passthrough kept 800.
- wallclock input gets `-framerate 1000`. Same experiment: 232 non-monotonic-dts warnings without it, 3 with it, all 800 frames kept in both.
- The `stop` event is not written to `events`, so `events` stays `[]` in normal runs as the plan specifies.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] wall-clock and concat recordings dropped about 70% of frames**
- **Found during:** Task 3 live smoke (encoder stderr: `frame= 202 ... drop=592` for 794 captured frames)
- **Issue:** `-fps_mode vfr` drops frames that land inside one assumed 25 fps tick. The recorder captured about 98 fps, so output had about 25 fps worth of frames.
- **Fix:** wallclock and concat use `-fps_mode passthrough`. cfr is unchanged.
- **Files modified:** `tools/record_sidecar/lib/encoder.mjs`, `tools/record_sidecar/test/encoder.test.mjs`
- **Verification:** isolated experiment (A vfr 234/800, B passthrough 800/800); live smoke `nb_read_frames` 794 for 794 written (was 202).
- **Committed in:** `f14fd50`
- **Note:** this deviates from the plan's literal `-fps_mode vfr` contract (the plan's own must-have and unit-test text). The plan's premise (vfr keeps the frame rate sane) was wrong.

**2. [Rule 1 - Bug] wall-clock output had non-monotonic timestamps**
- **Found during:** Task 3 decode check (`Application provided invalid, non monotonically increasing dts`)
- **Issue:** the image2pipe input defaults to 25 tbn, which quantizes wall-clock stamps to 40 ms.
- **Fix:** `-framerate 1000` on the wall-clock input.
- **Files modified:** `tools/record_sidecar/lib/encoder.mjs`, `tools/record_sidecar/test/encoder.test.mjs`
- **Verification:** isolated experiment D (default) 232 warnings, E (`-framerate 1000`) 3 warnings, both 800 frames kept. Live decode-warning lines dropped from about 29 KB to about 2 KB per run.
- **Committed in:** `f14fd50`

**3. [Plan premise] DevToolsActivePort is not written on the agent lane**
- **Found during:** Task 1 discovery
- **Issue:** the plan states Chrome writes `<user-data-dir>/DevToolsActivePort`. On the agent lane it does not; `src/browser/launch.rs` confirms Chrome only writes it for port=0.
- **Fix:** `bb_port` reads the port from the process command line first, and falls back to the file only if one exists.
- **Files modified:** `tools/record_sidecar/env.sh`, `tools/record_sidecar/results/discovery.txt`
- **Committed in:** `760ebfd`

**4. [Rule 3 - Process error, reverted] accidental commit of an unrelated file**
- **Found during:** Task 1 commit step
- **Issue:** I ran `gsd-tools.js commit --help` to check syntax; the tool treated it as a real commit and committed the pre-existing untracked `.planning/EXECUTION_LOG.md` as `b0e1e68` (message `--help`).
- **Fix:** `git reset --soft HEAD~1` and `git reset -q HEAD -- .planning/EXECUTION_LOG.md`. The file is untracked again, byte-for-byte unchanged. Never pushed.
- **Verification:** `git log` shows `337904e` as HEAD before the Task 1 commit; `git status` shows `?? .planning/EXECUTION_LOG.md` as at start.
- **Committed in:** none (reverted)

**Total deviations:** 4 (2 encoder bugs fixed under Rule 1, 1 plan-premise correction, 1 process error reverted)
**Impact on plan:** the two encoder fixes are necessary for the 02-02 measurements to mean anything, and they are covered by regression tests. No scope creep; nothing outside `tools/record_sidecar/` and `.planning/` changed.

## Issues Encountered

- **Baseline audit first attempt failed** (`timeout: command not found` on macOS). The file was rewritten by a rerun without the wrapper; the recorded output is from the rerun. No sidecar code existed at either attempt.
- **Intermittent screencast stall, not isolated.** One smoke attempt got 1 frame in 4 of 7 runs with `acks == frames == 1`. A later full re-run and a diagnostic run both got about 790 frames per 8 s, so I did not reproduce the stall. The host had load average about 5 from unrelated processes. Cause not verified; a window-occlusion hypothesis is untested. That attempt's data was deleted by the scratch-dir trap and is not in `smoke.json`.
- **vp8 queue drops** (136 of 792): the writer cannot keep up at 98 fps with `-crf 10 -b:v 2M -cpu-used 8`. Counted as drops, not fixed; a 02-02 measurement item.
- **Concat warnings remain.** About 200 non-monotonic-dts warnings per 794 frames (`decode_error_lines` 400 in the stored text, truncated to 600 chars). The index-based signature (dts 1, 2, 3) matches the pre-fix wall-clock issue, which suggests the concat demuxer inherits the 25 tbn default. Not fixed here: concat frames are all present and the file is readable, so it meets the plan's readability criterion. Open for 02-02 if concat stays a candidate.
- **Residual wall-clock warnings:** vp9_wall 23 lines, x264 6 lines, vp9_even 5 lines, vp9_nopage 7 lines (`decode_error_lines` in `smoke.json`). Reduced, not zero.
- **Ack at stop:** an earlier concat run wrote `acks_ok` one below `acks_sent` (792 of 793): the last ack was still in flight when the stats were written. The final run has 793 of 793. Harmless for the verdict, but 02-02 should compare `acks_sent` to `acks_ok` with that caveat in mind.
- **Not done (by user instruction):** `STATE.md` and `ROADMAP.md` not edited. `gsd-docs-updater` not run: its output would write repo docs outside the allowed paths (`tools/record_sidecar/` and `.planning/`).

## Next Phase Readiness

- Recorder and probe ready for 02-02 measurements. Open items for 02-02: vp8 queue throughput, concat dts warnings, and the Page.enable finding.
- The fixture page is a 98 fps animation. Real-site and stealth-relevant measurements in 02-02 should not assume the same capture rate.
- No blockers for 02-02. VER-01 is not closed by this plan.

## Self-Check: PASSED

- All 14 listed files present (env.sh, sidecar.mjs, lib/cdp.mjs, lib/encoder.mjs, both test files, README, baseline_audit.txt, discovery.txt, smoke.json, four fixtures).
- Commits `760ebfd`, `aa0ac06`, `f14fd50`, `d21aed9` present in `git log`.
- `node --test tools/record_sidecar/test/*.test.mjs`: 21 pass, 0 fail.
- Changed paths: no changes to `src/`, `tests/`, `npm/`, `Cargo.*`, `CHANGELOG.md`, or `.github/` since base `337904e`.

---
*Phase: 02-sidecar-prototype-gate*
*Completed: 2026-10-21*
