---
phase: 11-integration-docs
verified: 2026-10-26T16:05:00Z
verifier: opus
status: gaps_found
score: 4/5 success criteria verified (SC1-SC4 verified; SC5 failed on accuracy, 1 doc defect introduced by 69fbd40)
re_verification:
  previous_status: gaps_found
  previous_score: 4/5
  gaps_closed:
    - "Per-tab no-frames stop text: lines 81, 371, 693 and 728 now quote the per-tab form next to the follow form and tell harnesses to match 'no frames reached the encoder'. Verifier re-ran headless at 15:45Z and the printed text matches the documented form exactly"
    - "Line 698 'the one historical measurement' overclaim removed; the provenance sentence now covers only the fenced output blocks, which is true"
  gaps_remaining: []
  regressions:
    - "The new '## Measurements' section says the earlier-phase figures come from 'macOS with headless Chrome', but the Phase 2 figures it covers were measured on headed Chrome"
gaps:
  - truth: "The provenance statements in Measurements and Not verified are true against the cited evidence"
    status: failed
    failure_type: semantic_stub
    reason: "Line 698 says the earlier-phase measurements 'come from earlier phases, on macOS with headless Chrome'. Several of them are Phase 2 sidecar measurements on headed Chrome. tools/record_sidecar/RESULTS.md line 3 says 'headed Chrome unless stated', and results/core.json host has \"headless\": false. These include: the Phase 2 column of the size table (lines 263-266; the codec matrix re-encodes the headed ~1185x720 capture), 'about 100 headed' on line 80 (99.31 fps headed, RESULTS.md line 22/38), 'a minimized window (about 0.3 frames/s)' on line 82 (RESULTS.md line 42, which needs a headed window), and the one-off 37 s silence on line 82 (RESULTS.md line 29, Phase 2). Line 702 credits the frame rates to Phase 6 (06-02-live.txt, 06-05-live.txt), but both of those files are headless=1 and contain neither the headed ~100 fps nor the minimized 0.3 fps figure. Phase 2 / RESULTS.md is never named as a source for these. Separately, line 706 says 'Figures quoted in prose and the size table are earlier-phase measurements'. That is false as a blanket statement: the prose figures labelled 11-01/11-03 (lines 81, 371, 385, 398, 536, 689, 692, 693) are Phase 11 measurements."
    artifacts:
      - path: "docs/recording-and-launch-config.md"
        issue: "line 698 says 'headless Chrome' for all earlier-phase figures; line 702 names the wrong source for the headed and minimized frame rates and the 37 s silence; line 706 says all prose figures are earlier-phase"
    missing:
      - "Line 698: drop 'with headless Chrome', or say 'Phase 2 on headed Chrome (tools/record_sidecar/RESULTS.md), later phases headless'"
      - "Line 702: name Phase 2 (tools/record_sidecar/RESULTS.md) as the source of the headed ~100 fps, the minimized 0.3 fps and the 37 s silence; keep Phase 6 for the headless 60 fps, 1200/200 frames, the 1-frame idle page, the 8 s background tab and the drift figures"
      - "Line 706: 'Figures quoted in prose without an 11-0x label, and the size table, are earlier-phase measurements'"
---

# Phase 11: Integration Docs Re-verification Report (opus, adversarial)

**Phase Goal:** The QA flows that consume Bladebro can turn recording on per test case by following the docs, with no extra model effort
**Verified:** 2026-10-26T16:05:00Z
**Status:** gaps_found
**Re-verification:** Yes, after docs-only fix 69fbd40 (previous: gaps_found 4/5, commit c3b337f)

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Docs show the Claude Code GSD flow with BLADE_RECORD, record-start/stop and where the path appears | VERIFIED | Unchanged since c3b337f. ~/claude/get-shit-done is clean at 73f7629 |
| 2 | Docs show the DSH plugin flow doing the same | VERIFIED (static plus stub, as before) | DSH section unchanged |
| 3 | README replaces "Not supported" with a short pointer | VERIFIED | `git diff c3b337f..HEAD -- README.md .planning/HOOKS.md` is empty. Lines 597 and 651-657 are the fork pointer; "about 0.7 MB per minute" matches Phase 9 (0.67). The README's `BLADE_NO_WARMING` row (line 636) is upstream (b966576), not Phase 11 |
| 4 | Following each snippet against a fixture produces a recording at the documented path | VERIFIED | Verifier re-ran `evidence/scripts/q03-no-frames.sh` at 15:45Z with build 69fbd40: headless, scratch BLADE_HOME bbrec.WROp1SaXvE, fixtures on 127.0.0.1:8765. Short case: 3/3 follow sessions printed `✗ error: recording not clean: recording failed: no frames reached the encoder, so there is no video; ... left in place; ... (bytes 0, ...) ... Marker at 0.0 s`. The per-tab session printed `✗ error: recording not clean (per-tab, 1 file): tab 4E0B...: recording failed: no frames reached the encoder, so there is no video; ...-tab1-...webm left in place; ... (bytes 0, ...) ... Marker at 0.0 s`. That matches the doc's per-tab form on lines 81, 693 and 728, and the 0-byte and `Marker at 0.0 s` claims on line 693. Wait recipe: 3/3 saved, 11 frames, 1.1 s. Repaint recipe: 2/2 saved (10 and 7 frames this run, so it worked, with counts differing from the doc's recorded run). No leftover processes |
| 5 | docs/recording-and-launch-config.md is complete and accurate; examples run; drift tests pass | FAILED (accuracy) | doc_snippets check: 30 blocks, 0 failing. Drift tests: `launch_mode::registry` and `record::hub::schema_tests` 14/14 ok. The two previous gaps are closed. The new Measurements section misstates the browser mode and the sources (see gap) |

**Score:** 4/5

### Previous gaps

| Previous gap | Result |
|---|---|
| 1. Per-tab no-frames stop text misquoted | CLOSED. Lines 81, 371, 693 and 728 quote both forms and give the shared substring. The text was reproduced live, character for character apart from ids and paths |
| 2. "The one historical measurement" overclaim (line 698) | CLOSED as worded: the sentence now covers fenced output blocks only, which is true. A Measurements section was added, but it introduces a new inaccuracy (regression gap below) |

### Measurements section vs evidence

| Claim (lines 698-702) | Evidence | Result |
|---|---|---|
| Size table: Phase 2 and Phase 9, Phase 9 source 09-03-sizes.txt | 09-03-sizes.txt: 0.67/0.42/1.05/0.84/2.67/1.78 MB/min, headless daemon lane | TRUE |
| Phase 10 figures labelled 10-01/10-02 | 10-01-live.txt and 10-02-live.txt, headless | TRUE |
| Headless 60 fps, 1200 frames / 200 to the encoder in 20 s; 1 frame on a 20 s still page; 8 s background tab with 0 frames, hidden; drift -0.74 % to +0.16 % over 8 runs | 06-02-live.txt dur-anim, dur-idle, bg; 06-05-live.txt summary | TRUE, sources correct |
| "on macOS with headless Chrome" | Phase 2 RESULTS.md line 3: "headed Chrome unless stated"; core.json `"headless": false` | FALSE for the Phase 2 size column, "about 100 headed", minimized 0.3 fps and the 37 s silence |
| Frame rates from Phase 6 | The ~100 fps headed and 0.3 fps minimized figures come only from Phase 2 (RESULTS.md lines 38, 42) | WRONG SOURCE |
| Cross-site note: earlier phases | 08-tab-following evidence (127.0.0.1 to localhost) | TRUE (Phase 8, not named, but the claim only says "earlier phases") |
| Chrome version | The section omits a version, and Phase 2 (154.0.8037.58) and Phase 6 (version not recorded in the evidence) do not contradict that. The Not verified section says Chrome 154 for Phase 11; the Phase 11 evidence shows Chrome/154.0.8037.58 | CONSISTENT |

### Required Artifacts

| Artifact | Status | Details |
|---|---|---|
| `docs/recording-and-launch-config.md` | SUBSTANTIVE, 1 accuracy gap | 729 lines |
| `evidence/11-03-no-frames.txt` + `scripts/q03-no-frames.sh` | VERIFIED | Reproduces on build 69fbd40 |
| `evidence/snippet-map.tsv` | VERIFIED | 30 blocks pass |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `.mcp.json` env BLADE_RECORD=1 | recording on in the MCP server | process env | WIRED (re-run) |
| per-tab stop text | doc's per-tab form / substring `no frames reached the encoder` | stop result | WIRED (re-run) |
| no-frames recipes | first frame reaches the encoder before stop | timing | WIRED (re-run 3/3 and 2/2) |

### Scope checks

- `git diff 42ca29b..HEAD --name-only -- src`: only src/record/hub/{evidence,evidence_tests,finish,msg}.rs. `git log 9d20e81..HEAD -- src` is empty.
- 69fbd40 touches only docs/recording-and-launch-config.md.
- Read-only repos: ~/claude/get-shit-done is clean at 73f7629. ~/claude/gsd-dsh has only its pre-existing modified and untracked files (mtime 2026-10-20 10:43, before Phase 11).
- `BLADE_NO_WARMING` does not appear in docs/recording-and-launch-config.md.

### Test suite (Step 8b)

- `cargo build --release` OK, `cargo fmt --check` OK, `cargo clippy --release -- -D warnings` clean.
- `cargo test --release --no-fail-fast`: exit 0, 16 test-result lines all ok, 0 FAILED, lib 847 passed.
- Layer-1 gates: scripts/ holds only publish-npm.sh, so N/A. Charlotte QA and E2E: not a web project and no UI files, so N/A.
- Daemons stopped after the run; no bladebro or ffmpeg processes left.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| docs/recording-and-launch-config.md | 698, 702 | provenance misstated (headless claimed for headed Phase 2 figures; wrong source named) | Blocker (doc honesty) | gap |
| docs/recording-and-launch-config.md | 706 | blanket "prose figures are earlier-phase" contradicts the 11-0x-labelled prose figures | Blocker (doc honesty) | gap |
| src/record/hub/msg.rs | 138-143 | agent_vs_video note on a zero-frame stop (seen again in this run) | Warning (pre-existing, documented, deferred) | unchanged |

No logical_cohesion findings.

### Human Verification Required

None beyond the disclosed Not-verified items (Linux, Windows, headed Chrome, DSH host, Claude Code session).

### Gaps Summary

69fbd40 closes both previous gaps. The per-tab stop text is now quoted correctly (verified live) and the "one historical measurement" claim is gone. The new Measurements section is mostly right: the Phase 6, 9 and 10 sources check out. But it says every earlier-phase figure was measured "on macOS with headless Chrome" and credits frame rates to Phase 6. The Phase 2 figures on the page (the size table's Phase 2 column, "about 100 headed", the minimized 0.3 fps and the 37 s silence) come from tools/record_sidecar/RESULTS.md, which was measured on headed Chrome. Line 706 also says all prose figures are earlier-phase, although several are labelled 11-01/11-03. These are sentence-level fixes on lines 698, 702 and 706.

Note: `gsd-tools.js eval-candidate` is not installed at ~/.claude/get-shit-done/bin, so no regression candidates were written.

_Verified: 2026-10-26T16:05:00Z_
_Verifier: Claude (gsd-verifier, opus)_
