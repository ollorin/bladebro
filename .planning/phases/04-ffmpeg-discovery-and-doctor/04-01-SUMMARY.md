---
phase: 04-ffmpeg-discovery-and-doctor
plan: 01
requirements: [OUT-01]
commits: [2513e46, 39b1280, 5753ede]
---

# 04-01 Summary: ffmpeg lookup, probe, cache

Fork-only (SYNC-01). Layout per the coordinator override: `src/record/ffmpeg.rs` is a thin core (module map + re-exports); code lives in `ffmpeg/parse.rs`, `ffmpeg/lookup.rs`, `ffmpeg/probe.rs` (`report.rs` is 04-02). `src/record.rs` gained `pub mod ffmpeg;`, a module-map entry and re-exports. No edits to platform.rs, lib.rs, updater, Cargo.toml/lock; no new crate.

## Public API (`record::ffmpeg`)
- parse: `FfmpegVersion {Release, Git, Unknown}` (`display`, `is_old`), `EncoderSet` (`has`, `listed`), `parse_version`, `parse_encoders`, `VP9_ENCODER`, `X264_ENCODER`, `MIN_RECOMMENDED_MAJOR`.
- lookup: `TargetOs` (`current`, `exe_name`), `EnvFn`, `known_locations`, `Source` (`label`), `Found`, `Lookup {Found, OverrideUnusable, NotFound}`, `find_ffmpeg`, `find_ffmpeg_from_process`.
- probe: `PROBE_TIMEOUT`, `MAX_PROBE_OUTPUT`, `ProbeInfo`, `Discovery`, `probe`, `discover`, `discover_cached`.

## Decisions D4-1..D4-7 as implemented
All as specified: std probe with drain thread and 10 ms try_wait poll (D4-1/D4-5, CREATE_NO_WINDOW under cfg(windows)); BLADE_FFMPEG blank = unset, unusable override stops the search (D4-2); PATH skips empty/relative entries (D4-3); known lists for macOS/Linux/Windows compiled everywhere (D4-4); Git/N-/date builds never parse as a release (D4-6); only `V`-flag encoders after the separator (D4-7). `discover_cached` shares `lookup::process_env` with `find_ffmpeg_from_process`.

## Fixtures
MAC_VERSION (paths replaced with /opt/homebrew/Cellar), mac/Debian encoder tables, Debian version, no-libvpx, no-libx264, garbage, CRLF, plus the 16-row version table.

## Tests
- `record::ffmpeg`: 35 (parse 9, lookup 16, probe 10). Full suite: lib 368 -> 394, total across all binaries 435 (was ~400 before the plan), 0 failed.
- fmt, `clippy --release --all-targets -D warnings` clean. No process env mutation; no orphaned `sleep 30`; no scratch leftovers.
- Real local ffmpeg probe test ran (not skipped) on this machine.

## Deviations
None of substance. Clippy `cloned_ref_to_slice_refs` fixed in a test.

## Platform status
Only macOS (aarch64-apple-darwin) executed. Windows cfg block not compiled locally (no Windows target installed; no CI run). Not executed on Windows/Linux (no CI run; nothing is pushed); Windows/Linux lists are covered by pure string tests on macOS, and the unix fake-executable tests ran on macOS only (ETXTBSY retry helper is untested on Linux).

## Self-Check: PASSED
Files and commits 2513e46, 39b1280, 5753ede exist.
