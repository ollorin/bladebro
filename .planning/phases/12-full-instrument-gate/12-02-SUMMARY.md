# Plan 12-02 Summary: full instrument gate, recording off vs on, trial merge

**Phase:** 12-full-instrument-gate
**Requirements:** VER-04 (instrument gate with recording on), SYNC-01 (additive diff, scripted trial merge, merge procedure)
**Branch:** gsd/browser-recording, code under test at e4648a9 (plus this docs commit)
**Build:** bladebro v4.6.5 (e4648a9-dirty). The tracked tree has no modifications; the `-dirty` flag reflects the build-time working tree.
**Host:** macOS 26.7.1 arm64, Google Chrome 156.0.8078.48 (the plan template said 154; the installed version is 156)
**Date:** 2026-10-27

**Result:** The macOS gate is green for everything that ran (fmt, clippy both forms, 903 tests off and on, audit 100/100 off and on, the trial merge and additive check, scenario suites, live sites parity). Four things block a clean "gate passed":
1. diff_oracle was not run (python `websockets` missing; installs forbidden). Divergent 0 is not claimed.
2. qol_probe is 120/122 in both off and on (2 checks fail).
3. The live headless targets are missed in both modes: incolumitas WEBDRIVER FAIL, creepjs headless 33%.
4. ubuntu and windows CI were not run (push is the user's decision).

Evidence index: `.planning/phases/12-full-instrument-gate/evidence/12-02-*.txt`. Not-run record and CI commands: `.planning/phases/12-full-instrument-gate/evidence/12-02-not-run.md`.

## Results (plan table: Check | Off | On | Status | Evidence)

| Check | Off | On | Status | Evidence |
|---|---|---|---|---|
| fmt (`cargo fmt --check`) | clean | — | verified | 12-02-gate.txt |
| clippy `--all-targets -D warnings` | exit 0 | — | verified | 12-02-gate.txt |
| clippy CI form `-- -D warnings` | exit 0 | — | verified | 12-02-gate.txt |
| cargo test (macOS) | 903 passed, 0 failed, 18 ignored | 903 passed, 0 failed, 18 ignored (`BLADE_RECORD=1`) | verified | 12-02-gate.txt |
| trial merge + additive (SYNC-01) | additive: new=445, touched=22, unlisted_touched=0; merge "Already up to date", RESULT clean | — | verified with caveat (merge was trivial, see below) | 12-02-sync.txt, 12-02-selftest.txt |
| gate on merged tree | fmt exit 0, clippy exit 0, test 903/0/18 exit 0, audit 100/100 exit 0 | — | verified (merged worktree at e4648a9) | 12-02-sync.txt |
| audit (`bladebro audit`, macOS headless) | 100/100 PASS | idle-on 100/100 PASS; recorder attached 100/100 PASS (recorder attach lasted ~1 s, see caveats) | verified with caveat (Linux 111/111 not run) | 12-02-audit.txt, 12-02-audit-recording.txt |
| qol_probe (target 122/122) | 120 passed, 2 failed | 120 passed, 2 failed | FAILED (2 checks, identical off and on) | 12-02-instruments.txt |
| lane_matrix_local daemon (5 rounds) | exit 0 | exit 0 | verified (local substitute, `ps` not `/proc`) | 12-02-instruments.txt |
| lane_matrix_local oneshot (5 rounds) | exit 0 | exit 0 | verified (local substitute) | 12-02-instruments.txt |
| lane_matrix_local mcp (5 rounds) | exit 0 | exit 0 | verified (local substitute) | 12-02-instruments.txt |
| lane_matrix_local mcp-pipe (5 rounds) | exit 0 | exit 0 | verified (local substitute) | 12-02-instruments.txt |
| short_case_live | — | 19 PASS, 0 FAIL, exit 0 | verified | 12-02-scen-short.txt |
| exit_live | — | failures=0, exit 0 | verified | 12-02-scen-exit.txt |
| follow_live | off: failures=0 (no sidecar, CDP trace identical to base); on: close and per-tab-close failures=0 | — | verified | 12-02-scen-follow.txt |
| evidence_live (10-01 and 10-02 ids) | strict per-method count check FAILS in 3 runs; tolerant check passes | all on-side checks PASS | verified with caveat: strict check fails, tolerant passes | 12-02-scen-evidence-a.txt, 12-02-scen-evidence-b.txt, 12-02-scen-evidence-off-rerun.txt |
| sannysoft | passed=31, warn=0, failed=0 | passed=31, warn=0, failed=0 (equal) | verified (all pass) | 12-02-live-sites.txt |
| incolumitas | ok=35, fail=1 (WEBDRIVER) | ok=35, fail=1 (WEBDRIVER) (equal) | FAILED vs target (no bot detection); off == on verified | 12-02-live-sites.txt |
| creepjs | like headless 38%, headless 33%, stealth 0% | same (equal) | FAILED vs target (headless 0%); stealth 0% met; off == on verified | 12-02-live-sites.txt |

## Verified

| Check | Result | Evidence |
|---|---|---|
| fmt, clippy (both forms) | clean, exit 0 | 12-02-gate.txt |
| cargo test, recording off and on | 903 passed, 0 failed, 18 ignored, both runs | 12-02-gate.txt |
| Gate on the merged worktree | fmt, clippy, test (903/0/18), audit (100/100) all exit 0 | 12-02-sync.txt |
| Additive diff vs upstream/main | 445 new files, 22 touched files, all 22 listed in HOOKS.md, 0 unlisted | 12-02-sync.txt |
| Trial-merge tooling self-test | clean, hook-only, and unlisted cases all pass | 12-02-selftest.txt |
| audit, off and idle-on | 100/100 PASS both | 12-02-audit.txt |
| lane_matrix_local, 4 lanes, off and on | exit 0 each, 5 rounds each | 12-02-instruments.txt |
| short_case_live | 19 PASS, 0 FAIL | 12-02-scen-short.txt |
| exit_live | failures=0 | 12-02-scen-exit.txt |
| follow_live (close, per-tab-close, off) | failures=0 in all three sections | 12-02-scen-follow.txt |
| sannysoft, off and on | all checks pass, identical numbers | 12-02-live-sites.txt |
| evidence_live, on side | all on-side checks PASS in all runs | 12-02-scen-evidence-a.txt, 12-02-scen-evidence-b.txt |

## Verified with caveat

| Check | Caveat |
|---|---|
| Trial merge of upstream/main | Merge was trivial: upstream/main is still the merge-base (6b524d2), so git reported "Already up to date". The conflict paths are covered only by the self-test (clean, hook-only, unlisted), not by a real upstream merge. |
| audit with recorder attached | The recorder was attached for about 1 s and closed with `ConnectionLost { after_ms: 1001 }` after 5 frames (`12-02-audit-recording.txt`). It is not a multi-minute attached audit. The audit itself still scored 100/100. |
| audit 100/100 | This is the macOS headless number. The 111/111 target is the Linux (headed, Xvfb) total and was not run. |
| lane_matrix_local (4 lanes) | Local substitute for `tools/lane_matrix.py all 5`, which needs `/proc`. It ran with a `ps` sweep. The real-browser lane was not run. |
| evidence_live, off per-method counts | The strict per-method check fails in all 3 runs (2 FAIL lines per run, `off` section, base vs new). The cause is `Input.dispatchMouseEvent` (random mouse path). The off-rerun probe measured base 29/31 and new 31/27 for that method between runs of the same binary. The tolerant check, which excludes that method, passes in every run. Recorded as known noise, not a clean pass. The scenario was not edited. Three runs happened, one beyond the stop rule (see Deviations). |

## Failed (measured, not passed)

| Check | Result | Why it matters |
|---|---|---|
| qol_probe | 120 passed, 2 failed, in both off and on. Failing checks: "chord cleared the live editor" (readback `via wrappe`) and "nested scroller movement is reported as such" (outcome scrolled page delta+1552 off, +1912 on). | The target is 122/122. The two failures are identical with recording off and on, so recording does not cause them. The pre-recording status was not re-measured in this plan. Both are listed as known failures. |
| incolumitas, headless | WEBDRIVER FAIL (1 of 36 checks), both modes, identical. | AGENTS.md target is "no bot detection". Missed headless. Headed not run. Recording does not change the value. |
| creepjs, headless | headless 33%, like headless 38% (target 0%); stealth 0% (met). Both modes, identical. | AGENTS.md target is "headless 0% / stealth 0%". Headless missed. Headed not run. Recording does not change the value. |

## Not verified

| Check | Reason | Command (in `evidence/12-02-not-run.md`) |
|---|---|---|
| diff_oracle (`divergent: 0`) | Python `websockets` is not installed, and installs are forbidden. Nothing is claimed. | `python3 -m pip install websockets` then `python3 tools/diff_oracle/oracle.py`, and `--lane real` |
| ubuntu CI (fmt, clippy, test, build) | Branch never pushed; push is the user's decision. | `gh workflow run ci.yml --repo ollorin/bladebro --ref gsd/browser-recording` after the user pushes |
| windows CI, incl. `src/record/encoder/windows_tests.rs` (cfg(windows), 4 tests) | Never compiled here. | same workflow run |
| macOS CI (remote runner) | Only the local macOS run was done. | same workflow run |
| Linux audit 111/111 | Linux (headed, Xvfb) only. | `cargo build --release && ./target/release/bladebro audit` on Linux |
| tools/lane_matrix.py all 5 | Needs `/proc`; local substitute only. | `python3 tools/lane_matrix.py all 5` on Linux |
| real-browser lane (`rb`) | Not run. | `python3 tools/rb_live/attach_drift.py` and the rb lane instruments |
| Live sites, headed (AGENTS.md targets) | Only headless ran. | `python3 -I tools/record_sidecar/lib/live_sites.py --mode off --headed --out <file>` and `--mode on` |

## ROADMAP Phase 12 success criteria

| # | Criterion | Status | Reason |
|---|---|---|---|
| 1 | Tests for config, names, ffmpeg args, verdicts, fake encoder, 2 MB frame | met on macOS | 12-01: 30 rows ok, 0 MISSING, 0 FAILED; L7 Windows row CI pending. (Not part of this plan's criterion list, noted for completeness.) |
| 2 | clippy clean and `cargo test` passes on ubuntu, macos and windows CI | partly met | clippy and tests green locally on macOS (exit 0; 903/0/18). ubuntu and windows CI not run. |
| 3 | Recording on: audit 111/111, diff_oracle divergent 0, qol_probe 122/122, lane_matrix green, live sites pass with before/after | not met | diff_oracle not run; audit 100/100 on macOS (111 is Linux-only, not run); qol_probe 120/122 in both modes; lane_matrix run only as a local substitute; incolumitas WEBDRIVER fail and creepjs headless 33% in both modes. Before/after numbers were recorded. The plan's expected label was "partly met"; the executor scored "not met" because the stated qol 122/122 and live-site targets are measured misses, not just missing measurements. The coordinator should confirm this label. |
| 4 | Additive diff; trial merge conflict-free; gate green on merged tree; merge procedure documented | met on macOS | Additive: 445 new, 22 touched, all listed, 0 unlisted. Trial merge clean but trivial ("Already up to date", see caveats). Gate green on the merged worktree (903/0/18, audit 100/100). Procedure in `docs/upstream-merge-procedure.md`. |

## Deviations, reruns, flaky tests, models

- **Task 10 rerun beyond the stop rule.** evidence_live ran three times (`12-02-scen-evidence-a.txt`, `12-02-scen-evidence-b.txt`, `12-02-scen-evidence-off-rerun.txt`). The third run went beyond the stop rule. All three are kept as evidence; none was deleted. The strict `off` per-method check fails in all three. The scenario was not edited. The tolerant check passes in all three. This is the Phase 10-02 precedent (`10-02-gate.txt` line 57).
- **Task 6 recorder attach was ~1 s.** The "recorder attached" audit did not keep the recorder attached for minutes. It closed with `ConnectionLost` after 1 s (see caveats). The plan's substitute wording in `12-02-not-run.md` was kept, with an added caveat section.
- **Task 4 trial merge was trivial.** upstream/main equals the merge-base, so the merge had nothing to merge. Conflict handling is covered only by the self-test.
- **Flaky test.** `concurrent_cold_starts_have_one_owner` failed once in 12-01's full run and passed on re-run (see `12-01-SUMMARY.md`). Both full test runs in this plan passed (903/0/18), so no re-run was needed here.
- **12-01 arithmetic.** 12-01 planned 32 rows and 31 ok. The actual count is 31 rows and 30 ok (30 machine-checked rows plus L7, CI pending). No row is MISSING. The plan's own grep for `= 32` will not match.
- **Chrome version.** 156.0.8078.48 (the plan template said 154). Tests do not depend on the Chrome version.
- **Live headless numbers miss AGENTS.md targets.** incolumitas WEBDRIVER fail and creepjs headless 33% appear identically with recording off and on, so recording does not change them. Headed was not run.
- **Not-run record.** `12-02-not-run.md` keeps the plan text, with a short caveats section added at the end. The plan's "recorder attached" substitute line needed that caveat.
- **Model per task.** Tasks 1-11: the evidence files do not record the model used for each task, so none is attested here. Task 12: claude-haiku-5-5 (this executor).
- **Not done (by instruction).** No push, PR, install, or CI trigger. ROADMAP.md, REQUIREMENTS.md and STATE.md were not marked. EXECUTION_LOG.md, CHECKPOINT.json and other untracked files were left untracked.
