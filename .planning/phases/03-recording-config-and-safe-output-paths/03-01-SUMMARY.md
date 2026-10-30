# Phase 3 Plan 01: Record module, config and precedence resolver Summary

Pure-Rust `record` module with BLADE_RECORD / `record.json` / per-call settings resolved per-call > env > config file > default, with source-named errors and a `<blade_dir>/artifacts` guard.

**Status:** not executed on Windows/Linux (no CI run; nothing is pushed). Verified on macOS (darwin) only.

## Commits

| Commit | Message |
|--------|---------|
| 5782a59 | feat(03-01): record module, config value types and parsers |
| 1425e0c | feat(03-01): record.json loading and per-call > env > config > default resolver |

Note: 1425e0c was first committed without the `values.rs` change (`check_fps`), which made that commit fail to build alone. The commit was amended before anything else, so `1425e0c` is the single resolver commit and both commits build and test green. Hash for the resolver commit is the amended one.

## Public API

`src/record.rs` re-exports: `FileConfig`, `RecordConfig`, `RecordOverrides`, `load_file_config`, `resolve`, `resolve_from_env`, `Format`, `FrameSize`.

- `record::values`: consts `DEFAULT_FPS=10`, `MIN_FPS=5`, `MAX_FPS=15`, `DEFAULT_QUALITY=70`, `QUEUE_LEN=16`, `DEFAULT_WIDTH/HEIGHT=1280x720`, `MIN_WIDTH/HEIGHT=160x120`, `MAX_WIDTH/HEIGHT=3840x2160`, `CONFIG_FILE="record.json"`, `RECORDINGS_SUBDIR="recordings"`.
  - `Format::{WebM, Mp4}` with `parse`, `ext`, `encoder`; default WebM.
  - `FrameSize { width, height }` with `parse`, `Display`, `Default`.
  - `parse_fps(&str)`, `check_fps(i64)`, `parse_quality(i64)`.
  - `RecordEnv::{Unset, Set(Toggle)}`, `Toggle::{Off, On(Option<PathBuf>)}`, `parse_record_env(Option<&str>)`.
- `record::config`: `FileConfig` (`deny_unknown_fields`), `RecordOverrides` (raw strings: name, format, fps, size), `RecordConfig` (enabled, dir, format, fps, size, quality, queue_len, name), `load_file_config(&Path)`, `resolve(over, env, file, blade_dir)` (pure core), `resolve_from_env(over)` (impure wrapper, no unit test of its own).

## Decisions as implemented

- **D3-1 reject, never clamp.** Out-of-range fps, quality, size and odd sizes are errors. Each message names the source (`BLADE_RECORD_FPS:`, `record.json fps:`, `per-call fps:`) and the valid range. Odd sizes suggest the nearest smaller even size (`1281x720` gives `try 1280x720`).
- **D3-2 `deny_unknown_fields`** on `FileConfig`. `{"fsp":12}` errors and names the field and the file.
- **D3-3 no per-call dir.** `RecordOverrides` has no dir field. `enabled` and `dir` resolve env > file > default only.
- **D3-4 BLADE_RECORD.** `0|off|false|no` (case-insensitive) is Off. `1|on|true|yes` is On with the default dir. Unset or blank falls through. Any other value must be an absolute path, checked with `Path::is_absolute()`. Env `off` does not change the file dir.
- **D3-5 no env for quality or queue.** Quality comes from `record.json` only (file > default). Queue is the constant `QUEUE_LEN`.
- **D3-6 size and format bounds.** Each side is even, `160..=3840` by `120..=2160`. Format accepts `webm|mp4`, trimmed and case-insensitive. VP8 and AVI are rejected.
- **Artifacts guard (SC5).** A dir with `<blade_dir>/artifacts` as a path prefix is rejected with a message naming both paths. The check is component-wise, so `artifacts2` passes.
- **Validation order.** Every present layer (env, file, per-call) is parsed before precedence is applied. A broken lower layer errors even when a higher layer overrides it.

## Verification

- Test count: **358 before, 377 after** (+19 record tests: 5 in `record::values`, 14 in `record::config`).
- `cargo fmt --check`, `cargo clippy --release -- -D warnings`, `cargo test --release`: all green on the final tree (1425e0c).
- No `set_var` / `remove_var` anywhere in the record code. Tests call the pure resolver with a closure over a map. The one file-loading test uses a unique scratch dir under the system temp dir.
- Path literals go through an `abs()` helper, so the tests are meant to hold on Windows. This was not run on Windows.
- Plan hand-computed values: all held on first run. No expected values were changed.
- TDD note: tests and implementation were written in the same pass for each task. The tests were not observed failing before the code existed.

## Scope and deviations

- Only pre-existing file edited: `src/lib.rs`, one line `pub mod record;`. It is recorded in `.planning/HOOKS.md` (row for Phase 3, commit 5782a59). No edits to `src/platform.rs`, `Cargo.toml`, `Cargo.lock`, `npm/`, `CHANGELOG.md`, `.github/` or `src/updater/`.
- No new crates. Only `serde`, `serde_json` and `std` are used.
- Small addition beyond the plan text: `check_fps(i64)` is a public helper so `record.json` integers get the same range check as env and per-call strings. `parse_fps` parses as i64 and calls it.
- Not done: STATE.md, ROADMAP.md and REQUIREMENTS.md were not updated. The CTRL-02, CTRL-03, CTRL-06 and CTRL-07 marks are left to the orchestrator.

## Requirements

Covered by this plan: CTRL-02, CTRL-03, CTRL-06, CTRL-07 (per the plan frontmatter).

## Self-Check

Files exist: `src/record.rs`, `src/record/values.rs`, `src/record/config.rs`, `.planning/HOOKS.md`. Commits 5782a59 and 1425e0c are in `git log`.
