---
phase: 05-encoder-process-manager
plan: 02
subsystem: recording
tags: [ffmpeg, process-lifecycle, tokio, windows-reasoned]
requires: [05-01]
provides:
  - "record::encoder: ResolvedFfmpeg, resolve_from, resolve_ffmpeg, Encoder, EncoderConfig, push_tail, FINISH_TIMEOUT, STDERR_TAIL_BYTES, HOLD_FLUSH_INTERVAL"
requirements-completed: [SAFE-04, LIFE-05]
completed: 2026-10-22
---

# Phase 5 Plan 02: Encoder process lifecycle Summary

ffmpeg resolution off the runtime, argv spawn, bounded frame queue, paced stdin writer, stderr tail, stdin-EOF finalize with a bounded wait, kill-and-reap only as a last resort. Executed on macOS only; Windows/Linux not executed.

## Public API
- `resolve_from(&Discovery, TargetOs) -> Result<ResolvedFfmpeg, EncoderError>`; `async resolve_ffmpeg()` (discover_cached on `spawn_blocking`; closes the Phase 4 open item).
- `EncoderConfig { ffmpeg, spec, queue_len, finish_timeout }` + `new(ffmpeg, &RecordConfig)`.
- `Encoder::start(&cfg, &Path) -> Result<Encoder, EncoderError>`, `send_frame(&self, Vec<u8>, u64) -> bool`, `pid()`, `path()`, `frames_dropped()`, `async finish(self) -> EncoderReport`.
- Consts: `FINISH_TIMEOUT` 10 s, `STDERR_TAIL_BYTES` 4096, `HOLD_FLUSH_INTERVAL` 1 s, `KILL_REAP_TIMEOUT` 5 s, `STDERR_DRAIN_GRACE` 1 s, `CHILD_WAIT_FLOOR` 2 s.

## D5-9..D5-14 as implemented
- D5-9: mapping table as planned, tested.
- D5-10: order encoder-present, build_args, `Handle::try_current`, spawn; none create a process before passing.
- D5-11: `writer.rs` owns ChildStdin and Pacer; biased select, hold tick only when a frame is pending and the queue is empty; counters atomics, `write_error`/`end_at` OnceLock. Every return path drops stdin (EOF).
- D5-12: one deadline; writer abort drops stdin; child wait gets max(deadline, now + 2 s) (coordinator amendment); on expiry `timeout(5 s, child.kill())` (kill = signal + wait) so reaped is reported honestly; stderr drained with 1 s grace.
- D5-13: try_send; full or closed counts a drop.
- D5-14: `#[cfg(windows)] cmd.creation_flags(0x0800_0000)`; no other Windows-specific code.

## Windows compile status
cfg(windows) block NOT compiled locally (only aarch64-apple-darwin installed; no CI run). It is five lines using tokio's inherent `Command::creation_flags`.

## Tests
Before 537 total (lib 496); after 551 total (lib 510), 0 failed. fmt, clippy `--all-targets -D warnings` clean. `pgrep -fl 'ffmpeg|fake-enc'`: none; no `bladebro-enc-` scratch dirs left. Lifecycle tests (unix): success (argv unchanged, reaped), non-zero exit with stderr, hang killed and reaped, stuck reader (writer blocked in write, aborted then killed), early-exit reader, full queue drops without blocking, missing binary, encoder-missing and bad-path never spawn. Pure: resolve table, push_tail, EncoderConfig::new, start without a runtime.

## Deviations
- [Design] Stderr tail is `Arc<Mutex<Vec<u8>>>` written by the reader task instead of returned from it, so an aborted reader still leaves what it read (std mutex, never held across await).
- [Design] Hold-tick `interval.reset()` on every frame arrival (superset of the first-frame amendment), so ticks are always relative to the latest arrival.
- Extra tests beyond the plan: stuck reader, early exit, full queue.
- Writer lives in `writer.rs` and unix tests in `lifecycle_tests.rs` (coordinator amendment); `HOLD_FLUSH_INTERVAL` therefore defined in writer.rs and re-exported.
- Docs updater step not run (plan defers docs to 05-03); STATE.md not touched.

## Self-Check: PASSED
