---
phase: 11-integration-docs
verified: 2026-10-26T16:13:00Z
verifier: opus
status: passed
score: 5/5 success criteria verified
re_verification:
  previous_status: gaps_found
  previous_score: 4/5
  gaps_closed:
    - "Line 82: the animated-page drift now reads '+0.6 % to +1.2 % on 8 to 16 s recordings'. Checked against 06-VERIFICATION.md lines 42-44: bg-control 8 s +1.24 % (8100 vs 8001), trace 12 s +0.83 % (12100 vs 12000), resize 16 s +0.61 % (16100 vs 16002)"
    - "Line 702: the post-fix animated-page drift is now credited to .planning/phases/06-frame-source/06-VERIFICATION.md lines 42-44 (the lines hold those three runs). 06-05-live.txt is kept for the tick drift only. 06-02-live.txt is named for the dur-anim frame rate (1200 frames in 20 s) and the pre-fix drift 'up to +6.24 %' (06-02-live.txt line 51: bg-control drift_pct=+6.24)"
    - "Line 66 (previous info): 'about 0.6 s', which matches 07-VERIFICATION +0.66 s and the earlier CLI runs at +0.62 to +0.70 s"
    - "Line 166 (previous warning): 'no not-recorded note', which matches 08.1-02-live.txt line 148"
  gaps_remaining: []
  regressions: []
non_blocking:
  - "Line 702: 88ea6f0 left a duplicated fragment in the sentence: '... (`06-02-live.txt`, the `[bg]` scenario, headless) result (0 frames in 8 s, Chrome reported the tab hidden) is from Phase 6 (`06-02-live.txt`, the `[bg]` scenario, headless); ...'. Both copies are true, so the line is garbled but not false. Remove the second 'result (...) is from Phase 6 (...)' copy"
  - "Lines 204 and 237: the exit-path finalize is given as 'about 16 s' (10 + 5 + 1). The encoder also has CHILD_WAIT_FLOOR = 2 s (src/record/encoder/process.rs:44, 286), so the worst case is about 18 s, as line 73 says. The user-facing timeout advice (21 s on lines 134, 365, 729; client wait 30 s on line 237) still covers it, so nobody following the docs is misled. The same 16 s appears in src/record/exit.rs:11 and exit_tests.rs:217"
---

# Phase 11: Integration Docs Re-verification Report (opus, adversarial)

**Phase Goal:** The QA flows that consume Bladebro can turn recording on per test case by following the docs, with no extra model effort
**Verified:** 2026-10-26T16:13:00Z
**Status:** passed
**Re-verification:** Yes, after docs-only fix 88ea6f0 (previous: gaps_found 4/5, commit 3db97d2)

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Docs show the Claude Code GSD flow with BLADE_RECORD, record-start/stop and where the path appears | VERIFIED | Unchanged by 88ea6f0, which touched only lines 66, 82, 166 and 702. ~/claude/get-shit-done is clean at 73f7629. The sample outputs on lines 438 and 456-458 match 11-02-q01-qa-flows.txt (bytes 53535, 4.6 s, 46 frames, captured 35) |
| 2 | Docs show the DSH plugin flow doing the same | VERIFIED (static plus stub, as before) | Unchanged. npm/bladebro/dsh-plugin.mjs:25 `toolCallTimeoutMs: 180000` matches lines 467, 486 and 729 |
| 3 | README replaces "Not supported" with a short pointer | VERIFIED | `git diff 42ca29b..HEAD --stat -- README.md .planning/HOOKS.md`: README 8 lines (4+/4-), HOOKS 4+. No change since 3db97d2 |
| 4 | Following each snippet against a fixture produces a recording at the documented path | VERIFIED | doc_snippets check: 30 blocks, 0 failing. No src change since 9d20e81, so the earlier live runs (15:45Z, 16:02Z) still cover this tree. Every sample-output figure sampled in this pass is found verbatim in its 11-0x evidence file (table below) |
| 5 | docs/recording-and-launch-config.md is complete and accurate; examples run; drift tests pass | VERIFIED | All previous gaps are closed against evidence. The final pass sampled 25 more numeric and provenance claims and found none false. Two non-blocking nits are listed below. Drift tests: 14/14 |

**Score:** 5/5

### Previous gaps (88ea6f0)

| Item | Result |
|---|---|
| Line 82 "8 to 12 s" | CLOSED: "8 to 16 s". 06-VERIFICATION.md 42-44: 8 s +1.24 %, 12 s +0.83 %, 16 s +0.61 %, each about +100 ms |
| Line 702 animated-page drift source | CLOSED: names 06-VERIFICATION.md lines 42-44 (the three runs are on exactly those lines: bg, trace, resize). It says no raw file holds them, which is true. Grep of 06-0x-live.txt for 12100/16100/8100 finds no post-fix values |
| Line 702 06-02-live.txt role | CLOSED: frame rate (dur-anim, line 17: frames_received 1200, forwarded 200) and pre-fix drift up to +6.24 % (line 51) are both in that file |
| Line 702 tick drift | CLOSED: 06-05-live.txt only (-0.74 % to +0.16 %, checked before) |
| Line 66 "about 0.6 s" | CLOSED: 07-VERIFICATION +0.66 s; CLI runs +0.62 to +0.70 s |
| Line 166 "no not-recorded note" | CLOSED: 08.1-02-live.txt line 148 |

### Final pass: numeric and provenance claims not sampled in earlier passes

The earlier "checked" lists come from c026563, c3b337f, 266e3e6, fe413b0 and 3db97d2. None of the claims below was in them.

| Claim (line) | Evidence | Result |
|---|---|---|
| Poster ffmpeg about 0.07 s, 15 runs, max 0.074 s (300) | 10-01-live.txt 97-98 (15 values, max 0.074) | TRUE |
| record-stop median 0.632 s to 0.654 s, 3 runs each (300) | 10-01-live.txt 95 (base 0.655/0.632/0.62, new 0.649/0.654/0.661) | TRUE |
| Five clicks, colour change about 0.3 s before end_s; exploratory -0.2 to -0.3 s; end_s + 0.4 new colour, at_s - 0.3 old colour (342) | 10-02-live.txt 6-10, 39-44, 65-70, 96-98 | TRUE |
| Every first new-colour frame inside [at_s, end_s] (342) | for example webm blue: at_s 1.52 (1.22 + 0.3), first blue 1.96, end_s 2.26 | TRUE (sampled) |
| Per-tab file 2 start 3.91 s; click at local 0.67 s, colour at local 1.03 s (344) | 10-02-live.txt 91, 95, 98 | TRUE |
| Recording off: 11 distinct methods, dispatchMouseEvent base 23/30, new 34/30, stdout byte-identical, base d18dae1 (346) | 10-02-live.txt 113-137; 10-02-gate.txt 55-59 | TRUE |
| `run` with 3 steps is one entry (340) | 10-02-live.txt 76 | TRUE |
| Audit 100/100 both modes, macOS 26.6.2, Chrome 154.0.8037.58 (42, 45) | 04.1-03-SUMMARY.md 24-26; 04.1-VERIFICATION.md 132 | TRUE |
| Headless window 1880x1010, outer 1896x1010 at screenX/Y 22, ends 1918x1032, screen 1920x1080 (44) | src/launch_mode/display.rs 33-34; 04.1-06-SUMMARY.md 6 (22 + 1896 = 1918, 22 + 1010 = 1032) | TRUE |
| Artifacts rotate at 300 files (59) | src/artifacts.rs:25 MAX_ARTIFACTS = 300 | TRUE |
| Stop worst case about 18 s = 10 + 2 + 5 + 1 (73); 21 s with the poster (134) | encoder/process.rs 34-44, 286-300; poster.rs:31 | TRUE |
| Exit finalize worst case about 16 s (204, 237) | the same code path also has the 2 s CHILD_WAIT_FLOOR, so about 18 s | IMPRECISE (non-blocking, see below) |
| Idle default 600 s, checked every 15 s (214) | cli/daemon.rs 143-147; mcp/server/serve.rs:50 | TRUE |
| `bladebro stop` client waits up to 30 s (237) | cli/daemon.rs:577 read timeout 30 s | TRUE |
| Orphan ffmpeg SIGTERM then up to 2 s (274) | record/orphans.rs:38 TERM_WAIT_MS 2000 | TRUE |
| fps 5-15, size 160x120 to 3840x2160, defaults 10 / 1280x720 (20-21) | record/values.rs 27-51 | TRUE |
| Marker and verdict cut to 200 chars; timeline 1000 entries (302, 315-316) | marker.rs:16, timeline.rs 28-30 | TRUE |
| Project/record config 64 KiB; walk 64 levels (549, 554, 590) | launch_mode/project.rs:14, find.rs:11, record/config.rs:20 | TRUE |
| Letterbox tested with 1280x720 and 500x400 into one WebM and one MP4 (194) | encoder/size_change_tests.rs 78, 161, 166 | TRUE |
| 5 s first-frame warning (81) | source/watch.rs:16 NO_FRAMES_AFTER 5 s | TRUE |
| Sample outputs: 12952 B (109); 36204 B / 3.8 s / 38, Marker 3.7 s (115); at_s 1.49, marker 3.69 (328-330) | 11-01-s01-mcp-state.txt | TRUE |
| Tab samples 41830 B / 4.9 s; 37934 B / 4.8 s / 48, 0AEC at 1.7 s (153-154); per-tab 8976 B / 3.2 s / 32 and 10793 B / 1.7 s (283) | 11-01-s03-tabs.txt | TRUE |
| CLI sample 27424 B / 0.8 s / 8 frames (682) | 11-01-s02-cli.txt | TRUE |
| 11-03 recipes: wait 3/3 at 11 frames, 1.1 s; repaint 2/2 at 8 and 9 frames, 0.8 and 0.9 s (385, 398) | 11-03-no-frames.txt (five `recording saved` lines) | TRUE |
| switch-tab followed within about 0.25 s (160) | follow.rs POLL 200 ms + CONFIRM 50 ms. Phase 8 evidence holds no separate timing line | CONSISTENT with code (not re-measured) |

### Required Artifacts

| Artifact | Status | Details |
|---|---|---|
| `docs/recording-and-launch-config.md` | VERIFIED | 729 lines. Previous gaps closed. 2 non-blocking nits |
| `evidence/snippet-map.tsv` | VERIFIED | 30 blocks pass |
| `evidence/11-03-no-frames.txt` + `scripts/q03-no-frames.sh` | VERIFIED | unchanged |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `.mcp.json` env BLADE_RECORD=1 | recording on in the MCP server | process env | WIRED (unchanged) |
| per-tab stop text | doc's per-tab form / substring `no frames reached the encoder` | stop result | WIRED (unchanged) |
| Measurements bullet | cited evidence files | file names and line numbers | WIRED: every cited figure is in the file named for it |

### Scope checks

- `git diff 3db97d2..HEAD --name-only`: only docs/recording-and-launch-config.md (88ea6f0, 4 lines).
- `git diff 42ca29b..HEAD --name-only -- src`: only src/record/hub/{evidence,evidence_tests,finish,msg}.rs. `git log 42ca29b..HEAD -- src`: only 9d20e81.
- README / HOOKS: as in Truth 3, unchanged since 3db97d2.
- ~/claude/get-shit-done is clean at 73f7629.

### Test suite (Step 8b)

- `cargo fmt --check` OK. `cargo build --release` OK. Clippy was not re-run because nothing in src has changed since the clean run.
- Drift tests (`launch_mode::registry`, `record::hub::schema_tests`): 14 passed.
- `cargo test --release --no-fail-fast`: lib 847 passed, and every integration target passed except one failure, `tests/daemon_lifecycle.rs::concurrent_cold_starts_have_one_owner` (panic at line 204, its fixed 500 ms sleep before reading cli.pid). This is the known upstream flake: last changed in 8b3d82d, recorded in 06-VERIFICATION and in fe413b0's report, and no src changed in this phase. Re-ran `--test daemon_lifecycle` 3 times: 5/5 passed each time. The flake leaked one daemon (scratch BLADE_HOME `bladebro-daemon-race-66431`); it was killed.
- `doc_snippets.py check .../snippet-map.tsv docs/recording-and-launch-config.md`: 30 blocks, 0 failing.
- Layer-1 gates: scripts/ holds only publish-npm.sh, so N/A. Charlotte QA and E2E: not a web project and no UI files, so N/A.
- After the runs: `pgrep -fl "bladebro|ffmpeg"` shows nothing.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| docs/recording-and-launch-config.md | 702 | duplicated fragment "result (0 frames in 8 s, ...) is from Phase 6 (...)", left by 88ea6f0 | Non-blocking (garbled wording, both copies true) | delete the second copy |
| docs/recording-and-launch-config.md | 204, 237 | exit-path finalize "about 16 s"; code worst case about 18 s (CHILD_WAIT_FLOOR 2 s), as line 73 says | Non-blocking (precision; timeout advice of 21 s / 30 s still covers it) | say "about 18 s" (src/record/exit.rs:11 has the same 16 s) |
| src/record/hub/msg.rs | 138-143 | agent_vs_video note on a zero-frame stop | Warning (pre-existing, documented, deferred) | unchanged |
| tests/daemon_lifecycle.rs | 202-204 | fixed 500 ms sleep; flaked once and leaked a daemon | Warning (upstream, not Phase 11) | none for this phase |

No logical_cohesion findings.

### Human Verification Required

None beyond the disclosed Not verified items (Linux, Windows, headed Chrome, DSH host, Claude Code session).

### Gaps Summary

88ea6f0 closes every item in the previous report. Line 82 now gives the correct 8 to 16 s range. Line 702 names the file that actually holds each drift figure: 06-VERIFICATION.md lines 42-44 for the post-fix animated drift, 06-05-live.txt for the tick drift, and 06-02-live.txt for the frame rate and the pre-fix drift. Lines 66 and 166 take the suggested wording. The final pass checked 25 claims not sampled before: Phase 10 timings and alignment, Phase 4.1 audit and geometry, source constants, and every sample output against its 11-0x evidence. None is false. Two non-blocking nits remain: a duplicated fragment on line 702 introduced by the fix, and "about 16 s" on lines 204 and 237 where the code's worst case is about 18 s. Neither misleads a user following the docs. The one test failure is the known upstream daemon_lifecycle flake, and it passed 3 of 3 on re-run.

_Verified: 2026-10-26T16:13:00Z_
_Verifier: Claude (gsd-verifier, opus)_
