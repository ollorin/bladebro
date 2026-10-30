---
phase: 10-evidence-extras
verified: 2026-10-26T14:05:00Z
status: passed
score: 4/4 success criteria verified (OUT-06, OUT-07)
verifier: opus (adversarial, independent of 10-VERIFICATION-sonnet.md)
---

# Phase 10: Evidence Extras Verification Report

**Phase Goal:** A reviewer can see at a glance what the recording shows and jump to the moment that matters
**Verified:** 2026-10-26 (HEAD 1478b9d, base d18dae1)
**Status:** passed
**Re-verification:** No (independent adversarial pass; the sonnet report was not used as evidence)

## Gate runs (this verifier)

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --release --all-targets -- -D warnings` | clean |
| `cargo test --release --no-fail-fast` | EXIT 0; 891 passed, 0 failed, 18 ignored, 16 binaries (incl. `concurrent_cold_starts_have_one_owner`, which passed in the full run) |
| `cargo test --release --test record_evidence -- --ignored` with `REC_HUB_FIX`/`REC_HUB_FFMPEG`/`REC_HUB_FFPROBE` set | 2/2 passed live (7.9 s). Without those env vars both "pass" in 0.00 s (skip-by-return; see warnings) |
| Layer-1 gates (fp-gate, migration-security-gate, cve-scan-gate, no-check guard, P0 tier delta) | not shipped by this repo (`scripts/` holds only publish-npm.sh; no `apps/`): N/A |
| PRD-TRACE / Charlotte QA / E2E plan | N/A: no PRD-TRACE.md; Rust CLI/MCP binary, not a web project, no UI files |

## Observable Truths (ROADMAP Phase 10 success criteria)

| # | Truth | Status | Evidence (own runs) |
|---|---|---|---|
| 1 | `record-stop` returns a poster PNG path that opens and shows page content | VERIFIED | Live webm (follow) and mp4 (per-tab) daemon runs: `. Poster: <abs>.poster.png`; ffprobe `png,1280,720` = video size; magic 89504e47...; mode 0600; 1x1 tone green = the last page state (page went red -> blue -> green). Per-tab poster came from the tab-2 file (the followed tab). In-run (batch) stop: poster green too. |
| 2 | Optional marker; its offset appears in the result and the timeline | VERIFIED | `--value` with `\n`, `\t`, U+202E, U+200B, quotes, `</script>`, backslash, 500 extra chars: result `. Marker at 5.1 s` (note not echoed); timeline `{"kind":"marker","at_s":5.1,"note":"FAIL line2 evil \"q\" </script> \\ ZZZ..."}`: controls -> spaces, bidi/zero-width dropped (raw bytes checked), cut to 200 chars with `...`, JSON-escaped by serde. Bad/absent cases: unit tests `marker_bad_value_never_blocks_stop`, `marker_without_recording`; live off-lane text `. Marker not placed: no active recording`. |
| 3 | JSON timeline sidecar next to the video, action verdicts aligned to the video, from existing verdicts, no extra CDP calls | VERIFIED | Timeline is built in the two dispatch hooks from the `Result` the handler already returned (`timeline::end(tl, &result)`), no CDP handle reachable from `src/record/timeline.rs`. Alignment, own frame scan at 0.1 s: webm click1 [1.20,1.86] colour change at 1.6 (end_s-0.26), click2 [3.08,3.74] change at 3.5 (end_s-0.24); per-tab mp4 tab1 click [1.21,1.87] change 1.6, tab2 (start_s 3.31) click [6.46,7.13] change at local 3.6 = 6.91 (end_s-0.22). Every change inside its own [at_s,end_s]; the doc's "about 0.3 s before end_s, good to ~0.1 s" is honest. Failed click logged with `ok:false` and its error line. `files[]` basenames only, `start_s` per file. |
| 4 | Tests cover marker offsets and timeline generation; recording-off output unchanged | VERIFIED | Unit: marker.rs (9), timeline_tests.rs (17 incl. `stale_begin_after_stop_is_dropped`, `closed_log_ignores_pushes`, `log_caps_entries`, `json_escapes_hostile_text`, `recording_off_hook_is_inert`, `hooks_are_three_lines_each`), evidence_tests.rs (19 incl. `timeline_offsets_match_the_segment_clock`, `no_timeline_on_orphan_paths`, `poster_failure_never_fails_the_stop`, `poster_not_made_without_png_encoder`), poster_tests.rs (10 incl. fake timeout kill+remove, real ffmpeg). Live MCP off test passes (no sidecar, no evidence text). Recording-off: code path is one relaxed atomic load then `None` (no lock, no alloc, no I/O) before every act/see/state/run; `end(None, _)` returns immediately; result is passed by reference and unchanged. |

**Score:** 4/4

## Recording-off bar (judged)

The 10-02 evidence compares base d18dae1 vs HEAD: same 11 distinct CDP methods, equal per-method counts except `Input.dispatchMouseEvent`, normalized stdout byte-identical in both runs. Excluding `Input.dispatchMouseEvent` is honest: the base binary disagrees with itself (23 vs 30 in two runs), because biometric mouse paths are random, and the hook cannot emit Input events at all (static: `begin` returns before touching anything when `RECORDING` is false). The failing strict check is recorded as FAIL in the evidence file, not hidden. Accepted.

## Hook audit (upstream files)

`git diff d18dae1..HEAD` touches exactly four upstream files (all exist in `upstream/main`), each listed in .planning/HOOKS.md (Phase 10 rows): `src/mcp/server/proto.rs` (+3), `src/cli.rs` (+3), `src/mcp/tools.rs` (2 changed description lines, no new property/op, still 5 tools), `src/cli/help.rs` (1 changed + 1 added line). Every other changed source path is fork-owned (`src/record.rs`, `src/record/**`, `tests/record_*`, `tools/record_sidecar/**`, docs). No diff in Cargo.toml/Cargo.lock, npm/, src/stealth, src/browser*, src/page, src/action.

proto.rs / cli.rs hooks:
- Cannot panic: `include` uses `Value::get`/`as_str` with defaults; `clean_text` is char-safe; `offset_s` clamps with `max_s.max(0.0)` (NaN-safe); std mutexes recover from poison.
- No lock across the handler: `begin` clones an `Arc<Log>` out of a briefly held std mutex; `end` takes the Log mutex only for one push. The async hub lock is never taken by the hooks.
- No deadlock: `publish` (under the hub lock) and `current()` share one short std mutex; neither awaits while holding it.
- Error results unchanged: `end` borrows `&result`; the following match is upstream code. Unknown tool in cli.rs returns before `end`, but `include` yields None for it, so nothing is lost.
- No leak across recordings / after stop: `Begin` carries the log it started in; `finish_active` calls `log.take()` first, which closes it, so a late `end` is dropped (`stale_begin_after_stop_is_dropped`), and the next recording gets a new `Log`.

## Sidecar files

- O_EXCL + 0600 via the Phase 7 `create_unique` (single component, parent check). Live: symlinks planted at `<stem>.poster.png` and `<stem>.timeline.json` before stop were not followed; outputs went to `<stem>.1.*`, victim file intact, both in follow and per-tab.
- Path traversal: recording name `../../evil name%d` became `evil_name_d-...` (sanitize_stem: `[A-Za-z0-9._-]` only), so no `%` reaches the image2 muxer (and `-update 1` uses the name literally anyway).
- Exit/interrupt paths: live daemon `stop` while recording left only the `.webm` (no poster/timeline), matching unit tests `no_extras_on_orphan_paths` / `no_timeline_on_orphan_paths`.
- Poster: own process group, stdin null, kill_on_drop, 3 s total budget, PNG magic required, file removed on failure; failure is a `. Poster: not made (...)` note, stop ok/err unchanged (`poster_failure_never_fails_the_stop`). Missing png encoder: refused before spawn (`poster_not_made_without_png_encoder`). Lock-held latency: live stop wall 0.61-0.69 s including the poster; worst case adds 3 s under the hub lock, documented (stop up to ~21 s).

## Required Artifacts

| Artifact | Status |
|---|---|
| src/record/marker.rs, sidecar.rs, poster.rs, timeline.rs | exist, substantive, wired (hub/finish.rs -> evidence.rs) |
| src/record/hub/evidence.rs, hub/finish.rs | exist, substantive, wired from `try_handle` -> `stop_marked` |
| tests (poster_tests, timeline_tests, evidence_tests, tests/record_evidence.rs) | exist, real assertions, pass |
| docs/recording-and-launch-config.md "Poster, marker and timeline" | matches live output format (checked against my runs); drift test in suite passes |

## Anti-Patterns / Findings (non-blocking)

| File | Pattern | Severity | Impact |
|---|---|---|---|
| src/record/hub/finish.rs `idle_marker_text` + `msg::none()` | double period: `record-start <name> first.. Marker not placed: no active recording` (seen live, MCP and CLI) | info | cosmetic, agent-facing text |
| src/record/hub/evidence.rs `timeline_text` | `(1 actions)` | info | already logged in deferred-items.md (wording, not tests/QA) |
| src/record/timeline.rs `include` | Steps inside a `run` are not separate entries; a `run` that itself contains `record-start` ... `record-stop` yields `0 actions` (live: click inside the run absent from the timeline; the marker is present). Docs state "one entry per run call" but not this in-run case | warning | worth one docs line in Phase 11 (QA flows using batch-wrapped recording get an empty timeline) |
| src/record/poster.rs | O_EXCL placeholder is closed and ffmpeg reopens the path with `-y` (follows a symlink swapped in between); recordings dir is 0700, so only the same user can race it | info | no cross-user exposure |
| tests/record_support `fix()` | `--ignored` live tests return early (pass in 0.00 s) when `REC_HUB_FIX` is unset; same pattern as Phase 7-9 harness | warning | a CI that runs `--ignored` without env would show green without exercising anything; ran them with env here: real pass |
| logical_cohesion | none found: marker/sidecar/poster/timeline/evidence/finish each single-purpose, all under 500 LOC except tests | - | - |

Pre-existing, not this phase: `act click` verdict "no observable DOM change" for a background colour change (deferred-items.md, src/action/); first frame arriving late in a fast `run` makes a short video (clock starts at first frame, Phase 7 design).

## Human Verification Required

None blocking. Not verified by me: Windows/Linux builds of the poster `creation_flags`/`process_group` branches (CI covers), headed Chrome, stealth instruments (no stealth path changed).

## Gaps Summary

No gaps. All four success criteria hold under my own live runs (webm follow, mp4 per-tab, in-run, exit path, MCP on/off) and the full gate. Findings above are cosmetic or documentation follow-ups.

---

_Verified: 2026-10-26_
_Verifier: Claude (gsd-verifier, opus, adversarial)_
