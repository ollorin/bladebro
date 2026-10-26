---
phase: 11-integration-docs
verified: 2026-10-26T15:40:00Z
verifier: opus
status: gaps_found
score: 4/5 success criteria verified (SC1-SC4 verified; SC5 failed on accuracy, 2 small doc defects)
re_verification:
  previous_status: gaps_found
  previous_score: 3/5
  gaps_closed:
    - "Zero-frame short test case is now documented (QA-flows subsection, No-frames bullet, Known issues, Troubleshooting) with two recipes; verifier reproduced the failure and both recipes live"
    - "The state tabs mid-popup sample is now the Phase 11 output (EE14/0848/B425, matches 11-01-s03-tabs.txt lines 20-25); stale 'not re-run in Phase 11' line removed"
    - "MCP content[0] first-run note is documented (text matches src/mcp/server/serve.rs:280)"
  gaps_remaining:
    - "Per-tab no-frames stop text is misquoted (the gap-1 fix said it quotes the text each mode prints)"
    - "Not verified line claims a single historical measurement on the page"
  regressions: []
gaps:
  - truth: "The docs quote the stop text each mode actually prints for a zero-frame recording"
    status: failed
    failure_type: semantic_stub
    reason: "Line 81 says the text 'the default follow mode and per-tab mode print at stop is `recording not clean: recording failed: no frames reached the encoder, ...`'. Line 371 says the per-tab session failed 'all with the text below' (the follow-mode sample). Line 693 (Known issues) gives the same text with 'per-tab mode 1 of 1'. Per-tab mode actually prints `recording not clean (per-tab, 1 file): tab <id>: recording failed: no frames reached the encoder, ...`. This is in the phase's own evidence (11-03-no-frames.txt line 57) and was reproduced by the verifier at 15:35Z. A harness that matches the documented full string `recording not clean: recording failed: no frames` (troubleshooting line 720 uses that string) misses the per-tab failure. Line 292 documents the per-tab not-clean shape correctly, so the page contradicts itself."
    artifacts:
      - path: "docs/recording-and-launch-config.md"
        issue: "lines 81, 371 and 693 attribute the follow-mode text to per-tab mode; line 720 gives only the follow-mode string"
    missing:
      - "Quote the per-tab form (`recording not clean (per-tab, N files): tab <id>: recording failed: no frames reached the encoder, ...`) next to the follow form, or tell harnesses to match the substring `no frames reached the encoder`"
  - truth: "The Not verified provenance statement is honest"
    status: failed
    failure_type: semantic_stub
    reason: "Line 698 says 'The one historical measurement, the Phase 8.1 popup spike in Tab following, is labelled as such where it appears.' The page has other pre-Phase-11 measurements: the Phase 2 and Phase 9 size table (lines 261-270, labelled), the Phase 10 measurements (lines 300, 342, 344, 346, labelled 10-01/10-02), and several that carry no phase label and only say 'in testing' (line 80: 60 frames/s, 1200 frames in 20 s; line 81: background tab with 0 frames in 8 s; line 82: the 37 s silence and the -0.74 % to +0.16 % length figures; line 195: cross-site navigation). The result-sample part of the sentence is now true (every output block traces to Phase 11 evidence). The 'one historical measurement ... labelled' part is false."
    artifacts:
      - path: "docs/recording-and-launch-config.md"
        issue: "line 698 states that one historical measurement exists; there are several, some unlabelled"
    missing:
      - "Reword line 698 so it covers result samples only, or say that the prose measurements on lines 80-82 and 195 are from earlier phases (and label them)"
---

# Phase 11: Integration Docs Re-verification Report (opus, adversarial)

**Phase Goal:** The QA flows that consume Bladebro can turn recording on per test case by following the docs, with no extra model effort
**Verified:** 2026-10-26T15:40:00Z
**Status:** gaps_found
**Re-verification:** Yes, after gap-fix commit 3b040cd (previous: gaps_found 3/5, commit c026563)

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Docs show the Claude Code GSD flow with BLADE_RECORD, record-start/stop and where the path appears | VERIFIED | Unchanged since the previous run, apart from the content[0] addition (line 354), which was checked against src/mcp/server/serve.rs:280 (the warm note goes through relaunch_note). ~/claude/get-shit-done is still clean at 73f7629 |
| 2 | Docs show the DSH plugin flow doing the same | VERIFIED (static plus stub, as before) | DSH section unchanged since c026563 |
| 3 | README replaces "Not supported" with a short pointer | VERIFIED | `git diff c026563..HEAD -- README.md .planning/HOOKS.md` is empty. The upstream diff is still one Gotchas row plus the fork-owned "## Recording" block, and HOOKS rows 54-57 still describe it. Still correct: no README claim conflicts with the new no-frame section |
| 4 | Following each snippet against a fixture produces a recording at the documented path | VERIFIED | Verifier re-ran headless (scratch BLADE_HOME, BLADE_HEADLESS=1, ffmpeg from ~/.local/ffmpeg/bin, fixtures on 127.0.0.1:8765, the doc's `.mcp.json`). no-frames-short: 3/3 follow sessions failed with exactly the documented text (`✗ error: recording not clean: recording failed: no frames reached the encoder, so there is no video; <path> left in place; ... (no frames)]). Note: agent was on tab X at stop; video followed tab X (no frames). Poster: not made (no finalized file). Timeline: ... (0 actions). Marker at 0.0 s`), and each left a 0-byte webm. Per-tab: 1/1 failed, also 0 bytes, but with a different prefix (gap 1). no-frames-wait: 3/3 follow plus 1/1 per-tab saved, 11 frames, 1.1 s, ffprobe vp9 1280x720 10 fps duration 1.100000, poster present; the wait step reported `failed — ... wait timeout` exactly as documented. no-frames-repaint: 2/2 saved, 8 frames, ffprobe 0.800000. Other snippets: per-test-cli.sh (2 webm, 0.9/0.8 s, 0600), extract-path.sh (video/poster/timeline extracted), qa-per-test.jsonl over MCP (webm 1.1 s plus poster and timeline) |
| 5 | docs/recording-and-launch-config.md is complete and accurate; examples run; drift tests pass | FAILED (accuracy) | Complete. doc_snippets check: 30 blocks, 0 failing. Drift tests: `launch_mode::registry` 7/7, `record::hub::schema_tests` 7/7. Every one of the 10 output blocks was traced by content (not only the SNIPPET label) to Phase 11 evidence: 6c82d8/af4af0 (s01), b64fd0/a03e19/0901e8+2389e9 and the tabs ids (s03), 1c9b50 (s02), 2505f0/2f3988 (q01), fe93a8/0824b8 (11-03). Two inaccuracies remain (gaps 1 and 2) |

**Score:** 4/5

### Previous gaps

| Previous gap | Result |
|---|---|
| 1. Zero-frame short test case undocumented; No-frames bullet quoted the wrong text; agent/video note | Mostly closed. The new subsection "A test case that ends before its first frame", the Known issues bullets and the Troubleshooting entry are accurate for follow mode, and both recipes work (verifier-reproduced). The agent_vs_video wording bug is documented as a Known issue and logged in deferred-items.md as a pre-existing Phase 8.1 src item. That is the sanctioned channel for a docs-only phase, and no code fix means there is nothing to regression-test. Residual: the per-tab text is misquoted (new gap 1) |
| 2. Stale Not-verified line; Phase 8.1 tabs sample | Closed for samples: the tabs block now matches 11-01-s03-tabs.txt lines 21-25 verbatim (ids shortened), and the stale sentence is gone. Residual: the replacement sentence overclaims "the one historical measurement" (new gap 2) |

### Required Artifacts

| Artifact | Status | Details |
|---|---|---|
| `docs/recording-and-launch-config.md` | SUBSTANTIVE, 2 small accuracy gaps | 721 lines |
| `.planning/phases/11-integration-docs/evidence/11-03-no-frames.txt` + `scripts/q03-no-frames.sh` + 3 snippets | VERIFIED | The script reproduces. Its numbers match the doc (3/3, 1 per-tab, 11 frames 1.1 s, 8/9 frames) |
| `evidence/snippet-map.tsv` | VERIFIED | 4 new rows; the tabs-popup hash was updated |
| `deferred-items.md` | VERIFIED | Two DEFERRED entries, both pre-existing src behaviour outside the scope of a docs phase, with fix sketches |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `.mcp.json` env BLADE_RECORD=1 | recording on in the MCP server | process env | WIRED (re-run) |
| `state record-stop` text | report paths | sed extract | WIRED (re-run) |
| no-frames recipe (run wait / repaint) | first frame reaches the encoder before stop | timing | WIRED (re-run 4/4 and 2/2) |

### Scope checks

- `git diff 42ca29b..HEAD --name-only -- src`: only src/record/hub/{evidence,evidence_tests,finish,msg}.rs (9d20e81). `git log 9d20e81..HEAD -- src` is empty, so 3b040cd touched no src.
- 3b040cd touches only docs/recording-and-launch-config.md and .planning/phases/11-integration-docs/**.
- Read-only repos: ~/claude/get-shit-done is clean at 73f7629. ~/claude/gsd-dsh has the same pre-existing modified and untracked files as before (mtime 2026-10-20 10:43, before Phase 11). There are no new edits.

### Test suite (Step 8b)

- `cargo build --release` OK, `cargo fmt --check` OK, `cargo clippy --release -- -D warnings` clean.
- `cargo test --release --no-fail-fast`: exit 0, lib 847 passed, and every integration binary is ok. The daemon_lifecycle flake did not recur on this run.
- Layer-1 gates (scripts/fp-gate.ts and the others): not present (scripts/ has only publish-npm.sh), so N/A. Charlotte QA and E2E: not a web project and no UI files, so N/A.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| docs/recording-and-launch-config.md | 81, 371, 693, 720 | per-tab stop text misquoted | Blocker (doc accuracy) | gap 1 |
| docs/recording-and-launch-config.md | 698 | overclaimed provenance ("the one historical measurement") | Blocker (doc honesty) | gap 2 |
| docs/recording-and-launch-config.md | 80-82, 195 | measurements labelled only "in testing", with no phase | Info | feeds gap 2 |
| src/record/hub/msg.rs | 138-143 | agent_vs_video compares an id with a `(no frames)` label | Warning (pre-existing, documented, deferred) | wrong note on a zero-frame stop |
| tools/record_sidecar/scenarios/doc_snippets.py | n/a | output rows check only the SNIPPET label | Info | the verifier matched content by hand this time |

No logical_cohesion findings.

### Human Verification Required

None beyond the disclosed Not-verified items (Linux, Windows, headed Chrome, DSH host, Claude Code session).

### Gaps Summary

3b040cd closes the substance of both previous gaps. A QA flow that follows the docs now learns about the zero-frame stop and gets two recipes that work, and every output sample on the page is Phase 11 output. Two narrow text defects remain, each a sentence-level fix:

1. Per-tab mode prints `recording not clean (per-tab, 1 file): tab <id>: recording failed: no frames reached the encoder, ...`, but lines 81, 371 and 693 say it prints the follow-mode text, and the troubleshooting key (line 720) gives only the follow-mode string.
2. Line 698 claims the Phase 8.1 spike is "the one historical measurement" on the page. The page also has Phase 2, 9 and 10 measurements, plus several "in testing" figures with no phase label (lines 80-82, 195).

Note: `gsd-tools.js eval-candidate` is not installed at ~/.claude/get-shit-done/bin, so no regression candidates were written.

_Verified: 2026-10-26T15:40:00Z_
_Verifier: Claude (gsd-verifier, opus)_
