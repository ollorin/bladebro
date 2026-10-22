# 06-03 Summary: Live wire trace, slow consumer, stealth instruments, gates

Requirements: SAFE-01, CAPT-02. ROADMAP Phase 6 SC2 (live) and SC5. macOS arm64, headless Chrome 154, local 127.0.0.1 fixtures only, scratch BLADE_HOME per run. Not executed: Linux (Xvfb cadence, 111/111 audit), Windows, CI, diff_oracle. No `src/` change, no Cargo/npm/CHANGELOG/.github change, no new crate. No user-facing change, so docs/recording-and-launch-config.md is unchanged.

## Commits
- cc01253 test(06-03): live CDP wire trace and slow-consumer checks for the screencast source (Task 1)
- a235692 test(06-03): audit and battery A/B with the screencast source attached (Task 2)
- docs(06-03): phase 6 gate evidence (Task 3; hash in git log)
- docs(06-03): complete plan summary (this file + ROADMAP tick)

## Results
| instrument | bar | measured | result |
|---|---|---|---|
| wire trace (test-only WS proxy) | only Page.enable/startScreencast/screencastFrameAck/stopScreencast, enable first | {Page.enable:1, Page.startScreencast:1, Page.screencastFrameAck:720, Page.stopScreencast:1}; first = enable, startScreencast, then acks; wire == Source methods_sent; wire acks 720 == acks_sent 720 == frames_received 720 | PASS |
| Chrome argv | identical before/during/after | 531 bytes each, diff identical both ways | PASS |
| slow consumer (300 ms/frame, queue 16, 25 s) | queue <= 16, drops counted, frames == acks, CLI works | max_len 16; refused 147 == source queue_dropped 147; accepted 99 == forwarded 99; frames_received 1484 == acks_sent 1484; capped 1238; see content / state tabs / nav at +5.1/+10.1/+15.3 s: rc 0, 50/47/348 ms; frames continued after the same-tab nav (last frame 9.7 s after it); file decodes (249 frames, drift -0.41 %) | PASS |
| audit | 100/100 baseline and with recorder | without 100/100 exit 0; with recorder (attempt 1 of max 8) 100/100 exit 0, recorder 5 frames, 5 acks (browser closed after ~1.25 s: ConnectionLost, expected) | PASS |
| battery A/B/A2 (recorder attached, anim.html, 186 frames, 186 acks during B) | divergent 0 | keys 99, divergent [], noise [voices] | PASS |
| diff_oracle | divergent 0 | NOT RUN: python `websockets` missing, installs forbidden | not claimed |
| qol_probe (BLADE_HEADLESS=1) | not worse than 120/122 | 120 passed, 2 failed: 'chord cleared the live editor', 'nested scroller movement is reported as such' | PASS (= baseline) |
| fmt / clippy --all-targets -D warnings | clean | clean | PASS |
| cargo test --release | > 565 and > 620 | 622 passed, 0 failed, 1 ignored (record_live); flake did not trigger | PASS |
| SYNC-01 | src diff only HOOKS files | all listed files have HOOKS rows (or are 4.1 launch_mode); Phase 6 upstream edit: src/cdp/client.rs (57db2b5) only | PASS |
| leftovers after every run | 0 | 0 daemon/Chrome/ffmpeg/record_live/http.server of ours; none killed | PASS |

## Frame-size evidence (06-02 open item, now closed)
The driver now taps the sink and reads the JPEG SOF of every frame (`frame_sizes` in the report). A rerun of the resize scenario (evidence/06-03-resize.txt) shows Chrome did deliver different sizes mid-recording: 1280x628 (0 to 12.0 s), 1001x641 (4.3 to 9.0 s, the emulation), 1126x628 (one frame at 12.6 s) and 1101x614 (12.7 to 16.0 s, the window change). The output still decodes cleanly (164 frames, exit 0, 0 warnings) at 1280x720. Normal runs deliver 1280x628 (page scaled to the 1280x720 box).

## Deviations
- Driver extras beyond the plan (test code only): `frame_sizes` for all modes, `last_frame_ms` in source_json, scenario `all`, evidence file chosen by first argument. `bash tools/record_sidecar/scenarios/source_live.sh trace|slow|all` writes 06-03-trace.txt.
- Parser unit tests: `method_of` and `jpeg_size` (2 tests; test count 620 to 622).
- The audit part B recorder ran only ~1.25 s (the audit browser closes quickly); the bar (frames > 0, frames == acks, equal score) is met but the attachment window is short. The battery B (3 s, 186 frames) is the longer stealth exposure.

## Findings / open issues
- Stop latency (500 ms grace) and +0.4-0.5 s video duration offset from 06-02 remain (Phase 7 candidate).
- No frame loss after a same-tab nav in this run (the same-process nav keeps the page WebSocket); cross-process navigation and tab swaps remain Phase 8 (CAPT-04).
- Linux/Windows/CI, diff_oracle and the 111/111 audit figure not executed here.

## Self-Check: PASSED
