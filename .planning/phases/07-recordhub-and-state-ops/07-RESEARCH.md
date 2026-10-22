# Phase 7: RecordHub and state Ops - Research

**Researched:** 2026-10-22 (codebase read at HEAD b079fac, macOS; no network, no builds run)
**Domain:** wiring the finished Phase 5 `Encoder` and Phase 6 `FrameSource` to the `state` tool on three surfaces
**Confidence:** HIGH for code facts (file:line below), MEDIUM for two unrun CDP/behavior assumptions (flagged)

<user_constraints>
## User Constraints (from CONTEXT.md / DECISIONS.md; no CONTEXT.md exists for this phase)

### Locked Decisions
- D-01 (Option A): two new `state` ops `record-start` / `record-stop`, `record-start` reuses `name`, NO new properties (.planning/phases/01-decisions-and-upstream-issue/DECISIONS.md, D-01). Proposed strings: enum `, "record-start", "record-stop"`; description line `RECORDING: record-start <name> (video of the active tab; follows tab switches), record-stop (returns file path, bytes, duration, frames, drops). Needs ffmpeg; dir from BLADE_RECORD.`; name description `, or recording name`. Measured +232 chars (~58 tokens, 13.8% of `state`, 1.5% of all tools).
- D-02: dedicated never-rotated recordings dir; D-07: env/config default plus per-call `state` ops, no 6th tool, no new crate.
- The "Phase 7 gate" text in DECISIONS.md is superseded (header note, 2026-10-21): fork owner decides; no upstream issue.
- Upstream-merge rule (task brief): ALL logic in NEW files `src/record/hub.rs` + `src/record/hub/*.rs`; each pre-existing upstream file gets a tiny hook (<= ~10 lines) listed in `.planning/HOOKS.md`.

### Claude's Discretion
- Hub placement, config semantics of `enabled`, report format, test strategy.

### Deferred Ideas (OUT OF SCOPE)
- Tab following / per-tab files (Phase 8), finalize on every exit path (Phase 9), poster/markers/timeline (Phase 10).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CTRL-01 | Recording off => same CDP calls, no files | Single early-return claim function in the `handle_state` hook; hub never touched for other ops (section 3, 7) |
| CTRL-04 | `record-start`/`record-stop` via `state`, no 6th tool | Hook in `handle_state` covers MCP, daemon, batch/run (section 2) |
| CTRL-05 | second start errors naming the active file | Hub state machine (section 6) |
| OUT-05 | stop returns abs path, bytes, duration, frames, drops, segments | Report composition (section 6); duration is NOT in either existing report |
</phase_requirements>

## Summary

Everything the hub needs exists: `Encoder::start/finish` (src/record/encoder/process.rs) and `Source::start/stop` (src/record/source/capture.rs) are used together exactly as the live driver does (tests/record_live.rs:437-480). All three surfaces route `state` through the single function `handle_state(args, &mut Page)` (src/mcp/server/state.rs:12; callers proto.rs:98, cli.rs:185, act.rs:818, run.rs:448). One 4-line hook there makes record ops work over MCP, the unix daemon, and as batch/run steps, and leaves every other op untouched.

Recommended hub placement is a process-global singleton (`static OnceLock<tokio::sync::Mutex<Hub>>` in `src/record/hub.rs`), not a field next to `Page` in each loop: one process owns exactly one browser and one recording, and the global needs no signature change in `serve()`, `run_daemon()` or `handle_tools_call`. `Page` cannot carry it (`switch_tab` does `*self = new_page`, src/page/tabs.rs:257).

Three facts the planner must not miss: (1) neither `EncoderReport` nor `SourceReport` has a duration, so the hub must compute it; (2) `Page` exposes neither its debug `base` nor its target id, so one 4-line accessor hook (`Page::base`) is needed, and the target id comes from `Target.getTargetInfo`; (3) one-shot CLI (`--no-daemon`, `--host/--port`, and ALL of Windows, since the daemon is `#[cfg(unix)]`) exits after one call, so `record-start` must refuse there.

**Primary recommendation:** pure `Hub` struct (injectable ffmpeg/endpoint for tests) in `src/record/hub/`, a global wrapper, `handle_state` hook calling `hub::try_handle(args, page)`, per-call input = `name` only, `enabled` is a permission gate (refuse with an actionable error when off), failures returned as `Err` so `isError`/exit code are honest.

## 1. Exact public API (HIGH, read from source)

Re-exports in src/record.rs:31-56. All below are `pub`.

**Encoder** (process.rs)
- `resolve_ffmpeg().await -> Result<ResolvedFfmpeg, EncoderError>` (resolve.rs:61; async-safe, cached discovery). `ResolvedFfmpeg{path, version, encoders}` is plain data (resolve.rs:19-23), so tests can build one (`resolve_from` / struct literal).
- `EncoderConfig::new(ffmpeg, &RecordConfig)` (process.rs:55); fields `ffmpeg, spec(format,fps,size), queue_len, finish_timeout` are pub (tests shorten `finish_timeout`).
- `Encoder::start(&cfg, out: &Path) -> Result<Encoder, EncoderError>` (process.rs:100): sync fn, needs a tokio runtime, checks encoder presence and abs path with the right extension BEFORE spawning. Spawns with `kill_on_drop(true)`.
- `send_frame(&self, jpeg, ts_ms) -> bool` (process.rs:182; never blocks), `mark_end()` (216), `pid()`, `path()`, `frames_dropped()`.
- `finish(self).await -> EncoderReport` (process.rs:243): stdin EOF, bounded 10 s (`FINISH_TIMEOUT`), worst case about 18 s (docs/recording-and-launch-config.md "Stop"). `EncoderReport{outcome, path, bytes, frames_queued, frames_dropped, frames_written, frames_unwritten, gaps_clamped, pid, finalize_ms, stderr_tail}`; `.ok()`, `.verdict()`.
- `Outcome`: Finalized, NoFrames, ExitFailed, StoppedReading, EmptyOutput, Unfinalized, Truncated (report.rs:87-107). `EncoderError` Display strings already end "Browsing is unaffected." (report.rs:37-95), except BadOutputPath.

**FrameSource** (re-exported `Source as FrameSource`, record.rs:54)
- `SourceConfig::from_record(&RecordConfig)` (capture.rs:41); fields pub (`no_frames_after`, `stop_grace` overridable by tests).
- `Source::start(&Endpoint, target_id, cfg, sink: Arc<dyn FrameSink>).await -> Result<Source, SourceError>` (capture.rs:73). `Endpoint::Port{base:"host:port"}` resolves the page WS by target id over `/json` HTTP (5 retries x 100 ms); `Endpoint::Pipe` returns `SourceError::PipeTransport` immediately (capture.rs:80-84). Comment at capture.rs:51 says "Phase 7 maps `launched_pipe()` to `Pipe`".
- `Source::start_ws(ws_url, target_id, cfg, sink)` (capture.rs:105) = test seam (no `/json`).
- `src.stats()`, `src.health() -> Health{Waiting,NoFrames,Receiving,Quiet}`, `src.stop(self).await -> SourceReport` (capture.rs:206): sends stop, calls `sink.mark_end()` FIRST (so the video ends at the stop request), keeps acking `STOP_GRACE`=500 ms, bounded cap `2*grace+3 s`, drops the sink. `SourceReport{outcome(Captured|ConnectionLost|NoFrames), stats, target_id, elapsed_ms, last_frame_age_ms, quiet_tail_ms, hidden}`; `.ok()`, `.verdict()`.
- `FrameSink` is implemented for `Encoder` (sink.rs:21); pass `enc.clone() as Arc<dyn FrameSink>`. After `src.stop()` do `Arc::try_unwrap(enc)` then `enc.finish()` (record_live.rs:469-473). If the source's stop timed out the loop task may still hold the sink (JoinHandle drop does not abort): hub must retry `try_unwrap` briefly, then report honestly instead of `expect`.

**Wiring and transports.** Own connection per recording: `CdpClient::connect(ws)` (capture.rs:111) never touches `Page`'s session or bus. It therefore only works on the WebSocket transport. Pipe lane: `crate::browser::launched_pipe()` (browser/flags.rs:180) is true, and `Page.base == "pipe"` (boot.rs:116, `Page::attach(session, "pipe", ...)`). Hub rule: `if launched_pipe() || base == "pipe" { return Err(SourceError::PipeTransport.to_string()) }` before spawning ffmpeg. The CLI daemon is WS-only (daemon.rs:482-495). MCP default is WS (AGENTS.md gotcha).

**Config.** `resolve_from_env(&RecordOverrides{name,format,fps,size}) -> Result<RecordConfig,String>` (config.rs:128): reads env, `<blade>/record.json`, and the `.bladebro.json` found from the PROCESS cwd. `create_recording_file(&cfg) -> Result<(File, PathBuf),String>` (dir.rs:230) creates the 0600 `<stem>-<utc>-<id>.<ext>`; drop the `File`, pass the path to `Encoder::start` (ffmpeg uses `-y`).

## 2. Surface loops, Page ownership, dispatch (HIGH)

| Surface | Loop | Page held | state dispatch |
|---|---|---|---|
| MCP | `serve()` src/mcp/server/serve.rs:35; `let mut page: Option<Page>` :41-42 | local in serve; `handle_tools_call(id,&params,p)` :357 | proto.rs:98 `"state" => handle_state(&args, page)` |
| daemon (unix only) | `run_daemon()` src/cli/daemon.rs:58; `page` :136-137 | local | `cli::dispatch` (cli.rs:185) |
| one-shot CLI / `--host --port` / all Windows | `run_tool` cli.rs:549 -> `run_connected` cli.rs:672 | local `page`, process exits after | `dispatch` (cli.rs:685) |
| batch/run steps | act.rs:804-818, run.rs:435-448 | the same `&mut Page` | `handle_state` (so record ops work as steps for free; `{"action":"state","op":"record-start"}`) |

CLI parse: `parse_state_args` src/cli/args/state.rs:10. Ops are a const `OPS` list (:21-41); unknown ops are a loud usage error; `--name` flag already accepted (:60); positionals are resolved per-op in a `match op` whose default arm is `unreachable!("op validated above")` (:253). **Adding the ops to `OPS` without a match arm panics**: add `"record-start" | "record-stop"` arm that takes name from `--name` or the first positional (record-start) and rejects extras. (`compress` is precedent for an op in `OPS` and the handler but not in the MCP schema enum.)

Retry safety: `retry_safe` (mcp/server.rs:125-133) is an allowlist (`cookies|ls|ss|tabs`), so record ops are never replayed after a crash. Good, do not touch.

**Placement.** Global singleton in `src/record/hub.rs`: `static HUB: OnceLock<tokio::sync::Mutex<Hub>>`, with `Hub` a plain struct taking injected deps so unit tests build instances. Hold the async mutex across `stop` (up to ~18 s); the MCP loop is serial anyway (serve.rs:116 handles one request at a time), the daemon too.

**Page facts the hub needs.** `Page.base` is private (page/mod.rs:199) and there is no target-id field. Add `pub fn base(&self) -> &str` (hook, ~4 lines). Target id: `page.cdp_ref().send("Target.getTargetInfo", None)` -> `targetInfo.targetId` (same call already used on a page session at page/attach.rs:21; MEDIUM: only exercised there under incognito; verify live on WS and pipe-less lanes in plan 07-01). This extra method is sent only at `record-start`/`record-stop`, never in off mode.

**Shutdown / Chrome death (best effort; Phase 9 owns completeness).**
- MCP: all four Chrome teardown sites go through `shutdown_browser` (boot.rs:~203; calls from serve.rs:~210, ~223, ~327 and the exit path ~560), except attach mode where `browser` is `None`. Process exit is serve.rs after the loop (~556-575).
- Daemon: inline `b.shutdown()` at daemon.rs:267, 362, 412, 453; cleanup after the loop :437-455.
- Death of Chrome: the source reports `ConnectionLost` when its WS closes (capture.rs:376-378, liveness tick 250 ms); the encoder keeps its ffmpeg alive with whatever it has. A stale hub must not wedge: on any later record op, if the source is dead, say so and let `record-stop` finalize and report.
- Natural fallback if nothing runs: tokio shutdown drops `ChildStdin`, ffmpeg sees EOF and finalizes what it received (UNTESTED; `kill_on_drop` only fires if the `Encoder` is dropped, and a static is never dropped). Plan one test.
- Recommended minimum hooks: `finish_best_effort().await` in `boot.rs::shutdown_browser`, serve.rs post-loop, daemon.rs post-loop. It stores the finished report as `orphaned: Option<String>` in the hub so the next `record-stop`/`record-start` returns it ("previous recording ended with the browser: ..."). Everything beyond (signals mid-stop, SIGKILL reaper) is Phase 9.
- One-shot: `run_connected` gets a 1-line `crate::record::hub::set_ephemeral()` hook; hub then refuses `record-start` ("recording needs a long-lived session: use the MCP server or the daemon; one-shot CLI exits after each command"). On Windows the CLI is always one-shot, so only MCP records there (document).

## 3. Smallest hook set (HOOKS.md rows to add)

| # | File | Symbol / place | Approx lines | Content |
|---|---|---|---|---|
| 1 | src/mcp/tools.rs | `state` ToolDef: `op` enum (:166), description string (:154-159), `name` description (:168) | 3 lines touched | enum += `"record-start","record-stop"`; one `RECORDING:` line; ` , or recording name` |
| 2 | src/mcp/server/state.rs | top of `handle_state` (:12) | +4 | `if let Some(r) = crate::record::hub::try_handle(args, page).await { return r; }` |
| 3 | src/cli/args/state.rs | `OPS` (:21), usage string (:12-15), `match op` arm before :253 | ~10 | parse name |
| 4 | src/page/mod.rs | `impl Page` near `cdp_ref` (:660) | +4 | `pub fn base(&self) -> &str` |
| 5 | src/cli.rs | top of `run_connected` (:672) | +1 | `set_ephemeral()` |
| 6 | src/mcp/server/boot.rs `shutdown_browser`; src/mcp/server/serve.rs post-loop; src/cli/daemon.rs post-loop (:437) | | +1 each | `finish_best_effort().await` |
| 7 (optional) | src/cli/help.rs | state op list (:49, :639-) | +2 | document ops |

`src/record.rs` gets `pub mod hub;` (fork-owned, no hook). Hook 2 returns `None` immediately for any op other than the two (a string compare on `args["op"]`), before any lock, env read or CDP call: this IS the CTRL-01 mechanism. Phase 8 will additionally want a 1-line hook at the end of `Page::switch_tab` (page/tabs.rs:257); NOT needed in Phase 7 (see 5).

**Schema token delta.** Re-run Phase 1's script unchanged (01-01-PLAN.md:86-105; it slices `name: "state",` to `name: "run",` and `pub fn all_tools()` to `/// Serialize tool definitions`). Baseline today, verified now: state `raw=2038 collapsed=1681 est=420`, all_tools `raw=18963 collapsed=15581 est=3895` (anchors still valid). After the edit expect collapsed 1681+~232 = ~1913 (13.8%) and 15581+232 = ~15813; the planner should require within a few chars of Phase 1's table, or justify the change. Also add a unit test in tools.rs-adjacent hub test asserting `tools_to_json()` still has 5 tools and the `state` enum contains both ops (do not hand-edit tools.rs tests).

## 4. Config semantics and per-call overrides

**`enabled` (recommendation; it is a user-visible decision, see Open Questions).** Treat `enabled` as the operator's permission gate. `record-start` when `enabled==false` returns `Err("recording is off: the operator has not enabled it. Set BLADE_RECORD=1 (or record.enabled in .bladebro.json / record.json), then bladebro stop so the daemon re-reads it. Nothing was recorded.")`. Rationale: (a) CTRL-01/D-02 treat BLADE_RECORD as the on-switch, (b) docs already say enabled/dir "have no per-call layer ... the caller is an LLM, the output directory is an operator decision" (config.rs:5-6), (c) recordings capture credentials (SAFE-03), so an agent must not be able to start capture on a machine whose operator never opted in, and an explicit `BLADE_RECORD=0` must veto. In Phase 7, `enabled=true` does NOT auto-start a recording at session start (no requirement covers it); it only permits `record-start`.

**Per-call overrides.** `RecordOverrides` supports `format|fps|size|name` (config.rs:47-56) and docs say per-call exists for those, but Option A adds only `name` to the schema, so over MCP/CLI the only per-call input is `name`; format/fps/size come from env/project/user layers. Feasible without schema cost: none worth it (encoding them into `name` would be a hack). Phase 7 should pass `RecordOverrides{name: Some(..), ..Default}` and the docs line "Per-call input exists only for ... format, fps, size and name" (docs/recording-and-launch-config.md:53,90) must be corrected to say "library API; the `state` tool exposes only `name`" (DOC-02 drift test, src/launch_mode/registry.rs, may check keys; run it).

**Daemon caveat (MEDIUM).** `resolve_from_env` reads the DAEMON's env and cwd, not the CLI client's. `BLADE_RECORD=1 bladebro state record-start x` against an already-running daemon started without it will say "off". Unlike BLADE_HEADLESS there is no `request_tag` mismatch guard. Document in the error text (the "then bladebro stop" clause) and mention in docs; do not add a guard in Phase 7.

**Stop grace and no-frames surfacing.** Stop is slow by design: 500 ms source grace + up to 10 s drain + trailer wait (about 18 s worst case); report `Truncated`/`Unfinalized` as the encoder says. No-frames: `NO_FRAMES_AFTER`=5 s (watch.rs:16) with a once-only stderr warn (capture.rs:292-300). Surface it to the agent in two cheap places with no new tool calls: `record-start` result stays claim-free ("recording started: <path>; first frame not awaited"), and a second `record-start`/`record-stop` includes `src.health()` when it is `NoFrames`/`Quiet`. Final truth at stop: `SourceOutcome::NoFrames` and `quiet_tail_ms` (report.rs:57-67). Do not wait for a first frame inside `record-start` (adds latency, and a static page may legitimately send one frame only).

## 5. Tab switch with an active recording; target-id binding

- Binding: hub stores `target_id` from `Target.getTargetInfo` at start; `Source` is bound to that target's own WS (capture.rs:105-118), so it is immune to `switch_tab`: after `*self = new_page` the old tab keeps recording and Page's own `Page.enable`/session are separate. The old Page's `Drop` aborts only its own tasks.
- Consequence for Phase 7: after the agent switches/opens a tab, the video keeps showing the ORIGINAL tab. The doc string in D-01 ("follows tab switches") is false until Phase 8 and MUST NOT ship in the schema line unchanged. Recommend the Phase 7 description line drop "follows tab switches" (costs fewer tokens too), and Phase 8 re-adds it. This deviates from DECISIONS.md D-01 wording; flag to the user.
- Honest Phase 7 behavior without a switch hook: at `record-stop` (and double-start) call `Target.getTargetInfo` on the current `page.cdp_ref()`; if the id differs from the bound id, add `note: agent is now on tab <id>; the video shows only tab <bound> (tab following is not implemented yet)`. Segments in the report: exactly one `[{offset 0.0 s, tab <id>}]`.
- Closing the recorded tab: source becomes `ConnectionLost` (capture.rs:376-380); stop reports it with the kept file path. Note `close-tab` auto-switches Page (state.rs:98-116) but the recorder is already dead.
- `Source::start` finds the target by `/json` id; a popup target not yet in discovery retries 500 ms (capture.rs:89-100).

## 6. Stop report, outcome mapping, errors (SAFE-04)

**Computed fields (not in any existing report).**
- Duration: use `frames_written / fps` (the Pacer writes one JPEG per output frame at constant rate; frames_written counts each write, writer.rs:51; `-framerate fps` stamps frame i at i/fps, args.rs doc D5-1). That is the length of the file's content, not a wall clock. Cross-check in tests against wall time within 5% (Phase 6 measured -0.74..+1.24%). Label it "duration 8.1 s (frames x fps; not re-measured from the file)" only if a test shows doubt; otherwise just `duration`. Do not shell out to ffprobe (not guaranteed to exist).
- Dropped: `enc.frames_dropped` (queue-full; equals source `queue_dropped`, do not add both) + `src.stats.lost` (bus lag). Show `capped` (fps cap, intentional) only inside the long form.
- Frames: `frames_written` (video frames incl. holds) and, when different, `captured N` = `src.stats.forwarded`.
- Segments: Phase 7 = one. Keep the field shape `segments: [{at_s, tab}]` so Phase 8 only appends.

**Mapping table (combine `SourceReport.outcome` x `EncoderReport.outcome`).**

| Source | Encoder | Result | Return |
|---|---|---|---|
| Captured | Finalized | `recording saved: <abs path> (<bytes> B, <dur> s, <frames> frames, dropped <n>, segments 1 [tab ..])` + `quiet tail` note if `quiet_tail_ms` | `Ok` |
| NoFrames | NoFrames | source verdict (it knows `hidden`); file: none/empty and removed? see below | `Err` |
| ConnectionLost | Finalized | `recording interrupted: ... file kept and playable up to <t>` (path, bytes, frames) | `Err` (data present) |
| any | Truncated / Unfinalized / ExitFailed / StoppedReading / EmptyOutput | encoder `verdict()` (already honest, names path) + source line | `Err` |

Use `Err` for everything except Captured+Finalized so MCP `isError` and the CLI exit code (cli.rs:156-160 `is_error`) are truthful; in batch/run an `Err` step stops the batch (state.rs-callers run.rs:455-461) which is acceptable (user decision, see Open Questions). Never print "saved/recorded/finalized" unless `Captured && Finalized`.

**Errors needed (all must be actionable and end with the existing "Browsing is unaffected." pattern, and avoid the substrings "Target closed", "No target with given id", "Session closed", "Target.detachedFromTarget"; proto.rs:123-126 and serve.rs:408-411 treat those as a dead tab and trigger `recover_dead_tab`; never return `BladeError::Closed`, serve.rs relaunches Chrome on it):**
1. off: text above. 2. ephemeral surface (above). 3. pipe: reuse `SourceError::PipeTransport` Display (report.rs:100). 4. double start: `recording already active: <abs path> (started <n> s ago, <health>). record-stop it first.` (CTRL-05) 5. stop with none: `no active recording; record-start <name> first` (or return `orphaned` text). 6. ffmpeg/encoder/path/start failures: existing `EncoderError`/`SourceError` Display. 7. real-browser lane (see Open Questions). 8. bad args: `name` must be a string (the generic check already runs only after the hook; do it in hub). 9. config errors from `resolve_from_env` (already name their layer).

**Start-failure cleanup.** Order: cheap checks -> `resolve_ffmpeg` -> `create_recording_file` -> `Encoder::start` -> `Source::start`. If `Source::start` fails after the encoder spawned: drop the sink, `enc.finish().await`, and `remove_file` the empty file this call created (an empty `.webm` must not be left). Test it.

## 7. Test strategy

- Test framework: `cargo test --release` (628 passing baseline, 1 ignored; one pre-existing flaky `daemon_lifecycle::concurrent_cold_starts_have_one_owner`). Quick: `cargo test --release --lib record::hub`. Instruments: `python3 tools/qol_probe/run.py` (macOS baseline 120/122, the 2 failures pre-existing; compare before/after on the same machine, not to 122), `python3 tools/lane_matrix.py all 5`, `./target/release/bladebro audit` (macOS baseline 100/100, 111 is the Linux number). diff_oracle cannot run here (Python `websockets` missing; STATE.md).
- **Fake ffmpeg:** reuse `encoder/test_util.rs` (`Scratch`, `fake_ffmpeg`, `script`, `start_retry`) but it is a private `mod test_util` inside encoder.rs:21 and `source/fake_cdp.rs` items are `pub(super)`. Plan task: widen both to `pub(crate)` under `#[cfg(test)]` (fork files, no hook needed). Fake script that finalizes: `for a; do out="$a"; done; cat > "$out"` (lifecycle_tests.rs:25-26). Missing-ffmpeg test: inject a `ResolvedFfmpeg`/`EncoderError::FfmpegNotFound` through the Hub deps (production path uses `resolve_ffmpeg()`); do not depend on PATH.
- **Hub unit tests** (instance `Hub`, not the global): start/stop happy path with fake ffmpeg + `FakeCdp` via `Source::start_ws` (FakeCdp has no `/json`, so the Hub deps must accept a ws url or a connector fn); double-start error names the file; stop-without-start; off-mode refusal creates no file/dir; pipe refusal spawns nothing (assert no `fake-enc` process); ephemeral refusal; source `ConnectionLost` path (FakeCdp `close_now`); encoder `ExitFailed`; start-failure cleanup removes the empty file; `try_unwrap` failure branch; outcome table rows; message substrings guard (no "Target closed" etc.).
- **Off-mode no-op proof (CTRL-01):** (1) unit: for every existing op name (`tools.rs` enum + `compress`) `hub::claims(args)==false` and the global `HUB` is never initialized (expose `hub::initialized()` for tests); (2) schema test: tools.len()==5 and delta vs recorded baseline; (3) live, scratch `BLADE_HOME`, `BLADE_HEADLESS=1`: run a standard nav/see/state session, assert `<BLADE_HOME>/recordings` does not exist, `pgrep -f ffmpeg` empty, Chrome argv unchanged (Phase 6 measured argv bytes via `ps`, source_live.sh trace scenario); (4) CDP method trace of bladebro's OWN Page: there is no built-in trace; the proxy in tests/record_live.rs:~260-310 fronts only the recorder. A true byte-level Page trace would need a proxy in front of Chrome plus `/json`; treat as a spike, otherwise accept (1)-(3) as the proof because the hook returns before any I/O. Be explicit about which was done.
- **Live helpers (tools/record_sidecar):** `env.sh` (scratch BLADE_HOME, `fixture_up` on 127.0.0.1:8765, `bb_port`, `BLADE_FFMPEG` default `~/.local/ffmpeg/bin/ffmpeg`), `scenarios/source_live.sh` pattern (builds, starts daemon via `bladebro nav`, always `BLADE_HEADLESS=1`, `analyse` = ffprobe + full decode + drift). Add `scenarios/hub_live.sh`: daemon lane: `BLADE_RECORD=1 bladebro nav http://127.0.0.1:8765/anim.html`, `bladebro state record-start t1`, wait ~8 s (+ a `state open-tab` for the switch note), `record-stop`, ffprobe the reported path, double-start error, off-mode run, and MCP lane via a stdin JSON-RPC script (both lanes: AGENTS.md lane_matrix). Also a SIGTERM-the-MCP-mid-recording probe to record what the natural EOF fallback does.
- Wave 0 gaps: `src/record/hub.rs` + `hub/*.rs` and tests; widen test util visibility; `scenarios/hub_live.sh`; HOOKS.md rows; docs page (op documentation, correct the per-call claim, remove "Not functional yet", keep DOC-02 drift test green).

## 8. Risks and Open Questions

1. **`enabled` semantics (user decision).** Recommended: permission gate, no auto-start. Alternative: any `record-start` works regardless, and `enabled` only means auto-start/default-on (not built). Planner should confirm with the user before 07-02 locks the error text.
2. **Schema string "follows tab switches" is untrue in Phase 7** (section 5). Recommend dropping it now. Confirm.
3. **Real-browser lane (`rb`)**: recording the user's own browser. Recommend refusing in Phase 7 with a clear message (`crate::realbrowser::real_lane()`), pending a decision.
4. **Failure return type** (`Err` vs `Ok` text) affects batch/run abort behavior; recommended `Err` for any non-clean stop.
5. **`Target.getTargetInfo` without targetId on a page WS** is only used in the incognito path today (MEDIUM); fallback if it fails: match `Page.tab_targets()` by current URL (fragile) or add a Page accessor. Verify live on macOS in 07-01.
6. **Daemon env/cwd staleness** (section 4) and **Windows = MCP only** (daemon is `#[cfg(unix)]`). Windows code is unrun (Phase 5 w4 fakes never compiled; no CI run; nothing pushed).
7. **Exit-path finalization is only best effort** (Phase 9). SIGKILL leaves an unfinalized file; hub finish hooks bounded by about 18 s may delay shutdown; idle shutdown (600 s) ends Chrome and thus the recording, the stored `orphaned` report is the only trace.
8. **Carry-overs from Phase 5/6 verification:** crf 40 vs 36 awaiting user decision; animated pages run about +100 ms (docs wording, Phase 6 finding); `docs` claims "per-call format/fps/size".
9. **Global singleton in tests** can leak state between tests; keep all logic on `Hub` instances and keep the global wrapper trivially thin.
10. **Stop blocks the serial loop up to about 18 s**; MCP clients with short tool timeouts could give up. Unmeasured.
11. **Spike (not required):** a Page-level CDP trace for the off-mode proof.

## Standard Stack

No new dependencies (Cargo.toml untouched). Use what exists: `tokio` (Mutex, process), `serde_json`, `tracing` for the one stderr warn, `crate::record::{Encoder, FrameSource, resolve_from_env, create_recording_file}`.

## Don't Hand-Roll

| Problem | Use instead |
|---|---|
| ffmpeg lifecycle, verdicts | `Encoder`/`EncoderReport::verdict` (Phase 5) |
| capture, acks, no-frames honesty | `Source`/`SourceReport::verdict` (Phase 6) |
| file names, private dir | `create_recording_file`, `names.rs` |
| config layers | `resolve_from_env`, never re-read env in hub |
| error text | existing `EncoderError`/`SourceError` Display |

## Sources (all HIGH, repository files read in this pass)
src/record/{encoder,source}/** and record.rs; src/mcp/{tools.rs,server.rs,server/{state,proto,serve,boot}.rs}; src/cli.rs; src/cli/{args/state.rs,daemon.rs}; src/page/{tabs.rs,mod.rs,attach.rs}; src/browser/flags.rs; tests/record_live.rs; tools/record_sidecar/{env.sh,scenarios/source_live.sh}; docs/recording-and-launch-config.md; .planning/{PROJECT,REQUIREMENTS,ROADMAP,STATE,HOOKS}.md, phases/01 DECISIONS.md, phases/05+06 VERIFICATION.md, research/{ARCHITECTURE,PITFALLS}.md.

**Valid until:** next change to the files above (internal, fast-moving: upstream merges daily).
