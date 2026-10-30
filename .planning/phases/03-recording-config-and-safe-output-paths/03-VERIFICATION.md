---
phase: 03-recording-config-and-safe-output-paths
verified: 2026-10-21T00:00:00Z
status: passed
score: 3/4 success criteria verified (SC3 not verified - CI not run; plus OUT-04 artifacts-guard bypass and dir-create race)
verifier: sonnet-pass-1 + opus-final
final_pass_verified: 2026-10-21T12:30:00Z
gaps:
  - truth: "SC3: name/dir tests pass on ubuntu, macos and windows CI"
    status: failed
    failure_type: missing_test
    reason: "CI-only criterion; branch gsd/browser-recording is not pushed, no CI run exists (gh run list empty). Only macOS local run. Windows cfg(not(unix)) code was reasoned about, never compiled."
    artifacts:
      - path: "src/record/dir.rs"
        issue: "cfg(not(unix)) branch and Windows test literals never compiled or run"
    missing:
      - "Push the branch and get green fmt/clippy/test on ubuntu, macos and windows CI"
  - truth: "OUT-04: a recordings dir inside <data root>/artifacts is refused (README claims this)"
    status: failed
    failure_type: semantic_stub
    reason: "config.rs resolve checks dir.starts_with(blade_dir/artifacts) on the raw, uncanonicalized path; prepare_dir later canonicalizes but never re-checks. Probe: BLADE_RECORD=<bh>/recordings/../artifacts and (case-insensitive FS) <bh>/Artifacts both resolve OK and create_recording_file wrote into <bh>/artifacts/ (artifacts dir is 0700 as secure_create_dir_all makes it). rotate_artifacts deletes files named blade-*, so a recording with name 'blade-...' in that dir would be rotated away."
    artifacts:
      - path: "src/record/config.rs"
        issue: "lexical-only artifacts guard (line ~193)"
      - path: "src/record/dir.rs"
        issue: "prepare_dir/create_recording_file do not re-check the canonical dir against canonical artifacts dir"
    missing:
      - "After canonicalize in prepare_dir (or create_recording_file), refuse when the canonical dir starts_with canonicalize(blade_dir/artifacts) (compare case-insensitively on macOS/Windows or compare canonical forms of both)"
      - "Regression test with a '..' path and a symlink/case variant"
  - truth: "Recording file creation succeeds for every caller (unique file, no spurious failure)"
    status: failed
    failure_type: broken_chain
    reason: "prepare_dir first-create race: symlink_metadata NotFound -> create_dir; a concurrent creator makes create_dir fail with AlreadyExists, returned as 'cannot create recordings dir X: File exists (os error 17)'. Probe: 16 threads preparing the same new dir -> 14 errors. Same path also produces that non-actionable error for '..' dirs whose intermediate does not exist yet."
    artifacts:
      - path: "src/record/dir.rs"
        issue: "create_dir AlreadyExists not handled (lines 37-38)"
    missing:
      - "On AlreadyExists, fall through to re-stat + check_kind (and do not set created=true), plus a concurrency test"
not_verified:
  - "SC3: tests passing on ubuntu and windows CI (only macOS host run; nothing pushed)"
  - "SC4: Windows behavior (cfg(not(unix)) branch of dir.rs never compiled or run); ACL decision is documented, not measured"
  - "cargo clippy/test on Linux and Windows"
---

# Phase 3 Verification

Local run (macOS, BLADE_NO_UPDATE_CHECK=1): `cargo fmt --check` OK; `cargo clippy --release -- -D warnings` clean; `cargo test --release` 395 passed, 0 failed (354+6+1+5+14+10+2+3), matching baseline 358 + 37 new.

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | off by default; BLADE_RECORD=dir on; record.json applies; precedence unit-tested | VERIFIED | `config.rs::resolve` uses `call.or(env).or(file).unwrap_or(default)`; tests `off_by_default_...`, `blade_record_env_switches`, `file_enabled_and_dir_alone`, `*_precedence_each_layer_wins_in_turn` (format/fps/size), `broken_lower_layer_errors_even_when_overridden` |
| 2 | format/size/fps same precedence; defaults from Phase 2 verdict | VERIFIED | values.rs: webm, 10 fps (5-15), 1280x720, quality 70, queue 16 = RESULTS.md "Recommended defaults for Phase 3" |
| 3 | names sanitized, unique, create_new, canonical dir | VERIFIED locally; CI NOT VERIFIED | `names.rs` 31-row nasty table (traversal, leading `-`/`.`, drive letters, UNC, NUL, reserved device names, unicode, 200-char) plus invariant test; `dir.rs::create_unique` uses `create_new(true)`, retry-without-overwrite and exhaustion tests. ubuntu/windows CI not run |
| 4 | Unix 0700/0600; Windows ACL decision; README warning | VERIFIED on Unix; Windows NOT VERIFIED | `dir.rs` mode tests (`new_dir_0700_and_file_0600`, tighten, refuse open non-empty, symlink, other-owner refusal); WINDOWS-ACL-DECISION.md (Option A, inherit, states "not executed on Windows"); README "Recording" block opens with a sensitive-content warning |
| 5 | not in artifacts/, never rotated; clippy+test green | VERIFIED | `resolve` rejects dirs under `<blade>/artifacts` (test `artifacts_dir_is_rejected_but_sibling_names_pass`); default dir `recordings` (test `default_dir_is_recordings_never_artifacts`); no delete/prune code in non-test dir.rs (only `remove_dir_all` in tests); clippy/tests green |

Requirements: CTRL-02, CTRL-03, CTRL-06, CTRL-07, OUT-04, SAFE-02, SAFE-03 all covered by the above (SAFE-03 Windows half is decision-only).

## Scope hygiene
- Edits to pre-existing files outside src/record*: `src/lib.rs` (+1 `pub mod record;`) and `README.md` (+21, additions only). Both listed in `.planning/HOOKS.md` (commits 5782a59, 1f9e11d). Anything else: none.
- No changes to Cargo.*, npm/, CHANGELOG.md, .github/ since phase start.
- `.planning/EXECUTION_LOG.md` is untracked (not committed); `target` is gitignored and not tracked.
- Module is not wired into any runtime path (README says "Not functional yet"), so zero behavior change; consistent with phase scope.

## Anti-patterns / notes
- No stubs, no deferral language in must-haves. README claims "dir owned by another user refused": confirmed in dir.rs (euid check).
- Minor: `resolve_from_env` has no direct unit test (documented as thin wiring over tested pieces). Not a gap.
- Charlotte QA / E2E: not applicable (no UI, no web surface).
- Stealth instruments (audit, diff_oracle) not run: no stealth/injection code touched.

## Gaps
None verifiable locally. CI-only items listed under `not_verified` must be confirmed once the branch runs on ubuntu and windows CI.

## Final pass (opus)

Independent re-derivation from code, not from the pass-1 table. Date 2026-10-21, macOS arm64 host.

### Commands run
- `cargo fmt --check` -> exit 0
- `cargo clippy --release -- -D warnings` (the CI/CONTRIBUTING invocation) -> exit 0
- `cargo test --release` -> 395 passed, 0 failed (354+6+1+5+14+10+2+3)
- `cargo clippy --release --all-targets -- -D warnings` -> FAILS: `src/record/dir.rs:298` `clippy::double_ended_iterator_last` (`d.components().last()`); this lint is in Phase 3 test code. CI does not pass `--all-targets`, so not a CI break.
- Out-of-tree probe crate (scratchpad, depends on bladebro by path; repo untouched) for artifacts bypass, dir-create race, and extra sanitizer inputs.
- `git diff --name-status main...HEAD`: only pre-existing files modified are `README.md` and `src/lib.rs`, both in HOOKS.md with matching commits. No Cargo.*, npm/, CHANGELOG, .github/, artifacts code or rotation code touched.
- Layer-1 gates (fp-gate, migration-security-gate, cve-scan-gate, P0 tier delta): not present in this repo (scripts/ holds only publish-npm.sh) -> not applicable. Charlotte/E2E: not applicable (no UI).

### Confirmed correct
- Precedence per-call > env > file > default for format/fps/size; quality file-only; enabled/dir have no per-call layer. All layers parsed before precedence. Blank env = unset (both in `resolve` and `resolve_from_env`). `BLADE_RECORD=0|off|false|no` -> off and file dir still applies (tested). Relative dirs rejected in env and file.
- Sanitizer: output always `^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$`; traversal, drive, UNC, `\\?\`, NUL, leading `-`/`.`, trailing dot/space, non-ASCII, overlong all neutralized; truncate is safe (ASCII by then); reserved device names prefixed. Full file name always ends `-<stamp>-<id>.<ext>`, so a trailing dot/space or bare device name can never be the final name.
- `create_unique`: `create_new(true)` + `mode(0o600)` on Unix, single-Normal-component check, parent==dir check, AlreadyExists retry x8, no overwrite (tested). `short_id` is injective in `seq` over 2^24 per process.
- Unix: new or empty dir -> 0700, non-empty open dir refused without chmod, symlink and foreign-owner refused, ancestors validated via `platform::validate_dir_ancestors`. Tests are real (mode readbacks, not vacuous).
- No unwrap/expect in non-test code (`as_bytes()[0]` is guarded by `len()==1`).
- Windows compile (reasoned, not executed): `#[cfg(unix)]` on statements is stable; `created` is read via `let _ = created` so no unused-assignment lint; imports all used on Windows; test literals go through `abs()` producing `C:\...`; `"rel/dir"` and `"./x"` are relative on Windows; JSON in `load_file_config_cases` uses `{:?}` which escapes backslashes validly.
- README Recording block opens with "Not functional yet" and the sensitive-content warning; WINDOWS-ACL-DECISION.md honestly states "not executed on Windows".

### Defects

| # | Severity | Where | Defect |
|---|---|---|---|
| 1 | MEDIUM | config.rs ~193, dir.rs prepare_dir | Artifacts guard is lexical only. `BLADE_RECORD=<bh>/recordings/../artifacts` (second call, once `recordings/` exists) and `<bh>/Artifacts` on case-insensitive macOS/Windows resolve and create files inside `<bh>/artifacts/` (probe-confirmed). README promise "A dir inside <data root>/artifacts/ is refused" is false for these. Impact limited: `rotate_artifacts` only deletes `blade-*` names, so a recording named `blade-...` is the one that gets rotated; others also become readable via `see artifact=`. Fix: re-check canonical dir vs canonical artifacts dir after `canonicalize`. |
| 2 | MEDIUM-LOW | dir.rs 37-38 | First-create race: concurrent `prepare_dir` on a not-yet-existing dir -> `create_dir` AlreadyExists bubbles as "cannot create recordings dir X: File exists (os error 17)" (probe: 14 of 16 threads failed). Matters once two sessions/daemons start their first recording together (Phase 7+). Error string is also non-actionable. |
| 3 | NOT VERIFIED | SC3 / CI | ubuntu + windows CI never ran (branch not pushed). Per the CI-only rule this criterion is NOT VERIFIED; it cannot count as passed. |
| 4 | LOW | dir.rs:298 (test) | `clippy --all-targets -D warnings` fails (`double_ended_iterator_last`); use `next_back()` / `file_name()`. Not run by CI, but the repo's "zero warnings" norm is broken for test targets by this phase. |
| 5 | LOW | config.rs resolve_from_env | Non-UTF-8 `BLADE_RECORD` value: `std::env::var` errors -> treated as unset -> recording silently off with no error. Use `var_os` and report. |
| 6 | INFO | names.rs is_reserved | `COM0`/`LPT0` are on Microsoft's current reserved list; test asserts `com0` stays. Practically harmless here (the stem is never the whole base name unless it contains a dot, e.g. `com0.x` -> file `com0.x-<stamp>...` whose base before the first dot is `com0`). Superscript variants are already mapped to `_`. |
| 7 | INFO | dir.rs 37 then 64 | Dir is created with umask mode then chmodded to 0700 (brief 0755 window while empty). `DirBuilder::mode(0o700)` would close it. Euid (dir) vs uid (ancestors) owner checks differ slightly. |
| 8 | INFO | SAFE-02 | "ffmpeg invoked with an argv array" cannot be delivered before Phase 5 (no ffmpeg code exists). Not counted against Phase 3, but SAFE-02 should not be marked Complete until Phase 5 proves it. |
| 9 | INFO | Windows | Returned path will be the `\\?\C:\...` verbatim form from `canonicalize`; agent-visible and passed to ffmpeg later. Consider `dunce`-style stripping in Phase 5 (no new dep needed). |

### Verdict
`gaps_found`. Code quality is good and the core Unix behavior is real and tested, but: defect 1 makes a documented OUT-04 guarantee false, defect 2 is a real failure mode for concurrent first use, and SC3's CI part is not verified. Pass 1 missed defects 1, 2 and 4 and graded a not-run CI criterion as acceptable for `passed`.

## Gap closure (coordinator note)

Defects 1 (artifacts guard on resolved path), 2 (create_dir race) and 4 (--all-targets clippy) plus INFO 7 (dir created 0700) were fixed in b5b249a (see 03-04-SUMMARY.md). Coordinator re-ran: fmt OK, `cargo clippy --release --all-targets -- -D warnings` OK, `cargo test --release` all green, 400 lib+integration tests (baseline 358). Status updated to passed for everything verifiable on macOS.

Still NOT VERIFIED: ubuntu/windows CI (nothing pushed; not executed on Windows/Linux, cfg(not(unix)) path never compiled), Windows ACL decision unmeasured, SAFE-02 argv-array part is Phase 5. Open: non-UTF-8 BLADE_RECORD treated as unset; COM0/LPT0 not reserved. The gap-closure regression tests were not run against pre-fix code (probe-confirmed by the verifier). The Artifacts/case-insensitive test returns early on case-sensitive volumes.
