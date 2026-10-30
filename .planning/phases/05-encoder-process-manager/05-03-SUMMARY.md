---
phase: 05-encoder-process-manager
plan: 03
subsystem: recording
tags: [ffmpeg, lifecycle-tests, e2e, windows-written-not-run]
requires: [05-02]
requirements-completed: [SAFE-04]
requirements-pending: [LIFE-05 (SC4: Windows tests written, not executed)]
completed: 2026-10-22
---

# Phase 5 Plan 03: Lifecycle matrix, real ffmpeg e2e, docs note

Executed on macOS (aarch64) only. Linux and Windows were NOT executed. SC4 / LIFE-05 is "written, not executed", never "passing".

## Matrix (src/record/encoder/matrix_tests.rs, unix, 11 rows, all pass)
a empty pre-created -> EmptyOutput; b missing output -> EmptyOutput; c no frames -> NoFrames; d exits without reading -> StoppedReading; e ignores EOF -> Unfinalized reaped; f never reads, 256 KiB frames -> Unfinalized reaped (< 7 s); g slow consumer queue 2 / 100 frames -> queued+dropped == 100, dropped >= 90, live counter == report; h 100 KB stderr -> Finalized, tail <= 4096 ending LAST-LINE; i hostile path (spaces, metacharacters) -> exact file, no `pwned`; j idle hold -> >= 150 bytes before finish, frames_written 24..=28; k drop without finish -> child stopped/zombie within 3 s. Every row with a pid asserts reaped.
Windows rows w1-w4 are in `windows_tests.rs` (cfg(windows)) plus `Scratch::cmd` in test_util.rs: WRITTEN, NOT COMPILED OR RUN HERE. Known uncertainty (batch-file argument escaping, in the file header): a Spawn error in CI would call for a non-batch fake.
File layout deviation: matrix and Windows tests live in new files (`matrix_tests.rs`, `windows_tests.rs`) rather than inside the 05-02 `lifecycle_tests.rs`, which is unix-gated (coordinator split amendment).

## Real ffmpeg e2e (src/record/encoder/real_tests.rs, ffmpeg + ffprobe at ~/.local/ffmpeg/bin)
31 JPEG frames 1001x641, ts 0..3000 ms, fps 10, queue 64:
- webm: vp9, 1280x720, duration 3.100 s (span 3.0 s), 39942 bytes, 31 frames written, finalize_ms 51-55
- mp4: h264, 1280x720, duration 3.100 s, 27721 bytes, 31 frames written, finalize_ms 36-38
Skip path proven by running the built test binary directly with HOME=<scratch>, BLADE_FFMPEG unset: both tests print "skip: no real ffmpeg" and pass. With BLADE_FFMPEG=/nonexistent the override is not a file, so it falls back to ~/.local/ffmpeg and runs for real.

## Docs
`### How recording is encoded` (6 bullets) added under "Recording settings"; no new BLADE_* names (diff of name set: identical); registry drift tests green (7 passed).

## Deviations
- [Test race, not a code bug] Row d first slept a fixed 300 ms; under parallel load sh had not exited yet, the 10-byte writes landed in the pipe buffer and the honest outcome was EmptyOutput (failed ~5 of 6 runs in parallel, 0 alone). Fixed in the test: poll `ps -o stat=` until the child is gone/zombie before sending frames. No lifecycle code changed; the matrix found no bug in encoder code.
- Split test files as above. No code in process.rs/writer.rs touched.

## Final checks
fmt --check clean; `clippy --release --all-targets -D warnings` clean; `cargo test --release` TOTAL PASSED: 564 (lib 523; was 551 / 510), 0 failed, no flake. `record::encoder` run 3 more times: 49 passed each; matrix alone 6 green runs after the fix. `pgrep -fl 'ffmpeg|fake-enc'`: none; no `bladebro-enc-` scratch dirs. `git diff --stat fbad112 -- src/ Cargo.* npm CHANGELOG.md .github`: only src/record.rs and src/record/encoder*.
ROADMAP Phase 5 progress row: "3/3 plans executed; SC4 (windows-latest CI) pending: written, not executed (macOS only)". LIFE-05 not ticked in REQUIREMENTS.md.

## Self-Check: PASSED
