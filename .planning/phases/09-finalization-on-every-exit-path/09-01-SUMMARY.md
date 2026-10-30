---
phase: 09-finalization-on-every-exit-path
plan: 01
requirements: [LIFE-01, LIFE-03]
status: complete
---

# 09-01 Summary: finish() on every exit path

## What changed
- Daemon holes closed with one-line hooks in `src/cli/daemon.rs` (upstream file): `finish_best_effort().await` before `b.shutdown()` at the drift/lost relaunch, the `Closed` relaunch and the idle timeout; the Phase 7 post-loop hook moved above the login snapshot (net 0). Net change vs this plan: +3 lines, 1 moved. Commit 488c0e0; HOOKS.md rows (4) with that hash in f8ff8f9.
- ffmpeg is spawned with `process_group(0)` on unix (`src/record/encoder/process.rs`, fork-owned): a terminal Ctrl-C reaches bladebro only, which finalizes by closing stdin.
- New `src/record/exit.rs`: `MATRIX` (19 rows) plus file:line reasoning for signals and Windows. New `src/record/exit_tests.rs` (declared from `hub.rs` with `#[path]` so it reuses the hub test rigs) and `encoder/process_group_tests.rs`. `machine_tests.rs` helpers made `pub(super)` (fork-owned, visibility only).
- Docs: the Phase 7 "Not covered yet (owned by Phase 9)" text replaced by `### When a session ends` (matrix + details) in `docs/recording-and-launch-config.md` (fda4abb, plus a final wording fix in the summary commit).
- No new signal handler, no new option/env var, no other upstream file edited. No Cargo/npm/updater/tools.rs changes.

## Commits
488c0e0 fix (daemon hooks, process group, matrix + tests; tasks 1 and 2 landed together because the matrix tests pin the hooks), f8ff8f9 HOOKS hashes, fda4abb docs.

## MATRIX (from exit.rs)
| path | lane | outcome | test |
|---|---|---|---|
| record-stop | mcp / daemon | finalized | row_record_stop_mcp / _daemon |
| recorded tab closed (follow) | mcp+daemon | continues on next tab, finalized at stop | row_tab_close_follow |
| recorded tab closed (per-tab) | mcp+daemon | that file finalized, others keep recording | row_per_tab_close |
| stdin EOF | mcp | finalized, reported at next record op | row_stdin_eof_mcp |
| SIGTERM/SIGINT | mcp / daemon | same | row_signal_mcp / _daemon |
| bladebro stop | daemon | same (bound 16 s < client wait 30 s) | row_bladebro_stop_daemon |
| idle timeout | mcp / daemon | same | row_idle_mcp / _daemon |
| Chrome relaunch | mcp / daemon | same | row_relaunch_mcp / _daemon |
| relaunch, recorder connection lost | daemon | interrupted, partial kept | row_relaunch_daemon_dead_source |
| one-shot exit | oneshot | refused (no recording possible) | row_oneshot_refused |
| ffmpeg killed | mcp+daemon | interrupted, partial kept | row_kill_ffmpeg |
| kill -9 of bladebro | mcp+daemon | self-finalized by ffmpeg on stdin EOF | live: exit_live.sh kill9-parent (09-03) |
| Chrome killed | mcp+daemon | interrupted | planned: 09-02 |
| disk low / max duration / size | mcp+daemon | stopped, finalized | planned: 09-03 |
| Windows console close / logoff | mcp+daemon | not handled | n/a (reasoned) |

## Signal / Windows reasoning
MCP `wait_for_shutdown_signal` (boot.rs) and daemon SIGTERM/SIGINT (daemon.rs top) already break into the post-loop finish, so none was added. Windows: only ctrl_c handled; console close/logoff/shutdown not handled (needs SetConsoleCtrlHandler, upstream architecture change); job object and stdin EOF reasoned; NOT executed.

## Tests
- New: 18 (exit_tests: 16 incl. hooks_present_in_daemon, matrix_complete, matrix_texts_scrubbed; process_group: 1; plus matrix rows), full suite `cargo test --release --no-fail-fast`: 739 passed / 0 failed / 12 ignored (baseline 720). `cargo fmt --check` and `cargo clippy --release --all-targets -- -D warnings` clean. `launch_mode` tests (drift test) green.
- Flake: `concurrent_cold_starts_have_one_owner` failed in 3 of 5 full runs (load) and leaked scratch daemons (BLADE_HOME bladebro-daemon-race-<pid>); each was stopped with `BLADE_HOME=<dir> bladebro stop`; the test passed alone 4/4 and the last full run was green. No ffmpeg or fake-enc process left.

## Open items / not verified
- No live run: kill -9 timing, SIGTERM/SIGINT on a real MCP/daemon, `bladebro stop` with a real recording, idle timeout with real Chrome are NOT executed here (exit_live.sh is 09-03). The process-group effect on a real terminal Ctrl-C is proven only as pgid != parent pgid.
- Daemon/MCP hooks are tested at the hub level plus source-text pins, not by running the daemon.
- The `tests/record_hub.rs` daemon-stop row was not added: the plan allowed skipping if the harness cannot start a daemon without Chrome; the existing daemon_lifecycle test covers stop with no recording. The reordered post-loop is checked by `hooks_present_in_daemon` text pins only.
- Matrix test-name check in `matrix_complete` is a string search of exit_tests.rs.
- Rows for Chrome kill, disk low/size limits remain `planned: 09-02/09-03`; docs cells say so.
- Baseline test count (720) taken from CONTEXT, not re-measured on a pre-change tree.
- Linux and Windows not run.
