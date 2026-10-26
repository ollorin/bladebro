---
phase: 09-finalization-on-every-exit-path
verified: 2026-10-26T09:50:00Z
status: passed
score: 4/4 success criteria verified (macOS arm64; see "Not verified")
re_verification:
  previous_status: gaps_found   # opus adversarial pass on 9613f3b (G1 + ledger gap)
  previous_score: 3/4
  gaps_closed:
    - "G1: orphan sweeper no longer kills bladebro's own live encoder when bladebro is pid 1 (classify_as with injected `me`; Keep when ppid == me)"
    - "Ledger: daemon.rs post-loop hook no longer re-indents the two upstream comment lines; diff vs 9756923 is hook lines only and matches HOOKS.md"
  gaps_remaining: []
  regressions: []
---

# Phase 9: Finalization on Every Exit Path - Verification Report (re-verification after c0ea00d)

**Phase Goal:** However a session ends, the recording is finalized or honestly reported as interrupted, and no ffmpeg process is left behind.
**Verified:** 2026-10-26 (macOS arm64, Chrome 154 headless, ffmpeg n9.0.2 at ~/.local/ffmpeg/bin, release build of HEAD c0ea00d)
**Status:** passed (with the explicit "Not verified" list below)
**Re-verification:** Yes, after gap-closure commit c0ea00d. Every check below was re-run by this verifier; the fix commit's claims were not trusted.

Safety: fresh `mktemp -d` BLADE_HOME per run, BLADE_HEADLESS=1, BLADE_NO_UPDATE_CHECK=1, BLADE_RECORD=1, fixtures served by `python3 -m http.server 8799 --bind 127.0.0.1` (tools/record_sidecar/fixtures). Only processes this verifier started (or whose command line held its scratch home) were signalled. Final state: 0 bladebro / ffmpeg / scratch Chrome / fixture server left.

## History (previous passes, summarized)

- Sonnet advisory pass (09-VERIFICATION-sonnet.md): gaps_found, 3/4.
- Opus adversarial pass on 9613f3b: gaps_found, 3/4. Every exit path on both lanes and both tab modes finalized (files decoded in full, 0 leftovers); browser crash gave an honest `interrupted` note; disk-low stop live on a scratch disk image; instruments at baseline (772/0/12 tests, audit 100/100, qol 120/122 baseline). Two gaps: **G1** `classify()` treated ppid==1 as orphan unconditionally, so a bladebro running as pid 1 (init-less container) would SIGTERM/SIGKILL its own encoder at every record-start sweep (reproduced with product code); **ledger** rustfmt re-indented two upstream comment lines in `src/cli/daemon.rs`.

## Gap closure

### G1 - CLOSED

Code (src/record/orphans.rs:91-133): `classify` now delegates to `classify_as(p, dir, alive, std::process::id())`; the parent rule is `if p.ppid == me || (p.ppid != 1 && alive(p.ppid)) { Keep("parent alive") }`. New unit test `record::orphans_tests::bladebro_as_pid1_keeps_its_own_live_encoder` (ppid 1, me 1 -> Keep; ppid 1, me 777 -> Orphan) passes. Docs line 232 updated and now true.

Live, real processes (scratch crate linking the bladebro lib, no repo edit; all ffmpeg started by the probe, stdin held open by a pipe / FIFO):

| case | result |
|---|---|
| A: live ffmpeg child, real ps row (ppid = me) | `classify` -> Keep("parent alive") |
| A, with `alive` forced to false | Keep("parent alive") (the new ppid==me branch alone protects it) |
| A row rewritten to ppid=1, `classify_as(me=1)` (bladebro as pid 1) | Keep("parent alive") |
| A row ppid=1, `classify_as(me=<real pid>)` (old bug shape) | Orphan (rule still reaps a real ppid-1 orphan) |
| `sweep_in` on A's real row, alive=false, REAL killer (`kill_orphan`) | Keep; child still running afterwards |
| B: genuine orphan (shell launched ffmpeg and exited; reparented to launchd, ppid 1), bladebro argv + recordings-dir name | `classify` -> Orphan; `sweep_in` over the full `ps` list with the real killer killed only B (A kept); B gone < 4 s |
| End-to-end with the fixed binary (U1-U9 matrix: real orphan U3 + 8 near misses / live parent) | Chrome-launch sweep reaped U3 only; record-start sweep left all others; record-stop file saved; cleanup 0 left |

Attempts to break the new rule:
- pid reuse: `ppid == me` can only be true while this process is alive (our pid cannot be reused while we live), and a `ps` row with ppid == me is by definition our child at snapshot time. The SIGKILL path keeps its same-command-line re-check. No wrong protection found.
- bladebro is not a subreaper (no `prctl`/`PR_SET_CHILD_SUBREAPER` in src), so on a normal host no foreign orphan is ever reparented to it.
- Residual (safe direction, Info): when bladebro IS pid 1, any orphan in that container is reparented to it, so `ppid == me` also keeps a genuinely orphaned encoder left by a different bladebro process in the same container (shown: B with `classify_as(me=1)` -> Keep). The backstop is inert there, never destructive; ffmpeg's own stdin-EOF exit remains the primary mechanism. Docs say "keeps its own encoders", which slightly understates (it keeps all of them). Not a goal failure.
- Test coverage note (Info): the gap's suggested `sweep_in` test with me=1 was not added (sweep_in uses the real pid); covered by this verifier's live sweep_in run above.

### Ledger - CLOSED

`git diff 9756923..HEAD -- src/cli/daemon.rs src/session_profile/reap.rs`: daemon.rs = three `+1` hook lines (lines 267, 363, 414) and the post-loop hook moved below the two untouched upstream comment lines (`+1/-1`, no upstream line altered); reap.rs = one `+1` hook line. Matches HOOKS.md rows 488c0e0 / b95e023 ("moved above the login snapshot block (net 0 lines)").
All 29 changed non-.planning paths checked with `git cat-file -e upstream/main:<path>` (upstream/main 6b524d2): only `src/cli/daemon.rs` and `src/session_profile/reap.rs` exist upstream. Cargo.toml/lock, npm/, CHANGELOG.md, .github/, src/updater/, src/mcp/tools.rs: 0 diff.

## Success criteria

| # | Criterion | Verdict | Evidence (this pass) |
|---|---|---|---|
| 1 | Finalized on record-stop, tab close, daemon shutdown/idle, `bladebro stop`, one-shot, SIGTERM/Ctrl-C | VERIFIED | Smoke below (MCP EOF, MCP SIGTERM follow + per-tab, daemon `stop` follow + per-tab, daemon SIGTERM): every file ffprobe ok, full decode rc 0 / 0 error lines; full matrix from the previous pass unchanged by c0ea00d (changes touch only the orphan rule and a hook line position). |
| 2 | Browser crash mid-recording -> interrupted result, partial file kept | VERIFIED | kill -9 scratch Chrome, MCP and daemon: `previous recording interrupted: browser connection lost at 6.0 s; partial file kept: <path>` ; 5.9 s files decode; restart + stop saves a 2.0 s file that decodes. |
| 3 | No ffmpeg remains; reaper cleans orphaned encoders; orphan test | VERIFIED | Leftovers 0 in every smoke run (ffmpeg gone 30-64 ms after the exit action); real orphan reaped live; G1 closed (above). |
| 4 | Disk-low stop; size-per-minute documented | VERIFIED (previous pass, untouched by c0ea00d) | Live scratch-disk run in the previous pass; guard_tests seams for 2 h / 1 GiB pass in this run's suite. |

**Score:** 4/4

## Live smoke (this pass, release build of c0ea00d)

| id | exit action | bladebro exit | ffmpeg gone | files (ffprobe / decode) | leftovers |
|---|---|---|---|---|---|
| m-eof | MCP stdin EOF | 830 ms | 30 ms | 4.1 s ok | 0 (57 ms) |
| m-sigterm | SIGTERM MCP | 839 ms | 31 ms | 4.1 s ok | 0 |
| m-sigterm-pt | SIGTERM MCP, per-tab | 833 ms | 57 ms | 4.3 s + 2.4 s ok | 0 |
| m-kill_chrome | kill -9 Chrome | - | - | 5.9 s ok + 2.0 s restart ok; interrupted note | 0 |
| d-stop | `bladebro stop` | returned 836 ms | 33 ms | 4.1 s ok | 0 |
| d-stop-pt | `bladebro stop`, per-tab | returned 838 ms | 64 ms | 4.3 s + 2.4 s ok | 0 |
| d-sigterm | SIGTERM daemon | 827 ms | 30 ms | 4.1 s ok | 0 |
| d-kill_chrome | kill -9 Chrome | - | - | 5.9 s ok + 2.0 s restart ok; interrupted note | 0 |

## Instruments

| instrument | result | baseline |
|---|---|---|
| `cargo fmt --check` | rc 0 | clean |
| `cargo clippy --release --all-targets -- -D warnings` | rc 0 | clean |
| `cargo test --release --no-fail-fast` | 772 passed / 1 failed / 12 ignored; sole failure = known flake `concurrent_cold_starts_have_one_owner` (NotFound at daemon_lifecycle.rs:204); leaked race daemon stopped via `BLADE_HOME=<race dir> bladebro stop`; rerun `--test daemon_lifecycle`: 5/5 pass. Net: 773 tests, all green (one new test vs 772). | 772/0/12 |
| `BLADE_HEADLESS=1 bladebro audit` (scratch home) | 100/100, VERDICT PASS | 100/100 |
| Layer-1 gates (scripts/fp-gate.ts, migration-security-gate, cve-scan-gate, check-p0-tier-delta) | not present in this repo (N/A) | |
| qol_probe, lane_matrix, diff_oracle | not re-run (c0ea00d touches no input/stealth/launch code; previous pass at baseline) | |

Charlotte QA / E2E plan: not applicable (CLI/MCP binary, no web UI). New/changed impl files have test counterparts. No migrations. No deferral of this phase's tests.

## Anti-patterns / observations

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| src/record/orphans.rs | 130 | ppid==me also keeps foreign orphans when bladebro is pid 1 (it is their reaper) | Info | backstop inert in that environment; safe direction |
| src/record/orphans_tests.rs | 409 | no sweep_in-level test with me=1 (classify_as only) | Info | covered live by this verifier |
| src/record/exit_tests.rs | 150-175 | source-text pins for signal/idle rows (from previous pass) | Info | compensated by live runs |
| src/record/hub/machine.rs | ~245 | two times for one event (`at 6.0 s` vs `lost at 4.7 s`) (from previous pass; reconfirmed in smoke text) | Info | cosmetic |
| per-tab exit paths | - | a tab left before its first frame leaves an unreported 0-byte file on EOF/signal exits (previous pass O3) | Info | docs "plays in full" overclaims for that edge |
| MCP lane kill -9 | - | scratch Chrome not reaped by next MCP launch (pre-existing, also with recording off) | Info | deferred-items.md carve-out |

Logical cohesion: orphans.rs remains rule + lister + killer for one concern; no re-tangling.

## Not verified

- Real pid 1: the G1 fix is verified with the injected `me` (unit test + live real-process probe) and by reasoning that `std::process::id()` returns 1 there; no actual init-less Linux container run (not possible on this macOS host).
- Linux and Windows entirely (sweeper on Linux incl. subreaper desktops; Windows job-object reasoning).
- A real 2 h or 1 GiB recording (seam tests only; no knob by design, D9-13).
- Headed mode, `real` lane, diff_oracle, SIGHUP paths, per-tab 8-file cap at exit.
- This pass re-ran a smoke subset; the full exit matrix, disk-low live run and qol/lane_matrix are from the previous pass (on 9613f3b), not re-run here.

## Gaps Summary

Both gaps from the previous pass are closed. G1: the rule now keeps any encoder whose parent is this process, which covers bladebro-as-pid-1, while a real ppid-1 orphan is still reaped (shown live with real ffmpeg processes and with the fixed binary end to end). Ledger: the daemon.rs diff is hook lines only and matches HOOKS.md; no other upstream file changed. No regressions: fmt/clippy clean, the suite is green apart from the known flake (green on rerun), audit 100/100, smoke on both lanes finalizes or honestly reports interruption with 0 leftovers. Remaining items are platform/environment coverage, listed above.

---

_Verified: 2026-10-26T09:50:00Z_
_Verifier: Claude (gsd-verifier, adversarial re-verification of c0ea00d, opus)_
