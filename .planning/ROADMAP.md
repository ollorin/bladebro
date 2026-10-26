# Roadmap: Bladebro Session Recording

## Overview

Opt-in session video recording for Bladebro, built so a QA agent's run leaves a playable video at a predictable path reported in the tool result. The journey starts with no-code design decisions, then a throwaway sidecar prototype that must pass before any in-tree work. In-tree work then goes from pure, browser-independent pieces (config, safe paths, ffmpeg discovery, encoder process) through the frame source, the `state` op surface, tab following and finalization on every exit path. Evidence extras, integration docs and a full instrument gate (stealth, cross-platform, regression) close the milestone. Every in-tree phase carries its own tests; the final gate re-verifies everything together with recording on.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

**Standing rule (all phases):** any phase that adds or changes an option, environment variable, config key or per-call parameter must update `docs/recording-and-launch-config.md` (created in Phase 4.1) in the same phase; the verifier checks it and the drift test (DOC-02) enforces it once it exists.

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Decisions** - Record the design decisions for the `state` op shape, recordings directory and multi-tab policy (no code; no upstream approval needed, the fork is the main repo)
- [x] **Phase 2: Sidecar Prototype Gate** - Prove screencast capture, ffmpeg piping and tab-following in `tools/record_sidecar/` with recorded measurements (pass/fail gate)
- [x] **Phase 3: Recording Config and Safe Output Paths** - Config precedence, format/size/fps settings, sanitized names and a private, never-rotated recordings directory
- [x] **Phase 4: ffmpeg Discovery and Doctor** - Find, probe and cache ffmpeg; `bladebro doctor` reports it
- [x] **Phase 4.1: Headless/Headed Launch Switch and Layered Config (INSERTED)** - `BLADE_HEADLESS`, project/user config layers, honest stealth flag, mode shown in doctor, and the options reference page with a drift test
- [~] **Phase 5: Encoder Process Manager** (3/3 plans executed, verified on macOS; SC4 windows-latest CI pending) - Spawn, feed, finalize and honestly report an ffmpeg encoder on all three OSes
- [x] **Phase 6: Frame Source** - Ack-first screencast capture with a bounded, drop-counting queue and blank-video detection, stealth-neutral
- [x] **Phase 7: RecordHub and state Ops** - `state` `record-start` / `record-stop` with full stop report; zero behavior change when off
- [ ] **Phase 8: Tab Following** - One recording follows the active tab with segment boundaries; opt-in per-tab files
- [ ] **Phase 8.1: Follow Every Popup (INSERTED)** - A click-opened or script-opened popup becomes a segment without `switch-tab`; the video returns to the opener when it closes; `state tabs` and the segments agree (user decision 2026-10-26; runs after Phase 9 is verified)
- [ ] **Phase 9: Finalization on Every Exit Path** (EXECUTED, NOT VERIFIED) - Recordings finalize on every exit, crashes are reported honestly, no orphan ffmpeg, disk-low stop
- [ ] **Phase 10: Evidence Extras** - Poster frame, agent markers and an action timeline sidecar aligned to the video
- [ ] **Phase 11: Integration Docs** - How the Claude Code GSD flow and the DSH plugin enable recording per test case
- [ ] **Phase 12: Full Instrument Gate** - Test-suite completeness plus the full stealth, cross-platform and regression gate with recording on

## Phase Details

### Phase 1: Decisions
**Goal**: The open design questions have recorded answers before any schema code is written
**Depends on**: Nothing (first phase)
**Requirements**: VER-02
**Success Criteria** (what must be TRUE):
  1. The decision record states the chosen `state` op shape (Option A: `record-start` / `record-stop` reusing `name`), the dedicated `~/.blade/recordings/` directory and `BLADE_RECORD`, with the token cost of the schema addition measured
  2. A decision record in `.planning/` captures the multi-tab policy (one file following the active tab, segment boundaries), the recordings directory choice, and marks codec, fps cap and timestamp strategy as provisional pending Phase 2 measurements
  3. Nothing blocks Phase 7: the fork owner decides, and the upstream-issue draft is kept only as design rationale (it is not posted and will not be)
**Plans**: 1 plan

Plans:
- [x] 01-01: Write decision record (an unposted upstream-issue draft was also produced; now rationale only)

### Phase 2: Sidecar Prototype Gate
**Goal**: A standalone sidecar proves that CDP screencast plus ffmpeg produces a correct, playable recording across a tab switch, and its measurements decide the in-tree architecture and defaults
**Depends on**: Phase 1
**Requirements**: VER-01
**Success Criteria** (what must be TRUE):
  1. Running the sidecar in `tools/record_sidecar/` against a bladebro-driven Chrome (WebSocket transport, scratch `BLADE_HOME`) produces a playable 30 s WebM that spans a tab switch
  2. A results file in `tools/record_sidecar/` records effective fps, ack latency, drops under a slow consumer, frames vs acks over 60 s, duration drift vs wall clock, headed vs headless/Xvfb frame behavior, and mid-recording viewport change behavior on Chrome 151
  3. The results state whether bladebro's own CDP events kept flowing during capture and whether `Page.enable`/screencast changed `bladebro audit` (111/111) or `diff_oracle` (divergent 0)
  4. A written verdict picks the in-tree approach (existing `Page` session vs second client), timestamp strategy, codec default and fps/size defaults; if the gate fails, the roadmap is revised before Phase 3
  5. The sidecar has a README and runs as a supported probe (not shipped in the binary)
**Plans**: 3 plans

Plans:
- [x] 02-01: Sidecar CDP client: attach to a page target, `Page.startScreencast`, ack-on-receipt, pipe JPEG to ffmpeg, stdin-EOF finalize
- [x] 02-02: Tab-following and measurement harness (fps, ack latency, drops, drift, headed/headless, viewport change)
- [x] 02-03: Stealth check (audit, diff_oracle with screencast active), results file and architecture verdict

### Phase 3: Recording Config and Safe Output Paths
**Goal**: Users can turn recording on and tune it through env, config or per-call values with a predictable precedence, and every recording file lands safely in a private, never-rotated directory
**Depends on**: Phase 2
**Requirements**: CTRL-02, CTRL-03, CTRL-06, CTRL-07, OUT-04, SAFE-02, SAFE-03
**Success Criteria** (what must be TRUE):
  1. With `BLADE_RECORD` unset and no config, the resolved config is "off"; with `BLADE_RECORD=<dir>` it is on with that directory; a config file under `BLADE_HOME` applies, and precedence per-call > env > config > defaults is proven by unit tests
  2. Format (WebM default, MP4 optional), max frame size and fps resolve through the same precedence, with defaults taken from the Phase 2 verdict
  3. Caller names with traversal, leading `-` or `.`, drive letters, UNC prefixes or odd characters resolve to a safe, unique basename under the canonical recordings directory, created with `create_new`; tests pass on ubuntu, macos and windows CI
  4. On Unix the recordings directory is 0700 and files are 0600; the Windows ACL decision is written down; the README states that recordings can contain sensitive content
  5. Recordings never go to `~/.blade/artifacts/` and nothing rotates the recordings directory; clippy `-D warnings` and `cargo test --release` are green
**Plans**: 3 plans

Plans:
- [x] 03-01: `src/record.rs` header + `src/record/values.rs` + `src/record/config.rs`: settings, precedence, format/size/fps resolution, unit tests
- [x] 03-02: Name sanitization and recordings-dir creation (canonical dir, `create_new`, 0600/0700, helpers in `src/record/` (not `src/platform.rs`, SYNC-01)), cross-OS tests
- [x] 03-03: Windows ACL decision note and README sensitive-content warning

### Phase 4: ffmpeg Discovery and Doctor
**Goal**: Bladebro reliably finds a usable ffmpeg, knows which encoders it has, and tells the user in `doctor`
**Depends on**: Phase 3
**Requirements**: OUT-01, OUT-02
**Success Criteria** (what must be TRUE):
  1. ffmpeg is resolved from `BLADE_FFMPEG`, then PATH, then known install locations on each OS, and the version/encoder probe runs once per process (cached)
  2. `bladebro doctor` prints the ffmpeg path, version and available WebM/MP4 encoders, and says clearly when ffmpeg is missing or lacks an encoder for the configured format
  3. Unit tests cover the lookup order and parsing of `-version` / `-encoders` output; clippy and tests green on all three OSes
**Plans**: 2 plans

Plans:
- [x] 04-01: `src/record/ffmpeg.rs` lookup order (BLADE_FFMPEG > PATH > known dirs) + probe/parse + OnceLock cache, unit tests (moved out of platform.rs: fork-only, SYNC-01)
- [x] 04-02: `doctor` reporting for ffmpeg and encoders

### Phase 4.1: Headless/Headed Launch Switch and Layered Config (INSERTED)
**Goal**: A user can run QA suites with a hidden browser and everything else with a visible one, set per suite or per project, and every option and config is documented in one place that tests keep honest
**Depends on**: Phase 4
**Requirements**: HEAD-01, HEAD-02, HEAD-03, HEAD-04, DOC-02
**Success Criteria** (what must be TRUE):
  1. With `BLADE_HEADLESS=1` the managed browser launches with no visible window on macOS (process arguments show `--headless=new`, no window appears) and with `BLADE_HEADLESS=0` it launches visible; with the variable unset behavior is unchanged on every OS (macOS/Windows headed, Linux Xvfb with headless fallback)
  2. Settings resolve per-call > env > project config file (found walking up from the working directory) > user config > default, proven by unit tests; the recording settings from Phase 3 use the same layers; `bladebro headless on|off|status` writes and reads the user layer
  3. In headless mode the stealth layer's launched-headless flag is true and `bladebro audit` passes in both modes (100/100 on this macOS build); changing the mode while a daemon runs either restarts the browser or prints a message telling the user to run `bladebro stop`
  4. `bladebro doctor` and a one-line launch note show the mode and its source
  5. `docs/recording-and-launch-config.md` exists and documents every option and config added so far (BLADE_RECORD*, BLADE_FFMPEG, BLADE_HEADLESS, config files and precedence, headless-for-tests vs headed-for-everything-else examples); a drift test fails if a variable or key in the code is missing from the page or documented but absent from the code; the README holds only a pointer; all edits to existing upstream files are listed in `.planning/HOOKS.md`
**Plans**: 4 plans

Plans:
- [x] 04.1-01: `BLADE_HEADLESS` resolver + minimal launch hook in `src/browser/launch.rs` + launched-headless flag + tests
- [x] 04.1-02: Layered config (project file, user config, `bladebro headless` toggle), mode in doctor and launch note, daemon-restart handling
- [x] 04.1-03: `docs/recording-and-launch-config.md`, drift test, README pointer, live check in both modes
- [x] 04.1-05, 04.1-06 (gap closure): opus-found poisoned daemon, launch note, drift test, client-hints values, PATH-exec regression
- [x] 04.1-04 (gap closure): headless audit parity (screen pin + UA client-hints metadata), found by the 04.1-03 live audit (97/100 before, 100/100 after)

### Phase 5: Encoder Process Manager
**Goal**: Given a stream of JPEG frames, Bladebro produces a finalized video file and reports success only when that is proven, on Linux, macOS and Windows
**Depends on**: Phase 4
**Requirements**: SAFE-04, LIFE-05
**Success Criteria** (what must be TRUE):
  1. ffmpeg is spawned with an argv array (no shell), fed on stdin, stdout discarded, stderr kept as a bounded tail; the argument builder produces even-dimension scaling and the Phase 2 timestamp strategy, covered by argument tests
  2. Stop closes stdin and waits with a bounded timeout; a kill happens only as last resort and the result is then "unfinalized"; success is reported only on ffmpeg exit 0 with a non-empty file
  3. A missing ffmpeg, missing encoder or non-zero exit produces an honest, actionable error string, and browsing commands keep working
  4. On Windows the stdin-EOF finalize path works with no console window, verified by a fake-encoder lifecycle test running in the `windows-latest` CI job
**Plans**: 3 plans

Plans:
- [x] 05-01: `src/record/encoder.rs` + `encoder/{report,args,pace}.rs`: verdict strings + classify, argv builder (WebM VP9 / MP4 libx264, even-dim scale, safe output path), timestamp pacing (duplicate-on-write, D5-1) + tests
- [x] 05-02: `encoder/{resolve,process}.rs`: async-safe ffmpeg resolve, spawn, stdin writer task with progressive hold, stderr tail, bounded finish, kill fallback, Windows no-console flag + unix lifecycle tests
- [x] 05-03: fake-encoder lifecycle matrix (unix run; Windows written, not executed), real ffmpeg end-to-end per format, encoding note in docs
- SC4 status: Windows fake-encoder tests written (`src/record/encoder/windows_tests.rs`, cfg(windows)), not compiled or executed (macOS only, no CI run).

### Phase 6: Frame Source
**Goal**: Screencast frames flow from a real page into the encoder without ever stalling the CDP reader or changing what the page can see, and a blank capture is reported rather than passed off as success
**Depends on**: Phase 5
**Requirements**: CAPT-01, CAPT-02, CAPT-05, OUT-03, SAFE-01
**Success Criteria** (what must be TRUE):
  1. On the local fixture server, frames arrive as JPEG via `Page.startScreencast` and every `screencastFrame` is acked on receipt; frame count equals ack count in tests
  2. With a deliberately slow encoder, the CDP reader keeps serving other commands, the queue stays bounded, newest frames are dropped and the drop count is reported
  3. A loaded page that yields no frames for the configured period produces a "no frames" result instead of success; a 2 MB `Page.screencastFrame` passes through `shrink_oversized_event` unchanged
  4. On an idle page, video duration is within 5% of wall-clock time, and odd or changing frame sizes (e.g. 1001x641, mid-recording resize) still yield a playable file
  5. Only `Page` screencast methods are sent (CDP trace shows no `Runtime.enable`, `Emulation.*`, DOM overlay or new launch flag); `bladebro audit` 111/111, `diff_oracle` divergent 0, `qol_probe` 122/122 still pass
  - SC5 note (06-03, macOS headless): audit 100/100 without and with the recorder (111 is the Linux figure); wire trace = Page.enable, startScreencast, screencastFrameAck, stopScreencast only; Chrome argv identical; battery divergent 0; qol_probe 120/122 (the 2 known failures); diff_oracle NOT RUN (python websockets missing)
**Plans**: 3 plans

Plans:
- [x] 06-01: `src/record/source.rs` + `source/{capture,handler,frame,gate,report,wire,sink}.rs`: own page-WS connection, Page.enable + startScreencast, ack-first, fps gate (newest wins), bounded try_send hand-off, drop/lag counters, closed method set, pipe limitation; `shrink_oversized_event` screencast exemption (HOOKS) + 2 MB test
- [x] 06-02: `source/watch.rs` no-frames watchdog + docs; `tests/record_live.rs` (#[ignore]) live driver; duration, odd/changing size and background-tab live checks on fixtures
- [x] 06-03: live CDP wire trace + slow-consumer run, audit/battery A/B with the recorder attached, qol_probe, full gates

### Phase 7: RecordHub and state Ops
**Goal**: An agent can start and stop a named recording through the existing `state` tool and get a complete, honest report, while Bladebro behaves exactly as today when recording is not used
**Depends on**: Phase 6
**Requirements**: CTRL-01, CTRL-04, CTRL-05, OUT-05
**Success Criteria** (what must be TRUE):
  1. Calling `state` with `record-start` and a name, browsing, then `record-stop` leaves a playable file, and the result gives absolute path, bytes, duration, frame count, dropped-frame count and segment boundaries
  2. A second `record-start` during an active recording returns an error naming the active file; the tool count stays at five and the schema delta matches the shape in the Phase 1 decision record
  3. With recording off, a CDP trace of a standard session matches today's (no screencast, no new domains) and no recording files are created
  4. Existing `state` ops and the rest of the five tools are unchanged: `qol_probe` 122/122 and `lane_matrix` pass; integration tests cover start/stop, double-start error, off-mode no-op and missing-ffmpeg error
**Plans**: 3 plans

Plans:
- [x] 07-01: `src/record/hub.rs` + `hub/` RecordHub state machine (process-global, injected env), bound by target id
- [x] 07-02: `state` op schema + dispatch hooks (`mcp/tools.rs`, `mcp/server/state.rs`, CLI parity), stop report
- [x] 07-03: Integration tests (start/stop, double-start, off-mode trace, missing ffmpeg) + qol_probe/lane_matrix run

### Phase 8: Tab Following
**Goal**: A recording keeps capturing what the agent is looking at when it switches tabs or a popup opens, and users who want separate files per tab can have them
**Depends on**: Phase 7
**Requirements**: CAPT-03, CAPT-04
**Success Criteria** (what must be TRUE):
  1. In default mode, `switch-tab` and `open-tab` rebind capture into the same file, and `record-stop` lists each switch as a segment boundary with its time offset. A click-opened popup (`target=_blank` / `window.open`) leaves the opener as the agent's page, so it becomes a segment after `switch-tab` to it; a window-feature popup (`window.open(..., 'width=500,height=400')`) is followed without `switch-tab`, per live measurement (evidence/08-02-popup-measure.txt)
  2. Closing the recorded tab or a cross-origin process swap does not silently end the recording; the outcome is reported
  3. With per-tab-files mode enabled, each tab produces its own file and `record-stop` lists all of them
  4. Integration tests on popup and tab-switch fixtures cover both modes
**Plans**: 2 plans

Plans:
- [x] 08-01: Rebind on switch/open/popup in default mode + segment markers + fixtures/tests
- [x] 08-02: Per-tab-files mode + tests

### Phase 8.1: Follow Every Popup (INSERTED)
**Goal**: A recording follows every popup or new window the recorded tab opens, however it was opened, and what the video shows, what `state tabs` marks as current and what the stop report lists as segments all agree
**Depends on**: Phase 9 verified (it touches the same hub and follower code)
**Requirements**: CAPT-06
**Success Criteria** (what must be TRUE):
  1. On the local popup fixtures, a click-opened popup (`target=_blank` link and `window.open` without features) and a window-feature popup each become a segment within a bounded delay (measured and documented) without `switch-tab`, and `record-stop` lists the boundary with its offset and tab id
  2. When the popup closes the video returns to the opener (segment boundary), and closing the recorded popup never silently ends the recording
  3. `state tabs` marks the same tab as current that the video is following while recording (the Phase 8 mismatch is gone or, if Bladebro's own opener-stays-current behavior makes that impossible without upstream edits, the difference is documented and the stop report says which tab the AGENT is on versus which the VIDEO follows)
  4. Rapid popup bursts, popups opened by a popup, and popups blocked by the browser are covered by tests; per-tab mode gets one file per popup with the Phase 8 cap
  5. Zero or tiny hooks in upstream files (each listed in HOOKS.md), recording-off behavior unchanged (CDP method comparison), `docs/recording-and-launch-config.md` updated and the drift test green, audit and qol_probe not worse than baseline
**Plans**: 2 plans

Plans:
- [ ] 08.1-01: Popup detection (opener relationship) and rebind/return in default mode + fixtures/tests
- [ ] 08.1-02: Agent-vs-video tab agreement in `state tabs`/stop report, per-tab mode, docs, live checks

### Phase 9: Finalization on Every Exit Path
**Goal**: However a session ends, the recording is finalized or honestly reported as interrupted, and no ffmpeg process is left behind
**Depends on**: Phase 8
**Requirements**: LIFE-01, LIFE-02, LIFE-03, LIFE-04
**Success Criteria** (what must be TRUE):
  1. Recordings are finalized (stdin EOF + bounded wait) on `record-stop`, tab close, daemon shutdown or idle timeout, `bladebro stop`, one-shot CLI exit and SIGTERM/Ctrl-C, verified per path
  2. A browser crash or relaunch mid-recording yields an "interrupted/unfinalized" result and the partial file is kept
  3. After normal exits and simulated crashes no ffmpeg process remains; the reaper in `src/session_profile/reap.rs` cleans up orphaned encoders, covered by an orphan-process integration test
  4. When free space drops below the threshold, recording stops with a "disk low" result; size-per-minute numbers for the default and max settings are documented
**Plans**: 3 plans

Plans:
- [x] 09-01: `finish()` hooks in MCP teardown, daemon shutdown/idle, `bladebro stop`, one-shot CLI, signal handling
- [x] 09-02: Crash/relaunch handling + reaper extension for ffmpeg children + orphan test
- [x] 09-03: Free-space check ("disk low" stop) + size-per-minute measurements in docs

### Phase 10: Evidence Extras
**Goal**: A reviewer can see at a glance what the recording shows and jump to the moment that matters
**Depends on**: Phase 9
**Requirements**: OUT-06, OUT-07
**Success Criteria** (what must be TRUE):
  1. `record-stop` returns a poster frame PNG path that opens and shows page content
  2. An agent can pass an optional marker (e.g. a failure note) and its time offset appears in the result and the timeline
  3. A JSON timeline sidecar next to the video lists action verdicts with time offsets aligned to the video, built from existing verdicts with no extra CDP calls
  4. Tests cover marker offsets and timeline generation; recording-off output is unchanged
**Plans**: 2 plans

Plans:
- [ ] 10-01: Poster frame + marker parameter and offsets
- [ ] 10-02: Action timeline sidecar from existing verdicts + tests

### Phase 11: Integration Docs
**Goal**: The QA flows that consume Bladebro can turn recording on per test case by following the docs, with no extra model effort
**Depends on**: Phase 10
**Requirements**: INT-01, INT-02, DOC-01
**Success Criteria** (what must be TRUE):
  1. Docs show the Claude Code GSD flow (`gsd-charlotte-qa`, `gsd-test-writer`) enabling recording per test case with `BLADE_RECORD` and `state` `record-start` / `record-stop`, including where the path appears in the result
  2. Docs show the DSH plugin flow (`mcp__bladebro__*`) doing the same
  3. README documents the recording feature as actually built (ops, env/config keys, formats, result fields, ffmpeg requirement, size-per-minute, sensitivity warning), replacing "Session video recording: Not supported"
  4. Following each snippet against a fixture page produces a recording at the documented path
  5. `docs/recording-and-launch-config.md` is complete for the whole milestone (DOC-01): every option, config key, per-call parameter, result field and example is present, each copy-paste example has been run against a fixture, and the drift test passes
**Plans**: 2 plans

Plans:
- [ ] 11-01: README recording section + help text
- [ ] 11-02: GSD and DSH integration snippets, verified against a fixture

### Phase 12: Full Instrument Gate
**Goal**: The milestone is proven ready to merge into the fork's main: complete tests, green on all platforms, stealth unchanged with recording on, and upstream updates still merge cleanly
**Depends on**: Phase 11
**Requirements**: VER-03, VER-04, SYNC-01
**Success Criteria** (what must be TRUE):
  1. Tests exist and pass for config precedence, name sanitization, ffmpeg arguments, verdict strings, fake-encoder lifecycle, and the 2 MB `Page.screencastFrame` through `shrink_oversized_event`
  2. `cargo clippy --release -- -D warnings` is clean and `cargo test --release` passes on ubuntu, macos and windows CI
  3. With recording on: `bladebro audit` 111/111, `diff_oracle` divergent 0, `qol_probe` 122/122, `tools/lane_matrix.py all 5` green, and the live sites pass (sannysoft all pass, incolumitas no bot detection, creepjs headless 0% / stealth 0%) with before/after numbers recorded
  4. The diff against `upstream/main` is additive (new `src/record/` module plus a listed set of hook lines); a scripted trial merge of the current `upstream/main` into the branch is conflict-free (or conflicts only in the listed hook lines) and the instrument gate passes on the merged result; a merge procedure is documented (SYNC-01)
**Plans**: 2 plans

Plans:
- [ ] 12-01: Test-suite completeness audit and gap fill (VER-03)
- [ ] 12-02: Full gate run on all instruments and live sites, evidence write-up, additive-diff check

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 4.1 → 5 → 6 → 7 → 8 → 9 (verify) → 8.1 →  10 → 11 → 12

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Decisions | 1/1 | Complete (upstream issue dropped: fork is main) | 2026-10-21 |
| 2. Sidecar Prototype Gate | 3/3 | Complete (PASS-WITH-CAVEATS) | 2026-10-21 |
| 3. Recording Config and Safe Output Paths | 3/3 (+03-04 gap closure) | Complete (macOS only; CI not run) | 2026-10-21 |
| 4. ffmpeg Discovery and Doctor | 2/2 | Complete (macOS only; CI not run; 442 tests) | 2026-10-21 |
| 4.1 Launch Mode and Layered Config (INSERTED) | 6/6 | Complete (macOS only; CI not run) | - |
| 5. Encoder Process Manager | 3/3 | 3/3 plans executed; SC4 (windows-latest CI) pending: written, not executed (macOS only) | - |
| 6. Frame Source | 3/3 | Complete on macOS (headless; Linux/Windows not executed; diff_oracle not run) | 2026-10-22 |
| 7. RecordHub and state Ops | 3/3 | Complete (macOS headless; opus verified, +07-04 gap fix) | 2026-10-22 |
| 8. Tab Following | 2/2 | Executed (macOS headless; opus verifier: human_needed on popup wording; 720 tests; 0 upstream hooks) | 2026-10-23 |
| 8.1 Follow Every Popup (INSERTED) | 0/2 | Not started (runs after Phase 9 verification) | - |
| 9. Finalization on Every Exit Path | 3/3 | EXECUTED, NOT VERIFIED (verification resumes 2026-10-26) | - |
| 10. Evidence Extras | 0/2 | Not started | - |
| 11. Integration Docs | 0/2 | Not started | - |
| 12. Full Instrument Gate | 0/2 | Not started | - |
