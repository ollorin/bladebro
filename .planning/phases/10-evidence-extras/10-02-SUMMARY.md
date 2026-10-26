# 10-02 Summary: action timeline, alignment and the recording-off gate

**Status:** complete (4/4 tasks). Executor model: haiku for every task. No task escalated to sonnet.
Tasks 1-3 were executed in an earlier run; Task 4 (schema/help/docs, hook ledger, final gate) in this run.

## Commits
- 6d7bcd6 feat(10-02): action timeline log with inert-when-off hooks (Task 1)
- 6f01bb1 feat(10-02): timeline sidecar at record-stop from existing verdicts (Task 2)
- 502fb19 docs(10-02): hook shas for the timeline dispatch hooks (Task 2)
- 8ce4399 test(10-02): live timeline alignment, MCP and recording-off evidence (Task 3)
- 14d6f61 docs(10-02): poster, marker and timeline documented; schema text (Task 4, upstream text + docs + schema tests)
- ab3eedb docs(10-02): recording-off measurement note (Task 4)
- chore(10-02): final gate and hook ledger (Task 4, HOOKS.md rows, evidence/10-02-gate.txt, deferred item)
- docs(10-02): plan summary (this file)

## What changed
- `record-stop` writes `<primary video stem>.timeline.json` next to the video (0600, created new, basenames only, JSON version 1). Entries come from the results the `act`, `run` and `state` open-tab / close-tab / switch-tab calls already returned: `at_s`, `end_s`, `tool`, `action`, `ok`, `verdict` (first line, cleaned, 200 chars); the marker is a `{kind: marker}` entry. Cap 1000 entries, then counted in `dropped`.
- The stop result gains `. Timeline: <abs path> (N actions)` (`, K not kept` when dropped). It never changes ok/err.
- Two upstream hooks in the dispatch points every lane goes through: `src/mcp/server/proto.rs handle_tools_call` and `src/cli.rs dispatch` (3 lines each). With recording off, `begin` is one relaxed atomic load and returns None.
- Per-tab: one timeline named after the `-tab1-` file, `files[]` with each file's `start_s` on the recording clock.
- Schema and help text describe the marker, poster and timeline (see Schema budget below). Two upstream description strings and one help block changed; nothing else in upstream files.
- No new tool, property, op, dependency, CDP method (RecMethod stays 5), stealth path or upstream file beyond the ledger.

## Alignment (measured; `evidence/10-02-live.txt`, headless, Chrome 154, local fixtures)
Offset = the frame where the clicked button's colour changed, minus the click entry's `end_s`. Scan step 0.1 s.

| run | blue click | green click |
|---|---|---|
| webm | -0.3 s (first blue frame 1.96 s, end_s 2.26) | -0.3 s (first green 4.18 s, end_s 4.48) |
| mp4 | -0.3 s (first blue 1.82 s, end_s 2.12) | -0.3 s (first green 4.01 s, end_s 4.31) |
| per-tab, file 2 (local clock) | n/a | -0.3 s (first green 1.03 s, end_s 1.33) |

- Over the five clicks: min, median and max all -0.3 s. Exploratory runs before the official one gave -0.2 or -0.3 on the same grid, so the honest error bound is about 0.1 s (grid) plus the one-frame slot.
- Read: the visible change lands about 0.3 s before the recorded `end_s`; `end_s` + 0.4 s always showed the new colour; `at_s` - 0.3 s showed the previous colour. Every first new-colour frame lies inside its own `[at_s, end_s]` window.
- The docs state this as a bound, not as exact frame times (`docs/recording-and-launch-config.md`, "Clock and alignment").

## Recording-off result (`evidence/10-02-live.txt`, base d18dae1 vs HEAD, 2 runs each)
- Distinct CDP method sets equal (11 each).
- Every method except `Input.dispatchMouseEvent`: equal per-method counts in both runs.
- `Input.dispatchMouseEvent`: base 23 and 30, new 34 and 30. The base binary differs from itself, so this is noise from the random humanized mouse paths (`src/stealth/biometrics.rs`), not a regression. The strict per-method check fails once (base run 1 vs new run 1) and passes once (run 2 vs run 2); the live file keeps that FAIL line as recorded.
- Normalized stdout byte-identical (both runs, and base vs base).
- No timeline, poster or video file in any off home.
- Decision (user, Task 3 open item): the accepted bar is all other per-method counts equal, normalized stdout byte-identical, no sidecar files. This is documented in the docs and the gate file. No product code was changed to make the mouse count deterministic.

## Schema budget (`cargo test ... schema_tests`, printed)
- `enum 31 + line 232 + name delta 16 + value delta 28 = 307 chars (~76 tokens)`. HEAD before Task 4 was 230 chars; this task added 77 chars (~19 tokens): the RECORDING line's marker and poster/timeline wording (+49) and the `value` description (+28). Ceiling 330 (D10-6). No new property or op.

## Tests and gates (Task 4, `evidence/10-02-gate.txt`)
| gate | result | baseline |
|---|---|---|
| `cargo fmt --check` | clean (exit 0) | clean |
| `cargo clippy --release --all-targets -- -D warnings` | clean (exit 0, no warnings) | clean |
| `cargo test --release --no-fail-fast` | 891 passed, 0 failed, 18 ignored | 820 (08.1-02), 864 (10-01) |
| `cargo test --release --lib -- record::hub::schema_tests launch_mode` (drift and schema) | 73 passed, 0 failed | - |
| `BLADE_HEADLESS=1 bladebro audit` (scratch home) | SCORE 100/100, PASS | 100/100 |
| `BLADE_HEADLESS=1 python3 tools/qol_probe/run.py` | 120 PASS, 2 FAIL: 'chord cleared the live editor', 'nested scroller movement is reported as such' | 120/122, same two |
| lane matrix, recording off (`lib/lane_matrix_local.py`, 5 rounds each) | daemon 1.1-1.3 s, oneshot 0.7 s, mcp 0.8 s, mcp-pipe 0.8 s; all exit 0 | 09-03: 0.6-2.1 s |
| lane matrix, `BLADE_RECORD=1`, 5 rounds each | daemon 0.9-1.4 s, oneshot 0.6-0.7 s, mcp 0.8 s, mcp-pipe 0.7-0.8 s; all exit 0 | - |
| diff_oracle | NOT RUN (python `websockets` not installed) | - |

- Upstream check (`git diff --name-only d18dae1..HEAD` against `upstream/main`): exactly `src/cli.rs`, `src/cli/help.rs`, `src/mcp/server/proto.rs`, `src/mcp/tools.rs`; each has a Phase 10 row with its sha in `.planning/HOOKS.md`.
- Scope check (`Cargo.toml Cargo.lock npm CHANGELOG.md .github src/page src/state.rs src/action src/stealth`): empty.
- `git diff -U0 d18dae1` for tools.rs and help.rs shows only the hook lines (2 changed in tools.rs, 1 changed + 1 added in help.rs). No `"#` added inside a raw string.
- MCP live tests (`tests/record_evidence.rs`, ignored, run by `evidence_live.sh`): `mcp_stop_has_poster_timeline_marker` PASS, `mcp_recording_off_has_no_sidecars` PASS.

## Hooks added (all in HOOKS.md, Phase 10)
- `src/mcp/server/proto.rs` (6f01bb1): 3 lines around the handler match.
- `src/cli.rs` (6f01bb1): 3 lines in dispatch.
- `src/mcp/tools.rs` (14d6f61): RECORDING line + `value` description.
- `src/cli/help.rs` (14d6f61): RECORDING block (1 changed + 1 added line).

## Deviations
- [Plan deviation - docs example] The plan asked for a JSON example "trimmed to 3 entries". The only real timeline file with a marker that this run produced (the MCP run, `evidence/10-02-live.txt`) has 2 entries (one action, one marker). It is shown as is, not padded with invented entries.
- [Rule 3 - Environment, not product code] The first qol_probe run used a scratch home under `/private/tmp/claude-501/...`. The daemon could not bind its socket (path longer than SUN_LEN), every check fell back to one-shot Chrome, and the run showed 17 PASS / 58 FAIL and a crash in the probe. The rerun with `mktemp -d` (short $TMPDIR path) is the one recorded (120/122). The socket failure was confirmed directly with a daemon run from the long path; for the qol run itself it is inferred from the same symptom.
- [Rule 3 - Environment] Leftover processes from my own one-shot runs under the long scratch homes (one Chrome tree) were terminated by PID; checked afterwards: no process carries a scratch path.
- [Plan deviation - rounds] The lane matrix ran 5 rounds per lane, as the plan says. The 08.1-02 summary used 3 rounds. The fixture server for it (`python3 -m http.server 8765` over `tools/record_sidecar/fixtures`) was started by me and stopped after the runs.
- [Path] The lane matrix wrapper is `tools/record_sidecar/lib/lane_matrix_local.py` (the path in the plan), not `tools/lane_matrix_local.py`. The unmodified `tools/lane_matrix.py` was not run directly.
- [Derived number] The stop worst case in the docs is now "about 21 s" (18 s video + 3 s poster budget). The 18 s and 3 s are the code's budget constants, not a new live measurement of the total.

## Deferred
- [DEFERRED] Result text `(1 actions)` in the stop result (`. Timeline: ... (1 actions)`): grammar only, product text in `src/record/hub/evidence.rs`. Logged in `.planning/phases/10-evidence-extras/deferred-items.md`; not changed here (wording is a product surface, left for a deliberate change).
- [DEFERRED, from 10-01] `act click` verdict on a background-colour change (see `deferred-items.md`).

## Not verified
- diff_oracle (stock Chrome vs bladebro): not run, `websockets` missing. The stealth path was not changed by this plan, but the oracle bar stays open until it runs.
- Linux and Windows: not run (Darwin 25.6.0 only). The timeline JSON file mode check uses the Darwin stat branch.
- Live timeline with a failed poster or timeout, and the per-tab marker: covered by unit tests only (10-01 and 10-02 Task 1/2 tests), not live.
- Lane matrix "on" runs prove the lanes start with `BLADE_RECORD=1`; they do not record.
- Stealth baseline (creepjs / sannysoft / incolumitas) not re-measured: no stealth-path change in this plan.
