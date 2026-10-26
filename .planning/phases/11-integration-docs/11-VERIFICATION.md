---
phase: 11-integration-docs
verified: 2026-10-26T15:57:00Z
verifier: opus
status: gaps_found
score: 4/5 success criteria verified (SC1-SC4 verified; SC5 failed on accuracy, 1 source misattribution introduced by 945415e)
re_verification:
  previous_status: gaps_found
  previous_score: 4/5
  gaps_closed:
    - "Line 698 no longer says all earlier-phase figures were headless; it says Phase 2 was headed and later phases headless (true: RESULTS.md line 3, core.json headless=false)"
    - "Line 702 now names Phase 2 (RESULTS.md, core.json, extra.json, phase-02 deferred-items.md) for the about-100 headed fps (99.31), the minimized 0.3 frames/s (extra.json minimized.window_8s_to_18s) and the 37 s silence (deferred-items.md line 13-14). All three checked and true"
    - "Line 706 now excludes prose figures labelled 11-01/11-02/11-03 from the earlier-phase statement"
  gaps_remaining: []
  regressions:
    - "Line 702 now credits 'the background-tab result' to Phase 2 (headed, results/extra.json) and names Phase 6 only for 'the headless frame rate and drift'. The page's specific background-tab result (line 81: '0 frames in 8 s, and Chrome reported the tab hidden') is Phase 6 headless evidence (06-02-live.txt [bg]), not Phase 2"
gaps:
  - truth: "The provenance statements in Measurements are true against the cited evidence"
    status: failed
    failure_type: semantic_stub
    reason: "Line 81 says 'A background tab behaved this way in testing: 0 frames in 8 s, and Chrome reported the tab hidden.' That comes from .planning/phases/06-frame-source/evidence/06-02-live.txt [bg] (lines 30 and 45: headless, frames_received 0, health_at_stop NoFrames { for_ms: 8003, hidden: true }). Since 945415e, line 702 assigns 'the background-tab result' to Phase 2 (tools/record_sidecar/results/extra.json, measured headed), and the Phase 6 sentence covers only 'the headless frame rate and drift'. Phase 2's background runs do not support the line-81 figure: extra.json background.headed has 793 frames in the 8 s before the switch and 0 in the 16 s after; background.headless has 480 before and 0 after; neither records a hidden flag or a no-first-frame outcome. A reader who follows the cited source finds 8 s with frames, the opposite of the claim, and is told it was headed when it was headless. The previous report asked to keep Phase 6 for 'the 8 s background tab'; 945415e moved it instead. Phase 2 does support the general line-82 limit ('a background tab ... delivers no new frames'), so the fix is to name both."
    artifacts:
      - path: "docs/recording-and-launch-config.md"
        issue: "line 702 names the wrong source (Phase 2, headed) for the line-81 background-tab figure (0 frames in 8 s, hidden), which is Phase 6 headless (06-02-live.txt [bg])"
    missing:
      - "Line 702: name Phase 6 (06-02-live.txt [bg], headless) as the source of the line-81 background tab (0 frames in 8 s, Chrome reported it hidden); keep Phase 2 extra.json only for the line-82 limit (frames stop after the tab is backgrounded, headed and headless)"
      - "Optional, same sentence: also name Phase 6 (06-02-live.txt dur-idle) for the 1 frame on a 20 s still page, which is neither a frame rate nor drift"
---

# Phase 11: Integration Docs Re-verification Report (opus, adversarial)

**Phase Goal:** The QA flows that consume Bladebro can turn recording on per test case by following the docs, with no extra model effort
**Verified:** 2026-10-26T15:57:00Z
**Status:** gaps_found
**Re-verification:** Yes, after docs-only fix 945415e (previous: gaps_found 4/5, commit 266e3e6)

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Docs show the Claude Code GSD flow with BLADE_RECORD, record-start/stop and where the path appears | VERIFIED | Unchanged since 266e3e6 (945415e touches only lines 698-706). ~/claude/get-shit-done is clean at 73f7629 |
| 2 | Docs show the DSH plugin flow doing the same | VERIFIED (static plus stub, as before) | Unchanged |
| 3 | README replaces "Not supported" with a short pointer | VERIFIED | `git diff 42ca29b..HEAD -- README.md .planning/HOOKS.md`: only the Gotchas row and the fork-owned "## Recording" block in README, plus the two Phase 11 ledger rows and the scope note in HOOKS.md. Both rows match the README diff |
| 4 | Following each snippet against a fixture produces a recording at the documented path | VERIFIED | No src or binary change since the last live run (15:45Z, build 69fbd40: q03 short case 3/3 follow plus 1/1 per-tab, wait recipe 3/3, repaint recipe 2/2, all headless with a scratch BLADE_HOME). `git log 9d20e81..HEAD -- src` is empty, so that run still covers this tree. Not re-run |
| 5 | docs/recording-and-launch-config.md is complete and accurate; examples run; drift tests pass | FAILED (accuracy) | doc_snippets check: 30 blocks, 0 failing. Drift tests 14/14 ok. The three previous sub-gaps are closed. 945415e misattributes the line-81 background-tab figure (see gap) |

**Score:** 4/5

### Previous gap (lines 698, 702, 706)

| Part | Result |
|---|---|
| 698 "headless Chrome" for all earlier phases | CLOSED: "Phase 2 figures were measured headed, later phases headless". True for Phase 2 (RESULTS.md line 3) and for Phases 6, 8.1, 9 and 10 (headless evidence) |
| 702 wrong source for headed ~100 fps, minimized 0.3 fps, 37 s silence | CLOSED: RESULTS.md line 22 (99.31 fps headed), extra.json minimized.window_8s_to_18s (3 frames in 10 s = 0.3), phase-02 deferred-items.md lines 13-14 (37 s after 23 s, 1 of 7) |
| 706 blanket "prose figures are earlier-phase" | CLOSED: now excludes 11-0x-labelled figures |
| 702 "the background-tab result ... from Phase 2" (new) | REGRESSION: see gap |

### Sampled numeric claims vs evidence

| Claim (line) | Evidence | Result |
|---|---|---|
| Phase 2 size column 0.87 / 0.54 / 1.15 / 1.12 MB/min (263-266) | RESULTS.md codec matrix: 870,778 / 535,374 / 1,150,851 / 1,124,821 B | TRUE |
| 2 min at largest about 5 MB; 2 h about 80 MB / 320 MB (256, 270) | 2.67 x 2 = 5.3; 0.67 x 120 = 80; 2.67 x 120 = 320 | TRUE |
| Headless 60 fps, 1200 frames, 200 to the encoder in 20 s (80) | 06-02-live.txt dur-anim: frames_received 1200, forwarded 200 | TRUE (Phase 6, correctly named) |
| 1 frame on a 20 s still page (82) | 06-02-live.txt dur-idle: frames_received 1 | TRUE (Phase 6; not named, but covered by "earlier phases") |
| Drift -0.74 % to +0.16 % over 8 runs (82) | 06-05-live.txt summary line 82 | TRUE |
| 0 frames in 8 s, tab hidden (81) | 06-02-live.txt [bg]: NoFrames for_ms 8003, hidden true (headless). Not in Phase 2 extra.json | TRUE figure, WRONG SOURCE named on line 702 |
| Name rules: evil_name_d, 100 cut to 80, `...` to recording-, ok.name_1-2 kept (536) | 11-01-s02-cli.txt lines 95-107 (a-run lengths 100 and 80) | TRUE |
| Poster pixel rgb(2, 65, 252) (690) | 11-01 evidence: "poster 1x1 rgb ... 2 65 252" | TRUE |
| 0.8 s video, ffprobe 0.800000 for both runs (692) | two 0.800000 lines in 11-01 evidence | TRUE |
| Chrome 154 for Phase 11 (706) | 11-01-s01-mcp-state.txt: Chrome/154.0.8037.58 | TRUE |

### Required Artifacts

| Artifact | Status | Details |
|---|---|---|
| `docs/recording-and-launch-config.md` | SUBSTANTIVE, 1 accuracy gap | line 702 |
| `evidence/11-03-no-frames.txt` + `scripts/q03-no-frames.sh` | VERIFIED | reproduced at 15:45Z on the same src |
| `evidence/snippet-map.tsv` | VERIFIED | 30 blocks pass |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `.mcp.json` env BLADE_RECORD=1 | recording on in the MCP server | process env | WIRED |
| per-tab stop text | doc's per-tab form / substring `no frames reached the encoder` | stop result | WIRED |
| Measurements bullet | cited evidence files | file names | 1 misattribution (background tab) |

### Scope checks

- `git diff 42ca29b..HEAD --name-only -- src`: only src/record/hub/{evidence,evidence_tests,finish,msg}.rs; `git log 42ca29b..HEAD -- src` lists only 9d20e81.
- `git diff 266e3e6..HEAD --name-only`: only docs/recording-and-launch-config.md.
- Read-only repos: ~/claude/get-shit-done clean at 73f7629. ~/claude/gsd-dsh has only its pre-existing changes (README.md modified, upstream-updates.md untracked, both mtime 2026-10-20 10:43, before Phase 11).
- `BLADE_NO_WARMING` occurs 0 times in docs/recording-and-launch-config.md.

### Test suite (Step 8b)

- `cargo build --release` OK, `cargo fmt --check` OK, `cargo clippy --release -- -D warnings` clean.
- Drift tests (`launch_mode::registry`, `record::hub::schema_tests`): 14 passed.
- `cargo test --release --no-fail-fast`: lib 847 passed; one failure in `tests/daemon_lifecycle.rs::concurrent_cold_starts_have_one_owner` (line 204: `cli.pid` read 500 ms after spawning 8 daemons, while the machine was busy with the full suite). Re-ran `--test daemon_lifecycle` 5 times: 5/5 ok. This is an upstream timing-sensitive test (last touched in 8b3d82d), and no src or tests file changed since the previous all-green run, so it is a flake and not a Phase 11 defect. Its panic left an orphan `bladebro daemon` (pid 50854, scratch race home), which was killed.
- Layer-1 gates: scripts/ holds only publish-npm.sh, so N/A. Charlotte QA and E2E: not a web project and no UI files, so N/A.
- After the run: no bladebro, ffmpeg or scratch Chrome processes left.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| docs/recording-and-launch-config.md | 702 | wrong source named (Phase 2 headed) for the line-81 background-tab figure (Phase 6 headless) | Blocker (doc honesty) | gap |
| tests/daemon_lifecycle.rs | 202-204 | fixed 500 ms sleep before reading cli.pid; flaked once under load and leaked a daemon | Warning (upstream, not Phase 11) | none for this phase |
| src/record/hub/msg.rs | 138-143 | agent_vs_video note on a zero-frame stop | Warning (pre-existing, documented, deferred) | unchanged |

No logical_cohesion findings.

### Human Verification Required

None beyond the disclosed Not-verified items (Linux, Windows, headed Chrome, DSH host, Claude Code session).

### Gaps Summary

945415e fixes the three parts of the previous gap: the page now says Phase 2 was headed, names Phase 2 for the headed ~100 fps, the minimized 0.3 frames/s and the 37 s silence (all checked against RESULTS.md, extra.json and deferred-items.md), and the "earlier phases" sentence excludes 11-0x-labelled figures. One new misattribution: line 702 credits "the background-tab result" to Phase 2's headed extra.json and limits Phase 6 to "frame rate and drift". The page's specific background-tab figure (line 81: 0 frames in 8 s, Chrome reported the tab hidden) is Phase 6 headless (06-02-live.txt [bg]). Phase 2's background runs had frames during those first 8 s and recorded no hidden flag. This is a one-sentence fix on line 702. Every other sampled figure matched its evidence.

Note: `gsd-tools.js eval-candidate` is not installed at ~/.claude/get-shit-done/bin, so no regression candidates were written.

_Verified: 2026-10-26T15:57:00Z_
_Verifier: Claude (gsd-verifier, opus)_
