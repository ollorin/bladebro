---
phase: 12-full-instrument-gate
verified: 2026-10-27T12:00:00Z
status: gaps_found
score: 2/4 success criteria verified (1 and 4 met; 2 partly met; 3 not met)
re_verification:
  previous_status: gaps_found   # 12-VERIFICATION-sonnet.md (first verifier, same HEAD)
  previous_score: "2/4"
  gaps_closed: []
  gaps_remaining: [criterion 2, criterion 3]
  regressions: []
gaps:
  - truth: "SC2: clippy clean and cargo test pass on ubuntu, macos and windows CI"
    status: failed
    failure_type: missing_test
    reason: "Only the local macOS run happened. The branch was never pushed, so no CI ran. src/record/encoder/windows_tests.rs (cfg(all(test, windows)), 4 tests) has never been compiled. The cause is environmental: pushing is the user's decision."
    artifacts:
      - path: ".planning/phases/12-full-instrument-gate/evidence/12-02-not-run.md"
        issue: "CI commands documented, not executed"
    missing:
      - "User: push the branch and run ci.yml with workflow_dispatch, or open a PR into main. ubuntu, macos and windows must all be green."
  - truth: "SC3: with recording on, diff_oracle divergent 0"
    status: failed
    failure_type: missing_test
    reason: "Not run. The python websockets module is missing (I confirmed ModuleNotFoundError), and installs are forbidden. Nothing is claimed. The cause is environmental."
    artifacts:
      - path: "tools/diff_oracle/oracle.py"
        issue: "never executed in this phase"
    missing:
      - "User: install websockets, then run oracle.py with recording off and with an active recording (and --lane real). divergent must be 0."
  - truth: "SC3: with recording on, audit 111/111 and tools/lane_matrix.py all 5 green"
    status: failed
    failure_type: missing_test
    reason: "macOS headless audit scored 100/100 off and on. 111/111 is the Linux headed Xvfb total and was not run. lane_matrix.py needs /proc, so a local ps-based substitute ran 4 lanes; the rb lane was not run. In the substitute, 'on' only means BLADE_RECORD=1 was set; no capture was active. The audit with the recorder attached had the capture alive for only about 1 s (ConnectionLost after_ms 1001, 5 frames)."
    artifacts:
      - path: ".planning/phases/12-full-instrument-gate/evidence/12-02-audit-recording.txt"
        issue: "recorder attached for about 1 s of the audit, not for the whole audit"
      - path: ".planning/phases/12-full-instrument-gate/evidence/12-02-instruments.txt"
        issue: "lane 'on' = env switch only, no record-start"
    missing:
      - "On Linux: bladebro audit 111/111 and lane_matrix.py all 5 green (including the real lane), each with an active record-start capture running"
  - truth: "SC3: with recording on, qol_probe 122/122"
    status: failed
    failure_type: regression
    reason: "Measured at 120/122 with recording off and on. The failures are 'chord cleared the live editor' and 'nested scroller movement is reported as such'. They are pre-existing: the Phase 04.1 verifier measured upstream base 66eaaa0 at 120/122 with the same two failures. So this is not a recording regression, but the criterion as written is a measured miss and not a missing measurement. In the 'on' run qol had BLADE_RECORD=1 set but no active capture."
    artifacts:
      - path: ".planning/phases/12-full-instrument-gate/evidence/12-02-instruments.txt"
        issue: "120 passed, 2 failed (both modes)"
    missing:
      - "Either 122/122 on the reference host (Linux), or a ROADMAP amendment by the owner that accepts the two pre-existing upstream failures as the baseline"
      - "A qol run while a capture is active (not only the env switch)"
  - truth: "SC3: live sites pass with recording on (incolumitas no bot detection, creepjs headless 0%)"
    status: failed
    failure_type: missing_test
    reason: "Headless only. incolumitas WEBDRIVER fail (35/36) and creepjs headless 33% / like-headless 38%, identical with recording off and on, so recording is not the cause. sannysoft 31/0 and creepjs stealth 0% are met. The AGENTS.md targets are defined for a headed browser, and the headed run was not done. Before/after numbers are recorded and match the raw evidence."
    artifacts:
      - path: ".planning/phases/12-full-instrument-gate/evidence/12-02-live-sites.txt"
        issue: "headless misses vs targets; headed not run"
    missing:
      - "User: headed live_sites.py --mode off and --mode on. incolumitas has no fail and creepjs is headless 0% / stealth 0%, with off == on"
---

# Phase 12: Full Instrument Gate Verification Report

**Phase goal:** The milestone is proven ready to merge into the fork's main: complete tests, green on all platforms, stealth unchanged with recording on, and upstream updates still merge cleanly.
**Verified:** 2026-10-27 (HEAD a25fdbb; the code under test is e4648a9, and a25fdbb only adds 15 docs/evidence files)
**Status:** gaps_found
**Re-verification:** Independent second verification. The first verifier's sonnet report was not trusted; every load-bearing claim below was re-checked.

## Success criteria

| # | Criterion | Status | Evidence (independently checked) |
|---|---|---|---|
| 1 | Tests for config precedence, name sanitization, ffmpeg args, verdict strings, fake-encoder lifecycle, 2 MB screencastFrame through shrink_oversized_event | VERIFIED (macOS) | I grepped 10 test names from 12-01-ver03-map.md (C3, N3, F1, F4, V3, L3, S1, S2, S4, L7) and each exists as `fn` in the mapped file. S1 asserts `text.len() >= 2_000_000`. My `cargo test --release` run: 903 passed / 0 failed / 18 ignored, rc 0. The Windows .cmd fakes (L7) are gated `cfg(all(test, windows))`. They are written but never compiled, and that belongs to SC2. |
| 2 | clippy clean + cargo test pass on ubuntu, macos, windows CI | FAILED (partly met) | Locally: fmt rc 0, `cargo clippy --release -- -D warnings` clean, tests 903/0/18. No CI ran on any OS, and the branch has no remote ref (`git branch -r` shows nothing for it). |
| 3 | Recording on: audit 111/111, diff_oracle 0, qol 122/122, lane_matrix all 5 green, live sites pass with before/after | FAILED | The SUMMARY numbers match the raw evidence exactly: audit 100/100 off, idle-on and attached; qol 120/122 both modes; 4 substitute lanes exit 0 both modes; sannysoft 31/0/0, incolumitas 35/1 WEBDRIVER, creepjs 38/33/0, all identical off and on. diff_oracle was not run (websockets import fails here; I confirmed it). See the gaps. |
| 4 | Additive diff + listed hooks; scripted trial merge conflict-free; gate green on merged tree; merge procedure documented | VERIFIED (trivially) | `git diff --name-status upstream/main..HEAD`: 460 A, 22 M, 0 D/R. All 22 M files are named in .planning/HOOKS.md (I checked each one). upstream/main = merge-base = 6b524d2, and .git/FETCH_HEAD (fetched today 10:35) shows upstream `main` at 6b524d2. So "current upstream/main" really is the merge-base, and the merge correctly reports "Already up to date". I ran `trial_merge.sh --self-test`: clean, hook and unlisted cases all PASS, 0 FAIL. docs/upstream-merge-procedure.md exists. The sync evidence records the gate on the merged worktree as green. |

**Score:** 2/4 criteria verified.

## Honesty checks on the SUMMARY

- The recording on/off numbers in 12-02-SUMMARY.md match 12-02-live-sites.txt, 12-02-instruments.txt, 12-02-audit*.txt and 12-02-gate.txt line for line.
- The evidence_live known noise is labelled honestly. The strict `[off] per-method counts equal` check FAILs in all three runs, and these are kept as FAIL lines. The tolerant variant excludes only `Input.dispatchMouseEvent`, and the SUMMARY calls the result "known noise, not a clean pass". The rerun beyond the stop rule is disclosed.
- The trivial merge, the 1 s recorder attach and the headless-only live sites are all disclosed as caveats.
- One thing is under-disclosed. In the SUMMARY, "On" for qol_probe and lane_matrix_local means `BLADE_RECORD=1` was set and no capture was running. The instruments file says this in its section header ("no active capture"), but the SUMMARY table reads as if recording were active. Only the live-site runs (109/209/255 frames) and the ~1 s audit attach had a real capture.
- Nothing was pushed or installed: there is no remote branch, websockets is still absent, no trial-merge branch or worktree is left over, the tracked tree is clean, and no bladebro daemon/MCP was left running.

## Required artifacts

| Artifact | Status |
|---|---|
| evidence/12-01-ver03-map.md (31 rows, 30 ok, L7 CI pending) | VERIFIED |
| tools/record_sidecar/lib/trial_merge.sh (+ self-test) | VERIFIED (self-test run here) |
| .planning/HOOKS.md (covers all 22 touched upstream files) | VERIFIED |
| docs/upstream-merge-procedure.md | EXISTS |
| evidence/12-02-*.txt, 12-02-not-run.md | VERIFIED vs SUMMARY |

## Gates not applicable on this repo

- Layer-1 gates (Step 8g/8h): the repo ships no scripts/fp-gate.ts, migration-security-gate.ts or cve-scan-gate.ts (scripts/ holds only publish-npm.sh), and there is no apps/api.
- Step 8i: there is no check-p0-tier-delta.ts.
- Charlotte QA and E2E: this is a Rust CLI/MCP binary, not a web app.
- Migrations: none.
- Test-file coverage: this phase's only new code is trial_merge.sh, which has a self-test. 12-01 added no implementation.

## Anti-patterns

| File | Pattern | Severity | Impact |
|---|---|---|---|
| 12-02-SUMMARY.md results table | qol/lane "On" columns do not say that no capture was active | Warning | It overstates "with recording on" coverage for those two instruments |
| CI | Deferral of CI to the user ("CI pending") | Blocker for SC2 | Under QGATE-12 any deferral of verification fails the criterion. It is environmental here, but it is still unmet. |

## Status rationale and what would flip it

`passed` is not possible. SC2 ("green on ubuntu, macos and windows CI") and SC3 are not met, and the phase definition gives no environmental waiver: the goal says "green on all platforms". `human_needed` is also not valid, because an automated check fails: qol_probe reports 2 failed checks, and incolumitas and creepjs-headless are measured misses, not missing measurements. So the status is **gaps_found**.

Most of the remaining gaps are environmental or user actions, not code defects. Recording made no measured difference anywhere. The status flips to **passed** when all of the following hold:
1. CI ci.yml is green on ubuntu, macos and windows for gsd/browser-recording. The user pushes, then uses workflow_dispatch or a PR. This includes compiling the 4 windows_tests.
2. `pip install websockets` is done, and diff_oracle reports divergent 0 with recording off and with an active capture (plus `--lane real` at 0/0).
3. On Linux: audit 111/111 and `tools/lane_matrix.py all 5` green (including the rb lane), with an active record-start capture during the run.
4. qol_probe reaches 122/122 on the reference host, or the owner amends SC3 to accept the two pre-existing upstream failures. The Phase 04.1 verifier measured them at upstream base 66eaaa0, and they are identical off and on.
5. Headed live sites: incolumitas has no fail and creepjs is headless 0% / stealth 0%, with off == on. Or the owner amends SC3 to a "no off-vs-on delta" bar on this host.

If the owner accepts items 4 and 5 as amendments and only items 1 to 3 remain, the correct status is still gaps_found until those runs exist. They are verification runs, not judgement calls, so human_needed would not be correct for them either.

---
_Verified: 2026-10-27_
_Verifier: Claude (gsd-verifier, opus, adversarial second pass)_
