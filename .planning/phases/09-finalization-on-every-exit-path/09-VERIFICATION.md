---
phase: 09-finalization-on-every-exit-path
verified: 2026-10-26T09:45:00Z
status: gaps_found
score: 3/4 success criteria verified (SC3 fails on G1)
re_verification:
  previous_status: gaps_found   # 09-VERIFICATION-sonnet.md (advisory pass)
  previous_score: 3/4
  gaps_closed: []
  gaps_remaining:
    - "G1 sweeper ppid==1 rule kills bladebro's own live encoder when bladebro is pid 1 (confirmed, see below)"
  regressions: []
gaps:
  - truth: "The orphan sweeper never kills an ffmpeg whose bladebro is still running (LIFE-03b 'identified SAFELY'; docs: 'an ffmpeg whose bladebro is still running [is] never touched')"
    status: failed
    failure_type: semantic_stub
    reason: "classify() treats ppid==1 as orphan unconditionally. When bladebro itself is pid 1 (container entrypoint without an init, e.g. `docker run -i img bladebro mcp`), every encoder it spawns has ppid 1, and the guard sweeps the recording's own dir right after every record-start (guard.rs:43-46). Reproduced with the product's own classify/sweep_in/kill_orphan against a LIVE ffmpeg child whose ps row was given ppid=1: verdict Orphan, SIGTERM ignored while blocked on stdin, SIGKILL after 2 s (child exit status signal 9). Real ppid -> Keep('parent alive'). Linux-container only; not runnable on macOS as pid 1."
    artifacts:
      - path: "src/record/orphans.rs"
        issue: "line 124: `if p.ppid != 1 && alive(p.ppid)` -> ppid 1 always Orphan, even when pid 1 is this live process"
      - path: "src/record/hub/guard.rs"
        issue: "lines 43-46: sweep of the active recording's own dir at every record-start, after ffmpeg was spawned, so the PID-1 case kills the just-started encoder deterministically"
      - path: "src/record/orphans_tests.rs"
        issue: "no case for ppid == own pid / own pid == 1"
      - path: "docs/recording-and-launch-config.md"
        issue: "line 232 'its parent process is gone (or is pid 1) ... an ffmpeg whose bladebro is still running are never touched' is false when bladebro is pid 1"
    missing:
      - "In classify, Keep when p.ppid == std::process::id() (pass `me` into classify/sweep_in for testability); this also covers the pid-1 case. Optionally also skip the guard's record-start sweep entirely when std::process::id() == 1."
      - "Unit test: ProcInfo{ppid: me} with me == 1 -> Keep; sweep_in with me=1 never kills a child"
      - "Docs sentence adjusted"
  - truth: "Every edit to a pre-existing upstream file is a listed tiny hook (SYNC-01 / HOOKS.md)"
    status: failed
    failure_type: regression
    reason: "src/cli/daemon.rs post-loop: besides the moved hook line (listed as 'net 0 lines'), rustfmt re-indented the two upstream comment lines `// Cleanup: persist live logins...` and `// (Never on the real lane...)` into trailing-comment continuations (diff shows -2/+2 on upstream lines). Unlisted upstream-line change and a needless merge-conflict surface if upstream edits that comment. Minor."
    artifacts:
      - path: "src/cli/daemon.rs"
        issue: "lines ~446-448: hook placed above the comment block with a trailing `// fork hook` comment, so rustfmt aligned the following comment lines under it"
    missing:
      - "Place the hook line after the two untouched upstream comment lines (or put `// fork hook ...` on its own line above the call) so the diff is exactly +1/-1 hook lines; update HOOKS.md row if needed"
---

# Phase 9: Finalization on Every Exit Path - Verification Report (adversarial final pass)

**Phase Goal:** However a session ends, the recording is finalized or honestly reported as interrupted, and no ffmpeg process is left behind.
**Verified:** 2026-10-26 (macOS arm64, Chrome 154 headless, ffmpeg n9.0.2, release build of HEAD 9613f3b)
**Status:** gaps_found
**Re-verification:** After the advisory sonnet pass (09-VERIFICATION-sonnet.md). All probes below were RE-RUN by this verifier with its own harness (scratchpad, not exit_live.py); evidence files were not trusted.

Safety: every run used a fresh `mktemp -d` BLADE_HOME, BLADE_HEADLESS=1, BLADE_NO_UPDATE_CHECK=1, local fixtures on 127.0.0.1:8799 (tools/record_sidecar/fixtures). Only pids whose command line held the scratch home, or pids this verifier spawned, were signalled. No user Chrome/ffmpeg touched. Disk-low live run used a 560 MB scratch disk image (hdiutil, detached and deleted afterwards), not a real disk.

## Success criteria

| # | Criterion | Verdict | Evidence |
|---|---|---|---|
| 1 | Finalized on record-stop, tab close, daemon shutdown/idle, `bladebro stop`, one-shot exit, SIGTERM/Ctrl-C, per path | VERIFIED | Live table below: every drivable path in both lanes, follow and per-tab, every produced file decoded in full (rc 0, 0 error lines); one-shot refused with no file and no process. Hooks: daemon.rs:267/363/414/446, boot.rs:185 (`shutdown_browser`, used by serve.rs:203/215/338/597/617). |
| 2 | Browser crash/relaunch mid-recording -> interrupted/unfinalized result, partial file kept | VERIFIED | kill -9 of scratch Chrome main (mcp/daemon, follow/per-tab): next record-stop `previous recording interrupted: browser connection lost at 6.0 s; partial file kept: <path>. recording not clean: ...`; files 5.9-6.0 s decode in full; new start/stop saves. Idle relaunch: `previous recording ended with the browser: recording saved: ...` (30 s file decodes). |
| 3 | No ffmpeg remains after normal exits and simulated crashes; reaper cleans orphaned encoders; orphan integration test | FAILED (G1) | Leftover ffmpeg = 0 in every scenario (ffmpeg gone 27-161 ms after the exit action). Real orphan reaped live (U3 below). BUT the identification rule kills a live encoder when bladebro is pid 1 (G1, reproduced with product code). |
| 4 | Disk-low stop; size-per-minute documented | VERIFIED | Live on a scratch image: start at 546 MiB free OK; 60 MiB ballast -> 486 MiB; guard stopped and finalized in 1.46-1.65 s (follow and per-tab, all encoders); next op `previous recording stopped: disk low (486 MiB free); saved: <path> (bytes 49573, duration 4.1 s, ...)` (decodes); record-start refused `recording not started: disk low (486 MiB free in <dir>, need 500 MiB)`; after ballast removal start works. 2 h / 1 GiB: injected seam only (guard_tests max_duration_stops_and_finalizes, max_size_stops_and_finalizes, per_tab_size_finalizes_every_file pass); no env/config knob exists (consts, D9-13), so not drivable live. Sizes table in docs ("File size per minute"). |

**Score:** 3/4

## Live probe results (this verifier's harness)

All files: ffprobe duration + `ffmpeg -v error -i f -f null -`. Leftovers = ffmpeg/bladebro/Chrome under the scratch home, polled up to 10 s.

| id | exit action | bladebro exit | ffmpeg gone | files (decode) | leftovers |
|---|---|---|---|---|---|
| m-eof | MCP stdin EOF | 871 ms | 31 ms | 4.1 s ok | 0 (55 ms) |
| m-eof-pt | same, per-tab | 801 ms | 55 ms | tab2 2.4 s ok; tab1 **0 bytes** (tab left before its first frame; see O3) | 0 |
| m-sigterm / -pt | SIGTERM MCP | 854 / 808 ms | 27 / 56 ms | 4.1 s ok / 4.3 s + 2.3 s ok | 0 |
| m-sigint / -pt | SIGINT MCP | 866 / 882 ms | 28 / 59 ms | 4.1 s ok / 4.3 + 2.3 s ok | 0 |
| m-kill9 / -pt | kill -9 MCP | 36 / 46 ms | 118 / 158 ms | 4.0 s ok / 4.0 + 2.2 s ok | ffmpeg/bladebro 0; 8/12 scratch Chrome left (pre-existing, also with recording OFF: re-run here, 9 left; deferred-items.md) |
| m-kill_chrome / -pt | kill -9 Chrome | - | - | 5.9 s ok / 6.0 + 3.9 s ok; text `interrupted: browser connection lost` | 0 |
| m-kill_ffmpeg | SIGKILL ffmpeg | - | - | 0 bytes; text `interrupted: encoder exited at 6.0 s; file is empty, nothing playable` | 0 |
| m-kill_ffmpeg-pt | SIGKILL tab1 ffmpeg | - | - | tab1 0 bytes reported at stop as `recording failed: no frames reached the encoder`; tab2 6.3 s ok | 0 |
| m-term_ffmpeg | SIGTERM ffmpeg | - | - | 4.3 s ok; `interrupted: encoder exited ...; partial file kept` | 0 |
| m-idle / -pt | BLADE_IDLE_TIMEOUT=20 | Chrome+ffmpeg gone 26.5 / 26.4 s | | 30.0 s ok / 29.9 + 28.0 s ok; `previous recording ended with the browser: recording saved` | 0 |
| d-stop / -pt | `bladebro stop` | returned 847 / 949 ms `daemon stopped` | 29 / 70 ms | 4.2 s ok / 4.3 + 2.4 s ok | 0 |
| d-sigterm / -pt | SIGTERM daemon | 851 / 794 ms | 38 / 53 ms | ok / ok | 0 |
| d-sigint / -pt | SIGINT daemon | 796 / 790 ms | 28 / 54 ms | ok / ok | 0 |
| d-kill9 / -pt / -mp4 | kill -9 daemon | 36-50 ms | 33-161 ms | 4.0 s ok / 4.0 + 2.2 s ok / mp4 3.9 s ok | 0 after next launch (daemon reaper removes Chrome) |
| d-kill_chrome / -pt | kill -9 Chrome | - | - | 6.0 s ok / 6.0 + 3.9 s ok; `interrupted: browser connection lost` | 0 |
| d-kill_ffmpeg / -pt / -mp4 | SIGKILL ffmpeg | - | - | 0 bytes (webm and mp4), honest `file is empty` / per-tab `ffmpeg exited with signal: 9 ... may be incomplete`; other tab 6.4 s ok | 0 |
| d-idle / -pt | idle 20 s | 26.3 / 25.9 s | | 29.3 s / 29.2 + 27.2 s ok | 0 |
| tab close per-tab | close-tab tab2 | - | tab2 encoder exited at close, tab1 kept recording | 7.4 s + 1.8 s ok, note `tab ... gone at 5.3 s` | 0 |
| tab close follow | close-tab current | - | single encoder continues | 7.5 s ok, 3 segments | 0 |
| one-shot | `--no-daemon state record-start` | rc 1 | none started | no recordings dir | 0 |
| disk-low follow / per-tab | ballast on scratch image | - | 1.65 s / 1.46 s (both encoders) | 4.1 s / 6.0 + 2.4 s ok | 0 |

Per-tab: every encoder that existed finalized on every exit path. Observation: with two `open-tab` calls ~0.5 s apart only 2 of 3 tabs got a file (the briefly visited middle tab is never "visited" by the follower); Phase 8 behavior, outside Phase 9.

## Orphan sweeper (B) - live, real processes

Real ffmpeg processes started by the verifier, stdin held open via FIFOs so they cannot EOF; trigger = Chrome launch (`bladebro nav`, reap hook) then `record-start` (guard sweep):

| proc | description | after launch sweep | after record-start sweep |
|---|---|---|---|
| U1 | exact bladebro argv, other dir, orphaned (ppid 1) | alive | alive |
| U2 | exact argv in recordings dir, parent alive | alive | alive |
| U3 | exact argv in recordings dir, bladebro name, orphaned (REAL orphan) | **reaped** | gone |
| U4 | orphan in rec dir, non-bladebro name | alive | alive |
| U5 | orphan, bladebro name, `-f mjpeg -i pipe:0` | alive | alive |
| U6 | bladebro argv + trailing `-y` | alive | alive |
| U7 | relative mp4 output | alive | alive |
| U8 | output in rec subdir | alive | alive |
| U9 | `-f rawvideo -i pipe:0` | alive | alive |

No false positive on any user-style or near-miss ffmpeg on macOS. A real orphaned ffmpeg is reaped (refutes sonnet gap 2 "fakes only" as a product gap; the in-repo test still uses fakes, which CONTEXT explicitly allowed).
Attempt to create a bladebro-spawned orphan: SIGSTOP the encoder then kill -9 the daemon -> the kernel sends SIGHUP+SIGCONT to the now-orphaned stopped process group and ffmpeg finalizes on its own (file decodes, 0 left). So a SIGSTOPped encoder does not produce an orphan on macOS.
**G1 confirmed** (scratch crate linking the bladebro lib, no repo edit): live ffmpeg child, real row -> `Keep("parent alive")`; same row with ppid=1 -> `Orphan`; `sweep_in` + `kill_orphan` -> child exit status signal 9.
Also (non-blocking, reasoned, Linux not run): on Linux desktops with a child subreaper (systemd --user) orphans are reparented to a live subreaper, so the rule returns `parent alive` and the sweeper is inert there (safe direction); minimal images without procps `ps` also make it inert.

## Hooks ledger (D)

`git diff 9756923..HEAD -- . ':!.planning'`: 29 files; paths existing on upstream/main (6b524d2): only `src/cli/daemon.rs` and `src/session_profile/reap.rs`. reap.rs +1 (HOOKS row b95e023). daemon.rs: 3 one-line hooks + moved post-loop hook (HOOKS rows 488c0e0), plus 2 upstream comment lines re-indented by rustfmt (gap 2). Cargo.toml/lock, npm/, CHANGELOG.md, .github/, src/updater/, src/mcp/tools.rs: 0 diff.

## Instruments (E)

| instrument | result | baseline |
|---|---|---|
| `cargo fmt --check` | rc 0 | clean |
| `cargo clippy --release --all-targets -- -D warnings` | rc 0 | clean |
| `cargo test --release --no-fail-fast` | 772 passed / 0 failed / 12 ignored (flake did not fire; no live race daemon left) | 772/0/12 |
| `BLADE_HEADLESS=1 bladebro audit` (scratch home) | 100/100, VERDICT PASS | 100/100 |
| `BLADE_HEADLESS=1 tools/qol_probe/run.py` | 120 passed, 2 failed: exactly 'chord cleared the live editor', 'nested scroller movement is reported as such' | 120/122 |
| lane_matrix_local.py x5, BLADE_RECORD=0 | daemon 1.1-1.2 s, oneshot 0.6, mcp 0.7, mcp-pipe 0.7; all rc 0 | |
| same, BLADE_RECORD=1 | daemon 1.1-1.3, oneshot 0.6, mcp 0.7, mcp-pipe 0.7; all rc 0 | |
| Layer-1 gates (scripts/fp-gate.ts etc.) | not present in this repo (N/A) | |
| diff_oracle, `real` lane | not run (cannot run here / out of scope) | |

Charlotte QA / E2E plan: not applicable (CLI/MCP binary, no web UI). New impl files have test counterparts (orphans_tests, disk_tests, guard_tests, exit_tests, process_group_tests). No migrations.

## Anti-patterns / observations

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| src/record/orphans.rs | 124 | ppid==1 treated as orphan unconditionally | Blocker (G1) | kills own live encoder when bladebro is pid 1 |
| src/cli/daemon.rs | 446-448 | upstream comment lines re-indented | Minor (gap 2) | unlisted upstream edit, merge surface |
| src/record/exit_tests.rs | 150-175 | row_signal_*/row_idle_* are source-text pins + a generic orphan_cycle, not exercising the signal/idle path | Info | compensated by the live runs above (all green) |
| src/record/hub/machine.rs | ~245 | interrupted `at <T> s` is guard detection time (6.0 s) while the same message says `browser connection lost at 4.7 s` | Info | two times for one event; consider using the source's loss time |
| per-tab, exit paths | - | a tab left before its first frame leaves a 0-byte file and, on EOF/signal exits, nobody reports it (record-stop does report `no frames reached the encoder`) | Info (O3) | docs "file plays in full" overclaims for that edge |
| MCP lane kill -9 | - | scratch Chrome not reaped by the next MCP launch | Info, pre-existing (re-proven with recording OFF) | deferred-items.md carve-out |

Logical cohesion: orphans.rs (rule + ps lister + killer), disk.rs, guard.rs are single-responsibility; no re-tangling found.

## Sonnet-pass gaps: confirm/refute

1. G1 PID-1 false positive: CONFIRMED, now concrete (guard sweeps own dir at every record-start, so it is deterministic in that environment, not just "could").
2. Orphan test uses fakes only: REFUTED as a product gap. A real orphaned ffmpeg with bladebro's exact argv is reaped live; non-matching real processes survive. In-repo test with fakes was the agreed method.
3. Killed ffmpeg leaves 0-byte file: REFUTED as a gap. SC2 is about browser crash/relaunch, where the partial file is kept and plays. For a SIGKILLed encoder the text says `file is empty, nothing playable` (honest); SIGTERMed encoder gives `partial file kept` and the file plays (4.3 s).

## Not verified

Linux and Windows (the G1 scenario itself is Linux-container only; reproduced via product code, not as real pid 1); headed mode; `real` lane; diff_oracle; a real 2 h or 1 GiB recording (seam only, no knob); per-tab with the 8-file cap at exit; SIGHUP paths; Windows job-object reasoning.

## Gaps Summary

Everything on macOS works as documented across both lanes, both tab modes and every drivable exit path; instruments match baselines. One real defect blocks SC3: the orphan rule's "ppid 1 means orphan" is unsafe when bladebro is pid 1, and the guard runs that sweep right after every record-start, so recordings would be killed at once in an init-less container. Fix is surgical (Keep when ppid == own pid, plus a test and a doc line). A second, minor ledger gap: two upstream comment lines in daemon.rs were re-indented.

---

_Verified: 2026-10-26T09:45:00Z_
_Verifier: Claude (gsd-verifier, adversarial final pass, opus)_
