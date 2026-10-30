---
phase: 08-tab-following
plan: 01
subsystem: recording
tags: [tab-following, screencast, segments, follower-task, headless-live]
requires: [Phase 7 hub (07-01..07-04)]
provides: [default-mode tab following, multi-segment record-stop, SegmentSink/merge/pick building blocks]
affects: [08-02 per-tab files reuses Tracker, TabProbe, pick, SegmentSink]
requirements-completed: [CAPT-03, CAPT-05, OUT-05]
key-files:
  created:
    - src/record/hub/follow.rs, follow/{probe,pick,sink,merge,run,unit_tests}.rs, follow_tests.rs
    - src/record/encoder/size_change_tests.rs
    - tests/record_follow.rs, tests/record_support/mod.rs
    - tools/record_sidecar/scenarios/follow_live.sh, fixtures/c.html, fixtures/popup.html
  modified:
    - src/record/hub.rs, hub/{env,machine,msg,machine_tests,report_tests}.rs
    - src/record/source/{capture,fake_cdp}.rs, src/record/encoder.rs
    - tests/record_hub.rs, docs/recording-and-launch-config.md, tools/record_sidecar/README.md
duration: one session
completed: 2026-10-22
---

# Phase 8 Plan 01: Default-mode tab following Summary

One recording, one file, one ffmpeg: a follower task owned by the active recording polls Chrome's `/json`, decides which tab the agent is on and starts a new `Source` on it into the same `Encoder`. record-stop lists every confirmed switch as `[<offset> s tab <id>]` and reports closed/replaced tabs, lost browsers, failed rebinds and blank segments honestly. Zero edits to any file that exists in upstream/main.

## Commits
- `6d9479f` feat: follow building blocks (discovery evidence, follower, hub wiring, pick/sink/merge, `Source::is_finished`, `FakeOpts.ts_base`, two-size real-ffmpeg test, fixtures)
- `471781d` feat: follow_tests.rs scenarios + hub module wiring
- `cd574c8` fix: new-tab debounce (live finding, below)
- `ef6566f` test: follow_live.sh, tests/record_follow.rs, tests/record_support harness, smoke evidence
- `565dd53` docs: Tab following section, README row
(Task 1 and Task 2 code could not be split cleanly: the follower, the hub wiring and `msg` changes must compile together, so `6d9479f` carries the follower and its wiring and `471781d` carries the scenario tests.)

## Discovery conclusions (evidence/08-01-discovery.txt, headless Chrome on macOS)
- RULE: `Rule::Attached` is INVALID: `/json` carries no `attached` key at all (not even for the tab the recorder is connected to). Plan contingency applied: `const RULE = Rule::ListOrder` (first non-devtools page in Chrome's order). `Rule::Attached` stays implemented and table-tested.
- List order: open-tab puts the new page first immediately; switch-tab puts the target first within 50 ms (a transient `about:blank` target of type `other` appears first and is filtered); close-tab of the current tab leaves the agent's next tab first.
- Popup: a `target=_blank` link popup is inserted after the opener, but it is first in `/json` until the opener is re-activated (about 0.6 s, the click-verdict time). A `window.open(..., 'width=500,height=400')` popup is first and stays first even after the opener's re-activation.
- Cross-origin (127.0.0.1 -> localhost): target id unchanged, WS survived, frames continued, clean stop. The D8-4 contingency (re-send `Page.startScreencast` on `Page.frameNavigated`) was NOT needed and not built; `src/record/source/handler.rs` is untouched.

## Deviations
1. [Rule 1 - Bug, found live] The first live popup run (before the debounce) recorded a `target=_blank` link popup as a segment (`0.0 opener, 0.5 popup, 1.1 opener`). Fix: a tab that was never seen before must stay first for 1.5 s (`NEW_TAB_CONFIRM`) before it is followed; known tabs keep the 50 ms two-sample confirm. Tests: `a_popup_that_the_opener_re_fronts_in_time_is_not_a_segment` (fake) and the live popup scenario. Cost, documented: open-tab is followed ~1.7 s after the call; switch-tab to an open tab ~0.25 s.
2. [Rule 1 - design] A window popup (features) that stays first is followed (ListOrder cannot tell it from open-tab). CAPT-03's "popup rebinds" is accepted as: a popup becomes a segment after `switch-tab` to it (opener stays the agent's page for link popups); window popups are followed once in front for 1.5 s. A `Baseline` (a head that already differed from the bound tab at record-start is ignored until the head changes once) stops a lingering window popup from hijacking a recording. The video should return to the opener at the agent's next input action (code reading, not measured).
3. D8-2 order changed: the generation is bumped BEFORE `Source::start_ws` and restored on failure (otherwise the new tab's first frame, the only one a static page sends, would be discarded as stale).
4. Coordinator amendment "up to ~0.7 s for an immediate A->B->A return" is documented as an allowance ("usually ~0.25 s, up to ~0.7 s on a busy machine or a quick back-and-forth"); with ListOrder the retiring sources do not enter the decision, so 0.7 s was not measured.
5. `compose` is unchanged; failures that are not a SourceReport verdict are carried by a new `compose_follow` (merge.rs) that names them first. A rebound ConnectionLost adds no extra note (the `gone` note covers it).
6. In-flight-frames row: covered by the SegmentSink table (stale counted, never forwarded) and by scenarios where the old tab keeps emitting after the switch; there is no scenario-level counter of discarded frames.
7. report_tests.rs updated per the coordinator amendment (`tab_moved` replaced by `tab_gone`); `Knobs` gained `poll`, `confirm`, `new_confirm`, `no_frames_after` (machine_tests uses `..Default::default()`).
8. Fixtures/tests use `act click text=...`; harness inputs keep the REC_HUB_* prefix.

## Race review
- The follower never takes the hub mutex; `Hub::stop` awaits the follower's result outside any follower lock.
- Snapshot mutex (tab + health) is held only for a clone, never across an await. `FollowShared` uses a std mutex in short, non-async sections (including the non-blocking `Encoder::send_frame`).
- stop vs tick: the stop signal wins the next `select!` (biased); a tick/rebind in flight completes first (bounded: list 2 s, rebind 4 s); catch-up rebind waits up to 400 ms for the new tab's first frame BEFORE `mark_end`; `mark_end` is called exactly once by the follower (SegmentSink's is a no-op).
- Retiring sources hold only their own `SegmentSink`; stale/old-generation and out-of-order frames return true and are never fed to the Pacer. Every sink is dropped before the result is returned, so `take_encoder` succeeds (tested after 10 switches). Dropping a `Tracker` aborts the task; encoder `kill_on_drop` is the backstop (tested: no encoder left after dropping the hub).
- Tracker::stop is bounded (stop_grace*2 + 14 s); a stuck follower is aborted and reported "did not stop in time", not clean.

## Verification
- Tests: full `cargo test --release` = 701 passed, 0 failed (baseline 673; +28). `concurrent_cold_starts_have_one_owner` failed once in the very first baseline run (the known flake; it also leaked a daemon, which I killed) and passed on every later run. fmt clean, clippy `--all-targets -D warnings` clean.
- New tests: follow unit tables (agent_tab 11 rows, Confirm, Baseline, SegmentSink, merge a-h, compose_follow, msg), 17 follow scenarios (switch A-B-A with exact offsets `0.0/4.2/9.8`, one encoder pid, open-tab, popup behind opener, popup re-fronted, stale head, transient flip, rapid A-B-C, close, id change, no tab left, browser gone, rebind failure, zero-frame long/short, 10 switches, double start, finish_orphan, drop), `Source::is_finished`, real-ffmpeg two-size test (1280x720 then 500x400 into WebM and MP4: output 1280x720, 40 frames decoded == written, clean decode), machine test `k_stop_after_late_switch_shows_the_new_tab`.
- Smoke (evidence/08-01-smoke.txt, headless, scratch homes, local fixtures; final runs below the marker line): daemon, close, popup, xorigin, off, mcp (3 live tests), audit (100/100) all PASS with per-scenario cleanup PASS. perf (informational): lane_matrix daemon 3 rounds exit 0; daemon CPU single-tab 0.8 %, following 0.6 % (10 s means). The earlier FAIL lines in the file are the pre-fix popup/mcp runs and cleanup false positives (my own shell command line contained "ffmpeg"); explained in the file's first line.
- `off`: with BLADE_RECORD unset, base (bb9e7dd) vs new: identical distinct WS method set (13), identical `/json*` GET count (10 per run), no screencast method, no recordings dir, no ffmpeg.
- Sample stop lines: switch: `segments [0.0 s tab F249..., 3.9 s tab 746A..., 6.2 s tab F249...]` (duration 8.0 s, 80 frames, decoded 80, red/blue/red); close: `... Note: tab 3AA1... gone at 5.3 s; following tab B59C...`; popup (link): one segment; popup (window.open): `[0.0 s tab <opener>, 2.0 s tab <popup>]`.
- Upstream check: `git diff --name-only bb9e7dd..HEAD` contains no path present in `upstream/main`; src/mcp, src/cli, Cargo.toml/lock, npm, CHANGELOG.md, .github and `.planning/HOOKS.md` have no diff. The follower has no CDP/page import and sends no CDP method (the only `.send(` is the oneshot stop channel).
- After every live run: `bladebro stop`, then pgrep/ps showed no bladebro daemon/mcp, scratch Chrome, ffmpeg, cdp_tap or http.server of ours.

## Open issues
- ListOrder cannot distinguish a lingering window popup from open-tab; such a popup is followed (documented). On a headed macOS Chrome the order semantics were not checked (headless only, per the safety rules).
- `eval window.open` popups were not tested live. The return of a window popup to the opener at the next input action is not measured.
- Per-tab files (CAPT-04) and `record.tabs` are 08-02.

## Self-Check: PASSED
