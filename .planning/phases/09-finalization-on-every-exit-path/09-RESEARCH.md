# Phase 9: Finalization on Every Exit Path - Research

**Researched:** 2026-10-23
**Domain:** process lifecycle, signals, orphan reaping, disk guards (Rust/tokio, macOS-executed)
**Confidence:** HIGH for code facts (read directly), MEDIUM for design, LOW where marked UNVERIFIED

<user_constraints>
## User Constraints (from 09-CONTEXT.md, binding)
- Fork is main repo; only upstream constraint: `upstream/main` merges cleanly (SYNC-01). All logic in NEW files under src/record/. Each edit to a pre-existing upstream file is a tiny hook (<= ~10 lines), listed in .planning/HOOKS.md.
- Reaper (src/session_profile/reap.rs): prefer a one-line hook calling a new record module.
- Five tools only; no new tool/state op. Do NOT edit src/updater/** (copy the `df` approach). No new crates; no edits to Cargo.toml/Cargo.lock, npm/, CHANGELOG.md, .github/.
- Docs: update docs/recording-and-launch-config.md for every option/env var/result field/behavior touched; document the exit-path matrix; replace the Phase 7 text saying some paths do not finalize. Any new BLADE_* name/config key must be registered (drift test).
- LIFE-01..04 as in CONTEXT.md (finalize on every exit path; interrupted outcome on crash/relaunch; no ffmpeg left behind; disk-low + size/duration guard). Tests with fake CDP/encoder, injectable free-space fn, fake-process orphan test; live checks local fixtures only, BLADE_HEADLESS=1, scratch BLADE_HOME.
- Phase 8 popup decision pending: assume current behavior stands.
</user_constraints>

<phase_requirements>
| ID | Research support |
|----|------------------|
| LIFE-01 | Section 1: signal handlers and finish already exist on MCP + daemon-normal paths; 3 daemon paths skip finish |
| LIFE-02 | Section 2: hub keeps `active` forever after browser death until record-stop; detection points |
| LIFE-03 | Sections 2-4: kill -9 experiment, reaper hook, Windows job |
| LIFE-04 | Section 4: df copy, defaults, size-per-minute |
</phase_requirements>

## Summary
Most of LIFE-01 already exists. Both long-lived surfaces install signal handlers (tokio `signal` feature is enabled, Cargo.toml:24) that break the main loop, and both post-loop paths call `hub::finish_best_effort`. The real holes are in the DAEMON: idle timeout, drift/connection-lost relaunch and `Closed` relaunch call `browser.shutdown()` directly and never finish the hub. MCP routes every Chrome teardown through `shutdown_browser`, which finishes the hub first, so MCP is covered.

Empirical result (this research, macOS, real ffmpeg 8-style image2pipe/VP9 command, parent `kill -9`): ffmpeg exited within 250 ms (first poll), file was a valid WebM, ffprobe duration 2.0 s, full decode OK. Reason: parent death closes ffmpeg's stdin pipe, ffmpeg sees EOF and writes the trailer. So orphans arise only when ffmpeg does not read stdin (wedged, or test fakes); the sweep is a safety net, not the primary mechanism. A control with a stdin-less lavfi source survived as ppid 1, so the "orphan = ppid 1" rule is observable on macOS.

**Primary recommendation:** Add `src/record/{exit,orphans,disk}.rs` (+ a guard task), 3 one-line `finish_best_effort` hooks in daemon.rs, one sweep line in reap.rs, put ffmpeg in its own process group in fork-owned encoder/process.rs, and document the matrix. No new signal handlers.

## 1. Shutdown paths (evidence)

| Path | Where | Runs hub finish? |
|------|-------|------------------|
| MCP stdin EOF (`Ok(0)` -> break) | src/mcp/server/serve.rs:~96 break; post-loop 612-617 `shutdown_browser(b)` | YES via boot.rs:183-186 (`finish_best_effort` first line) |
| MCP SIGTERM/SIGINT/SIGHUP | boot.rs:194-211 `wait_for_shutdown_signal` (tokio unix signal; Windows: `ctrl_c` only), serve.rs:80 select arm -> break -> same post-loop | YES |
| MCP idle timeout | `BLADE_IDLE_TIMEOUT` secs, default 600 (boot.rs:74-80), checked every 15 s (serve.rs:~50 interval); serve.rs:590-597 -> `shutdown_browser` | YES (Chrome only; the MCP process stays alive) |
| MCP drift relaunch / crash relaunch / `Closed` retry | serve.rs:203, 215, 338 all `shutdown_browser` | YES (finish_orphan runs on a dead source -> ConnectionLost text stored in `orphaned`, shown as "previous recording ended with the browser: " on next record op, msg.rs:150) |
| Daemon SIGTERM/SIGINT | daemon.rs:24-40 (SIGHUP deliberately NOT caught; cli.rs:290-297 `ignore_sighup`), select arm 159 and 188 -> break; post-loop daemon.rs:445-454 | YES (finish at :452 AFTER login snapshot :445-451; snapshot could stall on dead Chrome, so finish should move before it - UNVERIFIED stall risk) |
| `bladebro stop` | daemon.rs "stop" tool (~:215-232): flushes knowledge, acks, break -> post-loop. Client waits up to 30 s read timeout (daemon.rs:573) + 50x100 ms for socket removal (:586-590) | YES, but worst finish ~18 s (docs line 72) + bounded wait 5 s may print "cleanup is still running" (:595) - add test |
| Daemon idle timeout | daemon.rs:400-414 `browser.take()` -> `b.shutdown()`, `page = None` | **NO** (hub stays `active` with dead source) |
| Daemon drift / connection-lost relaunch | daemon.rs:264-268 | **NO** |
| Daemon `Closed` relaunch | daemon.rs:359-363 | **NO** |
| One-shot CLI | cli.rs:672 `hub::set_ephemeral()` first line of `run_connected`; record-start refused (`Lane::Ephemeral`, machine.rs:127); exit via `exit_with` cli.rs:743 `process::exit` | n/a: nothing can be active; honest refusal exists |
| MCP final exit | main.rs:52-66 `exit_immediately` = `process::exit` after all teardown (parked stdin read) | after finish, fine |
| kill -9 / OOM / power loss | no handler possible | ffmpeg finalizes itself on stdin EOF (experiment) |
| Windows | only `ctrl_c` (boot.rs:208, daemon.rs:40); no console-close/logoff handler; TerminateProcess skips everything | stdin-EOF still reaches ffmpeg; see Section 3 job object |

Signals: no `ctrl_c`/signal use anywhere else (grep over src). Tokio features include `signal`, `process` (Cargo.toml:24); `libc = "0.2"` also present (Cargo.toml:37). No Cargo change needed.
Caveat (UNVERIFIED): `wait_for_shutdown_signal()` builds new `signal()` streams on every select iteration; once registered tokio replaces the default disposition process-wide, so a second SIGTERM during the (up to ~18 s) finish is swallowed, not fatal. A supervisor SIGKILL after a grace still finalizes via stdin EOF.
Terminal Ctrl-C: MCP/daemon-in-foreground share the terminal's process group with ffmpeg (encoder/process.rs:126-133 sets no process group), so SIGINT also hits ffmpeg; ffmpeg exits 255 racing our finish (encoder/report.rs:166, 394-397 already classify "exit code 255"/SIGINT as unverified). The daemon is setsid'd (cli.rs:279-280) so unaffected. Recommendation: spawn ffmpeg in its own process group (std `CommandExt::process_group(0)` via tokio Command, MSRV 1.86 ok; confirm with `cargo check`). Cost zero because ffmpeg exits on stdin EOF anyway.

## 2. src/record internals

- Hub: process-global `OnceLock<Mutex<Hub>>` (hub.rs:~52); `finish_best_effort` never initializes it (hub.rs:72-84). `Hub { active: Option<Active>, orphaned: Option<(bool,String)>, knobs }` (machine.rs:70-76). `Active { path, enc: Option<Arc<Encoder>>, rec: Tracker, started, fps }` (:60-66).
- `start` refuses when `active.is_some()` (machine.rs:131-142) -> after a daemon relaunch without finish the hub is "active forever" until `record-stop`; record-start fails with "recording already active ... record-stop it first". This is the LIFE-02 never-active-forever gap. `stop` then reports honestly (ConnectionLost verdict from `Source::stop`, capture.rs:221-262 maps task failure/timeouts to `ConnectionLost`) - so the outcome text already exists; what is missing is finalizing EARLY (close encoder stdin so the partial file is playable) and clearing `active`.
- `finish_orphan` (machine.rs:272-280): takes active, runs `finish_active`, stores `(ok, text)` in `orphaned`; next `stop` returns it prefixed (machine.rs:251-262), next `start` prepends it (:229-232).
- Encoder (encoder/process.rs): `kill_on_drop(true)` (:133), stdin piped, `FINISH_TIMEOUT` 10 s (:34), `KILL_REAP_TIMEOUT` 5 s (:38), `STDERR_DRAIN_GRACE` 1 s, `CHILD_WAIT_FLOOR` 2 s; `finish` (:233-323) closes queue, drops stdin (EOF), waits, kills only on deadline -> `Killed{reaped}` -> Unfinalized. Worst case ~18 s (docs:72). `write_error` (shared) is set by writer on stdin write failure (writer.rs:45-55) = encoder-death signal; not publicly exposed (needs a small accessor in fork-owned file).
- Per-tab (Phase 8): files owned by `Policy::PerTab(Files)` inside the Tracker task; `Tracker::stop` bounded (follow.rs:186+, cap = grace*2+14 s+extra).
- Tracker snapshot `health()` (follow.rs:157) refreshed every poll; `Source::is_finished()` (capture.rs:208) true when the capture loop ended (tab closed or browser gone); follower already rebinds on finished (follow/run.rs:276-300).
- Cheap interrupted detection points: (a) the Tracker/Source `is_finished()` + tab probe failing while Chrome is gone; (b) daemon hooks calling `finish_best_effort` at the three skipped sites (deterministic, reuses `finish_orphan`); (c) a hub-owned guard task polling every ~5 s that also covers "Chrome killed, no further calls" (finalize early and park result in `orphaned`). A recording generation id on `Active` stops a stale guard from stopping a newer recording. No page/relaunch counter exists today (grep "generation|relaunch_count" found none).
- The screencast source ends cleanly vs lost: `SourceOutcome::{Captured, ConnectionLost, NoFrames}` (source/report.rs:29-40); `ConnectionLost` is reported only if no stop was requested (capture.rs:~424).

## 3. Reaper and Windows job

- `reap_orphans()` (src/session_profile/reap.rs:25-33) runs at every Chrome launch via `SessionProfile::create` (session_profile.rs:240) and `create_real` (:308). Ownership proof there: `.blade-owner` pid file or `sess-<pid>` name + `platform::process_alive` (reap.rs:96-112), then pid from Chrome SingletonLock + `process_is_chrome` + cmdline contains profile dir (`process_uses_dir`, reap.rs:202-226; macOS uses `ps -p <pid> -o command=`; Windows trusts the pid).
- Smallest hook: one line at top of `reap_orphans()`: `crate::record::orphans::sweep();` (1 line, 1 file). Limitation: only runs on a Chrome launch (acceptable: an orphan ffmpeg needs a prior Chrome session; also call the same sweep from the guard task periodically and at hub start - all in new files).
- Safe identification (design): list via `ps -axo pid=,ppid=,command=` (macOS and Linux; works on both), then require ALL: argv[0] basename is ffmpeg; argv contains the exact bladebro sequence `-hide_banner -nostats -loglevel error -y -f image2pipe -framerate <n> -c:v mjpeg -i pipe:0` (encoder/args.rs:118-160); the LAST argument is an absolute path whose file name matches `<stem>-<utc-stamp>-<id>.(webm|mp4)` (names.rs:128-130) and whose parent dir equals the resolved recordings dir (if config cannot resolve it, skip the sweep); ppid is 1 or not alive (a live bladebro/other parent -> never touched). Path with spaces makes `ps command=` ambiguous: take it as the tail after `pipe:0` ... match by prefix/suffix, not by splitting (UNVERIFIED on Linux; Linux can instead read /proc/<pid>/cmdline NUL-separated, exact). Action: SIGTERM, wait <=2 s, then SIGKILL only if still same pid with same cmdline. Windows: no cheap cmdline read (reap.rs:218-225 says so); document the gap.
- Windows job: src/platform/windows_job.rs:45 `guard_browser_process_tree` puts the CURRENT process in a `KILL_ON_JOB_CLOSE` job (flags 0x2000, :60) and is called before spawning Chrome (browser/launch.rs:26, 361); children inherit. ffmpeg is spawned later by the hub from the same process, so it is already in the job: a hard parent kill on Windows tears down ffmpeg without any edit (written, not executed; a Windows kill mid-recording leaves an unfinalized file, since TerminateProcess gives no EOF-trailer chance, though job-close may race stdin EOF). Document honestly; no hook needed.

## 4. Disk, size, ffmpeg args

- `df` approach: src/updater/download.rs:483-515 (`df -k <dir>`, last line, field before the first `%` field = Available KiB; Windows returns Ok without checking). Copy into `src/record/disk.rs` as `free_bytes(dir) -> Option<u64>` behind an injectable `FnMut`/trait; Windows: `None` -> guard disabled (documented). macOS `df -k` first `%` field is Capacity, so the field before it is Available (OK).
- ffmpeg argv: output path is always the LAST arg, pushed as an OsStr, absolute, canonical recordings dir (encoder/args.rs:118-160, check_out_path :88); preceded by `-f webm|mp4`. Spawn site encoder/process.rs:126-133.
- Size per minute (Phase 2 RESULTS.md per CONTEXT): VP9 10 fps 1280x720 ~0.87 MB/min; x264 ~0.54; 15 fps ~1.15; 1080p ~1.12. Document these plus live measurements at execution.
- Proposed defaults (documented consts, NOT env/config keys, to avoid drift-test registration): disk-low threshold 500 MiB free (refuse start below it; stop-and-finalize when below during recording; check every 5 s); max duration 2 h (~105 MB at default); max file size 1 GiB per file (~10x the 2 h default, catches pathological static/huge frames); outcomes: `stopped: disk low (N MiB free)`, `stopped: maximum duration (2 h) reached`, `stopped: size limit (1 GiB) reached`, each FINALIZED and returned via the existing `orphaned` slot at next record-stop. Owner may prefer env override; if so register in launch_mode/registry.rs:48 area + docs.

## 5. Open items from earlier phases
- 07-VERIFICATION.md:112: daemon idle-timeout, drift relaunch, Closed relaunch, crash relaunch do not finalize - owned by Phase 9 (confirmed in code above).
- 07-VERIFICATION.md: `concurrent_cold_starts_have_one_owner` flakes and leaks a daemon.
- 05-VERIFICATION.md: Windows tests (windows_tests.rs) never run; SC4/LIFE-05 pending windows CI; Phase 5 note (c): a fake ignoring EOF with a grandchild leaves the grandchild (reparented) - relevant to the sweep test design; crf 40 vs 36 owner confirmation.
- 06-VERIFICATION.md: SC5 environment-limited items need owner acceptance.
- Phase 8: popup wording decision pending (assume unchanged).
- ffmpeg/ffprobe are not on PATH here; they live at `$HOME/.local/ffmpeg/bin/` (follow_live.sh:25-26 uses BLADE_FFMPEG/BLADE_FFPROBE defaults).

## Proposed design

New files (all fork-owned, no ledger entry):
- `src/record/exit.rs`: `RecordingGuard` task spawned by `Hub::start` (generation id on `Active`); every ~5 s: disk (injectable), duration, file size (`Tracker::files()` + metadata), Chrome-gone/source-finished liveness; on trigger lock hub, if generation matches call `finish_with_reason`, store `(ok, "stopped: ...")` in `orphaned`.
- `src/record/disk.rs`: `df` copy + threshold consts + start-refusal text.
- `src/record/orphans.rs`: `sweep()` with pure `classify(ProcInfo, dir) -> Orphan|Keep(reason)` and a `ps`/`/proc` lister; fake-process table tests.
- Edits in fork-owned `machine.rs` (generation id, `finish_with_reason`, start disk check), `encoder/process.rs` (own process group, `is_dead()` accessor), `hub.rs` (module decls).

Hooks in upstream files (HOOKS.md rows):
| File | Where | Change |
|------|-------|--------|
| src/cli/daemon.rs | ~:264 (before `b.shutdown()` in relaunch), ~:359 (Closed relaunch), ~:406 (idle) | 3 lines `crate::record::hub::finish_best_effort().await;` |
| src/session_profile/reap.rs | start of `reap_orphans()` :25 | 1 line `crate::record::orphans::sweep();` |
| src/cli/daemon.rs | :445-452 | optionally move the finish line above the login snapshot (move, 0 net) |
Nothing needed in boot.rs/serve.rs/cli.rs/main.rs or Cargo.toml.

Exit matrix rows (path x lane -> outcome; each needs a fake-CDP/fake-encoder test, live where drivable):
| Exit | mcp | daemon | one-shot |
|------|-----|--------|----------|
| record-stop | finalized | finalized | n/a (refused) |
| stdin EOF | finalized (post-loop) | n/a | n/a |
| SIGTERM/SIGINT | finalized (handler->break) | finalized | no recording possible |
| `bladebro stop` | n/a | finalized (check 5 s client wait) | n/a |
| idle timeout | finalized (Chrome only, note kept) | finalized after hook | n/a |
| relaunch drift/crash/Closed | interrupted note, partial kept | same after hook | n/a |
| kill -9 parent | ffmpeg self-finalizes <=~0.25 s (measured), file decodes; no note | same | n/a |
| kill Chrome | guard finalizes as interrupted | same | n/a |
| kill ffmpeg | writer error -> interrupted/unfinalized at next stop, hub cleared | same | n/a |
| disk low / max duration / max size | finalized, `stopped: ...` | same | n/a |
Windows: only stdin-EOF and job-object reasoning; never claimed tested.

## Risks and pitfalls
- Stale guard stopping a newer recording: generation id.
- Holding the hub mutex during the ~18 s finish blocks other record ops; guard must not hold it across awaits longer than `finish_active`.
- Sweeper false positives: cmdline proof + ppid + dir + name pattern; never kill live-parent ffmpeg; tests with fake `sleep`-based processes only.
- `ps` command column is space-joined (paths with spaces); prefer /proc on Linux.
- Daemon idle: alternative of skipping idle shutdown while recording was not chosen (changes upstream semantics); recommend finishing instead.
- Daemon post-loop ordering (snapshot before finish) may delay finalize if Chrome is dead.
- Disk check unavailable on Windows (guard off, documented).
- Known flake `concurrent_cold_starts_have_one_owner`.

## Validation Architecture
| Property | Value |
|----------|-------|
| Framework | Rust built-in tests (`cargo test --release`), baseline 720 pass/12 ignored |
| Quick run | `cargo test --release record::` |
| Full | `cargo test --release --no-fail-fast`; clippy `--all-targets -D warnings`; fmt |
| Live | tools/record_sidecar/scenarios/exit_live.sh (new), pattern follow_live.sh/hub_live.sh |

Req map: LIFE-01 matrix tests in src/record/hub (fake ffmpeg + FakeCdp, machine_tests style) + live per lane; LIFE-02 relaunch-mid-recording state machine test (finish_orphan then start works); LIFE-03 orphans classify table + fake-process test; process-group test; LIFE-04 injectable free-space fn test, duration/size via Knobs-shortened timings.
Wave 0: new test modules `exit_tests.rs`, `orphans_tests.rs`, `disk_tests.rs`; nothing to install.

## Sources
Primary (HIGH, repo code read): files cited inline above; own experiment (scratchpad p2.py, real ffmpeg, kill -9 parent: gone <=250 ms, decode OK, 2.0 s). No external web used.
**Valid until:** code-change dependent.
