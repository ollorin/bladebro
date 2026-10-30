---
phase: 06-frame-source
verified: 2026-10-22T11:00:00Z
status: passed
verifier: gsd-verifier (opus, final adversarial pass, re-verification after 06-05)
score: 8/8 must-haves verified (SC1-SC4, SC5 method set and A/B stealth, Lagged ack, ends-at-stop, test suite green). SC5 literal instrument bar is environment-limited and needs user acceptance (see below)
re_verification:
  previous_status: gaps_found
  previous_score: 5/8
  gaps_closed:
    - "Flaky lagged-bus test: 20/20 isolated release runs green (0.76-0.79 s each); the record:: lib subset 10/10; the full suite never failed on it in 5 runs; the no-lag-acks mutation now fails it 3/3"
    - "Hold-flush tick ignored end_at: the tick now clamps to end_at; mutations (no clamp, max instead of min, final hold ignoring end_at) are all killed; live off-second stops whose tick fell inside the grace (extra 370 ms, 550 ms) came out +0.32 % and -0.60 % (pre-fix +5.00 %)"
    - "Ack-before-sink order not pinned: new test ack_reaches_the_wire_before_the_sink_runs fails when the forward is moved before spawn_ack (mutation killed)"
  gaps_remaining: []
  regressions: []
---

# Phase 6: Frame Source Verification Report (final, after 06-05)

**Phase Goal:** Screencast frames flow from a real page into the encoder without ever stalling the CDP reader or changing what the page can see, and a blank capture is reported rather than passed off as success.
**Verified:** 2026-10-22 (HEAD 7351769 = fix 42c90ce + report commit; macOS arm64, Chrome 154, headless)
**Status:** passed (no code gaps). The SC5 environment limits below need explicit user acceptance.
**Re-verification:** Yes. Previous status gaps_found (3 gaps). All 3 are closed and I re-checked each one myself.

## Verdict
The 3 gaps from the previous pass are fixed and verified. I ran each fix, mutation-tested it and checked it live. The test suite is green and the lagged test is now deterministic. The tick clamp is correct, including the edge cases. Ack-before-sink order is pinned by a test. Wire discipline, NoFrames honesty and resize/idle durations were re-checked live. No code gap remains.

What remains is one minor docs inaccuracy (non-blocking, see Findings) and the environment-limited SC5 items, which were honestly disclosed and which the user has to accept.

## What I ran myself (commands + output)
Live rules for every run: BLADE_HEADLESS=1; a fresh BLADE_HOME per scenario (env.sh/new_home with mktemp); BLADE_NO_UPDATE_CHECK=1; 127.0.0.1:8765 fixtures; EVIDENCE redirected to scratch, so the committed evidence was not touched; no headed run.

| Command | Result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo clippy --release --all-targets -- -D warnings` | exit 0 |
| lagged test alone, release, x20 (`--exact`) | **20/20 pass**, 0.76-0.79 s each |
| `cargo test --release --lib record::` x10 | 10/10 pass (202 tests each) |
| `ack_reaches_the_wire_before_the_sink_runs` alone x10 | 10/10 pass |
| `cargo test --release` x5 | runs 2-5: **628 passed, 0 failed, 1 ignored**. Run 1: 596 passed, 1 failed. The failure was `tests/daemon_lifecycle.rs::concurrent_cold_starts_have_one_owner` (cli.pid not present after its fixed 500 ms sleep). That test predates the phase (last change 8b3d82d, an ancestor of efd87ac) and is unrelated to recording. All phase tests passed in all 5 runs. |
| `source_live.sh dur-tick8` with REC_LIVE_EXTRA_MS = 0 / 370 / 550 / 730 / 910 (plus a 6th run at 0 whose log was overwritten by a naming slip on my side) | wall 8001 -> 8000 ms (-0.01 %); 8373 -> 8400 (+0.32 %); 8551 -> 8500 (-0.60 %); 8732 -> 8700 (-0.37 %); 8912 -> 8900 (-0.13 %). All decode exit 0, frames == acks, leftovers 0. In the 370 and 550 runs the last frame came at about 7600 ms, so the 1 s tick (about 8600 ms) landed **inside** the stop grace, the exact pre-fix +5 % case; both are now clamped. |
| `source_live.sh bg` | bg: outcome NoFrames, `health_at_stop NoFrames { for_ms: 8003, hidden: true }`, verdict "recording failed: no frames reached the encoder, so there is no video", no output file, one stderr WARN at 5.0 s. bg-control: 81 frames written, 8100 vs 8001 ms (+1.24 %), decode exit 0, leftovers 0 |
| `source_live.sh trace` | wire_methods = {Page.enable 1, startScreencast 1, screencastFrameAck 720, stopScreencast 1}; all 7 PASS checks (subset of 4, enable first, start before first ack, wire acks 720 == acks_sent 720, wire == methods_sent, frames 720 == acks 720, Chrome argv identical before/during/after at 531 bytes); 12100 vs 12000 (+0.83 %); decode exit 0; leftovers 0 |
| `source_live.sh resize dur-idle` | resize: sizes 1280x628 / 1001x641 / 1101x614, 16100 vs 16002 (+0.61 %), decode exit 0, 161 frames. dur-idle 20 s: 1 frame, 20000 vs 20000 (+0.00 %). Leftovers 0 |
| Process diff vs a baseline taken before the live runs | only new process: a Chrome renderer helper with ppid 1139 (the user's own Chrome, not mine); no daemon, bbrec Chrome, ffmpeg, record_live or http.server left; port 8765 free |
| `git diff efd87ac^..HEAD --stat` outside src/record*, tests, tools, docs, .planning | only `src/cdp/client.rs` (+12 -1) |
| Cargo.toml/Cargo.lock/npm/CHANGELOG.md/.github in that range | no changes |
| Commit trailers efd87ac^..HEAD | 20/20 carry `Co-authored-by: Claude <claude@anthropic.com>` |
| `.planning/HOOKS.md` row 6 | matches the client.rs diff exactly (const + oversize condition, 8 MiB bound, Page.screencastFrame only) |

Cleanup of my own processes and files: one daemon (`bladebro daemon`, BLADE_HOME bladebro-daemon-race-52399) was leaked by the pre-existing daemon_lifecycle test during full run 3. I killed it and removed its temp dir. The scenario script leaves one empty `bbrec.*` dir per invocation (env.sh creates one, then new_home replaces BLADE_HOME). I removed my own empty ones. Older ones not from this session were left alone.

### Mutation checks (scratch git worktree with its own CARGO_TARGET_DIR; repo source never edited; worktree removed)
| Mutation | Result |
|---|---|
| A1 capture.rs: forward to the sink before spawn_ack/lag acks | **killed**: ack_reaches_the_wire_before_the_sink_runs FAILED (58 passed, 1 failed) |
| T1 writer.rs: tick clamp removed | **killed**: hold_tick_after_mark_end_does_not_write_past_it |
| T2 writer.rs: `held.max(..)` instead of `min` | **killed**: same test |
| T3 writer.rs: final hold uses elapsed instead of end_at | **killed**: mark_end_stops_the_final_hold_before_finish and the tick test |
| M7 capture.rs: no lag acks sent | **killed 3/3** (deterministic now; previously the test was flaky) |

## Tick-clamp review (writer.rs 90-104, 107-121)
- **end_at unset:** `held = arrived.elapsed()`, the same as before. Idle-page progressive flush is unchanged (dur-idle 20 s +0.00 %).
- **Tick racing mark_end:** `end_at` is a OnceLock. A tick that reads it before the set writes up to "now", which is earlier than the mark. A tick after the set is capped at the mark. The Pacer is cumulative (written vs slots), so the final `on_end(ts + (end_at - arrived))` only adds what is still owed and nothing is double-written. Correct in both orders.
- **end_at earlier than arrival** (a frame that arrived after the mark): `saturating_duration_since` gives 0, so the tick writes nothing past the frame's own ts. Correct.
- **Frames arriving in the stop grace window:** capture.rs still forwards frames received after `mark_end` (and the held trailing frame via take_held), each with its own timestamp. The video therefore ends at the last in-grace frame rather than exactly at the stop request. In addition, `Pacer::on_end` forces the final frame to be written at least once, which can add up to one slot (100 ms at 10 fps). Measured on animated pages: +98 to +100 ms (trace 12 s +0.83 %, bg-control 8 s +1.24 %, resize 16 s +0.61 %). The theoretical bound is the 500 ms grace plus 1 slot, if Chrome kept emitting for the whole grace. Observed in-grace frames stopped within about 100 ms of the stop. Severity: minor. It is well inside OUT-03's 5 % (which is defined on an idle page) for any recording of a few seconds or more, and it is no longer the hold-flush defect.
- **Worst-case drift on short runs:** idle/tick pages: at most about half a slot plus the first-frame offset (measured -0.74 % to +0.32 %, at most about 60 ms on 8 s). Animated pages: about +1 slot, roughly 100 ms (+1.25 % on 8 s, +5 % on a 2 s run). The pacer docs already state that spans under about 1 s can exceed 10 % by one slot.

## Observable Truths
| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | SC1 / CAPT-01: JPEG via startScreencast; every frame acked on receipt; frames == acks | VERIFIED | live trace 720 == 720 on the wire; ack-before-sink pinned by a test (A1 killed) |
| 2 | SC2 / CAPT-02: slow encoder does not block the reader; bounded queue; newest dropped and counted | VERIFIED | code (try_send, own connection), limits tests; 06-03 live slow run (prior pass re-ran it: refused 148 == queue_dropped 148); not re-run in this pass |
| 3 | SC3 / CAPT-05: no frames -> NoFrames, not success; 2 MB frame passes the hook | VERIFIED | live bg (NoFrames hidden=true, failure verdict, no file); hook_tests 2 MB; HOOKS row 6 |
| 4 | SC4 / OUT-03: idle duration within 5 %; odd/changing sizes playable | VERIFIED | dur-idle 20 s +0.00 %; tick8 x5 within -0.60..+0.32 %; resize 1001x641/1101x614 playable, +0.61 % |
| 5 | SC5 / SAFE-01: only Page screencast methods, no new flag; audit/oracle/qol | VERIFIED for the method set, argv and the A/B checks. The literal instrument bar is environment-limited (see below) | live trace: 4 methods, argv identical |
| 6 | Lagged frames acked (bounded, cached id) | VERIFIED | M7 killed deterministically; lagged test 20/20 |
| 7 | Video ends at the stop request (06-04/06-05) | VERIFIED (hold-flush defect fixed; the in-grace frame and final-slot effect of about 100 ms on animated pages is minor, see Findings) | T1/T2/T3 killed; live off-second runs clamped |
| 8 | Test suite green | VERIFIED | 628/0/1 in 4 of 5 full runs; the 1 failure was a pre-existing, out-of-phase daemon test |

## Requirements Coverage
CAPT-01, CAPT-02, CAPT-05, OUT-03: satisfied at the source/encoder level. The user-facing configuration of the no-frames period arrives with Phase 7. SAFE-01: the wire/argv/injection part is satisfied; the "audit 111/111 with diff_oracle divergent 0" part is environment-limited (below).

## Findings (non-blocking)
| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| docs/recording-and-launch-config.md | 75 | "(at most about 60 ms, since the video is built from whole frame slots)" and "now ends at the stop request" are stated generally, but they were measured only on the tick page | Minor (docs) | Animated pages run about 100 ms long (+0.6 to +1.24 %), from the forced final slot plus frames that arrive during the grace. This phase's own evidence shows it (06-03 trace +0.83 %, 06-04 and this pass bg-control +1.22/+1.24 %). Suggested wording: "within about one frame slot (100 ms at 10 fps) of the recording time; on animated pages frames still in flight at the stop can add up to that". |
| tests/daemon_lifecycle.rs | 196-204 | fixed 500 ms sleep before reading cli.pid; leaks a daemon when it panics or races | Pre-existing, out of phase | 1/5 full-suite flake on this host under load. Not a Phase 6 regression. Worth a separate fix. |
| tools/record_sidecar/scenarios/source_live.sh + env.sh | - | leaves one empty mktemp `bbrec.*` dir per invocation | Nit (test tooling) | Temp-dir litter only; no profile data |
No TODO or stub found. logical_cohesion: fine (capture.rs is about 423 LOC and focused; writer.rs is 125). Layer-1 gates (fp-gate, migration-security, cve-scan, p0-tier) are not shipped in this Rust repo: N/A. Charlotte QA/E2E: not a web/UI phase: N/A. No migrations.

## SC5 / environment items the user must accept (not code gaps)
1. **diff_oracle was never run in this phase.** Python `websockets` is missing and installs are forbidden. The substitutes are the probe-battery A/B (divergent 0 over 99 keys, with vs without the recorder) and the wire trace (4 Page methods, identical argv). These substitutes are strong but not equivalent: they compare bladebro with itself, not with stock Chrome. To close it: run `python3 tools/diff_oracle/oracle.py` on a host with `websockets`.
2. **audit 100/100 on macOS instead of 111/111 (the Linux figure).** The audit was identical with and without the recorder.
3. **qol_probe 120/122.** The 2 failures are pre-existing and off the recorder path ("chord cleared the live editor", "nested scroller movement is reported as such").
4. **No Linux/Windows build and no CI run.** Only macOS arm64, Chrome 154, headless. The stealth baseline in AGENTS.md was measured on Chrome 151.

## Not verified (honestly)
- diff_oracle, audit 111 on Linux, Linux/Windows/CI, the rb lane, the Chrome 151 baseline.
- Not re-run by me in this pass: the slow-consumer live scenario and source_stealth.sh (audit/battery A/B). Both were run by me in the previous pass at da5e35c. 06-05 touched only writer.rs, tests, docs and the harness, not the source/wire path.
- Chromium's frames_in_flight floor under the up-to-2 extra Lagged acks (reasoned, not checked in Chromium source).
- The theoretical 500 ms in-grace extension; only the about 100 ms case was observed.

## Gaps Summary
No code gaps. Previous gaps 1-3 are closed and verified: run 20x, mutation-killed, and checked live. One minor docs inaccuracy should be corrected (non-blocking). The phase passes subject to the user accepting the 4 environment-limited SC5 items above.

_Verified: 2026-10-22_
_Verifier: gsd-verifier (opus, final adversarial pass, re-verification after 06-05)_
