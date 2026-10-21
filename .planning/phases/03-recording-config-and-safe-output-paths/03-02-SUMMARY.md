# Plan 03-02 Summary: safe names and private recordings dir

Requirements: OUT-04, SAFE-02, SAFE-03 (Unix part).

## API
- `record::names`: `sanitize_stem`, `utc_stamp`, `short_id`, `file_name`, `MAX_STEM` (80), `FALLBACK_STEM` ("recording").
- `record::dir`: `prepare_dir`, `create_unique`, `create_recording_file`, `MAX_CREATE_ATTEMPTS` (8).
- Re-exported from `record`: `sanitize_stem`, `file_name`, `create_recording_file`, `prepare_dir`.

## Decisions as implemented
- D3-7: chars outside `[A-Za-z0-9._-]` -> `_`, `_` runs collapse, leading/trailing `-._` trimmed, cap 80, re-trim, empty -> "recording".
- D3-8: Windows device names (CON PRN AUX NUL COM1-9 LPT1-9, case-insensitive, part before first '.') get `rec_` on every OS.
- D3-9: names carry UTC stamp + 6-hex id; `create_new` (O_EXCL), up to 8 attempts, never overwrites.
- D3-10: dir symlink/non-dir/foreign owner refused; mkdir for last component; 0700 if created or empty; non-empty with group/other bits refused (error names `chmod 700`), never chmodded.
- D3-11: non-unix keeps symlink/is_dir/create_new/parent checks; no mode bits (ACL decision in 03-03).
- SC5: nothing rotates the dir (module doc); default dir is `<blade_dir>/recordings`, tested to have no `artifacts` component; src/artifacts.rs does not reference it.

## Sanitize table
All 31 rows of the plan table passed on first run with the hand-computed expected values (no value was changed). Extra invariant inputs: 200 x chars, `a`x79+`..b`, `con.`+80 z, `con`+100 dots.

## Tests
Before: 377. After: 395 (+18: 6 names, 12 dir incl. 5 cfg(unix)). fmt, clippy -D warnings, full test all green. No leftovers under real home or temp dir.

## Deviations
- Helpers live in src/record/ (names.rs, dir.rs), not src/platform.rs (user decision; ROADMAP aligned). No pre-existing file edited; src/platform.rs only called; HOOKS.md unchanged. src/record.rs is a fork-only file from 03-01.

## Verification scope
cfg(unix) permission tests ran on macOS only. Not executed on Windows/Linux (no CI run; nothing is pushed).

## Commits
- 7428696 feat(03-02): recording name sanitization and unique file names
- dir commit: see git log (feat(03-02): private recordings dir and create_new recording files)
