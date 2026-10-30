# 07-01 Summary: RecordHub state machine and report mapping

Requirements: CTRL-05 (double start names the file), OUT-05 (stop report fields). Provides the `record_op` claim that 07-02 hooks for CTRL-01.

Commits: 5c5590b (report mapping + messages + record_op), b0ff57c (Hub, HubEnv, global, test-visibility widening).

## API as built (all fork-owned, `src/record/hub.rs` + `hub/`)

- `hub.rs`: `record_op(&Value) -> Option<RecordOp{Start,Stop}>` (string compare on `args["op"]`, no I/O); `static HUB: OnceLock<tokio::sync::Mutex<Hub>>`, `hub()`, `set_ephemeral()`, `is_ephemeral()`, `finish_best_effort()` (never initializes the global; finishes an active recording, stores the orphan result, `tracing::warn!` to stderr), `#[cfg(test)] initialized()`.
- `env.rs`: `HubEnv { config(&self,name)->Result<RecordConfig,String>; async ffmpeg()->Result<ResolvedFfmpeg,EncoderError>; lane()->Lane; async current_tab()->Result<String,String>; attach()->Attach }`, `Lane{Ws,Pipe,Ephemeral,Real}`, `Attach{Port(base),Ws(url)}`.
- `machine.rs`: `Hub::{new, with_knobs(Knobs{stop_grace, finish_timeout}), start(&mut, &impl HubEnv, name), stop(&mut, &impl HubEnv), finish_orphan(), is_active(), orphaned_text()}`, `HubError{Usage,Failed}`, `take_encoder(Arc<Encoder>, tries, wait)`.
- `report.rs`: `compose(&SourceReport,&EncoderReport,fps,&[Segment],tab_note)->(bool,String)`, `Segment{at_s,tab}`, `fields_line`.
- `msg.rs`: all texts (off, ephemeral, pipe, real_lane, usage, none, active, started, not_finalized, tab_moved, orphan_prefix, health_label) and `scrub` (four dead-tab substrings -> `tab gone`).

## Outcome table as implemented

Ok only for Source Captured + Encoder Finalized: `recording saved: <abs path> (bytes N, duration D.D s, frames N, dropped N, segments [0.0 s tab T], captured N only if != frames)`, plus a quiet-tail note and the tab note. Every other pair is `HubError::Failed("recording not clean: <encoder verdict if encoder not Finalized, else source verdict>; <fields>")`. duration = frames_written / fps (0.0 when fps is 0); dropped = encoder frames_dropped + source lost (queue drops counted once).

Start order: name -> lane refusals -> Active (CTRL-05) -> config + enabled gate -> ffmpeg -> current_tab -> create file -> Encoder::start -> Source::start/start_ws. Source failure: encoder taken back and finished, the file this call created removed, Hub stays Idle. Hub becomes Active only after both parts started.

## Tests

27 new (13 report/msg/record_op in Task 1, 14 machine scenarios a-l plus l2 orphan-prefix and no-op orphan in Task 2). `cargo test --release --lib record::hub` 5/5 runs green (27 passed each); `pgrep -fl fake-enc` empty. Full suite 655 passed, 1 ignored, 0 failed (baseline 628 + 27). `concurrent_cold_starts_have_one_owner` failed once in the first full run (known flake), passed on rerun. fmt clean, `clippy --release --all-targets -D warnings` clean, registry drift test green.

## Deviations

- [Rule 3 - Lint] `Hub`, `HubEnv`, `Lane`, `Attach`, the global accessors and the submodules are `pub` rather than `pub(crate)`: in a library crate, `pub(crate)` items unused until 07-02 wires the hook would trip `dead_code` under `-D warnings`. `#[allow(async_fn_in_trait)]` is on `HubEnv` (static dispatch). Note for 07-02: futures from `async fn` in `HubEnv` are not `Send`-bounded; fine for `&impl HubEnv` on the serve/daemon loops, but a `tokio::spawn` of them would not compile.
- Test knobs are `Knobs{stop_grace, finish_timeout}` on `Hub::with_knobs`, applied to `SourceConfig::stop_grace` and `EncoderConfig::finish_timeout`.
- Report tests live in `hub/report_tests.rs` declared from `hub.rs` (not from `report.rs`), likewise `machine_tests.rs` (`cfg(all(test, unix))`).
- No upstream file touched; only `src/record.rs`, `src/record/**` and `.planning/**` changed; HOOKS.md unchanged.
- Not verified here: `Target.getTargetInfo` on a live page (07-02 `HubEnv` live impl); no Chrome in this plan.

## Self-Check: PASSED
Files exist: src/record/hub.rs, hub/{msg,report,report_tests,env,machine,machine_tests}.rs; commits 5c5590b and b0ff57c present.
