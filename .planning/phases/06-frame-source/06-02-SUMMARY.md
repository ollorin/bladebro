# 06-02 Summary: No-frames watchdog and live proof

Requirements: CAPT-05, OUT-03. ROADMAP SC3 (no-frames half), SC4. Live runs on macOS arm64, headless Chrome 154, local 127.0.0.1 fixtures. No Linux or Windows result is claimed (Xvfb cadence and Windows occlusion unmeasured).

## Commits
- 3123331 feat(06-02): no-frames watchdog for screencast capture (Task 1)
- test(06-02): live screencast source checks on headless Chrome fixtures, b7b5998 (Task 2)
- 0936da3 docs(06-02): how screencast frames are captured and its limits (Task 3)

## The no-frames rule as implemented
- `watch.rs`: `Liveness` (pure, ms since start), `Health {Waiting, NoFrames, Receiving, Quiet}`, `NO_FRAMES_AFTER` = 5 s; `SourceConfig::no_frames_after` (const default, shortened in tests; no env or config key).
- Zero frames at `stop()` -> `SourceOutcome::NoFrames {waited_ms, hidden}`, `ok() == false`, at any duration. `ConnectionLost` with zero frames keeps its own outcome.
- `Source::health()` while running; one `tracing::warn!(target: "bladebro.record")` on the transition (checked from the 250 ms liveness tick); recording continues.
- After >= 1 frame, silence is reported not failed: `last_frame_age_ms`, `longest_gap_ms`, `quiet_tail_ms` and the verdict note; no page-touching probe (comment at the decision site).
- `Page.screencastVisibilityChanged` is received only (nothing requested); `hidden` is named in the verdict. Verdict strings are D6-11 verbatim.
- Handler's own gap computation was replaced by `Liveness` (snapshot fills `longest_gap_ms`); first-frame gap is measured from start.

## Live results (evidence/06-02-live.txt, all numbers from REC_LIVE_REPORT / ffprobe / ffmpeg decode)
| scenario | rx / ack / fwd / capped / dropped / lost | ffprobe | wall ms | duration ms | drift | decode exit / warn lines / frames | outcome |
|---|---|---|---|---|---|---|---|
| dur-idle 20 s | 1 / 1 / 1 / 0 / 0 / 0 | vp9 1280x720 | 20001 | 20500 | +2.49 % | 0 / 0 / 205 | Captured, encoder Finalized (quiet-tail note present) |
| dur-tick 20 s | 21 / 21 / 21 / 0 / 0 / 0 | vp9 1280x720 | 20002 | 20500 | +2.49 % | 0 / 0 / 205 | Captured / Finalized |
| dur-anim 20 s | 1200 / 1200 / 200 / 1000 / 0 / 0 | vp9 1280x720 | 20001 | 20400 | +1.99 % | 0 / 0 / 204 | Captured / Finalized (60 fps in, 10 fps out, frames == acks) |
| resize 16 s | 912 / 912 / 155 / 757 / 0 / 0 | vp9 1280x720 | 16001 | 16500 | +3.12 % | 0 / 0 / 165 | Captured / Finalized |
| bg (T1 behind b.html) 8 s | 0 / 0 / 0 | no file | 8003 | n/a | n/a | n/a | source NoFrames (hidden: true, waited 8.5 s), encoder NoFrames, no file; stderr warning at 5 s seen on a rerun |
| bg-control (front tab) 8 s | 480 / 480 / 81 / 399 / 0 / 0 | vp9 1280x720 | 8000 | 8400 | +5.00 % | 0 / 0 / 84 | Captured / Finalized |

Every scenario: fresh scratch BLADE_HOME, BLADE_HEADLESS=1, BLADE_NO_UPDATE_CHECK=1, fixtures only; `bladebro stop` and pgrep afterwards found 0 leftover daemon, Chrome, ffmpeg or record_live processes in each. No headed run. The bg scenario was rerun alone after adding a tracing subscriber to the driver (the first full run printed no warning because the driver had none); both runs are in the evidence file.

## Deviations and honest notes
- Video duration is a constant 0.4 to 0.5 s longer than the stop() wall clock (the final hold plus the 500 ms stop grace; not investigated further). Within the 5 % bar for the 20 s runs; on the 8 s control it is exactly +5.00 %, which is outside "within" for a short run but that run had no duration bar. Documented in the docs page. Phase 7 may want to measure wall time to the stop's end or trim the hold.
- Resize: `Browser.setWindowBounds` was NOT refused in headless (both calls returned ok, bounds were 1880x1010 before). The emulation (1001x641, then clear) and window change (1101x701) all succeeded and the file decoded cleanly at 1280x720. Whether Chrome actually delivered differently sized frames was not measured (the driver does not log frame metadata sizes), so the evidence shows playability, not proof that frame sizes changed.
- Background tab: Chrome delivered zero frames and sent `screencastVisibilityChanged {visible:false}`; no deviation (the expected behaviour).
- Existing 06-01 tests updated: the stop test with zero frames now expects NoFrames; the unreadable-frames verdict assertion uses `contains` (new suffix).
- BSD sed note: README line and ROADMAP tick done with sed -i '' / python.
- `record_live.rs` uses `tracing_subscriber` (existing dependency) for stderr output only; no Cargo change.

## D6-12 driver justification
`tests/record_live.rs` is an `#[ignore]` integration test: invisible to `cargo test`, not shipped, no CLI command, no sixth tool, no Cargo.toml edit, public lib API only, attached to the same Chrome a real `bladebro nav` daemon launched. Resize emulation lives only there (harness comment says it must never move to src/); `grep Emulation|setWindowBounds src/record` finds nothing.

## Tests
`cargo test --release`: 610 -> 620 passed, 0 failed, 1 ignored (record_live). New: 2 watch table tests, 5 watch_tests scenarios (a-e), 3 report tests. fmt --check and `clippy --release --all-targets -D warnings` clean; `launch_mode::registry` 7 passed. The flake did not trigger.

## Open issues
- Stop latency still includes the 500 ms grace (from 06-01) and drives the duration offset above.
- Frame size changes not directly observed (see above).
- Linux/Windows not run.

## Self-Check: PASSED
