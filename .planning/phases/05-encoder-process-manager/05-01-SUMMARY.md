---
phase: 05-encoder-process-manager
plan: 01
subsystem: recording
tags: [ffmpeg, encoder, argv, pacing, verdicts]
requires: []
provides:
  - "record::encoder: report (EncoderError, ExitInfo, Outcome, EncoderReport, classify, tail_for_verdict), args (EncodeSpec, build_args, check_out_path, even_size, clamp_fps), pace (Pacer, repeat_counts, MAX_GAP_MS)"
affects: [05-02, 05-03]
requirements-completed: [SAFE-04]
completed: 2026-10-22
---

# Phase 5 Plan 01: Pure encoder half Summary

Pure, process-free half of the ffmpeg encoder: honest verdicts + outcome classification, the argv builder and timestamp pacing, all table-tested. Executed on macOS only.

## Public API
- `report`: `EncoderError` (Display = verdict), `ExitInfo`, `Outcome`, `EncoderReport {ok, verdict}`, `classify`, `tail_for_verdict`, `VERDICT_TAIL_CHARS`.
- `args`: `EncodeSpec {from_config}`, `build_args(spec, &FfmpegVersion, out) -> Result<Vec<OsString>, EncoderError>`, `check_out_path`, `even_size`, `clamp_fps`.
- `pace`: `Pacer {new, on_frame, advance_to, on_end, written, clamped}`, `repeat_counts`, `MAX_GAP_MS`.
- Re-exported from `src/record.rs` (`build_args, classify, repeat_counts, EncodeSpec, EncoderError, EncoderReport, ExitInfo, Outcome, Pacer`).

## Decisions as implemented
- D5-1: duplicate-on-write at constant rate (`-framerate`, `-fps_mode passthrough`, no output `-r`); Pacer slots = round-half-up cumulative. Planning measurement: 31 copies of a 1001x641 JPEG at `-framerate 10` gave 3.100 s ffprobe duration for webm/vp9 and mp4/h264 (ffmpeg n9.0.2). This deliberately deviates from the Phase 2 "concat" recommendation (stdin required, no spool dir). Spans under about 1 s may exceed the 10 % duration bar by one slot.
- D5-2: VP9 `-crf 40` (measured RESULTS.md numbers were at 40).
- D5-3: Release major <= 5 gets `-vsync passthrough`; major >= 6, Git, Unknown get `-fps_mode passthrough`.
- D5-4: path rejected when it starts with '-', is relative, or has the wrong extension (case-insensitive); explicit `-f webm|mp4`; `-y`.
- D5-5: `-hide_banner -nostats -loglevel error`, `-an`.
- D5-7: no option, env var or config key added.
- D5-8: classify precedence Killed, NoFrames, ExitFailed, StoppedReading, EmptyOutput, Finalized.

## Plan reallocation
Verdict strings moved here from 05-03 (05-02's lifecycle returns them). Report module is `src/record/encoder/report.rs`, not `src/record/report.rs`, keeping `record::report` free for Phase 7's stop report and Phase 5 code in one folder.

## Tests
Baseline 515, now 537 passed (+22; lib 496), 0 failed. fmt, clippy `--all-targets -D warnings` clean; `launch_mode::registry` drift scan green.

## Deviations
- [Rule 3 - lint] Test table type aliased to satisfy clippy `type_complexity`.
- ROADMAP "VP9/VP8" wording already absent from the replaced 05-01 line; plan lines for 05-01..03 rewritten as specified.
- STATE.md not updated (execution constrained to src/record.rs, ROADMAP and docs); the `src/record.rs` pace re-exports landed in the summary commit.
- Docs update step skipped: plan defers docs to 05-03.

## Self-Check: PASSED
