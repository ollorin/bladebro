# 06-01 Summary: Frame source core

Requirements: CAPT-01, CAPT-02, SAFE-01 (method half). ROADMAP SC1, SC2 and the `shrink_oversized_event` half of SC3, proven against an in-process fake CDP endpoint (no Chrome). Executed on macOS only; no cross-OS result is claimed.

## Commits
- 57db2b5 feat: keep screencast frames intact through the CDP event size bound (hook + fake endpoint + hook tests)
- 2e9f562 docs: list the screencast hook in HOOKS.md
- 17d246b feat: screencast frame parsing, fps gate and source verdicts
- 4942e69 feat: ack-first screencast source with bounded hand-off and closed method set

## Public API (`bladebro::record::source`, re-exported in `record`)
`Source` (record re-export: `FrameSource`, because `record::Source` is already `ffmpeg::Source`), `SourceConfig` (+`from_record`), `Endpoint {Port{base}, Pipe}`, `FrameSink` (impl for `Encoder`), `RecMethod`, `ACK_TIMEOUT`, `STOP_GRACE`, `SourceStats`, `SourceOutcome`, `SourceReport` (`ok()`, `verdict()`), `SourceError`, `FpsGate`/`GateStep`, `parse_frame`/`parse_frame_owned`/`decode_jpeg`/`ts_ms_from_metadata`.
`Source::start(&Endpoint, target_id, cfg, Arc<dyn FrameSink>)`, `start_ws(ws_url, ...)`, `stats()`, `async stop(self) -> SourceReport`; `Drop` aborts the loop and closes the WS.

Files (all new, fork-only): `src/record/source.rs` + `source/{capture,handler,frame,gate,report,wire,sink}.rs`; test-only `fake_cdp.rs`, `hook_tests.rs`, `capture_tests.rs` + `capture_tests/{flow,limits,lifecycle}.rs`. Split per reviewer note (b): `wire.rs` (RecMethod + single send path) and `sink.rs` (FrameSink) out of capture.rs; capture tests split into three files (all files < 350 lines).

## Decisions as implemented
- D6-1: own `CdpClient::connect` to the target's page WS found by `/json` lookup by target id (5 tries, 100 ms); wrapped as `CdpSession::root`. Chosen over browser-WS + `Target.attachToTarget` because it needs no `Target.*` method and binds by target id, as `attach_tab_target` does.
- D6-2: `Endpoint::Pipe` -> `SourceError::PipeTransport` before any I/O; verdict text verbatim.
- D6-3: `RecMethod` closed enum; `Wire::send(RecMethod, ..)` is the only send path; per-method counts in `methods_sent`.
- D6-4: ack decided and spawned before decode/gate/sink on every path (JoinSet, ACK_TIMEOUT 2 s); `frames_received == acks_sent + unacked`; cached session id for params-stripped frames.
- D6-5: reader only broadcasts; the loop's only awaits are the select; `FrameSink::send_frame` = `Encoder::send_frame` try_send; `Lagged(n)` -> `lost`; `Closed` or a closed client -> `ConnectionLost`.
- D6-6: `FpsGate` newest-wins with timer trailing flush and flush at stop; base64 decoded only for forwarded frames.
- D6-7: `ts_ms = round(metadata.timestamp*1000)`, epoch-now fallback counted `ts_fallback`.
- D6-8: hook as specified. D6-9: no new option, env var or config key; limits are consts.
- biased select order (stop, frame, timer, stop-grace, liveness) is pinned by `flood_keeps_the_fps_cadence_so_the_timer_is_not_starved` (frames always ready still forward at cadence) and `held_trailing_frame_is_flushed_by_the_timer` (silence: only the timer can deliver); handler unit test pins the arrival-path flush.

## Hook (the only upstream edit: `src/cdp/client.rs`, 13 lines added, 1 line changed)
```rust
pub(crate) const MAX_SCREENCAST_EVENT_BYTES: usize = 8 * 1024 * 1024;   // under MAX_EVENT_BYTES, with doc comment
// route_cdp_text:
if text.len() > MAX_EVENT_BYTES
    && !(ev.method == "Page.screencastFrame" && text.len() <= MAX_SCREENCAST_EVENT_BYTES)
{
    shrink_oversized_event(&ev.method, &mut ev.params);
}
```
HOOKS.md row (phase 6, `route_cdp_text` + `MAX_SCREENCAST_EVENT_BYTES`, commit 57db2b5). Upstream `reliability_tests` untouched and green.

## Tests
Total `cargo test --release`: 565 (baseline given) -> 610 passed, 0 failed (+45 new: 5 hook, 20 pure pieces/handler/wire/sink, 10+ capture scenarios, rest unit). `record::source` runs 45 tests. fmt --check and `clippy --release --all-targets -D warnings` clean. The known flake `concurrent_cold_starts_have_one_owner` did not trigger. No fake-enc/ffmpeg processes left.

## Deviations
- [Rule 3 - naming] `record::Source` already exists (`ffmpeg::Source`); re-exported as `FrameSource` in `record.rs` (the `source::Source` path is unchanged).
- [Rule 2 - correctness] CdpClient's event bus never reports `Closed` while a client handle lives (our `Wire` holds one), so a dead connection would never be noticed from `recv`. Added a 250 ms liveness tick on `is_closed()` in the loop (no upstream change); covered by `a_closed_connection_reports_connection_lost`.
- Task 1 hook tests were written together with the hook (no separate red run).
- Test 3/4 sizing adjusted from the plan: at fps 15 a 300-frame flood only forwards ~5 frames, which cannot fill a 16-slot queue, so the stuck-sink test floods 2000 frames at 1 ms (forwarded == 16 exactly) and the slow-consumer test uses queue 2 / 300 ms drain.
- The fake's `ping_method` option was dropped (the test pings `Page.enable` on the source's own client; the fake answers every method).
- Plan test 6 (Lagged/Closed) lives as handler unit tests in `handler.rs` rather than in capture_tests.
- Commit 1 was amended once to remove a duplicated Co-authored-by trailer added by gsd-tools; later commits use plain git.
- No new upstream edits beyond client.rs; no Cargo/npm/CHANGELOG/.github edits; no new crate; no option/env/config key added.

## Open issues
- Stop always waits the full `stop_grace` (500 ms) before returning; Phase 7 may care about stop latency.
- Frames above the 8 MiB cap arrive stripped and are counted `bad_frames` (acked via the cached id).
- Live Chrome behaviour (real ack rule, background tabs) is for 06-02/06-03.

## Self-Check: PASSED
