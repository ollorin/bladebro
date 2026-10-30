# 07-02 Summary: state ops wired on every surface

Requirements: CTRL-01 (early return, no I/O for other ops), CTRL-04, CTRL-05, OUT-05 surfaced.

Commits: be1a3d6 (hooks + PageEnv + CLI + tests), e0b93f3 (HOOKS rows), 381e0c3 (schema + measurement), 4f37f73 (schema HOOKS row), aef58c7 (docs).

## Hooks (diff lines, hash)

| File | Hook | Lines | Hash |
|---|---|---|---|
| src/mcp/server/state.rs | `hub::try_handle` early return, first statement of `handle_state` | 4 | be1a3d6 |
| src/page/mod.rs | `Page::base()` | 5 | be1a3d6 |
| src/cli.rs | `set_ephemeral()` in `run_connected` | 1 | be1a3d6 |
| src/cli/args/state.rs | OPS, usage, `record-start` arm (calls `hub::cli_name`), `record-stop` in the no-extra-args arm, `#[path]` test mod | 15 (~10 logic, 4 test mod) | be1a3d6 |
| src/cli/help.rs | COMMANDS line, help_json ops, state help block | 8 | be1a3d6 |
| src/mcp/server/boot.rs | `finish_best_effort` first line of `shutdown_browser` | 2 | be1a3d6 |
| src/cli/daemon.rs | `finish_best_effort` before post-loop browser shutdown | 1 | be1a3d6 |
| src/mcp/tools.rs | enum +2, RECORDING line, name description | 3 places | 381e0c3 |

serve.rs needs no edit: its post-loop exit goes through `shutdown_browser`. All rows are in `.planning/HOOKS.md`.

## Schema token delta (evidence/07-02-schema.txt)

Phase 1 script unchanged. state ToolDef collapsed 1681 -> 1914 (+233 chars), all_tools 15581 -> 15814 (+233). ~58 tokens (233/4), 13.9 % of state, 1.5 % of all tools. D-01 predicted 232 chars / ~58 tokens. In-test measure of added text alone: 230 chars, budget 240. 5 tools, no new property.

## Live smoke (headless, scratch BLADE_HOME, fixture anim.html, BLADE_RECORD=1)

- stop with nothing active: `no active recording; record-start <name> first.` exit 1
- start: `recording started: .../smoke-20261022T122754Z-c1f8f8.webm (tab A89A5E50...); first frame not awaited. record-stop saves it.`
- second start: `recording already active: ... (started 0.0 s ago, waiting for first frame). record-stop it first.` exit 1
- stop after 4 s: `recording saved: .../smoke-...webm (bytes 51945, duration 4.1 s, frames 41, dropped 0, segments [0.0 s tab A89A...])` exit 0
- `--no-daemon state record-start x`: ephemeral refusal, exit 1
- After `bladebro stop`: no ffmpeg; the one-shot Chrome started by `--no-daemon` was killed by hand (own scratch home), leaving no process.

## Target.getTargetInfo (D7-11)

Primary path worked on the WS page session (`Target.getTargetInfo` -> `targetInfo.targetId`); the `/json` fallback was not needed or implemented.

## Tests

668 passed, 0 failed (baseline 655 after 07-01). New: adapter_tests (lane mapping, error mapping, pre-lock name check), state_parse_tests (CLI parse table, compiled into `cli::args::state` via `#[path]` because `cli::args` is private), schema_tests (5). `concurrent_cold_starts_have_one_owner` failed once in the first full run, passed on rerun. fmt, clippy --all-targets -D warnings, launch_mode registry drift tests green.

## Deviations

- [Rule 3] CLI parse tests cannot reach the private `cli::args` module from `record::hub`; used a 3-line `#[path]` test mod in `args/state.rs` (counted in its 15 lines).
- `hub::cli_name` helper holds the CLI name logic so the arm stays small.
- No serve.rs hook (covered by `shutdown_browser`).
- Docs example claims for DSH limited to dsh-plugin.mjs options; one-char drift between D-01 (232) and built (233) in the evidence note is unexplained filler, the measured numbers are exact.

## Self-Check: PASSED
