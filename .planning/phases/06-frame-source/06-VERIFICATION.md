---
phase: 06-frame-source
verified: 2026-10-22T10:40:00Z
status: gaps_found
verifier: gsd-verifier (opus, final adversarial pass)
score: 5/8 must-haves verified (SC1-SC4 + Lagged-ack behavior verified; SC5 literal instrument bar environment-blocked; 06-04 duration fix and test-suite reliability FAILED)
re_verification:
  previous_status: gaps_found
  previous_score: 4/5 success criteria (sonnet, first pass)
  gaps_closed:
    - "Lagged (bus-overflow) frames are never acked: product behavior now acks min(n,2) with the cached id (verified in code, killed-mutation M3/M7, and the passing runs of the new test show lost=2616 lag_acks=2)"
  gaps_remaining:
    - "SC5 literal bar (audit 111, diff_oracle, qol 122): environment-blocked, not a code gap (see Not verified)"
    - "Cross-platform CI: not executed"
  regressions:
    - "cargo test --release is no longer reliably green: the new 06-04 test lagged_bus_still_acks_and_the_wire_stays_allowlisted fails in 4 of 6 full release lib runs"
gaps:
  - truth: "Test suite passes (cargo test --release green, non-negotiable #8)"
    status: failed
    failure_type: regression
    severity: blocker
    reason: "The 06-04 test record::source::capture_tests::flow::lagged_bus_still_acks_and_the_wire_stays_allowlisted is flaky on unmodified HEAD da5e35c: 4/6 full `cargo test --release --lib` runs FAILED (scenario timed out at the 20 s guard), 8/12 isolated release runs failed, 3/6 debug runs failed. Instrumented (scratch worktree only): in failing runs the stalled sink ran to its own 15 s deadline (longest_gap_ms 15004, lost 0, lag_acks 0), i.e. the bus never overflowed while the loop was blocked, so Lagged never happened and the test waits 12 s + up to 14 s > 20 s guard. Cause is the test design: BlockOnce blocks a tokio worker with std::thread::sleep in a 2-worker runtime, which can strand the reader/fake-server tasks (LIFO slot is not stealable). The product lag path itself works when the overflow does occur (lost 2616, lag_acks 2). CI runs exactly `cargo test --release`, so CI would be red most of the time."
    artifacts:
      - path: "src/record/source/capture_tests/flow.rs"
        issue: "lagged_bus_still_acks_and_the_wire_stays_allowlisted: std::thread::sleep stall on a tokio worker; nondeterministic; 06-04-FIXES.md claims 'about 0.7 s'"
    missing:
      - "Make the stall deterministic (e.g. tokio::task::block_in_place inside the sink, or more workers plus a stall that does not hold a runtime worker), and count probe Lagged as progress"
      - "Prove stability: e.g. 20 consecutive green `cargo test --release --lib record::source` runs"
  - truth: "06-04 Gap-3 fix: the video ends at the stop request (docs: '8.00 s for an 8.00 s recording, 0.0 % drift')"
    status: failed
    failure_type: semantic_stub
    severity: major
    reason: "mark_end only clamps the FINAL hold (writer.rs on_end path, lines 101-107). The writer's 1 s hold-flush tick (writer.rs lines 90-97) computes est = ts + arrived.elapsed() and never consults shared.end_at, so a tick that fires during the 500 ms stop grace still writes the held frame past the stop mark. Reproduced live: dur-tick8 run 1 = ffprobe 8400 ms vs wall 8000 ms (+5.00 %, 84 frames written); last frame at 7381 ms, so the tick fired at about 8381 ms, inside the 8000-8500 ms grace, exactly +0.4 s. The 3 further tick8 runs (last frame at about 7600 ms, tick after the writer closed) gave -0.01/-0.04/-0.04 %, matching 06-04's numbers. Worst case is +grace (0.5 s = +6.25 % on 8 s). For an idle page the tick phase is first-frame time mod 1 s; the harness stops on whole seconds (REC_LIVE_SECS is u64), which hides this, but an agent stopping at an arbitrary time hits it about half the time. The unit test mark_end_stops_the_final_hold_before_finish waits 700 ms (< the 1 s tick), so it cannot catch this. The fix is non-deterministic and the docs overclaim."
    artifacts:
      - path: "src/record/encoder/writer.rs"
        issue: "hold-flush tick branch ignores end_at (lines 90-97)"
      - path: "src/record/encoder/lifecycle_tests.rs"
        issue: "mark_end test sleeps 700 ms, never crosses HOLD_FLUSH_INTERVAL"
      - path: "docs/recording-and-launch-config.md"
        issue: "line 75 claims '8.00 s for an 8.00 s recording, 0.0 % drift' unconditionally"
    missing:
      - "In the tick branch, clamp est to end_at (or skip the flush once end_at is set)"
      - "A deterministic test: send a frame, sleep about 900 ms, mark_end, sleep about 400 ms (a tick lands after the mark), finish; frames_written must equal the slots up to the mark"
      - "Correct the docs sentence after the fix is measured"
  - truth: "Ack is issued before decode/gate/sink (D6-4, SC1 'acked on receipt') is pinned by a test"
    status: failed
    failure_type: missing_test
    severity: minor
    reason: "Mutation M1 (move spawn_ack after handler.forward in capture.rs run) SURVIVED: all 58 record::source tests still pass. The code is correct today (verified by reading, and the live wire trace shows 720 frames = 720 acks), but no test would catch a reorder. The count tests only prove every frame is eventually acked."
    artifacts:
      - path: "src/record/source/capture_tests/flow.rs"
        issue: "no ordering assertion (e.g. a sink that records whether the ack for that frame already reached the fake endpoint)"
    missing:
      - "A test where send_frame asserts the fake has already logged the ack for that frame (or a slow sink that shows acks are not delayed by decode/sink)"
---

# Phase 6: Frame Source Verification Report (final adversarial pass)

**Phase Goal:** Screencast frames flow from a real page into the encoder without ever stalling the CDP reader or changing what the page can see, and a blank capture is reported rather than passed off as success.
**Verified:** 2026-10-22 (HEAD da5e35c, macOS arm64, Chrome 154, headless)
**Status:** gaps_found
**Re-verification:** Yes. The first pass (sonnet) was gaps_found; 06-04 fixes (e4fb927, b29386d, da5e35c) re-checked.

## Verdict in one paragraph
The frame source itself is sound: it uses its own connection, sends only 4 Page methods, acks every frame, hands frames off through a bounded try_send queue, reports NoFrames honestly, and keeps the hook exemption tight. I reproduced all of that live. The phase is still **gaps_found** for two code reasons. (1) The new 06-04 Lagged test is flaky enough that `cargo test --release` failed 4 of 6 times on unmodified HEAD. (2) The 06-04 "video ends at the stop request" fix is incomplete: the writer's 1 s hold-flush tick ignores `end_at`, and I measured +5.00 % (8.40 s for 8.00 s) live. A third, minor gap: no test pins the ack-before-decode order (a mutation survived). The SC5 environment limits (diff_oracle not runnable, macOS audit 100 instead of 111, qol 120/122) are honestly disclosed and substituted. On their own they would be a user-acceptance item, not a code gap.

## What I ran myself (commands + output)
All live runs: BLADE_HEADLESS=1, a fresh BLADE_HOME=$(mktemp -d) plus per-scenario homes, BLADE_NO_UPDATE_CHECK=1, 127.0.0.1:8765 fixtures, evidence written to scratch (committed evidence files untouched).

| Command | Result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo clippy --release --all-targets -- -D warnings` | exit 0 (CLIPPY_RC=0) |
| `cargo test --release` (1st run) | 583+6+1+5+14+2+10+2+3 = **626 passed, 0 failed, 1 ignored** |
| `cargo test --release --lib` x6 | **4/6 FAILED**: `lagged_bus_still_acks_and_the_wire_stays_allowlisted ... FAILED`, "582 passed; 1 failed ... finished in 22.8s"; 2/6 ok (583 passed) |
| same test isolated, release x12 | pass=4 fail=8 ("scenario timed out: Elapsed(())" at capture_tests.rs:21) |
| `source_live.sh dur-tick8 bg dur-idle` | dur-tick8: wall 8000, ffprobe 8.400000 s, **drift +5.00 %**, 84 frames written, last_frame_ms 7381, decode exit 0. bg: outcome `NoFrames { waited_ms: 8504, hidden: true }`, verdict "capture failed: no frames for 8.5 s ... (not a success)", encoder "recording failed: no frames reached the encoder", no output file, one stderr WARN at 5.0 s. bg-control: 480 frames = 480 acks, 8100 vs 8002 (+1.22 %). dur-idle 20 s: 1 frame, 20.000 s vs 20001 (-0.00 %), quiet-tail note in verdict. leftovers 0 |
| scratch copy of the script: dur-tick8 x3, dur-idle8 x4 | tick8: -0.01/-0.04/-0.04 % (last frame at about 7600 ms); idle8: -0.01/-0.01/-0.02/-0.01 %; leftovers 0 |
| `source_live.sh trace slow` | trace: wire = {Page.enable 1, startScreencast 1, screencastFrameAck 720, stopScreencast 1}, all 7 PASS checks, Chrome argv 531/531/531 bytes, identical before/during/after; 12.1 s vs 12.0 (+0.83 %). slow: max_len 16, refused 148 == queue_dropped 148, accepted 99 == forwarded 99, frames 1484 == acks 1484, `see content`/`state tabs`/`nav` rc 0 in 50/46/341 ms, frames after same-tab nav: YES; decode exit 0 |
| `source_stealth.sh` | [A] audit 100/100; [B] audit with recorder 100/100 (frames 5 == acks); [C] diff_oracle NOT RUN; [D] battery divergent=0, keys=99, noise ['voices'] |
| `python3 -I -c "import websockets"` | ModuleNotFoundError (confirms the diff_oracle blocker) |
| `python3 tools/qol_probe/run.py` | 120 passed, 2 failed: "chord cleared the live editor", "nested scroller movement is reported as such" (the same 2 recorded as baseline in 06-03-gates) |
| Process scan vs saved baseline after every batch | no bladebro daemon, Chrome with bbrec homes, ffmpeg, record_live, or http.server left; the only new Chrome helper had ppid 1139 (the user's own Chrome) |
| `git diff efd87ac^..HEAD --stat` outside src/record, tests, tools, docs, .planning | `src/cdp/client.rs +13 -1`, `src/record.rs +8` (fork-only module) |
| commit trailers efd87ac^..HEAD | 18/18 carry `Co-authored-by: Claude` |

### Mutation checks (scratch git worktree + its own target dir; repo source never edited; worktree removed afterwards)
| Mutation | Result |
|---|---|
| M1 ack spawned after decode/sink | **SURVIVED** (58/58 pass) -> gap 3 |
| M2 extra `Runtime.enable` via the raw session | killed (only_the_four..., hidden_event...) |
| M3 Lagged acks unbounded (no `min(MAX_LAG_ACKS)`) | killed (lagged_acks_with_the_cached_id_bounded) |
| M4 no `sink.mark_end()` on stop | killed (stop_marks_the_end_before_the_grace_wait) |
| M5 zero frames no longer converted to NoFrames | killed (4 tests) |
| M6 hook without the 8 MiB cap | killed (frame_above_the_cap_is_still_stripped) |
| M7 no lag acks sent | killed (lagged_bus...; note that test is also flaky) |
| M8 no ack for a stripped/undecodable frame | killed (2 tests) |
| baseline, unmodified, debug x6 | 3/6 failed on the lagged test -> gap 1 |

## Observable Truths
| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | SC1 JPEG via startScreencast, every frame acked on receipt, frames == acks | VERIFIED (order not test-pinned, gap 3) | capture.rs: spawn_ack before forward; handler decides ack before decode. Live: trace 720 = 720 wire acks, slow 1484 = 1484, control 480 = 480 |
| 2 | SC2 slow encoder: reader serves, queue bounded, newest dropped, counted | VERIFIED | live slow run (above); recorder's own connection; Encoder::send_frame try_send refuses newest; limits.rs tests |
| 3 | SC3 no frames -> NoFrames; 2 MB frame passes the hook unchanged | VERIFIED | live bg: NoFrames hidden=true, "not a success", no file; stop() converts Captured+0 frames; hook_tests two_mb... (M6 killed) |
| 4 | SC4 idle duration within 5 %; odd/changing sizes playable | VERIFIED for the integer-second runs I made (idle 20 s -0.00 %, idle 8 s <= 0.02 %); resize/odd sizes from prior evidence only (06-03-resize.txt, not re-run) | see gap 2 for the non-integer stop-time risk |
| 5 | SC5 only Page screencast methods; audit / diff_oracle / qol still pass | Method set and argv VERIFIED live; audit 100/100 A == B, battery divergent 0, qol 120/122 baseline-identical; **diff_oracle not run, 111 not reachable on macOS** | environment, not code (see Not verified) |
| 6 | 06-04 gap 4: Lagged frames acked (bounded, cached id) | VERIFIED (behavior) | handler.rs 64-85, capture.rs 373-377; M3/M7 killed; passing runs show lost 2616, lag_acks 2 |
| 7 | 06-04 gap 3: video ends at the stop request | FAILED | gap 2 |
| 8 | Test suite green | FAILED | gap 1 |

## Required Artifacts / Key Links
- src/record/source/{capture,handler,frame,gate,report,sink,watch,wire}.rs: exist, substantive, wired (re-exported in src/record.rs as FrameSource, etc.). The live harness tests/record_live.rs drives the real Source + Encoder. No Phase 7 caller yet (by design; RecordHub is Phase 7).
- Hook: client.rs route_cdp_text exempts only `Page.screencastFrame` with `text.len() <= 8 MiB`. HOOKS.md row 6 matches the diff and commit 57db2b5 exactly.
- Source -> Encoder: FrameSink::send_frame -> Encoder::send_frame (try_send); FrameSink::mark_end -> Encoder::mark_end (OnceLock, first wins; finish keeps the earlier mark).

## Edge cases examined
- Frame > 8 MiB: params stripped to `{}`. Acked with the cached id (counted acked_with_cached_id plus bad_frames); if it is the very first frame, counted `unacked`. Unreachable at the default 1280x720 maxWidth/maxHeight. OK.
- Session id absent from the cache on Lagged: no ack, counted only (unit-tested). OK.
- Extra acks vs Chrome flow control (observation, not a gap): a Lagged event can send up to 2 acks when the lost events held no frame. Chrome's in-flight counter would then be decremented below the true count, which loosens its 2-in-flight throttle by at most 2 per Lagged event. This is reachable only if more than 16384 events overflow the recorder's private, Page-only connection. Note: I did not check the Chromium-side counter floor in Chromium source.
- Stop during backlog: the biased select keeps draining and acking until the grace deadline, then stopScreencast has been sent. OK.
- mark_end vs finish race: OnceLock; mark_end is called from the loop before the sink Arc is released; finish's set fails if already marked. OK.
- Frame arriving in the grace window: forwarded with its own timestamp, so it extends the video by its gap (trace on anim.html: +0.83 %). Minor, inherent. Separately, the hold-flush tick extends it with no new frame at all (gap 2).
- No-frames honesty: zero frames at stop is NoFrames at any duration; silence after >= 1 frame is "Quiet", reported in the verdict, never a failure (documented as an inherent limit: static page vs stall cannot be told apart without touching the page). The NoFrames during recording is a stderr warning only; the failure verdict comes at stop() or via health(). Acceptable per D6-10.
- Pipe transport: Endpoint::Pipe -> SourceError::PipeTransport with no I/O; the message names BLADE_TRANSPORT and `bladebro stop` and ends "Browsing is unaffected." Unit-tested.

## Anti-Patterns Found
| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| src/record/source/capture_tests/flow.rs | ~200-215 | std::thread::sleep blocking a tokio worker in a test | Blocker | flaky required gate |
| src/record/encoder/writer.rs | 90-97 | hold flush not bounded by end_at | Major | video longer than the recording, up to +0.5 s |
| src/record/encoder/lifecycle_tests.rs | 267 | test window (700 ms) shorter than the mechanism under test (1 s tick) | Major | hollow guard for the 06-04 claim |
| docs/recording-and-launch-config.md | 75 | unconditional "0.0 % drift" claim | Minor | overclaim |
No TODO/stub. logical_cohesion: fine; source/ is split into focused children; capture.rs 423 LOC.

## SC5 / environment substitutions: judgment
- audit 100/100 (macOS) instead of 111/111 (Linux): honest and equivalent in intent. A (no recorder) == B (recorder attached), so the recorder changes nothing the audit sees. Not a code gap.
- diff_oracle: genuinely not runnable (websockets missing; installs forbidden), stated as NOT RUN with nothing claimed. Substituted by the Phase 2 probe battery A/B: divergent 0 across 99 keys, plus a wire trace proving only 4 Page methods and an identical Chrome argv. This is strong but **not equivalent**: diff_oracle compares against stock Chrome, while the battery compares bladebro with vs without the recorder. Not a code gap, but **the user must accept** that the DIVERGENT=0 bar was never measured in this phase, or run `python3 tools/diff_oracle/oracle.py` (with `--lane real` if desired) on a host with `websockets`.
- qol_probe 120/122: the 2 failures are the same pre-existing ones, and the recorder is not on that path. The user must accept those 2 as baseline or have them fixed outside this phase.
- Linux/Windows/CI: not executed. Note CI would currently hit gap 1.
Had gaps 1-3 not existed, these items alone would make the phase **human_needed** (user acceptance), not gaps_found.

## Not verified (honestly)
- diff_oracle (not runnable here); audit 111 on Linux; Linux/Windows builds and CI; the rb lane; the Chrome 151 baseline (Chrome 154 used).
- resize / odd-size scenario not re-run by me (06-03-resize.txt read only).
- Chromium's frames_in_flight floor behavior under extra acks (from memory, not checked).
- Idle-page drift at a non-integer stop time: derived from code plus the tick8 reproduction, not run directly (the harness only takes whole seconds).

## Gaps Summary
1. **Blocker:** a flaky 06-04 test makes `cargo test --release` red about 2/3 of the time on this host.
2. **Major:** the "ends at stop request" fix is incomplete. The hold-flush tick ignores `end_at`; reproduced +5.00 % (worst case +0.5 s); the guard test and the docs overclaim.
3. **Minor:** ack-before-decode order has no test (mutation survived).
SC1-SC4 behavior, SAFE-01 wire discipline, the hook and NoFrames honesty are verified live by this pass.

_Verified: 2026-10-22_
_Verifier: gsd-verifier (opus, final adversarial pass)_
