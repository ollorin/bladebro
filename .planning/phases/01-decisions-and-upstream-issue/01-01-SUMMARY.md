---
phase: 01-decisions-and-upstream-issue
plan: 01
subsystem: docs
tags: [decisions, upstream-issue, mcp-schema, token-budget]

requires: []
provides:
  - DECISIONS.md: decision record D-01..D-07 with a measured token cost and the Phase 7 BLOCKED gate
  - UPSTREAM-ISSUE-DRAFT.md: unposted feature-request draft for dondai44423/bladebro
affects: [phase-2, phase-7]

tech-stack:
  added: []
  patterns: [draft-before-post for ask-first schema changes]

key-files:
  created:
    - .planning/phases/01-decisions-and-upstream-issue/DECISIONS.md
    - .planning/phases/01-decisions-and-upstream-issue/UPSTREAM-ISSUE-DRAFT.md
    - .planning/phases/01-decisions-and-upstream-issue/01-01-SUMMARY.md
  modified: []

requirements-completed: []  # VER-02 NOT complete: issue not yet opened or answered

key-decisions:
  - "D-01 Option A: state ops record-start / record-stop reusing name; Option B rejected for token cost"
  - "D-02 Recordings dir ~/.blade/recordings/ (BLADE_RECORD override), never rotated, not artifacts"
  - "D-03 Multi-tab: one file following the active tab with segment boundaries; per-tab files opt-in"
  - "D-04/D-05/D-06 codec, fps/size and timestamp strategy are PROVISIONAL pending Phase 2"
  - "Phase 7 is BLOCKED until an upstream answer or a user-approved fallback is recorded in DECISIONS.md"

patterns-established:
  - "Token cost stated as measured chars/4 from source, with relative % increase and caveat"

duration: short (docs only)
completed: 2026-10-21
---

# Phase 1: Decisions and Upstream Issue Summary

**Decision record D-01..D-07 with measured schema cost (+232 chars, about 58 est. tokens, 13.8% of the `state` ToolDef) and an unposted upstream issue draft; Phase 7 gated**

## Performance

- **Duration:** short (docs only, no build or test run)
- **Completed:** 2026-10-21
- **Tasks:** 3 of 3
- **Files modified:** 3 created, 0 existing modified

## Accomplishments
- DECISIONS.md records Option A with a measured token-cost table (script output copied verbatim, re-run and matched).
- DECISIONS.md records the recordings directory, multi-tab policy, and marks codec, fps and timestamp strategy PROVISIONAL.
- DECISIONS.md contains the `## Phase 7 gate` section: Phase 7 is BLOCKED.
- UPSTREAM-ISSUE-DRAFT.md follows feature_request.yml headings, sets tool = state, and carries the same token numbers. Header reads DRAFT - NOT POSTED.

## Success criteria status

| # | Criterion | Status |
|---|-----------|--------|
| 1 | Upstream issue exists on dondai44423/bladebro | Drafted, awaiting user approval to post (UPSTREAM-ISSUE-DRAFT.md); NOT posted |
| 2 | Decision record (multi-tab, recordings dir, provisional codec/fps/timestamps) | Met (DECISIONS.md) |
| 3 | Issue answer or fallback recorded before Phase 7 | NOT met - Phase 7 BLOCKED gate recorded in DECISIONS.md |

## Measured token cost

Method: source text of `src/mcp/tools.rs`, whitespace collapsed, chars/4 approximation; not a tokenizer count.

| item | chars | est_tokens (chars/4) |
|------|-------|----------------------|
| existing `state` ToolDef (collapsed) | 1681 | 420 |
| all five tools `all_tools()` (collapsed) | 15581 | 3895 |
| addition: enum fragment | 31 | 7.8 |
| addition: description line | 182 | 45.5 |
| addition: `name` description suffix | 19 | 4.8 |
| addition total | 232 | 58 |
| addition as % of `state` ToolDef | - | 13.8% |
| addition as % of `all_tools()` | - | 1.5% |

Absolute numbers differ from AGENTS.md's ~1,900-token figure because of Rust syntax and JSON escaping; compare relative increases.

## Task Commits

Tasks 1-3 were committed together as one docs commit (see git log for the hash, subject `docs(01-01): decision record and unposted upstream issue draft`) per the plan's Task 3 commit step.

## Files Created/Modified
- `.planning/phases/01-decisions-and-upstream-issue/DECISIONS.md` - decision record D-01..D-07, token table, Phase 7 gate
- `.planning/phases/01-decisions-and-upstream-issue/UPSTREAM-ISSUE-DRAFT.md` - unposted upstream issue draft
- `.planning/phases/01-decisions-and-upstream-issue/01-01-SUMMARY.md` - this file

## Decisions Made
See DECISIONS.md D-01..D-07. Key: Option A schema shape, dedicated recordings dir, one-file-follows-active-tab multi-tab policy, three PROVISIONAL decisions, Phase 7 BLOCKED.

## Deviations from Plan

None - plan executed as written. The Step 1 measurement script ran without changes to its slicing anchors.

## Issues Encountered

None.

## Guardrail check

`git status --porcelain` and `git diff --name-only HEAD` showed only `.planning/` paths. No change under `src/`, `tests/`, `tools/`, `npm/`, `Cargo.*`, `CHANGELOG.md`, `README.md` or `.github/`. Pre-existing untracked `.planning/EXECUTION_LOG.md` and `.planning/phases/01-decisions-and-upstream-issue/CHECKPOINT.json` were left untouched and not committed.

## Verification

- `cargo` was NOT run (docs-only plan, hard constraint).
- No `gh` write command was run; nothing posted, commented or opened.
- No `git push` was run.
- The Step 1 measurement was re-run after writing DECISIONS.md; the numbers match the table exactly.

## VER-02

VER-02: pending (issue not yet opened or answered).

## Next action for the user

Review UPSTREAM-ISSUE-DRAFT.md and approve posting (or edit it first). Phase 7 stays blocked until an upstream answer or a user-approved fallback is recorded in DECISIONS.md.

No code changed; cargo not run; nothing pushed or posted.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phases 2-6 do not depend on the Phase 7 gate and may proceed.
- Phase 7 is BLOCKED (see DECISIONS.md `## Phase 7 gate`).

---
*Phase: 01-decisions-and-upstream-issue*
*Completed: 2026-10-21*
