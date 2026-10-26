---
phase: 09-finalization-on-every-exit-path
verified: 2026-10-26
status: gaps_found
score: 3/4 criteria passed on macOS; SC3 has one open design gap
---
# Phase 9 verification (sonnet pass; opus pass to follow)

Method: read SUMMARYs, evidence files, code; ran `cargo test --release --lib record` (344 passed, 0 failed); no ffmpeg left. Full instruments NOT run.

## Criteria
1. Finalize on every exit path: PASSED (macOS, headless, scratch homes). Hooks: src/cli/daemon.rs (3 relaunch/idle hooks + post-loop), src/record/hub.rs:79 finish_best_effort, process_group at src/record/encoder/process.rs:139, MATRIX in src/record/exit.rs, tests exit_tests.rs. Live: evidence/09-03-live.txt (91 PASS/0 FAIL: EOF, SIGTERM, SIGINT, bladebro stop, idle, kill-9, both lanes). One-shot is refused (hub::set_ephemeral), not finalized: honest, matches the "one-shot CLI exit" wording only by refusal. Windows/Linux not run.
2. Crash/relaunch gives interrupted + partial kept: PASSED. src/record/hub/guard.rs (2 s tick, generation check), msg.rs texts; evidence/09-02-kill9.txt and 09-03-live.txt (kill Chrome: decodes, "previous recording interrupted"; kill ffmpeg: "file is empty, nothing playable"). Caveat: killed-ffmpeg case yields a 0-byte file (VP9 written at finalize), so "partial kept" holds only for browser loss. Per-tab ffmpeg death not noticed early (unit only).
3. No ffmpeg left; reaper cleans orphans, orphan integration test: GAPS (minor). Normal exits and crashes leave 0 ffmpeg (live). Hook at src/session_profile/reap.rs:21; sweeper src/record/orphans.rs; orphans_tests.rs has a fake-process integration test. But a real orphaned ffmpeg was never produced (ffmpeg exits on EOF in ~60 ms), so the sweeper is proven on fakes only; SIGSTOP case not run. See sweeper review.
4. Disk low stop + size-per-minute docs: PASSED with caveat. src/record/disk.rs, guard limit_hit, start refusal; disk_tests.rs; sizes in evidence/09-03-sizes.txt and docs. Disk low / 2 h / 1 GiB driven only by injected values, never live; Windows disk guard is off (None).

## Hooks ledger: PASSED
Upstream-existing files changed since 9756923 (excluding .planning): only src/cli/daemon.rs (+6/-3) and src/session_profile/reap.rs (+1). Matches HOOKS.md Phase 9 rows exactly: 3 one-line hooks + 1 moved line (daemon.rs diff confirmed, each tagged `fork hook`), 1 line in reap.rs. Nits: rustfmt re-indented the post-loop comment (cosmetic merge noise in an upstream file, 2 comment lines); the Phase 9 rows sit under the "Phase 8: no new hooks" paragraph rather than in the table (formatting only). All other changed paths are new files (src/record/**, tools/record_sidecar/**, docs).

## Orphan sweeper review
Four-part rule (orphans.rs classify): basename ffmpeg, ` -f image2pipe ` + ` -i pipe:0 ` + ` -f webm|mp4 `, absolute output whose parent == recordings dir with bladebro's `<stem>-<stamp>-<6hex>` name, ppid 1 or dead. A user's ffmpeg would need all four including writing into ~/.blade/recordings (or the configured dir) with that name: practically cannot match. Gaps:
- G1: "ppid == 1 means orphan" is wrong when bladebro itself is PID 1 (Docker/container entrypoint). Its live ffmpeg child has ppid 1 and the sweep (at Chrome launch and record-start via guard.rs:45) could SIGTERM its own live encoder. Unverified, Linux only; suggest excluding ppid==1 when pid 1 is our own process/ancestor, or require the pid not to be a child of our pid.
- G2: configured recordings dir shared by two bladebro installs: safe (live parent is kept).
- Pid reuse guarded by same-cmdline check before SIGKILL. Windows no-op. Install path containing " -" is never matched (safe direction).

## Not verified
Linux, Windows (process_exited false there, sweeper no-op, job-object reasoned); headed; `real` lane; real orphan ffmpeg; SIGSTOPped parent; disk low/2 h/1 GiB live; per-tab live exit runs; MCP kill -9 leaves scratch Chrome (pre-existing, deferred-items.md); diff_oracle not run; audit 100/100 and qol 120/122 are the SUMMARY's claims, not re-run; test baseline 720 not re-measured; Phase 5 LIFE-05 Windows CI item remains open (not a Phase 9 deliverable but referenced); STATE/ROADMAP not updated.

## Gaps (for plan --gaps, none blocking on macOS)
1. Sweeper PID-1 false positive (G1), no test for bladebro-as-pid-1.
2. SC3 "orphan-process integration test" uses fakes; no real-orphan proof.
3. Killed-ffmpeg leaves empty file: report is honest but SC2 "partial kept" is not literally met for that case.
