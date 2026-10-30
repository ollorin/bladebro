# 11-02 Summary: QA flows record per test case (Claude Code GSD, DSH), README pointer, hook ledger

**Status:** complete (5/5 tasks). Executor: Haiku for every task (user policy). No task escalated to sonnet.
No source code changed in this plan; `src/record/hub/**` is unchanged since 11-01.

## Commits
- b709ead docs(11-02): README points to the recording page; recording is no longer 'not supported' (Task 4, README only)
- 261a8ac docs(11-02): hook ledger rows for the README pointer (Task 4, HOOKS.md)
- 19b7c68 docs(11-02): QA flows (Claude Code GSD, DSH) record per test case; every block run against fixtures (Tasks 1-3, 5: page, snippet map, evidence)
- this file (with the `.planning/ROADMAP.md` plan-name fix that was uncommitted at the start): docs(11-02): plan summary

## What each flow needs
- **Claude Code GSD flow:** the server entry from `.mcp.json` (`command` = this fork's build, `env` `BLADE_RECORD=1`), a `tools:` line in the user's copy of gsd-charlotte-qa with the five `mcp__bladebro__*` names, and per test case two `state` calls (`record-start`, `record-stop`) around the steps. gsd-test-writer (Bash only) uses the CLI daemon and parses the stop line with `sed`.
- **DSH plugin flow:** `dsh plugin --profile web add bladebro`, the plugin options with `binaryPath` (the npm binary has no recording), a profile patch with `BLADE_RECORD`, and the same per-test calls.
- Where the paths appear: the stop result (MCP text, CLI stdout, CLI `--json` `text` field). The `--json` shape was taken from `11-01-s02-cli.txt`, not from a 11-02 run.

## Evidence (11-02-q01-qa-flows.txt, headless, scratch BLADE_HOME, fixture on 127.0.0.1:8765)
- MCP Claude Code entry, one test case (`07-02-login-invalid-password`, 2 actions, marker `FAIL: error banner never appeared`): video 53535 bytes, 4.6 s, 46 frames, dropped 0; poster 45066 bytes; timeline 981 bytes, `(2 actions)`, marker at 4.6 s. ffprobe: vp9, 1280x720, 10/1, 4.600000 s.
- Tools line: all five names in `gsd-agent-tools.yml` are exposed by the server (5 PASS).
- CLI path extraction (`t03`, daemon): video 29668 bytes, poster 42244 bytes, timeline 688 bytes; all three paths exist (3 PASS).
- DSH profile (plugin file loaded with a stub bridge, its config object through the MCP server): saved a recording, 52055 bytes, 4.6 s, 46 frames, at the reported path.
- The click verdict on the fixture said `clicked (no observable DOM change)` in both runs, which is the documented verdict miss (Known issues); the recording was still made.
- `no leftover processes` reported by all scripts.

## Claude Code client (11-02-q02-claude-code.txt): NOT VERIFIED
Run once (`claude -p`, model haiku, neutral temp dir, the docs' `.mcp.json`), both the direct run and the subagent run. Both replies were the same, and no tool call happened:

`Failed to authenticate: OAuth session expired and could not be refreshed`

No recording was made. The page's Not verified bullet says so. Not retried, per the plan.

## Snippet check
`python3 -I tools/record_sidecar/scenarios/doc_snippets.py check .planning/phases/11-integration-docs/evidence/snippet-map.tsv`: `26 blocks, 0 failing` (20 from 11-01 + 6 new). The six new rows: `gsd-agent-tools` (config), `qa-per-test` (run), `qa-per-test` (output), `extract-path` (run), `extract-path` (output), `dsh-profile-record` (config). The moved `claude-code.mcp.json` and `dsh-plugin-options.json` blocks keep their 11-01 rows.

## Gates (evidence/11-02-gate.txt)
- `doc_snippets.py check`: 26 blocks, 0 failing.
- `cargo fmt --check`: clean.
- `cargo clippy --release --all-targets -- -D warnings`: clean.
- `cargo test --release --no-fail-fast`, first run: 892 passed, 1 failed, 18 ignored. The failure is the known flaky `concurrent_cold_starts_have_one_owner` (panic at `tests/daemon_lifecycle.rs:204`). Its leaked daemon (`BLADE_HOME=.../bladebro-daemon-race-6759`) was stopped with `bladebro stop`.
- Rerun alone: `concurrent_cold_starts_have_one_owner` ok. Totals with the rerun: 893 passed, 0 failed, 18 ignored (the same as 11-01).
- Drift tests (`launch_mode::registry`, `record::hub::schema_tests`): 14 passed, 0 failed.
- The gate file keeps the first run as recorded. The `[1-9][0-9]* failed` check in Task 5 therefore matches that line by design, as in 11-01.
- `git diff --name-only 42ca29b..HEAD -- src`: only the four `src/record/hub/` files from 11-01.
- No leftover bladebro daemon or `bbrec` process after the runs.

## README and hook ledger
- README (b709ead): the one upstream row changed (`**Session video recording**` in Gotchas & limits). `git diff upstream/main -- README.md` shows one removed line. The fork-owned block is now `## Recording`, a short pointer to the page. "Not functional yet" is gone.
- HOOKS.md (261a8ac): two Phase 11 rows with `b709ead`, plus the Phase 11 sentence.

## Roadmap criterion 3
ROADMAP criterion 3 ("README documents the recording feature") is met by the short README block and the Gotchas row pointing to the page, as DOC-02 asks. The README keeps a pointer only.

## Deviations
- [Rule 2 - accuracy] The DSH profile patch (`dsh-profile-record.yml`) is shown as YAML on the page, as the plan asks, but DSH is not installed. The run used the same object as JSON (`dsh-profile-record.json`, `binaryPath` substituted) through the plugin stub. The page's Not verified section already says a DSH host was not run.
- [Rule 2 - accuracy] The `--json` shape of the stop result is quoted from `11-01-s02-cli.txt`, because no 11-02 run used `--json`.
- [Rule 2 - accuracy] Only the Claude Code bullet of Not verified changed (Task 2 said to append the failure); no run was made with other credentials or flags.
- [Plan note] The gsd-dsh tool map is described as it reads at planning time (`tool-map.ts` maps the Charlotte tools to `drop`; no bladebro entry). The page says so and does not claim a GSD port exists.
- [Scope] `.planning/phases/11-integration-docs/CHECKPOINT.json` is untracked and was not part of this plan; left alone.

## Not verified
- A Claude Code client session (the Task 2 attempt failed on auth), the gsd-charlotte-qa and gsd-test-writer agents themselves, a DSH host, Linux, Windows, headed Chrome, the real-browser lane, a 2 h recording, a full disk. Listed on the page under Not verified.
