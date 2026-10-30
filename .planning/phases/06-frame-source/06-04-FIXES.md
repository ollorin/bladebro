# 06-04 Fixes: verification gaps 3 and 4

## Gap 4: Lagged frames were never acked
- Cause: `RecvError::Lagged(n)` only added to `lost`. Chrome stops sending after 2 unacked frames, so frames the bus overwrote could stall the screencast.
- Fix (`src/record/source/handler.rs`, `capture.rs`, `report.rs`): on Lagged, ack with the cached session id, `min(n, 2)` times per event (`MAX_LAG_ACKS`), only when an id is known. Counted in the new `SourceStats.lag_acks`, NOT in `acks_sent`, so `frames_received == acks_sent` still holds for frames actually seen; wire acks = `acks_sent + lag_acks`. Verdict string unchanged.
- Tests: handler unit `lagged_acks_with_the_cached_id_bounded` (+ extended no-id case); fake-endpoint `lagged_bus_still_acks_and_the_wire_stays_allowlisted` (stalls the loop, floods 19000 events past the 16384 bus, asserts lost > 0, lag_acks >= 2, every wire ack uses the cached id, wire acks == acks_sent + lag_acks, methods stay within the 4-method allowlist; about 0.7 s).

## Gap 3: video 0.4-0.5 s longer than stop() wall clock
- Cause (recorder, not Phase 5 design): the encoder holds the last frame until `end_at`, which `Encoder::finish` sets. `Source::stop` waits the 500 ms stop grace (acks for in-flight frames) before returning, and the caller only then calls `finish`, so the grace was added to the video (source `elapsed_ms` 8504 vs wall 8003 confirmed it).
- Fix: `FrameSink::mark_end()` (default no-op); the loop calls it the moment stop is requested, before the grace. `Encoder::mark_end()` sets `end_at` first (OnceLock, first set wins; `finish` keeps the earlier mark). No change to Pacer/writer semantics; behaviour is identical when mark_end is never called. `tests/record_live.rs` SizeTap forwards `mark_end`.
- Tests: encoder `mark_end_stops_the_final_hold_before_finish` (marked: 1 write; unmarked after a 700 ms wait: >= 6); source `stop_marks_the_end_before_the_grace_wait`.
- Docs: `docs/recording-and-launch-config.md` limit sentence updated.
- Tooling: added scenario `dur-tick8` to `tools/record_sidecar/scenarios/source_live.sh` (8 s tick.html).

## Live check (headless, scratch BLADE_HOME, local fixtures)
| run | wall_ms | ffprobe | drift |
|---|---|---|---|
| before (6-02 / re-run before fix) | 8003 | 8500 ms | +6.21 % |
| after #1 | 8001 | 8000 ms | -0.01 % |
| after #2 | 8002 | 8000 ms | -0.02 % |
Decode exit 0, 0 warning lines. `bladebro stop` + process scan: no leftover chrome/ffmpeg/http.server (the script's own "leftover" count of 14 on the after-runs was its pgrep matching command lines of my shell that contained the strings `record_live`/`target/release/bladebro`; verified by ps).
Evidence: `.planning/phases/06-frame-source/evidence/06-04-live.txt` (last run).

## Gates
fmt clean; clippy `--all-targets -D warnings` clean; `cargo test --release` 626 passed, 0 failed (baseline 622, +4).
