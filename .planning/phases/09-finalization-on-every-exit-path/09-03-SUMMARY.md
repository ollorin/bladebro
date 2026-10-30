---
phase: 09-finalization-on-every-exit-path
plan: 03
requirements: [LIFE-04, LIFE-01, LIFE-02, LIFE-03]
status: complete
---

# 09-03 Summary: disk low and size limits, live exit proof, final instruments

## What changed
- **`src/record/disk.rs` (new, fork-owned)**: `free_bytes(dir)` (copy of the updater's `df -k` approach, not imported; `parse_df` pure; Windows returns `None`), consts `DISK_LOW_MIB = 500`, `MAX_DURATION = 2 h`, `MAX_FILE_BYTES = 1 GiB`, `FreeFn` type. `disk_tests.rs`: df table (macOS, Linux, wrapped name, spaces, garbage), real probe, strict threshold, no-updater pin.
- **Decision D9-13 (consts, no option)**: confirmed by measurement. Largest setting is 2.67 MB/min, so 2 min is about 5 MB and 500 MiB stays; 2 h is 80 MB (default) to 320 MB (largest), so the 2 h limit bites before 1 GiB. No BLADE_* name, no config key.
- **Injection**: `HubEnv::free_fn()` (default = `disk::free_bytes`) instead of the planned `free_bytes(&self, dir)`: the guard has no env, so the hub captures the probe in `Active` at start. Tests override it with an `Arc<AtomicU64>` (`u64::MAX` = unknown = `None`).
- **Start refusal** (`machine.rs`): after the config/enabled checks and before ffmpeg is resolved or any file/dir is created: `recording not started: disk low (<N> MiB free in <dir>, need 500 MiB)`. Strictly below refuses; unknown never refuses.
- **Guard** (`guard.rs`): after the health check, the tick probes free space in `spawn_blocking` OUTSIDE the hub lock, then `Hub::limit_hit` (disk low, `started.elapsed()` vs max duration, per-file size via `metadata`; per-tab uses `Tracker::files()`), then `Hub::stop_on_limit(gen, why)`: finalizes, parks `(finish ok, text)`. Stale generation does nothing. Texts (`msg.rs`): `previous recording stopped: disk low (<N> MiB free); saved: <path> (...)`, `... maximum duration (2 h) reached; saved: ...`, `... size limit (1 GiB) reached; saved: ...` (per-tab: `reached by <file>; saved (per-tab, N files): ...`). These are Ok results (finalized), not errors.
- **MATRIX final**: no `planned:` rows; disk/duration/size are 4 rows pointing at guard tests; `FINALIZED_GONE` outcome for stdin EOF / SIGTERM / SIGINT / `bladebro stop` (the process is gone, so nothing can report; the old wording "reported at next record op" was wrong for those and is corrected in code and docs). `matrix_has_no_planned_rows` + stricter `matrix_complete`.
- **Live**: `tools/record_sidecar/scenarios/exit_live.sh` + `lib/exit_live.py` (15 scenarios, both lanes), `lib/sizes_live.py`, README rows.
- **Docs** (`docs/recording-and-launch-config.md`): matrix cells with live numbers, limit-stop texts, new `### Disk space and limits`, `### File size per minute`, live-proof bullet and the MCP kill -9 Chrome exception.

## Commits
5cf1495 (task 1: disk guard + MATRIX), d84f63f (task 2: live scenarios + sizes), 101f9ea (task 3: docs, evidence re-run after a label fix), instruments commit and this summary follow.

## Live results (evidence/09-03-live.txt: 91 PASS, 0 FAIL; headless, scratch homes, 127.0.0.1 fixtures)
| id | exit action | result |
|---|---|---|
| mcp_eof | stdin EOF | MCP exited in ~0.8 s; file 4.1 s decodes in full; leftovers 0 (29 ms) |
| mcp_sigterm / mcp_sigint | SIGTERM / SIGINT to MCP | exited ~0.8 s; ffmpeg gone with it; decodes; leftovers 0 |
| daemon_stop | `bladebro stop` | returned in ~0.84 s (`daemon stopped`), no "cleanup is still running"; decodes; leftovers 0 |
| daemon_sigterm / daemon_sigint | signal to daemon pid (cli.pid of the scratch home) | exited ~0.8 s; decodes; leftovers 0 |
| mcp_idle / daemon_idle | BLADE_IDLE_TIMEOUT=20 | Chrome+ffmpeg gone after 26.8 s / 26.3 s (15 s check); next record-stop text `previous recording ended with the browser: recording saved: ... (bytes ..., duration 30.0 s ...)` (MCP: after the one-line `note: Chrome was restarted after idle shutdown` line); file decodes; next start+stop saves |
| mcp_kill9 / daemon_kill9 | kill -9 bladebro | ffmpeg gone 62 ms / 58 ms later (stdin EOF), file 4.0 s decodes; 0 ffmpeg after the next launch (daemon lane: also 0 Chrome; MCP lane: see not-verified) |
| mcp_kill_chrome / daemon_kill_chrome | kill -9 scratch Chrome main | ffmpeg gone ~2.1 s later; record-stop `previous recording interrupted: browser connection lost at 6.0 s; partial file kept: <path>. recording not clean: browser connection lost at 4.5 s; capture interrupted: ...`; file decodes; nav works again; next start+stop saves |
| mcp_kill_ffmpeg / daemon_kill_ffmpeg | kill -9 scratch ffmpeg | record-stop `previous recording interrupted: encoder exited at 6.0 s; file is empty, nothing playable: <path>. recording not clean: recording failed: ffmpeg exited with signal: 9 (SIGKILL) after 39 frames; ...`; never `recording saved`; next start+stop saves |
| oneshot | `--no-daemon record-start` | refused (long-lived session text), no file, no process |

Disk low, 2 h and 1 GiB are NOT driven live (never fill a disk; 2 h too long): injected-value unit tests only.

## Sizes (evidence/09-03-sizes.txt, 60 s each, anim.html fixture)
default webm 0.67 MB/min (Phase 2: 0.87); mp4 0.42 (0.54); 15 fps 1.05 (1.15); 1080p 0.84 (1.12); max webm 3840x2160 15 fps 2.67; max mp4 1.78.

## Instruments (evidence/09-03-instruments.txt)
| instrument | result | baseline |
|---|---|---|
| `cargo fmt --check`, `clippy --release --all-targets -D warnings` | clean | clean |
| `cargo test --release --no-fail-fast` | 772 passed / 0 failed / 12 ignored (727 lib; 09-02: 758, +14) | 720 pass / 0 / 12 |
| flake `concurrent_cold_starts_have_one_owner` | did not fail; no rerun; no leaked scratch daemon (cli.pid check) | may fail |
| `BLADE_HEADLESS=1 bladebro audit` | 100/100 PASS | 100/100 |
| `BLADE_HEADLESS=1 qol_probe/run.py` | 120 passed, 2 failed: only 'chord cleared the live editor' and 'nested scroller movement is reported as such' | 120/122 same two |
| lane_matrix_local x5, recording off: daemon / oneshot / mcp / mcp-pipe | 1.0 1.1 2.1 1.3 1.3 s / 0.6 x5 / 0.7 0.7 0.8 0.8 0.7 / 0.7 0.7 0.8 0.7 0.7; all exit 0 | 09-02 daemon: 0.8-1.3 s |
| same, BLADE_RECORD=1 | daemon 0.9 1.1 1.4 1.5 1.3 / oneshot 0.6-0.7 / mcp 0.7 1.0 0.9 1.0 0.8 / mcp-pipe 0.7-0.8; all exit 0 | |
| upstream audit vs b157f15 | only `src/cli/daemon.rs` (+6/-3, 09-01 rows) and `src/session_profile/reap.rs` (+1): exactly the HOOKS.md rows; Cargo*, npm, CHANGELOG, .github, src/updater, src/mcp/tools.rs: 0 diff lines | |
| diff_oracle | NOT RUN (cannot run here) | |

## Not verified / open
- Disk low, max duration and size limit never happened for real (injected values and 300 ms / 4 KiB limits only); `df` parsing proven on text tables plus one real call on macOS. Linux and Windows not run (Windows: disk guard off by design, reasoned).
- MCP lane, `kill -9` of the MCP server: its scratch Chrome is left alive (8 processes) after the next MCP launch and shutdown, with recording off too; the daemon lane reaps it. Pre-existing upstream behavior, out of scope (deferred-items.md); exit_live.sh reports it as INFO and kills those scratch pids itself, so mcp_kill9's "leftovers 0" excludes Chrome. ffmpeg and bladebro leftovers are 0 there.
- A real orphaned ffmpeg was never produced (ffmpeg always exits on EOF within ~60 ms); the sweeper is proven on fake processes only. SIGSTOPped-parent case not run.
- Per-tab live runs were not part of exit_live.sh (follow mode only); per-tab limit and Chrome-loss behavior is unit-level.
- Headed mode never run. The `real` lane was skipped. Fixture is one synthetic moving-box page; sizes are one run per setting.
- The kill-Chrome result text varies by race (`interrupted` when the guard wins, `ended with the browser` when the relaunch finds Chrome first); the script accepts both and the evidence shows `interrupted` in all runs.
- The 772 total sums every test binary; baseline 720 was taken from CONTEXT, not re-measured.
- STATE.md / ROADMAP.md were not updated by this run.

## Deviations
- [Rule 3] `HubEnv::free_fn` instead of `free_bytes(&self, dir)` (guard has no env). Test-only struct fields `free` added to the two test envs; `Knobs` literals got `..Default::default()`.
- [Rule 1] Matrix outcome wording for stdin EOF / signals / stop corrected (nothing can report after the process is gone).
- A first live run failed one check (mcp_kill9 Chrome leftover); investigated, shown to be unrelated to recording, logged as deferred, harness now reports it explicitly. No product bug was found by the live runs, so no new regression fix this plan.

## Self-Check: PASSED
Files present: src/record/disk.rs, disk_tests.rs, tools/record_sidecar/scenarios/exit_live.sh, lib/exit_live.py, lib/sizes_live.py, evidence/09-03-{live,sizes,instruments}.txt. Commits 5cf1495, d84f63f, 101f9ea exist.
