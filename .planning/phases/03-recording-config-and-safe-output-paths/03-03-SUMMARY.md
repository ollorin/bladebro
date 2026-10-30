# Plan 03-03 Summary: Windows ACL decision and README recording section

Requirements: SAFE-03 (Windows decision and README warning), OUT-04, CTRL-02 (documented).

**Status:** not executed on Windows/Linux (no CI run; nothing is pushed). Docs only: no code, Cargo, npm, CHANGELOG or .github changes.

## Commits

| Commit | Message |
|--------|---------|
| 1f9e11d | docs(03-03): Windows ACL decision and README recording section |
| (this commit) | docs(03-03): complete plan summary (SUMMARY.md, HOOKS.md row sha) |

## Decisions

- **Windows ACL: option A, inherit the parent ACL.** Nothing implemented. Options B (`icacls` via process), C (raw Win32 FFI) and D (crate) are compared in `WINDOWS-ACL-DECISION.md` with reasons. Revisit when a Windows CI runner exists (prefer C over B) or a shared-location leak is reported.
- **README:** one added block, `## Recording (in development)`, inserted before `## 🔄 Maintenance`. It states the feature is not functional, warns about sensitive on-screen content, and documents `BLADE_RECORD`, `BLADE_RECORD_FORMAT`, `BLADE_RECORD_FPS`, `BLADE_RECORD_SIZE`, `record.json` fields, precedence, and the dir/name/ACL rules. Values were copied from `src/record/values.rs` and `src/record/config.rs` and `src/record/dir.rs`, not from memory.

## README facts checked against code

- Defaults and ranges: webm, fps 10 (5-15), size 1280x720 (even, 160x120 to 3840x2160), quality 70 (1-100, file only).
- `BLADE_RECORD`: `0|off|false|no` off, `1|on|true|yes` on with default dir, absolute path on with that dir, relative rejected.
- No per-call `enabled`, `dir` or `quality` (D3-3, D3-5). Precedence per-call > env > file > default applies to format, fps, size.
- Default dir `<data root>/recordings`. Refused when inside `<data root>/artifacts` (component-wise check).
- Unix: new or empty dir set to 0700; files 0600; non-empty group/other-readable dir refused with a `chmod 700` hint; symlinked dir and foreign-owned dir refused.
- Windows: no mode bits; inherit the parent ACL. Data root on Windows is `%USERPROFILE%\.blade`.
- Nothing outside `src/record/` calls the module yet (grep of `src/`), so "records nothing today" is accurate.

## Corrections to the plan text

- The plan cited DECISIONS.md D-07 for "no new dependencies". In DECISIONS.md, D-07 is "no sixth tool". The ACL note cites CLAUDE.md's "ask first" rule and the unchanged Cargo.toml instead.
- The plan's README draft said the artifacts rule means recordings are "never under artifacts/". Enforced behaviour is a refusal of such a dir, so the README says that.
- The plan's draft said config errors "name the file". Only parse errors name the file; value errors name the source key (`record.json fps:` etc.). The README says that.
- The plan's README draft gave Windows a `.planning` link. Dropped from the user README.

## Verification

- `git diff -- README.md`: 21 added lines, 0 removed. Additions only.
- `cargo fmt --check`: exit 0.
- Not run in this plan (no code changed): clippy and `cargo test --release`. The plan's "final phase gates" step asked for them; they were skipped per the instruction to run fmt only. Last recorded green state is 03-02 (395 tests).
- Not run: `bladebro audit`, `tools/diff_oracle`, `tools/qol_probe`, or any Chrome launch.

## HOOKS.md

- Row `3 | README.md | ... | 1f9e11d` added. Existing Phase 3 row for `src/lib.rs` (5782a59) unchanged.
- Pre-existing files edited in Phase 3 so far: `src/lib.rs` (+1 line, 03-01) and `README.md` (+21 lines, 03-03).

## Self-Check

- `.planning/phases/03-recording-config-and-safe-output-paths/WINDOWS-ACL-DECISION.md`: created.
- README block present and additions only.
- Commit `1f9e11d` present in `git log`.
- `.planning/EXECUTION_LOG.md` and `target/` not staged.
