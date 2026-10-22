---
phase: 07-recordhub-and-state-ops
verified: 2026-10-22T13:26:00Z
status: passed
score: 12/12 must-haves verified
re_verification:
  previous_status: gaps_found
  previous_score: 10/12
  gaps_closed:
    - "state tabs marks exactly the agent's tab while recording (761910a): live MCP and daemon lanes show one `*` in every step, including switching back to the recorded tab; off mode unchanged with no extra CDP call"
    - "docs `captured` row now matches report.rs (src.stats.forwarded after the fps cap) (761910a)"
  gaps_remaining: []
  regressions: []
human_verification: []
---

# Phase 7: RecordHub and state Ops Verification Report (re-verification after 07-04)

**Phase Goal:** An agent can start and stop a named recording through the existing `state` tool and get a complete, honest report, while Bladebro behaves exactly as today when recording is not used.
**Verified:** 2026-10-22T13:26:00Z (macOS arm64, Chrome 154, headless, a scratch BLADE_HOME for every run, BLADE_NO_UPDATE_CHECK=1, 127.0.0.1 fixtures only)
**Status:** passed
**Re-verification:** Yes. It follows the 13:15 pass (gaps_found 10/12). Fixes: 761910a (code + doc) and 8cc1ccc (HOOKS row, 07-04-SUMMARY).

## Gates run in this pass

| Gate | Result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo clippy --release --all-targets -- -D warnings` | exit 0 |
| `cargo test --release` | Run 1: `daemon_lifecycle::concurrent_cold_starts_have_one_owner` failed (tests/daemon_lifecycle.rs:204 unwrap NotFound). This is the known cold-start race flake; the file is untouched by this phase. The failed run left its own daemon (BLADE_HOME bladebro-daemon-race-38048), which I stopped with `bladebro stop`. Run 2 (`--no-fail-fast`): exit 0. lib 628 (625 + 3 new tabs_marker_tests), cdp_mock 6, cli_endpoint 1, daemon_lifecycle 5, page_unit 14, record_hub 2 (+6 ignored), record_live 2 (+1 ignored), security_audit 10, stealth_behavior 2, xdg_home 3 |
| hub_live.sh full (EVIDENCE redirected to scratch) | 37 PASS, 1 FAIL. The FAIL is `[cdp-trace] could not build the base binary`, a harness defect: `ensure_base` runs a second time and its `rm -rf` + `git worktree add` fails on the worktree that daemon-off already registered (see Anti-Patterns). Rerun with `HUB_BASE_BIN=target/hub-base/target/release/bladebro --only cdp-trace`: failures=0, identical distinct method sets, no screencast, `Target.getTargetInfo` base=0 new=0 |
| record_hub live tests (`--ignored`, inside hub_live mcp) | 6/6 pass |
| `bladebro audit` (headless, via hub_live) | SCORE 100/100, VERDICT PASS |
| qol_probe | 120 passed, 2 failed. Both are the known pre-existing failures (chord cleared the live editor; nested scroller movement) |
| lane_matrix (lane_matrix_local.py) | daemon, oneshot, mcp, mcp-pipe 5/5 rounds each |
| MCP record-start/stop + ffprobe (own driver) | `recording saved ... (bytes 8792, duration 2.2 s, frames 22, dropped 0, segments [0.0 s tab DC0C...], captured 1)` + tab note. ffprobe vp9 1280x720, duration 2.200000, nb_read_frames 22 = reported, file 8792 B = reported, mode 0600, full decode exit 0 |
| Daemon timed run | duration 4.1 s, frames 41 (ffprobe 41 / 4.100000) vs start-returned..stop-called 4.119 s; start-call..stop-return 4.760 s (+0.66 s, docs: about 0.55 s) |
| Layer-1 gates (8g-8i) | Not applicable: the repo ships no scripts/fp-gate.ts, migration-security-gate.ts, cve-scan-gate.ts or check-p0-tier-*.ts, and no apps/api. No migrations |
| Cleanup | After `bladebro stop` per home and `pgrep -fl "bladebro|ffmpeg|cdp_tap|http.server"`: nothing left. I killed only my own fixture server (port 8791) and removed my three scratch homes. No harness worktree left (`git worktree list` shows only the main tree) |

## Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | record-start, browse, record-stop leaves a playable file and the result has abs path, bytes, duration, frames, dropped, segments (SC1, OUT-05, CTRL-04) | VERIFIED | Own MCP driver: `recording saved: /private/var/.../recordings/etc_evil_name_rm_-rf-...webm (bytes 39401, duration 4.6 s, frames 46, dropped 0, segments [0.0 s tab 9231...])`. ffprobe vp9 1280x720 4.600000 s, nb_read_frames 46 = reported, full decode exit 0 with no stderr, bytes = file size, dir 0700, file 0600; hostile name `../../etc/evil name;rm -rf` sanitized inside the recordings dir |
| 2 | Duration matches the start-returned..stop-called window | VERIFIED | 4.6 s video vs 4.559 s window (+0.9 %). start-call..stop-return 5.200 s (stop latency 0.587 s), consistent with the new "Recording time" docs. The 7.9 % "drift" in hub_live is that harness's CLI-wall measurement, not a product error |
| 3 | Second record-start errors naming the active file (SC2, CTRL-05) | VERIFIED | MCP: `recording already active: <same path> (started 0.0 s ago, receiving). record-stop it first.`, isError. CLI daemon: rc=1, same text. During a tab switch: same. Exactly one file |
| 4 | Five tools; schema delta per Phase 1 D-01 (SC2, CTRL-04) | VERIFIED | tools/list = act, see, state, run, vision. The diff is enum +2, one RECORDING line and the `name` suffix. Re-measured with the Phase 1 method (collapsed source chars/4) on b079fac vs HEAD: state ToolDef 1680 -> 1913 = +233 chars, all_tools 15546 -> 15779 = +233, about 58 tokens (D-01 planned 232 / 58). Description says "active tab at start", with no tab-following claim |
| 5 | Recording off: same CDP as before, no files, no ffmpeg (SC3, CTRL-01) | VERIFIED | cdp_tap trace one-shot base vs new: identical distinct method sets, no screencast. Chrome argv identical to base. MCP off: no `recordings` dir, no ffmpeg child, refusal names BLADE_RECORD and `bladebro stop`. Code: `try_handle` returns None on a string compare before any lock/env/I/O; `finish_best_effort` returns before initializing the hub |
| 6 | Honest refusals: off, none-active, pipe, one-shot, missing ffmpeg, bad name | VERIFIED | Live MCP: off, `no active recording; record-start <name> first.`, pipe (`BLADE_TRANSPORT=pipe has no debugging port...`), BLADE_FFMPEG bogus (`...unusable (not found); PATH was not searched...`), nothing discoverable (`ffmpeg not found ... brew install ffmpeg ... bladebro stop`, 0.5 s). CLI: missing name / extra args rc=2 usage; `--no-daemon` rc=1 "long-lived session", no recordings dir. No refusal created a dir or spawned ffmpeg |
| 7 | Refusals and errors never trigger dead-tab recovery or a restart | VERIFIED | `map_err` yields only Usage/Other (never Closed/Transport); `scrub` strips the four substrings serve.rs:409-412 matches; after every refusal the page was unchanged (same URL, no restart note) |
| 8 | A tab switch during recording does not corrupt hub state | VERIFIED | open-tab + switch-tab mid-recording: double start still refused, stop `recording saved` with the note `agent is now on tab ...; the video shows only tab ...`, decodes clean. A new start on tab B, then closing tab A, then stop: clean. No ffmpeg left |
| 9 | Exit-path finalize matches the docs claim | VERIFIED | Live: MCP stdin EOF -> file finalized (3.1 s, 31 frames, decodes). MCP SIGTERM -> finalized, WARN log `recording saved`. Daemon `bladebro stop` with active recording -> finalized (3.1 s, decodes). Docs now name the uncovered daemon idle/drift/Closed/crash-relaunch paths as Phase 9; code confirms those call `b.shutdown()` directly |
| 10 | Recording does not make existing state ops give false output | VERIFIED | Gap 1 closed. Live MCP (headless, scratch home, 127.0.0.1 fixtures): record-start on anim, `tabs` = one `*` (anim); open-tab b, `tabs` = one `*` (B) twice; after record-stop one `*` (B). Live daemon: start on A, `tabs` one `*` A; open-tab B one `*` B; switch-tab A one `*` A; switch-tab B one `*` B; `--json` text the same; after stop one `*`. Off mode (MCP, no BLADE_RECORD): same flow prints one `*`, no `recordings` dir. Off-mode CDP: cdp-trace one-shot base (80d0172) vs HEAD, `state tabs` included: identical method sets, `Target.getTargetInfo` 0 vs 0. Code: `own_target` returns at `HUB.get()?` (no hub init, no lock, no CDP) when nothing recorded |
| 11 | Docs accurate for every touched field | VERIFIED | Gap 2 closed. Row now says frames passed to the encoder after the fps cap, may exceed or be lower than `frames`, shown only when different. Code: report.rs prints `src.stats.forwarded` only when `!= enc.frames_written`; handler.rs increments `forwarded` only after the gate and a successful `send_frame`. Live reports agree (frames 22 / captured 1; frames 41 / captured 2; frames 63 / captured 8) |
| 12 | Upstream hook ledger complete and exact (SYNC-01) | VERIFIED | Re-run: `git diff b079fac..HEAD` touches exactly 9 files present in upstream/main: the previous 8 (unchanged sizes) plus src/state.rs +8/-1. Each has a Phase 7 HOOKS.md row; the new row names `StateOp::ListTabs`, `hub::own_target` + `hub::is_current`, "~6 lines" (actual +8/-1, one 8-line rustfmt block; immaterial understatement), commit 761910a. be1a3d6, 381e0c3, 761910a resolve. All 16 commits in b079fac..HEAD carry `Co-authored-by: Claude <claude@anthropic.com>`. Cargo.toml, Cargo.lock, npm/, CHANGELOG.md, .github/ untouched; no .DS_Store, media, secrets or build output committed |

**Score:** 12/12

## Required Artifacts

| Artifact | Status | Details |
|---|---|---|
| src/record/hub.rs (+env, live_env, machine, msg, report) | VERIFIED | Substantive state machine, wired from handle_state (MCP, daemon, one-shot, run/batch, act->state) |
| src/record/hub/*_tests.rs | VERIFIED | machine a-l2 scenarios over fake ffmpeg + FakeCdp (unix only), report outcome table, scrub, lane mapping, parse, schema budget; real assertions, none hollow |
| tests/record_hub.rs | VERIFIED (live parts #[ignore]) | 6 live MCP tests pass with `--ignored`; 2 always-on |
| tools/record_sidecar/scenarios/hub_live.sh, lib/cdp_tap.py, lib/lane_matrix_local.py | VERIFIED (harness warning) | Every product check green. A default full run gives one false FAIL from the `ensure_base` re-entry (see Anti-Patterns); with HUB_BASE_BIN, cdp-trace is green |
| docs/recording-and-launch-config.md | VERIFIED | `captured` row fixed in 761910a; matches report.rs:57-58 (`src.stats.forwarded`, counted in handler.rs `forward` after the FpsGate cap) |
| .planning/HOOKS.md | VERIFIED | see truth 12; new src/state.rs row (761910a) |
| src/record/hub.rs `own_target` / `is_current` + src/state.rs ListTabs hook | VERIFIED | own_target: `HUB.get()?` then `is_active()` before any CDP call; one `Target.getTargetInfo` on the agent's session only while recording. 3 unit tests in `tabs_marker_tests` |

## Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| MCP/daemon/run `state` | hub | `handle_state` first statement `try_handle` | WIRED (live on MCP, CLI daemon, run steps) |
| hub start | encoder + frame source | `Encoder::start`, `FrameSource::start(Endpoint::Port)` | WIRED (ffmpeg child observed, frames written) |
| MCP shutdown_browser / daemon post-loop | hub finalize | `finish_best_effort` | WIRED (EOF, SIGTERM, daemon stop proven) |
| one-shot CLI | refusal | `set_ephemeral` in run_connected | WIRED |
| tools/list | record ops | tools.rs enum/description | WIRED |
| `state tabs` (MCP, daemon, run/batch: shared `state::perform`) | hub `own_target` / `is_current` | src/state.rs ListTabs | WIRED (live on MCP and daemon) |

## Requirements Coverage

| Requirement | Status | Note |
|---|---|---|
| CTRL-01 | SATISFIED | truth 5 |
| CTRL-04 | SATISFIED | truths 1, 4 |
| CTRL-05 | SATISFIED | truth 3 |
| OUT-05 | SATISFIED | all fields present and true; `captured` doc now matches the code |

## Honesty / safety review (SAFE-04)

- Ok only for clean capture + clean encoder; everything else is an Err that still carries path/bytes/frames; "saved" never appears in an Err (report.rs, report_tests).
- Start failure after ffmpeg spawned: the encoder is taken back, finished, and the empty file removed. A `take_encoder` failure is reported, not panicked.
- Cancelled stop: `stop()` moves Active out before awaiting. No surface wraps tool calls in a timeout or cancel (MCP uses only `catch_unwind`; daemon and run dispatch run inline in the select arm), so this is not reachable today. If it ever is, Drop aborts the source and `kill_on_drop` kills ffmpeg. The hub ends Idle, not wedged, but that report is lost and the file is unfinalized.
- A panic inside the hub leaves the tokio mutex usable (no poisoning) and the Active is dropped as above.

## Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| src/record/hub.rs `own_target` | 79-85 | If `Target.getTargetInfo` fails, `.ok()?` returns None and the marker falls back to `attached` (the old double `*`) | Info | Degrades only on a CDP error during an active recording; no false claim beyond the pre-fix behavior |
| src/record/hub.rs `tabs_marker_tests` | - | The regression test covers the pure `is_current`, not the ListTabs call site or `own_target`. The 07-VERIFICATION missing list also asked for a hub_live.sh / live tabs step, which was not added | Warning (non-blocking) | A future edit that drops the call in ListTabs would pass CI. Live behavior is proven by this pass in both lanes. Recommended: one `tabs` assertion in the ignored live test in tests/record_hub.rs |
| tools/record_sidecar/scenarios/hub_live.sh `ensure_base` | 177-185 | Called by both daemon-off and cdp-trace; the second call deletes the registered worktree dir and `git worktree add` fails | Warning (harness only) | A default full run reports one false FAIL. Fix: return early when BASE_BIN is already set. The product check passes when run with HUB_BASE_BIN |
| .planning/HOOKS.md src/state.rs row | 38 | "~6 lines" vs actual +8/-1 | Info | Approximate size, the symbol and commit are exact |
| src/record/hub.rs `try_handle` | - | record-stop silently ignores extra args over MCP | Info | carried over; harmless |
| - | - | logical_cohesion | none | the tabs hook is two small fns in hub.rs plus a call in ListTabs |

## Residual risks (not blocking Phase 7)

- Daemon idle-timeout, drift relaunch, Closed relaunch and crash relaunch do not finalize. This is documented and owned by Phase 9.
- The live tests stay `#[ignore]` (they need Chrome + ffmpeg), so CI covers the unit and fake-CDP layers only. The tabs fix is proven live only by this pass (see the Warning above).
- `machine_tests` are unix-only. Linux/Windows CI was not run here.
- Chrome 154 on this host vs the audited 151. 100/100 is the macOS headless bar; 111/111 and diff_oracle were not run.
- The roadmap SC4 literal says 122/122. It is 120/122 here with the same two pre-existing failures (accepted baseline).
- `daemon_lifecycle::concurrent_cold_starts_have_one_owner` flakes and leaks a daemon when it fails. This predates Phase 7 and the file is untouched by it.

## Not verified

- Linux/Windows builds and CI; headed mode; diff_oracle; real-browser lane live.
- `tabs` during recording via a run/batch step (it shares `state::perform` with the verified paths, so it is covered by the code argument only).

## Gaps Summary

No blocking gaps remain. Both 13:15 gaps are closed and were re-proven live. `state tabs` shows exactly one `*` while recording in the MCP and daemon lanes, including on the recorded tab itself. Off mode is unchanged: one `*`, no hub init, and no extra CDP call (the cdp-trace shows getTargetInfo 0 vs 0). The `captured` doc row matches the code. The ledger is exact for 9 upstream files, all commits carry the trailer, and nothing forbidden was committed. fmt, clippy, tests (after the known flake rerun), audit 100/100, qol 120/122 (known two), lanes and the MCP record-start/stop + ffprobe regression are green. Two non-blocking warnings: the regression test for the tabs fix is unit-level only, and hub_live.sh's `ensure_base` breaks on a full run.

---

_Verified: 2026-10-22T13:26:00Z_
_Verifier: Claude (gsd-verifier, re-verification after 07-04)_
