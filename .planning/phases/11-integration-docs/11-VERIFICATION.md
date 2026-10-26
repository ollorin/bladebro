---
phase: 11-integration-docs
verified: 2026-10-26T16:04:00Z
verifier: opus
status: gaps_found
score: 4/5 success criteria verified (SC1-SC4 verified; SC5 failed on accuracy, 1 drift-figure mismatch in line 82 and its source on line 702)
re_verification:
  previous_status: gaps_found
  previous_score: 4/5
  gaps_closed:
    - "Line 702 now credits the line-81 background-tab figure (0 frames in 8 s, Chrome reported the tab hidden) to Phase 6 (06-02-live.txt [bg], headless). Checked: 06-02-live.txt lines 30 and 45, health_at_stop NoFrames { for_ms: 8003 / 8002, hidden: true }, frames_received 0. Phase 2 keeps only the headed ~100 fps, minimized 0.3 frames/s and 37 s silence, all three checked before"
  gaps_remaining: []
  regressions: []
gaps:
  - truth: "The drift figures on the page match their evidence, and the Measurements section names the file that holds them"
    status: failed
    failure_type: semantic_stub
    reason: "Line 82 says 'On animated pages the video ran about 100 ms long (+0.6 % to +1.2 % on 8 to 12 s recordings)'. The post-fix animated-page measurements are trace 12 s +0.83 % (12100 vs 12000), bg-control 8 s +1.24 % (8100 vs 8001) and resize 16 s +0.61 % (16100 vs 16002). The +0.6 % run was 16 s long, so '8 to 12 s' is wrong (8 to 16 s). These three numbers exist only in .planning/phases/06-frame-source/06-VERIFICATION.md (lines 42-44, 66: verifier re-runs, no raw evidence file). Line 702 names 06-02-live.txt and 06-05-live.txt for 'the headless frame rate and drift'. 06-05-live.txt holds only the tick-page drifts (-0.74 % to +0.16 %, true). 06-02-live.txt holds the pre-fix animated drifts (dur-anim +1.99 %, resize +3.12 %, bg-control +5.00 % / +6.24 %). A reader who follows the citation for the animated-page figure finds numbers 2 to 5 times larger, from before the fix the same paragraph describes. The text dates from Phase 6 (b079fac), but SC5 is 'complete and accurate', and the provenance sentence on line 702 is Phase 11 text."
    artifacts:
      - path: "docs/recording-and-launch-config.md"
        issue: "line 82: '8 to 12 s recordings' should be '8 to 16 s' (the +0.6 % run was the 16 s resize run)"
      - path: "docs/recording-and-launch-config.md"
        issue: "line 702: the animated-page drift is credited to 06-02-live.txt / 06-05-live.txt; it comes from the Phase 6 verifier re-runs in 06-VERIFICATION.md (lines 42-44), and 06-02-live.txt holds the pre-fix values"
    missing:
      - "Line 82: change '8 to 12 s' to '8 to 16 s' (or list the three runs: 8 s +1.24 %, 12 s +0.83 %, 16 s +0.61 %)"
      - "Line 702: name .planning/phases/06-frame-source/06-VERIFICATION.md (Phase 6 verifier re-runs, lines 42-44) as the source of the animated-page drift, keep 06-05-live.txt for the tick-page drift, and say 06-02-live.txt holds the frame rate (dur-anim) and the pre-fix drift"
---

# Phase 11: Integration Docs Re-verification Report (opus, adversarial)

**Phase Goal:** The QA flows that consume Bladebro can turn recording on per test case by following the docs, with no extra model effort
**Verified:** 2026-10-26T16:04:00Z
**Status:** gaps_found
**Re-verification:** Yes, after docs-only fix b172171 (previous: gaps_found 4/5, commit fe413b0)

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Docs show the Claude Code GSD flow with BLADE_RECORD, record-start/stop and where the path appears | VERIFIED | Unchanged (b172171 touches only line 702). ~/claude/get-shit-done is clean at 73f7629 |
| 2 | Docs show the DSH plugin flow doing the same | VERIFIED (static plus stub, as before) | Unchanged. `dsh` is still not on PATH, as the Not verified section says |
| 3 | README replaces "Not supported" with a short pointer | VERIFIED | `git diff 42ca29b..HEAD -- README.md .planning/HOOKS.md`: Gotchas row plus the fork-owned Recording block in README; two Phase 11 ledger rows and a scope note in HOOKS.md, matching the README diff. README's "about 0.7 MB per minute at the defaults" matches 09-03-sizes.txt (0.67) |
| 4 | Following each snippet against a fixture produces a recording at the documented path | VERIFIED | doc_snippets check 30 blocks, 0 failing. No src change since 9d20e81, and the live run at 15:45Z covers this tree. New live run today (16:02Z, scratch BLADE_HOME, headless, daemon lane): 3/3 `recording saved` on anim.html |
| 5 | docs/recording-and-launch-config.md is complete and accurate; examples run; drift tests pass | FAILED (accuracy) | Snippets and drift tests pass, and the previous gap is closed. Line 82 gives the wrong length range for the animated-page drift, and line 702 names a source file without those numbers (see gap) |

**Score:** 4/5

### Previous gap (line 702, background tab)

| Part | Result |
|---|---|
| line-81 figure (0 frames in 8 s, hidden) credited to Phase 2 | CLOSED: now "from Phase 6 (`06-02-live.txt`, the `[bg]` scenario, headless)". 06-02-live.txt lines 30/45: frames_received 0, NoFrames for_ms 8003/8002, hidden true. Phase 2 sentence no longer claims it |

### Additional numeric claims sampled in this pass (not sampled before)

| Claim (line) | Evidence | Result |
|---|---|---|
| Phase 9 size column 0.67 / 0.42 / 1.05 / 0.84 / 2.67 / 1.78 MB/min (263-268) | 09-03-sizes.txt: 669452 / 422264 / 1055699 / 840574 / 2670641 / 1779523 B over ~60.1 s | TRUE |
| Exit timings: stdin EOF ~0.8 s, SIGTERM/SIGINT ~0.8 s, `bladebro stop` ~0.85 s, idle ~26 s, kill -9 ffmpeg gone 62 / 58 ms, Chrome kill ffmpeg gone ~2.1 s (211-220) | 09-03-live.txt: 818, 801/786, 791/781, 840 ms; idle 26.8 / 26.3 s; {46361: 62}, {46524: 58}; 2174 ms | TRUE |
| ffmpeg killed: 0 bytes, `encoder exited` (219, 227-228) | 09-03-live.txt lines 166, 182 | TRUE |
| Leftovers gone within 0.1 s, kill -9 MCP exception (240) | 09-03-live.txt took 26-34 ms; mcp_kill9 10282 ms with orphaned Chrome noted | TRUE |
| Popup follow delays: blank 0.05-0.06, js 0.06, xorigin 0.05-0.21, window 0.20-0.21, MCP 0.00-0.17 (162) | 08.1-02-live.txt lines 40, 61, 82, 103, 275-281 | TRUE |
| Spike: popup segment 1.8 s (blank) / 2.0 s (window.open) (162) | 08.1-01-spike.txt line 47 | TRUE |
| Per-tab popup closed at 4.1 s got a 2.4 s file (294) | 08.1-02-live.txt line 189 | TRUE |
| Burst: six segments (166) | 08.1-02-live.txt line 148: six segments, no "not recorded" note | TRUE (see warning on "no note") |
| Tick drift -0.74 % to +0.16 % over 8 runs, stops off a whole second (82) | 06-05-live.txt lines 9-82 (wall 8686, 8824, 8060, 8197 ms) | TRUE |
| Animated drift +0.6 % to +1.2 % on 8 to 12 s, about 100 ms (82) | 06-VERIFICATION.md 42-44, 66: 8 s +1.24 %, 12 s +0.83 %, 16 s +0.61 %, +98 to +100 ms | ~100 ms and range TRUE; "8 to 12 s" FALSE; source not in the cited files (GAP) |
| 20 s still page received 1 frame (82) | 06-02-live.txt dur-idle frames_received 1 | TRUE |
| Fixed values: 500 ms grace, 10 s drain, 5 s reap, 1 s stderr, 3 s poster, 4 KiB stderr tail, 200 ms poll, 50 ms confirm, 300 ms settle, 2 s guard, 500 MiB, 2 h, 1 GiB, 8 files, queue 16, 1000 entries, 80 chars, 10 min gap clamp, VP9 crf 40 realtime, x264 veryfast crf 28 faststart (60-73, 134, 159-161, 232, 250-253, 288-291) | src/record constants (capture.rs, process.rs, poster.rs, follow.rs, guard.rs, disk.rs, files.rs, values.rs, timeline.rs, names.rs, pace.rs, encoder/args.rs) | TRUE |
| Stop adds about 0.55 s over the video duration (66) | 07-VERIFICATION.md: +0.66 s; own CLI run today: +0.62 / +0.62 / +0.70 s (includes two CLI process startups); stop call itself 0.64-0.65 s | ROUGHLY TRUE ("about"; see warning) |
| Not verified section: Chrome 154 headless, macOS arm64; claude -p OAuth failure; dsh missing; interactive entry listed tools only | 11-01-s01-mcp-state.txt; 11-02-q02-claude-code.txt lines 17, 23; s06-dsh.txt line 34 and `which dsh` (not found); s05-modes.txt lines 79-83 | TRUE |

### Required Artifacts

| Artifact | Status | Details |
|---|---|---|
| `docs/recording-and-launch-config.md` | SUBSTANTIVE, 1 accuracy gap | lines 82 and 702 (animated-page drift) |
| `evidence/snippet-map.tsv` | VERIFIED | 30 blocks pass |
| `evidence/11-03-no-frames.txt` + `scripts/q03-no-frames.sh` | VERIFIED | unchanged since the 15:45Z reproduction |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `.mcp.json` env BLADE_RECORD=1 | recording on in the MCP server | process env | WIRED |
| per-tab stop text | doc's per-tab form / substring `no frames reached the encoder` | stop result | WIRED |
| Measurements bullet | cited evidence files | file names | 1 mismatch (animated-page drift) |

### Scope checks

- `git diff 42ca29b..HEAD --name-only -- src`: only src/record/hub/{evidence,evidence_tests,finish,msg}.rs; `git log 42ca29b..HEAD -- src`: only 9d20e81.
- `git diff fe413b0..HEAD --name-only`: only docs/recording-and-launch-config.md (1 line).
- README / HOOKS: as in Truth 3.
- Read-only repos: ~/claude/get-shit-done clean at 73f7629; ~/claude/gsd-dsh has only its pre-existing changes (README.md modified, upstream-updates.md untracked).

### Test suite (Step 8b)

- `cargo build --release` OK, `cargo fmt --check` OK. Clippy not re-run (no src change since the clean run at 15:57Z).
- Drift tests (`launch_mode::registry`, `record::hub::schema_tests`): 14 passed.
- `cargo test --release --no-fail-fast`: all green (lib 847 passed; every integration target ok, including daemon_lifecycle this time).
- `doc_snippets.py check`: 30 blocks, 0 failing.
- Layer-1 gates: scripts/ holds only publish-npm.sh, so N/A. Charlotte QA and E2E: not a web project and no UI files, so N/A.
- After the runs: `pgrep -fl "bladebro|ffmpeg"` shows nothing; the scratch home of the stop-timing run reported no leftovers.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| docs/recording-and-launch-config.md | 82 | "8 to 12 s" for a range whose +0.6 % end is a 16 s run | Blocker (doc accuracy) | gap |
| docs/recording-and-launch-config.md | 702 | animated-page drift credited to files that hold only tick drift (06-05) and pre-fix animated drift (06-02) | Blocker (doc honesty) | gap |
| docs/recording-and-launch-config.md | 166 | "six segments and no note": the burst stop did carry a note ("agent was on tab ..."); only the "not recorded" note was absent | Warning | suggest "and no not-recorded note" |
| docs/recording-and-launch-config.md | 66 | "about 0.55 s longer": measured +0.62 to +0.70 s through the CLI (07: +0.66 s) | Info | "about 0.6 s" would be closer; not blocking |
| src/record/hub/msg.rs | 138-143 | agent_vs_video note on a zero-frame stop | Warning (pre-existing, documented, deferred) | unchanged |

No logical_cohesion findings.

### Human Verification Required

None beyond the disclosed Not verified items (Linux, Windows, headed Chrome, DSH host, Claude Code session).

### Gaps Summary

b172171 closes the previous gap: the background-tab figure on line 81 is now credited to Phase 6 headless (06-02-live.txt [bg]), which matches the evidence. The Not verified section and every other sampled figure check out against their evidence. That covers Phase 9 sizes, exit-path timings, popup follow delays, the spike offsets, the per-tab popup file, tick drift and the source constants. One remaining mismatch, in the same provenance class: line 82's animated-page drift says "+0.6 % to +1.2 % on 8 to 12 s recordings", but the +0.6 % run was 16 s long. Those post-fix numbers exist only in 06-VERIFICATION.md. Line 702 sends the reader to 06-02-live.txt and 06-05-live.txt, and 06-02-live.txt holds the pre-fix animated drifts (+2 % to +6 %). Fix: change "8 to 12 s" to "8 to 16 s" and name 06-VERIFICATION.md (lines 42-44) for the animated-page drift on line 702.

Note: `gsd-tools.js eval-candidate` is not installed at ~/.claude/get-shit-done/bin, so no regression candidates were written.

_Verified: 2026-10-26T16:04:00Z_
_Verifier: Claude (gsd-verifier, opus)_
