---
phase: 11-integration-docs
verified: 2026-10-26T15:25:00Z
verifier: opus
status: gaps_found
score: 3/5 success criteria verified (SC1, SC2, SC3 verified; SC4 partial; SC5 failed on accuracy)
gaps:
  - truth: "A QA test case recorded by following the docs (record-start, test steps, record-stop) produces a recording at the documented path"
    status: failed
    failure_type: semantic_stub
    reason: "A test case that ends within about 0.5 s of record-start without a repaint (an assertion-only case: record-start, see, record-stop) gets no frame and returns an error, `recording not clean: recording failed: no frames reached the encoder, so there is no video; <path> left in place` with a 0-byte file, no poster and `Marker at 0.0 s`. Reproduced by the verifier: MCP lane 7/7 failures with no wait (5/5 saved once the case lasted 1 s or more); CLI daemon lane 4/12 failures. The QA-flow section and Known issues do not mention this. Known issues says only that 'a short test gives a short video'. The docs' 'No frames' bullet (line 81) gives the failure text as `no frames for N s since recording start`, but follow mode (the default) prints the encoder text above instead."
    artifacts:
      - path: "docs/recording-and-launch-config.md"
        issue: "The QA flows section (lines 347-366), Known issues (lines 653-658) and the 'No frames' bullet (line 81) do not describe the observed no-frame failure of short, non-painting test cases, and they quote a failure text that the default mode does not print"
      - path: "src/record/hub/msg.rs"
        issue: "agent_vs_video compares the agent id with the video label `<id> (no frames)`, so the stop text says `agent was on tab X at stop; video followed tab X (no frames)` when the agent and the video are on the same tab. The doc (line 130) says this note appears only when they differ"
    missing:
      - "Document the zero-frame failure for test cases that end in under about 1 s without a repaint, with the exact text, and give QA flows a recipe that avoids it, or fix it in fork-owned src/record (for example, await or force a first frame before the stop on a static page)"
      - "Correct the 'No frames' bullet so it quotes the text each mode actually prints"
      - "Fix the agent/video note (compare ids, not labels) in src/record/hub, or document it; add a regression test either way"
  - truth: "The Not verified section and the sample provenance are honest"
    status: failed
    failure_type: regression
    reason: "Commit d3af2db replaced the start, stop, follow and per-tab samples with Phase 11 run lines and states that no earlier-phase output samples remain. Line 662 still says the result samples from earlier phases (the start result, the first stop result, tab following, 'the per-tab result with three files') were not re-run in Phase 11. That statement is now false: those samples are Phase 11 output and the per-tab sample has 2 files. The `state tabs` mid-popup sample (lines 185-190, ids 9EF9/0A7E) is still Phase 8.1 output (it appears only in .planning/phases/08.1-follow-every-popup/evidence/08.1-02-live.txt). It is not in the Phase 11 evidence that snippet-map.tsv points to (11-01-s03-tabs.txt shows ids 0848/B425 for this output), and nothing marks it as earlier-phase output. doc_snippets.py checks only the `SNIPPET <label>` line for output rows, so this mismatch passes the check."
    artifacts:
      - path: "docs/recording-and-launch-config.md"
        issue: "line 662 is stale; the sample at lines 185-190 is from Phase 8.1 and is presented as Phase 11-era real output"
    missing:
      - "Rewrite line 662 to match the current samples; either replace the tabs-popup sample with the 11-01-s03-tabs.txt lines or label it as Phase 8.1 output"
---

# Phase 11: Integration Docs Verification Report (opus, adversarial)

**Phase Goal:** The QA flows that consume Bladebro can turn recording on per test case by following the docs, with no extra model effort
**Verified:** 2026-10-26T15:25:00Z
**Status:** gaps_found
**Re-verification:** No. An independent sonnet report exists (`11-VERIFICATION-sonnet.md`, status passed). This report does not rely on it, and two of its conclusions do not hold (see Gaps).

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Docs show the Claude Code GSD flow (gsd-charlotte-qa, gsd-test-writer) with BLADE_RECORD, record-start/stop and where the path appears | VERIFIED | Doc lines 347-427. Claims checked in ~/claude/get-shit-done (73f7629, clean tree): gsd-charlotte-qa `tools:` lists only Charlotte tools; gsd-test-writer has no MCP tools; `test_flows` / `e2e_flows` exist (lines 23, 110, 264). Verifier re-ran the extract-path snippet: video, poster and timeline paths were extracted and the files exist (0600); the poster pixel is blue (00 41 ff) |
| 2 | Docs show the DSH plugin flow (mcp__bladebro__*) doing the same | VERIFIED (static plus stub) | Doc lines 429-452. Checked: `npm/bladebro/dsh-plugin.mjs` accepts the options binaryPath/env/cwd/toolCallTimeoutMs/reconnect, with a default of 180000; the `binary.cjs` error text matches the doc; the npm README has `dsh plugin --profile web add bladebro` and says DSH refuses a duplicate namespace. In ~/claude/gsd-dsh, `tool-map.ts` maps the charlotte tools to `drop` and has no bladebro entry, and `config/gsd-headless.patch.cordis.yml:31` says a patch row REPLACES the whole config. A DSH host was not run; the docs disclose this |
| 3 | README replaces "Session video recording: Not supported" with a short pointer (per the brief) | VERIFIED | `git diff upstream/main -- README.md`: one Gotchas row changed, plus the fork-owned "## Recording" block (ops, BLADE_RECORD, WebM/MP4, ffmpeg 4.3+, ~0.7 MB/min, sensitivity warning, link). Both are in HOOKS.md (Phase 11 rows, b709ead). Every commit hash in HOOKS.md resolves |
| 4 | Following each snippet against a fixture produces a recording at the documented path | PARTIAL, counted as FAILED | The verifier re-ran 5 snippets headless with a scratch BLADE_HOME, BLADE_HEADLESS=1 and ffmpeg from ~/.local/ffmpeg/bin. All 5 matched: Example 5 per-test CLI (2 webm files in <BLADE_HOME>/recordings, dir 0700 and files 0600, vp9 1280x720, 0.9/0.8 s); extract-path; MCP qa-per-test with the `.mcp.json` env (webm 1.5 s plus poster and timeline); the project file `.bladebro.json` (recording in <project>/recordings at 8 fps; nothing in BLADE_HOME); the Known issue for record-start/stop inside a run (timeline held only the marker). But the same flow for an assertion-only test case fails with 0 frames (gap 1) |
| 5 | docs/recording-and-launch-config.md is complete and accurate (DOC-01), every example run, drift tests pass | FAILED (accuracy) | Complete: every env var (BLADE_HEADLESS, BLADE_RECORD, _FORMAT, _FPS, _SIZE, _TABS, BLADE_FFMPEG), config keys, per-call params, precedence, defaults and ranges (matching values.rs/config.rs), examples, troubleshooting, Known issues, Not verified. Drift: `launch_mode::registry` 7/7 pass; doc_snippets.py check: 26 blocks, 0 failing. Inaccurate: the no-frames text and the stale provenance statement (gaps 1 and 2) |

**Score:** 3/5

### Required Artifacts

| Artifact | Status | Details |
|---|---|---|
| `docs/recording-and-launch-config.md` | EXISTS, SUBSTANTIVE, 2 accuracy gaps | 684 lines |
| `README.md` (pointer + row) | VERIFIED | additive except for the one Gotchas row, listed in HOOKS |
| `.planning/HOOKS.md` Phase 11 rows | VERIFIED | b709ead exists and touches only README.md |
| `src/record/hub/{evidence,finish,msg}.rs` wording fix (9d20e81) | VERIFIED | "1 action" singular; join_note avoids `..`; covered by tests `timeline_text_counts_one_action_singular` and `join_note_never_doubles_the_period` |
| `.planning/phases/11-integration-docs/evidence/*` | VERIFIED with one caveat | tabs-popup output row is not backed by matching evidence content (gap 2) |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `.mcp.json` env BLADE_RECORD=1 | recording on in the MCP server | process env | WIRED (verifier run) |
| `state record-stop` text | the test report paths | text parse (sed) | WIRED (verifier run) |
| `.bladebro.json` record.dir | <project>/recordings | project-file discovery from cwd | WIRED (verifier run) |
| DSH profile env | `bladebro mcp` env | dsh-plugin.mjs spreads options to the bridge | WIRED (code read; stub evidence) |

### Scope checks

- `git diff 42ca29b..HEAD --name-only`: the only src changes are src/record/hub/{evidence,evidence_tests,finish,msg}.rs. No stealth, upstream or src change outside src/record/**.
- Read-only repos: ~/claude/get-shit-done has a clean tree. ~/claude/gsd-dsh has one modified file (docs/browser-automation-research/README.md) and one untracked file (upstream-updates.md), both with mtime 2026-10-20 10:43, before Phase 11 (2026-10-26). They were not made by this phase.
- Phase 10 open items: documented under Known issues (the empty timeline for a run that contains both record-start and record-stop, which the verifier re-confirmed live; the click verdict miss; switch-tab to the current tab). deferred-items.md entries are pre-existing behavior items, which is the sanctioned channel.
- Phase 8.1 popup semantics (300 ms in front, re-front skip in src/page/mod.rs, window-feature popup, burst note, noopener): documented. The Not verified section covers Linux, Windows, headed Chrome, the real lane, the DSH host and the Claude client, but contains the stale statement in gap 2.

### Test suite (Step 8b)

- `cargo build --release` OK; `cargo fmt --check` OK; `cargo clippy --release -- -D warnings` clean.
- `cargo test --release`: the first run failed `daemon_lifecycle::concurrent_cold_starts_have_one_owner` (a 500 ms timing race under the parallel load of the full run, in an upstream test the phase did not change; the panic leaked one scratch daemon, which the verifier killed). The test passed 3/3 when rerun alone. A full `--no-fail-fast` rerun was all green (lib 847 passed, every integration binary ok). This is the same flake as recorded in 11-02-gate.txt.
- Layer-1 gates (scripts/fp-gate.ts and the others): not present in this repo (scripts/ holds only publish-npm.sh), so N/A. Charlotte QA and E2E: not a web project and no UI files, so N/A.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| src/record/hub/msg.rs | 138-143 | agent_vs_video compares an id with the label `<id> (no frames)` | Warning (pre-existing, Phase 8.1) | a false "agent was on tab X; video followed tab X" note in the zero-frame error |
| docs/recording-and-launch-config.md | 662 | stale provenance claim | Blocker for doc honesty | gap 2 |
| docs/recording-and-launch-config.md | 353, 358 | "The result starts `recording started:`" | Info | on the first MCP call against a fresh profile, content[0] is the upstream `note: first run ... warmed` block and `recording started:` is content[1]. A parser that reads only content[0] misses the path |
| tools/record_sidecar/scenarios/doc_snippets.py | n/a | output rows check only the SNIPPET label | Info | the doc_snippets check cannot catch sample-provenance drift, as in gap 2 |

No logical_cohesion findings: the 9d20e81 change stays inside msg/evidence helpers.

### Human Verification Required

None beyond the disclosed Not-verified items (Linux, Windows, headed Chrome, a DSH host, a Claude Code session). These do not affect the status.

### Gaps Summary

The docs are extensive. The configuration, precedence, examples and both integration flows match the code and the read-only repos, and the verifier reproduced the happy-path snippets. Two gaps block a pass:

1. The per-test-case QA flow fails in a common case the docs do not cover. An assertion-only test case that lasts under about 1 s on a static page gets no frame, and the stop is an error (`recording not clean: recording failed: no frames reached the encoder`), with a 0-byte file and a self-contradictory agent/video note. The doc quotes a different no-frames text.
2. The Not verified provenance line (662) became false after d3af2db, and one output sample (`state tabs` mid-popup) is still Phase 8.1 output that the Phase 11 evidence does not back.

_Verified: 2026-10-26T15:25:00Z_
_Verifier: Claude (gsd-verifier, opus)_
