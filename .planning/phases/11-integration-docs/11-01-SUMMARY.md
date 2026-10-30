# 11-01 Summary: recording and launch doc page, every snippet run against fixtures

**Status:** complete (8/8 tasks). Executor: Haiku for every task (user policy). No task escalated to sonnet.
Tasks 1-5 ran in an earlier session; Tasks 6-8 in this one.

## Commits
- 9d20e81 fix(record): say 1 action, and one period before an idle marker note (Tasks 1-2)
- 75fd79c chore(11-01): add doc-snippet helpers and evidence scripts for the recording page (Task 3)
- 5a83604 docs(11-01): evidence from the MCP, CLI and tabs doc-snippet runs (Task 4)
- b928e52 docs(11-01): evidence from the project-file, launch-mode, DSH and env-error doc-snippet runs (Task 5)
- 3f08c61 docs(11-01): run every recording/launch doc snippet against fixtures; per-call params, known issues, not verified (Tasks 6-8)
- this file: docs(11-01): plan summary

## What changed
- `src/record/hub/msg.rs` `join_note`, used by `finish.rs` `stop_marked` (both no-active-recording paths): one period between an idle stop text and its marker note.
- `src/record/hub/evidence.rs` `actions_word`: `(1 action)`, `(N actions)`, `, K not kept` unchanged. Tests in `evidence_tests.rs`.
- `docs/recording-and-launch-config.md`:
  - Task 6: the stop-result sample is the real `ev-` run (1 action, marker at 3.7 s); the `timeline` row says `(N actions; 1 action for one)`; the no-recording marker row gives the full text; the timeline JSON sample is the real `ev-` file; the window-feature popup paragraph matches the s03 measurement; the DSH example is a JSON block with `binaryPath`; intro sentence on the run coverage.
  - Task 7: `## Per-call parameters` (before Project file), `### 5. Per-test recording from a shell (CLI daemon)` (its bash block is `evidence/snippets/per-test-cli.sh`, its stop line from the 11-01 run), `## Known issues` (4 items), `## Not verified`, and six troubleshooting bullets.
- Helpers copied (Task 3): `tools/record_sidecar/scenarios/mcp_drive.py`, `dsh_stub.mjs`, `doc_snippets.py`; one line in `tools/record_sidecar/README.md`.
- Evidence: `evidence/scripts/` (common.sh, s01-s07, q01, q02), `evidence/snippets/`, `evidence/11-01-s01..s07*.txt`, `evidence/snippet-map.tsv` (20 rows), `evidence/11-01-gate.txt`.
- `.planning/phases/11-integration-docs/deferred-items.md`: switch-tab-on-opener (deferred, upstream area), the carried-over click-verdict and in-run timeline items (documented), and the two Phase 10 cosmetic fixes (fixed).
- No upstream file, no stealth file, no README change. `git diff 42ca29b..HEAD --name-only -- src` lists only `src/record/hub/{msg,finish,evidence,evidence_tests}.rs`.

## Snippet check
`python3 -I tools/record_sidecar/scenarios/doc_snippets.py check .planning/phases/11-integration-docs/evidence/snippet-map.tsv` exit 0: `20 blocks, 0 failing`.
- 12 run/config rows: block text verbatim in the named evidence file next to `SNIPPET <label>`.
- 8 output rows: `SNIPPET <label>` checked only. Four of them keep sample text from earlier phases (start-result, stop-result, follow-result, per-tab-result); the named Phase 11 run printed a line of the same shape. The per-tab sample shows 3 files (Phase 10); the Phase 11 run printed 2. Stated in the map header and in the page's Not verified section.

## Gates (evidence/11-01-gate.txt)
- `cargo fmt --check`: clean.
- `cargo clippy --release --all-targets -- -D warnings`: clean.
- `cargo test --release --no-fail-fast`, first full run: 892 passed, 1 failed, 18 ignored. The failure is `concurrent_cold_starts_have_one_owner` (the known flaky race). A daemon from that test's scratch home (`bladebro-daemon-race-98433`) was still running; stopped with `BLADE_HOME=<that home> bladebro stop`.
- Rerun alone: `concurrent_cold_starts_have_one_owner` ok. Totals with the rerun: 893 passed, 0 failed, 18 ignored (the plan's expected 893).
- Drift tests (`launch_mode::registry`, `record::hub::schema_tests`): 14 passed, 0 failed (run in Task 6).
- `BLADE_HEADLESS=1` audit, scratch home: SCORE 100/100, VERDICT: PASS.
- The gate file keeps both test runs (first failing run, then the rerun) as recorded. The Task 8 verify regex `[1-9][0-9]* failed` therefore matches the recorded first-run line by design.

## Popup facts (s03, window-feature popup, headless, local fixtures)
- After `switch-tab <opener>` while the agent is already on the opener: the popup row keeps ` R`; the opener row is `* ` (no `R`). Recorded as the Known issue.
- After `close-tab <popup>`: the opener row is `*R`.
- After `switch-tab <other>` then `switch-tab <opener>`: the opener row is `*R`.
- Input on the opener (`press Tab`) did not bring the video back (the popup row kept ` R`).

## Deviations
- [Rule 2 - accuracy] Task 6 item 4: the timeline JSON keys are shown in the order the file stores them (alphabetical: clock, dropped, duration_s, entries, files, version, video), not the plan's order (version first). The page says "Real file", so it follows the file. The sample is pretty-printed with one line per file entry, as before.
- [Rule 2 - accuracy] Task 6 item 6 (DSH): the plan said that without `binaryPath` the plugin "starts the binary of the npm platform package". Not run here; the source of `npm/bladebro/dsh-plugin.mjs` throws `bladebro: platform package "<pkg>" not installed` when that package is missing. The page states the throw from source, not a run.
- [Rule 2 - accuracy] Task 7 Known issues: removed "(or `batch`)" and "(as separate calls or one `run`)" (neither was measured); the verdict-miss sentence says what the readback reported instead of asserting why; the short-test example is corrected to what was run (record-start, one click, record-stop on a loaded page: t01-blue, 0.8 s; the plan said "navigate-then-click").
- [Rule 2 - accuracy] Task 7 Not verified: the plan's "Everything on this page was run on macOS" is replaced by a scoped statement, because the file-size, follow-delay and earlier result samples were not re-run in Phase 11.
- [Dropped] Task 7 E: the troubleshooting bullet `recording needs a long-lived session` was dropped (its text is not in any Phase 11 evidence file, per the user's note). The refusal-table row in "Recording with the state tool" still quotes that message; it is source-backed (`src/record/hub/msg.rs:36`), not evidence-run. It is a pre-existing row outside this task's edit list.
- [Plan deviation] Task 6 says "Edit only these places"; the intro sentence and the Example 5 section were inserted at the spots the plan named. No other content changed.
- [Template] `get-shit-done/templates/summary.md` does not exist in this repo; the summary follows the 10-02 SUMMARY layout.
- [Scope] Task 8's commit adds `11-02-PLAN.md` and `assets/`, as the plan lists them. The 11-02 plan itself was not executed.
- [Not committed, untouched] `.planning/ROADMAP.md` (modified before this session), `.planning/EXECUTION_LOG.md`, `CHECKPOINT.json` files, `.DS_Store`, `tests/eval-regressions/`. STATE.md and ROADMAP.md were not updated (not in Tasks 6-8).
- [Environment] No headed browser was started. Every script ran headless against 127.0.0.1:8765 and 8000 with scratch homes; `leftover processes` checks: 7 of 7 scripts reported none.

## Deferred
- [DEFERRED] `state switch-tab <id>` to the tab the agent is already on does not re-front the video (`deferred-items.md`).
- [CARRIED from 10] `act click` verdict misses a background-colour change (`deferred-items.md`; documented under Known issues).
- [CARRIED from 10, documented] In-run `run` timeline is empty (`deferred-items.md`; documented under Known issues).

## Not verified
- Linux, Windows, headed Chrome, a real DSH host, a real Claude Code session, the real-browser lane refusal, a real 2 h recording, a full disk. Listed on the page under Not verified.
- Earlier-phase samples on the page were not re-run (see Snippet check).
