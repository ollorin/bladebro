---
phase: 08-tab-following
plan: 02
subsystem: recording
tags: [per-tab-files, record.tabs, BLADE_RECORD_TABS, encoder-lifecycle, headless-live]
requires: [08-01 follower, SegmentSink, merge, follow_live.sh, record_support]
provides: [record.tabs option (follow | per-tab), per-tab files with a cap of 8, compose_per_tab]
requirements-completed: [CAPT-04, CAPT-05, OUT-05]
key-files:
  created:
    - src/record/hub/follow/files.rs
    - src/record/hub/follow_report.rs, follow_report_tests.rs, per_tab_tests.rs
    - .planning/phases/08-tab-following/evidence/08-02-smoke.txt
  modified:
    - src/record/{values,config,layered,names}.rs, src/record.rs, src/launch_mode/registry.rs
    - src/record/hub.rs, hub/{machine,msg,live_env,follow}.rs, hub/follow/run.rs, hub/follow_tests.rs (helpers made pub(super))
    - RecordConfig literals: encoder/{process,args}.rs, hub/machine_tests.rs, tests/record_live.rs
    - tests/record_follow.rs, tools/record_sidecar/{scenarios/follow_live.sh,README.md}, docs/recording-and-launch-config.md
completed: 2026-10-22
---

# Phase 8 Plan 02: Per-tab files Summary

`record.tabs=per-tab` (or `BLADE_RECORD_TABS=per-tab`) makes the 08-01 follower write one file and one ffmpeg per tab (at most 8 open), finalize a file when its tab closes, and list every file in one compact record-stop line. Zero edits to any file that exists in upstream/main; src/mcp, src/cli, Cargo.toml/lock, npm, CHANGELOG.md, .github and `.planning/HOOKS.md` have no diff since bb9e7dd (checked, empty output).

## Commits
- `08aeedc` feat: record.tabs option through the layered resolver (Tabs enum, registry, docs rows, drift tests, live_env helper)
- `a5ca6de` feat: per-tab recording files with a cap of 8 open files
- `ceefe33` test: live per-tab scenarios (+ smoke evidence)
- `dedd83f` docs: per-tab recording files
- this SUMMARY commit (also ticks 08-01/08-02 in ROADMAP)

## Option resolution (record.tabs)
| layer | source | error prefix |
|---|---|---|
| per-call | `RecordOverrides.tabs` (library only; the `state` tool passes the name only, asserted by `live_env::tests`) | `per-call tabs:` |
| env | `BLADE_RECORD_TABS` (blank = unset) | `BLADE_RECORD_TABS:` |
| project | `.bladebro.json` `record.tabs` | `<path> record.tabs:` |
| user | `record.json` `tabs` | `record.json tabs:` |
| default | `follow` | |
All layers are parsed before precedence. Registry (`RECORD_FILE_KEYS`, `ENV_VARS`) and the docs (env table, both Config keys rows, precedence row, threat-model bullet) updated; drift tests pass both ways.

## Concurrency review
- Ownership of an `Arc<Encoder>`: the `TabFile` (until a finalize task takes it with `Option::take`) plus one transient clone inside each live source's `SegmentSink`. In `Policy::One` the follower holds the `inner` clone and drops it with the policy before returning, so `take_encoder` works as in 08-01.
- Every path that releases an encoder before `take_encoder`: close (`mark_end`, take from the map, spawn `finalize` which waits for sinks up to `2*grace+4 s`), stop (`mark_end` all, retire current source, join pending sources, then finalize all concurrently), abort (Tracker dropped: task aborted, `kill_on_drop`).
- No follower state lock is held across an await: `Files` is owned by the follower task (no mutex); the snapshot mutex is cloned-from only. Finalize tasks own their encoder and touch nothing shared.
- Stop vs close race: a file already being finalized by a close holds its `JoinHandle` in `TabFile.fin`; `finish` awaits that handle instead of finishing it again (`f.enc` is `None`). A file is never finished twice.
- A tab is considered closed only after 2 consecutive lists without it (a one-off list glitch must not end a file); the tab the follower still has a source on is never finalized until it is retired.
- Bounded stop: each file's handle is awaited against one deadline `finish_timeout + 2*grace + 5 s`; on expiry the task is aborted (encoder dropped, `kill_on_drop`) and the file is listed `not finalized`. `Tracker::stop` cap grew by the same amount in per-tab mode. Test: a fake ffmpeg that ignores EOF is killed, stop returns in under 6.2 s, no pid survives.

## Cap policy (flag for the coordinator)
The 9th open tab is not recorded and the result says `tab <id> not recorded from <T> s: per-tab cap of 8 open files reached (close a tab to free one)`. As decided (coordinator amendment) this is a NOTE on a clean result, not a failure. Live proof: `per_tab_cap` saved 8 files with the note, `recording saved`. A tab that hit the cap is retried when the agent returns to it (silent the second time, recorded if a slot was freed).

## Test count
- Full `cargo test --release`: 720 passed, 0 failed (08-01: 701; +19: names, layered/tabs, live_env, 7 follow_report, 8 per-tab scenarios...). The known flake `concurrent_cold_starts_have_one_owner` failed once in the first full run (its scratch daemon from `bladebro-daemon-race-*` leaked; I killed it, pid verified by its BLADE_HOME) and passed on the rerun. fmt clean, `clippy --release --all-targets -D warnings` clean.
- Per-tab scenarios (fake ffmpeg + FakeCdp): A-B-C-A (3 files, 3 pids, return reuses file 1, bytes == frames * frame size), close finalizes before stop, cap of 8 then slot reuse with a 10th tab, failing encoder isolated to its tab, non-creatable file named (recordings dir chmod 555) with file 1 intact, bounded stop with a hung ffmpeg, `finish_orphan`, double start refused naming the first file and file count. Follow-mode tests unchanged and green.

## Live smoke (headless, scratch BLADE_HOME, 127.0.0.1 fixtures; evidence/08-02-smoke.txt)
74 PASS, 0 FAIL. Scenarios: `per-tab` (project-file layer enables recording and per-tab, 3 ffmpeg while 3 files open, 3 files tab1/2/3, each 1280x720 vp9, full clean decode, decoded frames == reported frames, red/blue/green, then `BLADE_RECORD_TABS=follow` beats the project file: one file), `per-tab-close` (closed tab's ffmpeg gone and its file ffprobe-readable and decodable while the daemon still records), `daemon`, `popup`, `close`, `xorigin` (08-01 paths unchanged), `mcp` (5 live tests incl. `per_tab_three_files`, `per_tab_cap`), `audit` (100/100), `perf` (informational; the bare-`wait` fix works, lane_matrix daemon exit 0, daemon CPU single-tab 0.9 %, following 0.7 %). After every scenario: stop and pgrep: no daemon/mcp, Chrome, ffmpeg or http.server of ours left. Not re-run: `off` (builds the pre-phase binary; the off path is not touched by this plan: nothing runs when `BLADE_RECORD` is unset).

Real per-tab stop line (daemon, project file; ids shortened):
`recording saved (per-tab, 3 files): .../pt-tab1-20261022T145420Z-dd5570.webm (bytes 18877, duration 10.7 s, frames 107, dropped 0, segments [0.0 s tab 182C..., 8.7 s tab 182C...], captured 42); .../pt-tab2-...webm (bytes 13282, duration 6.6 s, frames 66, dropped 0, segments [0.0 s tab B397...], captured 16); .../pt-tab3-...webm (bytes 11114, duration 3.4 s, frames 34, dropped 0, segments [0.0 s tab 583E...], captured 14)`
Cap line (MCP): `recording saved (per-tab, 8 files): ... Note: tab 774B... not recorded from 19.2 s: per-tab cap of 8 open files reached (close a tab to free one)`.

## Deviations
1. [Design] The "closed at T s" information is a stop-result note (`tab <id> closed at <T> s`) after the file lines, not inside a file's segments text (keeps `fields_line` unchanged and parseable). In per-tab mode the 08-01 note `tab <A> gone at <T> s; following tab <B>` appears too when the closed tab was the one the agent was on, so a closed current tab yields both notes (visible in the real `per-tab-close` result; accurate, slightly redundant).
2. [Design] `compose_per_tab` takes a `PerTabResult { files, failures, notes }` (FileOutcome has `tab`, `n`, `path`, `src`, `enc: Result<EncoderReport, String>`, `segments`, `failures`, `closed_at`); a file whose encoder could not be finished is `Err` and printed `<path> (not finalized)`. Only the first failing verdict goes in the header; later reasons are appended to the `. Note:` slot so none is lost.
3. [Design] Encoders are finished inside the follower in per-tab mode (it owns them), so `Tracker::stop` returns an `Outcome { One, PerTab }`; the hub no longer holds an encoder in per-tab mode (`Active.enc: Option`).
4. [Design] A tab whose file cannot be created or whose cap is hit is "parked": its predecessor's source retires, nothing is retried each tick, and the next attempt is on re-entry. A tab that comes back after its file was finished is `not recorded ... its file was already finished`.
5. A zero-argument `live_env` helper (`overrides_for`) was added with a unit test, per the coordinator amendment; the plan listed live_env.rs only through that amendment.
6. `follow_tests.rs` helpers (`Rig`, `Env`, `view`, `fake`, ...) were made `pub(super)` and `Rig::with(...)` added so per-tab tests reuse the rig; `Rig::start` now accepts both start texts. Follow scenarios unchanged.
7. `run.rs` is 506 lines (target ~500): all per-file lifecycle is in `files.rs`; run.rs only gained the `Policy` indirection, `parked` handling and the per-tab finish branch. It still has one responsibility (the follower task).
8. Tests were written together with the code rather than strictly first (the Policy refactor had to compile before any scenario could run); every scenario in the plan's table exists and passes.
9. The first run of `per_tab_cap` in unit tests exposed a test-writing error (the first tab's id was wrong, so it was seen as closed); fixed in the test, not a product bug.

## Open issues
- Per-tab dormant files hold the last frame while the agent is elsewhere (documented, as decided); long multi-tab sessions cost one ffmpeg per open file (up to 8).
- The active-recording refusal text reports the file count from the follower's per-tick snapshot, so it can lag a new file by one poll (about 0.2 s).
- Per-tab mode was exercised headless on macOS only; headed Chrome tab-list semantics were not checked (as in 08-01).
- Phase 9 still owns exit-path finalization (daemon idle shutdown, relaunch paths); per-tab `finish_orphan` is covered for the MCP/daemon-exit path only.

## Self-Check: PASSED
- Created files exist: files.rs, follow_report.rs, follow_report_tests.rs, per_tab_tests.rs, 08-02-smoke.txt.
- Commits exist: 08aeedc, a5ca6de, ceefe33, dedd83f (all end with the Co-authored-by trailer).
- Gates: fmt, clippy --all-targets -D warnings, full tests 720/0.
