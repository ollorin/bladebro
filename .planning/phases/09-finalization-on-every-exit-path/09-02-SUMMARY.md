---
phase: 09-finalization-on-every-exit-path
plan: 02
requirements: [LIFE-02, LIFE-03]
status: complete
---

# 09-02 Summary: interrupted detection, orphan sweeper, kill -9 measured

## What changed
- **Guard** (`src/record/hub/guard.rs`, new): one task per recording, tick 2 s (`GUARD_TICK` const, tests pass their own tick). Triggers: follower lost Chrome (3 failed tab-list reads, ~0.6 s; new `Tracker::browser_lost()` from a `lost` flag the follower already tracked and now publishes in its snapshot) or `Encoder::is_dead()` (writer error or ffmpeg exited, non-reaping `waitid(WNOWAIT)` on unix). It ends the recording early (stdin closed, partial file playable) and parks the result.
- **Generation id** (`machine.rs`): `Active.gen`, `Hub::active_gen/verdict/interrupt_if(gen, why)/dir_of`. A stale guard (gen mismatch) exits silently and never stops a newer recording.
- **Texts** (`msg.rs`): `previous recording interrupted: <reason> at <T> s; partial file kept: <path>. <finalize report>`; reasons `browser connection lost` / `encoder exited`; `file is empty, nothing playable: <path>` when the file has 0 bytes (found live, see below). A parked interrupted text is not prefixed with "ended with the browser" (`msg::orphan_text`).
- **Sweeper** (`src/record/orphans.rs`, new): see rule below. One-line upstream hook at the first line of `reap_orphans()`; the guard also sweeps the recording's own dir at record-start.
- MATRIX rows (exit.rs): Chrome killed, ffmpeg killed (guard), recorder connection lost, per-tab Chrome killed, orphan sweep; kill -9 row now points at the live evidence.
- Docs: `docs/recording-and-launch-config.md` (interrupted texts, guard, `### Leftover encoders`, matrix cells filled with measured numbers).
- No new option/env var/config key; no Cargo/npm/updater/tools.rs edits.

## Commits
43ef5dd guard + generation + interrupted texts (task 1), b95e023 orphan sweeper + reaper hook (task 2), a9a07c0 fix: empty file wording (found by the live run), 518a90b real ffmpeg parent-death test, kill -9 evidence, docs, HOOKS row. The SUMMARY commit follows.

## Upstream edits (HOOKS.md)
Only `src/session_profile/reap.rs`: +1 line (first line of `reap_orphans()`), row with b95e023. `git diff -U0 43ef5dd^..HEAD -- src/session_profile/reap.rs` = 1 added line.

## Race review
- guard vs record-stop: both take the hub lock; whoever is second sees gen mismatch / not active (guard -> Gone; stop -> reads the parked result).
- guard vs `finish_best_effort` (relaunch/idle/shutdown hooks): same lock; the hook finalizes and parks "ended with the browser"; the guard then sees Gone. Live: a stop issued ~1.2-1.5 s after a Chrome kill gets the "ended with the browser" text (relaunch hook won), after ~2.5 s the "interrupted" text (guard won). Both are honest and the file decodes.
- Lock hold: the guard holds the lock for the check (no I/O) and, on a trigger, one finish: same bound as `finish_orphan` (worst case ~16 s).
- Guard never imports page/CDP (test `browsing_unaffected` pins it).

## Orphan rule (orphans.rs)
Kill only if ALL: program basename `ffmpeg`/`ffmpeg.exe` (program = text before the first ` -`); command line contains ` -f image2pipe ` then ` -i pipe:0 ` then ` -f webm|mp4 `; the tail after that muxer is an absolute path whose parent == the recordings dir and whose name matches `<stem>-<YYYYMMDDTHHMMSSZ>-<6 hex>.webm|mp4`; ppid is 1 or not alive. SIGTERM, wait <= 2 s, SIGKILL only if the same pid still has the same command line. Never pid <= 1 or self. Deviation from the plan: one `ps -axww -o pid=,ppid=,command=` lister for macOS and Linux (no separate /proc reader); the command line is one string and the output path is matched as the tail, so spaces in the dir work. Windows: no-op (job object).
Fake-process technique that worked: a symlink `<scratch>/bin/ffmpeg -> /bin/sh` run as `ffmpeg -c 'trap "exit 0" TERM; while :; do sleep 0.2; done' ffmpeg <real build_args argv>` (ps shows a program named ffmpeg and the real argv; `arg0` override was not needed). Orphans via `sh -c '"$@" & echo $!'` (parent exits, ppid becomes 1). Test spawns 2 matching orphans, 1 live-parent match, 1 other-dir orphan, 1 user-argv orphan: exactly the two matching orphans die, the others survive, every pid is gone at the end.

## Measured (macOS arm64, Chrome 154 headless, real ffmpeg, scratch homes; evidence/09-02-kill9.txt)
| experiment | result |
|---|---|
| kill -9 daemon mid-recording | ffmpeg gone 94-157 ms after the kill (exits 0 on stdin EOF), ffprobe 3.0 s, full decode exit 0; orphaned scratch Chrome removed by the next launch's reaper; ffmpeg leftovers 0; sweeper not needed |
| unit: real ffmpeg, stdin dropped | exit 0, 61 ms (`parent_death_eof_finalizes`) |
| kill -9 Chrome (no agent call) | recording ended ~1.0 s later, decodes (3.9 s); record-stop: `previous recording interrupted: browser connection lost at 4.0 s; partial file kept: <path>. recording not clean: ...` rc 1; browsing and a new record-start work afterwards |
| kill -9 ffmpeg | noticed within the 2 s tick; `previous recording interrupted: encoder exited at 4.0 s; file is empty, nothing playable: <path>. ...ffmpeg exited with signal: 9`; the file is 0 bytes (VP9 output is written at finalize), decode exit 183; next record-start works |
| lane_matrix_local.py daemon 5 rounds | before (43ef5dd): 1.5, 1.1, 1.3, 1.3, 1.3 s; after: 0.8, 1.1, 1.3, 1.3, 1.3 s (sweep is on a detached thread; unchanged within noise) |

Live finding fixed in a9a07c0: the first version said "partial file kept" for the 0-byte file left by a SIGKILLed ffmpeg and judged emptiness before the finalize; now judged after, with a regression test.

## Tests and gates
- New: `guard_tests` 11 (chrome_killed_no_calls, relaunch_mid_recording, lost_cdp_only, kill_ffmpeg, stale_guard, normal_stop_guard_exits, per_tab_interrupted, browsing_unaffected, texts_scrubbed, empty_file wording x2), `orphans_tests` 7 (classify table, args/classifier drift pin, ps parse, sweep_in rule, fake-process integration, reaper hook pin, missing dir), `parent_death_eof_finalizes`. 3 stable runs of guard tests.
- `cargo fmt --check` clean; `cargo clippy --release --all-targets -- -D warnings` clean; `cargo test --release --no-fail-fast`: 758 passed / 0 failed / 12 ignored (09-01: 739). Two full runs, the flake `concurrent_cold_starts_have_one_owner` did not fail; no leaked scratch daemon, no ffmpeg or bladebro process left (`pgrep`).

## Not verified / open
- Windows and Linux not run. Windows: `process_exited` returns false (the write error is the only encoder-dead signal), sweeper is a no-op; reasoned only. Linux `ps -axww` form assumed, not run.
- Per-tab mode: only the browser trigger applies; a killed ffmpeg of one per-tab file is not noticed early (shows in that file's line at stop). The guard test for per-tab covers Chrome loss only.
- Not run in this plan: SIGTERM/SIGINT/stdin EOF/`bladebro stop`/idle-timeout live runs (09-03 exit_live.sh); audit, qol_probe, full lane_matrix (mcp, oneshot, mcp-pipe lanes) were not re-run here, only `daemon` x5 for cold start.
- kill -9 live run done on the CLI daemon lane only, not MCP stdio.
- The sweeper's `ps` string form cannot recognize an ffmpeg install path that contains ` -` (kept, never killed: safe direction). The reaper hook only runs when a Chrome launches; a machine that never launches bladebro again keeps a (rare) orphan until then. A real orphaned ffmpeg was never produced live (ffmpeg always exited on EOF), so the sweeper is proven on fakes only.
- The interrupted text repeats the file path up to three times (header + finalize report); kept for honesty over leanness, since it is a rare error path.
- The finalized video's duration after Chrome kill includes the held last frame up to the finalize (4.0 s reported vs ~3.7 s of captured content).
- `Encoder::is_dead` is exercised on unix only via real kills of the fake encoder; the idle-encoder (no frames flowing) waitid path is not separately tested.
- Baseline 739 test count taken from 09-01 summary, not re-measured.

## Self-Check: PASSED
Files present: src/record/hub/guard.rs, guard_tests.rs, src/record/orphans.rs, orphans_tests.rs, evidence/09-02-kill9.txt. Commits 43ef5dd, b95e023, a9a07c0, 518a90b exist.
