---
phase: 12-full-instrument-gate
verified: 2026-10-27T00:00:00Z
status: gaps_found
score: 2/4 criteria met (1 and 4 on macOS only); 2 partial, 3 not met
gaps:
  - truth: "clippy and cargo test pass on ubuntu, macos and windows CI"
    status: partial
    failure_type: missing_test
    reason: "Only local macOS run (903/0/18, clippy clean). Branch never pushed; ubuntu/windows CI not run; cfg(windows) tests never compiled."
  - truth: "With recording on: audit 111/111, diff_oracle divergent 0, qol_probe 122/122, lane_matrix all 5, live sites pass"
    status: failed
    failure_type: missing_test
    reason: "audit 100/100 macOS only (111 is Linux); diff_oracle not run; qol 120/122 (off and on); lane_matrix.py not run (local substitute, 4 lanes, no rb); incolumitas WEBDRIVER fail and creepjs headless 33% (headless, off==on)."
---
# Phase 12 Verification (sonnet)

Verdict: **gaps_found**. The SUMMARY is honest; the gaps are genuine unmet targets, not overclaims.

## Per-criterion
| # | Criterion | Verdict | Basis |
|---|---|---|---|
| 1 | VER-03 tests | MET (macOS); Windows L7 CI pending | Independently grepped: representative test fns exist once each in src (two_mb_screencast_frame_passes_intact, exact_webm_argv, sanitize_stem_table, success_finalizes_and_reaps, format_precedence_each_layer_wins_in_turn, no_frames_is_an_error_with_the_encoder_verdict, w1_drains_stdin_and_writes_output). Map: 30 ok, 0 missing, L7 labelled CI pending. Full run had one flake (concurrent_cold_starts_have_one_owner), passed on rerun; SUMMARY discloses it and that failed=1 remains in the totals line. |
| 2 | clippy/test on ubuntu+macos+windows CI | PARTIAL | Judged partial: local macOS green only; the criterion literally requires CI on three OSes, none ran. Cannot be met on this host. |
| 3 | Instrument gate with recording on | NOT MET | Judged not met rather than partial: besides unmeasured items (diff_oracle, Linux 111, lane_matrix.py, rb), there are measured misses (qol 120/122, evidence shows identical failing checks with recording off and on; incolumitas WEBDRIVER; creepjs headless 33%). Before/after numbers are recorded, and off==on for audit, live sites, qol. Audit recorder-attached run lasted ~1 s only (disclosed). |
| 4 | Additive diff, trial merge, merge procedure | MET (with caveat) | Re-ran: all 22 modified files vs upstream/main are in .planning/HOOKS.md (0 unlisted); `git diff f856c93..HEAD -- src` is empty (no src change in phase 12); trial_merge.sh --self-test: 3 PASS, 0 FAIL; docs/upstream-merge-procedure.md exists. Caveats: upstream/main == merge-base (6b524d2), so the merge was "Already up to date" and exercised no real conflicts (disclosed). Additions also include a src/launch_mode/ module and tests/ files, not only src/record/ (additive, but beyond the roadmap wording). |

## Honesty checks
- SUMMARY does not claim Windows/Linux CI, diff_oracle, 122/122, Linux 111, or live-site targets; each is listed under Not verified/Failed. Evidence text (instruments, audit, live-sites, sync) matches the SUMMARY numbers.
- Minor: SUMMARY says model per task unattested for tasks 1-11; the 12-01 summary says Haiku. Not material. Criterion 3 label ("not met") differs from the plan's expected "partly met"; SUMMARY flags it for coordinator.
- Nothing pushed/installed: origin/main = 94d7c44 (no local branch refs under origin besides main/HEAD), no extra worktrees, git worktree list shows only the main one, no new branches beyond gsd/browser-recording and main. Tree has only pre-existing untracked files.

## Recommended next steps (no fixes done)
Push/PR to run ubuntu+windows CI; run diff_oracle with websockets, Linux audit/lane_matrix/qol, headed live sites; investigate 2 qol failures and headless incolumitas/creepjs against a baseline.
