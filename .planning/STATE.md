# Project State

> **Phase 8.1 verified (2026-10-26, macOS headless; opus passed 5/5; 820 tests; audit 100/100; qol 120/122; hooks: src/page/mod.rs 1 line, src/state.rs +2/-6, both in HOOKS.md).** Next: Phase 10.
> **HOLD LIFTED (user "limits reset, you can continue", 2026-10-26):** finish Phase 10, then Phases 11 and 12.

> **RESUMED (user, 2026-10-26):** Phase 9 verified; order now: Phase 8.1 (follow every popup, user decision) -> Phases 10, 11, 12. Hold note of 2026-10-23 superseded.

## Project Reference

See: .planning/PROJECT.md (updated 2026-10-21)

**Core value:** A QA run with recording enabled always leaves a playable video file at a predictable path that the tool result reports, without touching the stealth path or breaking Bladebro's five-tool contract.
**Current focus:** Phase 8 - Tab following (Phase 7 executed and opus-verified on macOS; Phase 6 executed and opus-verified on macOS; Phase 5 executed and opus-verified on macOS; SC4/LIFE-05 Windows CI pending, nothing pushed)

> Roadmap execution resumed 2026-10-22 (was on hold after Phase 4.1). Phase 4.1 edited ~15 upstream files (HOOKS.md); Phase 5 edited none (SYNC-01 clean).

## Current Position

Phase: 7 complete on macOS (670 tests; opus final passed 12/12 after 07-04 gap fix; 9 upstream hooks in HOOKS.md; schema +233 chars ~58 tokens; audit 100/100, qol 120/122; Linux/Windows/CI not run); Phase 6 complete on macOS (headless; Linux/Windows/CI not run; diff_oracle not run, audit 100/100 not 111, qol 120/122 baseline); next is Phase 7 (RecordHub and state Ops)
Plan: 3 of 3 in current phase (Phase 2 complete, gate PASS-WITH-CAVEATS; see tools/record_sidecar/RESULTS.md)
Status: Phase 6 executed (628 tests; opus final verdict passed after 2 fix rounds 06-04/06-05; one upstream hook src/cdp/client.rs; open for Phase 7: stop grace/duration tail, Lagged ack bound). Earlier: Phase 5 executed (565 tests, opus final verdict passed, macOS only; open: crf 40 vs 36 user decision, Windows CI). Earlier: Phase 4.1 executed and verified on macOS only (515 tests; Windows/Linux not executed, no CI run, nothing pushed). BLADE_HEADLESS, .bladebro.json project layer, launch.json user layer, `bladebro headless`, docs/recording-and-launch-config.md with drift test. Ready to plan Phase 5.
Last activity: 2026-10-21 - Phase 4.1 executed and verified

Progress: [████░░░░░░] 33% (4/12 phases executed)

## Performance Metrics

**Velocity:**
- Total plans completed: 0
- Average duration: -
- Total execution time: 0.0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

**Recent Trend:**
- Last 5 plans: -
- Trend: -

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [User 2026-10-26]: popup decision = FOLLOW CLICK-OPENED POPUPS TOO. Inserted Phase 8.1 (CAPT-06), to run after Phase 9 verification. Supersedes the 'pending owner decision' below.
- [Phase 8 status 2026-10-23]: executed and verified on macOS (720 tests pass, 0 fail, 12 ignored; live default and per-tab runs verified independently by the orchestrator). Opus verdict human_needed on ONE owner decision: click-opened popups are followed only after switch-tab; window-feature popups are followed by the video while `state tabs` names the opener (roadmap criterion 1 was reworded by the coordinator in 16cf41a). Orchestrator recommends ACCEPT; pending the owner's answer. Execution continues with Phase 9 under the assumption that current behavior stands; if the owner rejects, a follow-up phase changes popup following.
- [User 2026-10-21]: Phase 4.1 inserted: `BLADE_HEADLESS` switch (tests hidden, everything else visible), layered config per-call > env > project file > user toggle > default (headless is suite-level; no per-test relaunch op), mode shown in doctor. Documentation standing rule: every option/config is documented in docs/recording-and-launch-config.md, enforced by a drift test (DOC-02), completeness verified in Phase 11 (DOC-01).
- [Roadmap]: Sidecar prototype (Phase 2) is a pass/fail gate; if it fails, revise the roadmap before Phase 3
- [User 2026-10-21]: Fork is the main repo; no upstream approval, issue or PR needed. Keep the diff additive so `upstream/main` merges stay clean (SYNC-01, verified in Phase 12). AGENTS.md ask-first rules are upstream contributor rules, not gates for us.
- [Roadmap]: Recordings use a dedicated never-rotated dir (default `~/.blade/recordings/`), not artifacts
- [Roadmap]: Each plan sized for ~100k tokens of context (Haiku executor); execution is sequential
- [Roadmap]: No web UI in this milestone, so Charlotte QA criteria do not apply; qol_probe/lane_matrix/audit/diff_oracle are the regression gates

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 2]: CDP screencast behavior on Chrome 151, `Page.enable` stealth impact, and occlusion/headless frame cadence are unmeasured
- [Phase 5]: Windows lifecycle code + tests written but NEVER compiled/run (no windows-latest CI yet); w4/.cmd fakes unverified

## Session Continuity

Last session: 2026-10-21
Stopped at: Roadmap and state initialized
Resume file: None

- [Phase 9 status 2026-10-26]: VERIFIED (sonnet gaps_found, opus final passed after fix c0ea00d: sweeper kept its own encoder when bladebro is pid 1; daemon hook no longer re-indents upstream comments). 772 pass / 0 fail / 12 ignored (known flaky race test once), audit 100/100, qol_probe 120/122 (two known), lane matrix rc 0. Not verified: Linux/Windows, real pid-1 container, real 2h/1GiB recording, headed, real lane. Next: Phase 8.1.
