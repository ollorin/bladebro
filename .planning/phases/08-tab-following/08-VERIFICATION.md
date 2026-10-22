---
phase: 08-tab-following
verified: 2026-10-22T20:15:00Z
status: gaps_found
score: 3/4 success criteria fully verified (criterion 1 partial on popup wording; criterion 4 blocked by a failing test)
re_verification: false
gaps:
  - truth: "Test suite is green (cargo test --release, 720 expected)"
    status: failed
    failure_type: missing_test
    reason: "src/record/hub/per_tab_tests.rs::a_stop_is_bounded_when_one_encoder_ignores_eof fails at line 333 (count_alive(\"sleep 4242.7\") left 0, right 1) in 3 of 3 full-suite runs; it passes 3 of 3 when run alone. The assertion uses a system-wide pgrep -f after a fixed 300 ms sleep, so it races the fake ffmpeg shell exec'ing sleep under parallel load. Full-suite result: 719 passed, 1 failed (plain cargo test --release also stops after the lib target on this failure, so later targets are skipped)."
    artifacts:
      - path: "src/record/hub/per_tab_tests.rs"
        issue: "line 333 asserts on a pgrep count after a fixed sleep instead of polling (until(...)) for the hung encoder to be alive"
    missing:
      - "Replace the fixed sleep + count_alive == 1 assertion with a bounded poll until the hung `sleep 4242.7` process exists, then assert"
  - truth: "A popup rebinds capture into the same file (ROADMAP criterion 1 wording)"
    status: partial
    failure_type: semantic_stub
    reason: "A click-opened popup is NOT followed: the opener stays the agent's page and the popup becomes a segment only after the agent runs switch-tab (tests/record_follow.rs follow_popup asserts 'a link popup is not a segment'). Only window-feature popups are followed, and the docs say that path was 'read from the code, not measured live'. A documented, defensible reading of 'capture what the agent is looking at', but not the literal ROADMAP wording."
    artifacts:
      - path: "docs/recording-and-launch-config.md"
        issue: "lines 149-150 document the narrower behaviour; ROADMAP wording not amended"
    missing:
      - "Fork owner decision: amend ROADMAP Phase 8 criterion 1 to 'a popup becomes a segment once the agent switches to it', or implement following of click popups"
      - "Live measurement of the window-features popup path (currently code-read only)"
  - truth: "No stale Phase 7 text/harness assertions remain"
    status: partial
    failure_type: missing_artifact
    reason: "tools/record_sidecar/scenarios/hub_live.sh line 126 still asserts the Phase 7 note 'agent is now on tab'; that string no longer exists in src/, tests/ or docs/, so that check would fail. The doc itself is clean (no 'tab note' text)."
    artifacts:
      - path: "tools/record_sidecar/scenarios/hub_live.sh"
        issue: "line 126 greps for a removed note"
    missing:
      - "Update or remove that check (expect segments [...] / 'following tab' wording)"
---

# Phase 8: Tab Following Verification Report

**Phase Goal:** A recording keeps capturing what the agent is looking at when it switches tabs or a popup opens, and users who want separate files per tab can have them
**Verified:** 2026-10-22
**Status:** gaps_found
**Re-verification:** No, initial verification

## Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Default mode: switch-tab/open-tab rebind into the same file/encoder; record-stop lists segment boundaries with offsets | VERIFIED (live) | daemon scenario: one ffmpeg pid across the switch; segments [0.0 s tab A, 3.9 s tab B, 6.0 s tab A]; one vp9 1280x720 file, 80 decoded == 80 reported frames; colours red, blue, red |
| 1b | A popup rebinds into the same file | PARTIAL | click popups not followed until switch-tab (by design, asserted in tests/record_follow.rs follow_popup); window-features popups followed per code but not measured live. See gap 2 |
| 2 | Closing the recorded tab / cross-origin swap does not silently end the recording; outcome reported | VERIFIED (live) | close scenario: recording saved, segments [0.0 s B, 3.3 s A, 5.3 s B], note "tab A gone at ...; following tab B", exit 0, 74/74 frames, colours blue then red. Docs record that 127.0.0.1 to localhost kept the same tab id and frames |
| 3 | Per-tab opt-in: file per tab, record-stop lists all, cap 8, error isolation, bounded stop | VERIFIED (live + unit) | per-tab scenario: 3 ffmpeg while recording, 3 files -tab1/2/3-, each decodes cleanly, colours red/blue/green, header "recording saved (per-tab, 3 files)"; BLADE_RECORD_TABS=follow overrides the project file to one file. per-tab-close: closed tab's file finalized (ffprobe 3.0 s) while recording continued. Cap 8, isolation, bounded stop covered by unit tests in per_tab_tests.rs / follow_report_tests.rs, except the flaky test in gap 1 |
| 4 | Integration tests cover both modes | VERIFIED with caveat | tests/record_follow.rs plus fake-CDP unit tests for both modes; one new unit test fails in the full suite (gap 1) |

## Required Artifacts / Wiring

- record.tabs / BLADE_RECORD_TABS in the layered resolver (src/record/layered.rs: per-call, env, project, user, default follow), registered in src/launch_mode/registry.rs (ENV_VARS, RECORD_FILE_KEYS), covered by tabs_parse_table and tabs_precedence_default_and_errors tests; docs rows at lines 21, 165, 218, 234.
- docs/recording-and-launch-config.md has Tab following and per-tab sections; no "tab note" text remains.
- MCP schema unchanged: git diff bb9e7dd..HEAD has no src/mcp or src/cli paths.
- Upstream hooks: diff lists only .planning, docs, src/record/*, src/launch_mode/registry.rs, tests/record_*, tools/record_sidecar. git cat-file -e upstream/main:<path> failed for every non-.planning path (upstream/main = 6b524d2), so no upstream file edited; HOOKS.md unchanged, consistent with zero new hooks.
- No Cargo/npm/CHANGELOG/.github changes; no EXECUTION_LOG, .DS_Store or target tracked.
- All 15 commits in bb9e7dd..HEAD carry a Co-authored-by trailer.

## Commands run

- cargo fmt --check: clean. cargo clippy --release --all-targets -- -D warnings: clean.
- cargo test --release --no-fail-fast: 719 passed, 1 failed (a_stop_is_bounded_when_one_encoder_ignores_eof), expected 720. Failed 3 of 3 full runs, passed 3 of 3 alone: load/timing race. concurrent_cold_starts_have_one_owner did not fail. One earlier full run hung about 46 min in the lib binary (no children; I killed it); not reproduced in two later runs, unexplained.
- Live (BLADE_HEADLESS=1, scratch BLADE_HOME, BLADE_NO_UPDATE_CHECK=1, BLADE_RECORD=1, local fixtures) via follow_live.sh --only daemon|close|per-tab|per-tab-close: every functional check passed. The "[final] leftovers" FAIL in each run was a stray `bladebro daemon` (BLADE_HOME bladebro-daemon-race-22701) leaked by the existing daemon_lifecycle test of the cargo run, not the scenarios; killed, and ps then showed no bladebro, ffmpeg, Chrome-scratch or fake-enc processes.
- ffprobe/ffmpeg verification by the harness: vp9 1280x720, clean full decode, decoded frames == reported frames, per-segment colours.

## Human verification

None for automated criteria; fork owner decision needed on popup wording (gap 2).

## Gaps Summary

Both modes work end to end live. Three items block a clean pass: a Phase 8 unit test that fails in the full suite (fix the race), popup behaviour narrower than the ROADMAP wording and partly unmeasured, and a stale Phase 7 assertion in hub_live.sh.

---

_Verified: 2026-10-22_
_Verifier: Claude (gsd-verifier)_
