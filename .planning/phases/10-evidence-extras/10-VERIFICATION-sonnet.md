---
phase: 10-evidence-extras
verified: 2026-10-26T14:00:00Z
status: passed
score: 4/4 must-haves verified
gaps: []
---

# Phase 10: Evidence Extras - Verification (sonnet, independent)

**Goal:** a reviewer can see what the recording shows and jump to the moment that matters. **Status: passed.**

## Success criteria
| # | Criterion | Result | Evidence |
|---|---|---|---|
| 1 | record-stop returns a poster PNG that opens and shows content | VERIFIED | Fresh live run (BLADE_HEADLESS=1, BLADE_RECORD=1, scratch BLADE_HOME, mark.html): result text `Poster: .../v1-...poster.png`; ffprobe png 1280x720 (same as the vp9 webm); 1x1 tone rgb (1,158,0) = the green last frame; mode 0600 |
| 2 | Marker offset in result and timeline | VERIFIED | `. Marker at 3.0 s`; timeline has kind=marker at_s 2.97. Hostile `value` (newline, U+202E, U+200B, 5000 chars) was cleaned and bounded to 200 chars; the note is not echoed |
| 3 | JSON timeline from existing verdicts, no extra CDP | VERIFIED | `<stem>.timeline.json`, 0600, 2 action entries (0.0, 0.97) + marker, verdict = first result line. Code: `timeline::begin/end` only reads the tool's returned Result; no CDP/Page access in src/record/timeline.rs |
| 4 | Tests for marker offsets and timeline; recording-off unchanged | VERIFIED | marker/sidecar/poster/timeline/evidence unit tests + tests/record_evidence.rs pass. Live recording off: `record-stop --value hi` returns only the usual "no active recording" error plus the marker-not-placed note; no recordings dir, no poster or timeline file |

## Constraints
- Cargo.toml/Cargo.lock: no diff since d18dae1. src/stealth, src/browser, src/cdp: no diff.
- Upstream files changed since base (per `git cat-file -e upstream/main:`): src/cli.rs, src/cli/help.rs, src/mcp/server/proto.rs, src/mcp/tools.rs only. Each diff is the hook listed in HOOKS.md (3-line begin/end in proto.rs and cli.rs; one RECORDING line + `value` description in tools.rs; two help lines). No new tool, op or property. Everything else is src/record/** or tests/tools/docs.
- Recording-off path: `begin` returns after one atomic load (`recording_active`); no allocation, no CDP.
- Sidecars: created through `create_unique` (O_EXCL, 0600, single path component, numbered on collision, symlink test); name derived from the video stem, so no traversal. The failed poster file is removed. Timeline stores basenames only.
- Adversarial: marker injection and huge marker (cleaned, 200 chars); entries after stop (log is closed by `take()` before the stop; later pushes ignored); 1000-entry cap with `dropped` count; per-tab (poster from the followed tab's file, timeline beside the first file, `files[]` clocks); stop latency 0.635 s live, poster budget 3 s documented in docs.
- docs/recording-and-launch-config.md has the Poster/marker/timeline section, result fields, timeout note and a real example.

## Gate
- `cargo fmt --check`: clean. `cargo clippy --release --all-targets -- -D warnings`: clean.
- `cargo test --release --no-fail-fast`: 845 lib tests and all other targets pass, except `tests/daemon_lifecycle.rs::concurrent_cold_starts_have_one_owner`, which failed once in the full parallel run and passed 5/5 when rerun alone. It is a daemon-lifecycle race outside Phase 10 files; treat it as flaky, not a gap.

## Non-blocking observations
- The `act click` verdict says "no observable DOM change" for a click that changed a background colour (already in deferred-items.md; not a Phase 10 regression, visible in the timeline verdict text).
- "(1 actions)" grammar is deferred by the team (product-surface wording).
- Timeline verdicts can contain typed or page text; docs warn about this.
- An action still running when record-stop arrives is not in the timeline (by design, documented via the log close).
- Test files over 500 lines (evidence_tests.rs 676 etc.) are tests only.
