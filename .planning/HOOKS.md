# Upstream hook ledger (SYNC-01)

Every edit this project makes to a file that exists in `upstream/main` is listed here, so a merge of upstream can be
reviewed quickly and conflicts localized. New files (e.g. `src/record/**`, `tools/record_sidecar/**`) need no entry.

| Phase | File | Lines / symbol | Reason | Added by commit |
|-------|------|----------------|--------|-----------------|
| 3 | src/lib.rs | `pub mod record;` (between realbrowser and reddit) | expose the fork-only recording module | 5782a59 |
| 3 | README.md | new "## Recording (in development)" block before the Maintenance heading (additions only) | document recording config and the sensitive-content warning (SAFE-03) (superseded in 4.1: block is now a pointer) | 1f9e11d |
| 4 | src/updater/doctor.rs | `checks.push(check_ffmpeg());` after check_disk_space + `fn check_ffmpeg` before check_version | doctor reports ffmpeg path/version/encoders (OUT-02); logic in src/record/ffmpeg/report.rs | f7e51f1 |
| 4 | README.md | one `BLADE_FFMPEG` row after `BLADE_RECORD_SIZE` in the Recording env table (addition only) | document the ffmpeg override (superseded in 4.1: block is now a pointer) | d6361fa |
| 4.1 | src/lib.rs | `pub mod launch_mode;` (between knowledge and logins) | expose the fork-only launch-mode module | 1a51fd7 |
| 4.1 | src/browser/launch.rs | `launch_inner_stage`: display block uses `launch_mode::{effective, plan_display, note_launch}`; non-Linux `mode_str` | BLADE_HEADLESS + honest launched-headless flag (HEAD-01/03/04) | dcc534e |
| 4.1 | src/browser/pipe.rs | `launch_pipe_inner_stage`: same hook | same, MCP pipe transport | dcc534e |
| 4.1 | src/browser/alerts.rs | HEADLESS_NOTE condition `&& !launch_mode::headless_by_request()` | no false "Xvfb missing" advisory when headless was requested | dcc534e |
| 4.1 | src/main.rs | `"headless"` in the `is_cli_cmd` list and the CLI arm pattern | route `bladebro headless` to the CLI | 6f33da0 |
| 4.1 | src/cli.rs | `"headless" => launch_mode::run_cli(...)` arm in run_cli_inner | headless toggle dispatch | 6f33da0 |
| 4.1 | src/cli.rs | `launch` field via `launch_mode::request_tag()` in send_to_daemon | tag daemon requests with the resolved mode (HEAD-03) | 06fa377 |
| 4.1 | src/cli/help.rs | COMMANDS + ENVIRONMENT lines, suggest list, help_json commands list, help_json `env` entry, two arms (json + text) | document headless and BLADE_HEADLESS | 6f33da0 |
| 4.1 | src/updater/doctor.rs | `checks.push(check_launch_mode());` + `fn check_launch_mode` | doctor shows launch mode and source (HEAD-04) | 6f33da0 |
| 4.1 | src/cli/daemon.rs | mismatch guard after refresh_lane/drifted, before need_launch | refuse a mode mismatch with a `bladebro stop` instruction (HEAD-03) | 06fa377 |
| 4.1 | README.md | "## Recording (in development)" body reduced to a pointer to docs/recording-and-launch-config.md (fork-owned block; table and paragraphs moved to the page) | DOC-02 | 27f6e3f |
| 4.1 | src/browser/flags.rs | `launch_args`: `pin_headless_window` block pushes `--window-size` + `--screen-info` (non-Linux headless); window-size unit test assertion | headless screen was Chrome's 800x600 default (audit parity, 04.1-04) | 50a97c6 |
| 4.1 | src/page/attach.rs | UA override block: Runtime.evaluate probe + `userAgentMetadata` via `launch_mode::uad::build`; comment | headless override without metadata left navigator.userAgentData empty (04.1-04) | 50a97c6 |
| 4.1 | src/cli.rs | `run_tool`: `launch_mode::request_tag()` validation as the first statement of the unix daemon block; `launch_mode::daemon_start_note()` print after the daemon is confirmed running | an invalid setting never starts a daemon; the client prints the launch note (daemon stderr is /dev/null) (04.1-05 GAP1/GAP2) | fb04617 |
| 4.1 | src/mcp/server/serve.rs | "Could not launch Chrome" message tail via `launch_mode::launch_error_suffix(&e.to_string())` | no false "check Chromium is installed" for a launch-setting error (04.1-05) | fb04617 |
| 4.1 | src/page/attach.rs | UA override: probe removed; `userAgentMetadata` via `uad::derive` only when `headless_by_request()`, real full version from `Browser.getVersion` | client hints equal headed Chrome; unset Linux fallback unchanged (04.1-05 GAP4) | fb04617 |
| 4.1 | src/browser/flags.rs | headless pin pushes `display::HEADLESS_WINDOW_SIZE` / `HEADLESS_SCREEN_INFO`; window-size test assertion | window inside the pinned screen (04.1-05 L1) | fb04617 |
| 6 | src/cdp/client.rs | `MAX_SCREENCAST_EVENT_BYTES` const under MAX_EVENT_BYTES + the oversize condition in `route_cdp_text` | Page.screencastFrame (base64 JPEG, routinely > 16 KiB) must keep `data` and `params.sessionId` (the ack id); exemption bounded at 8 MiB; only the recorder's own connection receives this event (CAPT-01, SC3) | 57db2b5 |
| 7 | src/mcp/server/state.rs | `handle_state` first statement: `hub::try_handle` early return (4 lines) | record-start/record-stop reach the hub from MCP, daemon, one-shot and batch through one hook; other ops return before any I/O (CTRL-01, CTRL-04) | be1a3d6 |
| 7 | src/page/mod.rs | `Page::base()` accessor (5 lines) | the hub attaches the recorder's own CDP connection through the page's debugging endpoint | be1a3d6 |
| 7 | src/cli.rs | `hub::set_ephemeral()` first line of `run_connected` (1 line) | one-shot CLI refuses record-start (the process exits after the call) | be1a3d6 |
| 7 | src/cli/args/state.rs | OPS + usage string, `"record-start"` arm calling `hub::cli_name`, `record-stop` in the no-args arm, `#[path]` test mod (15 lines, ~10 of them logic) | CLI parity for the two ops; parse tests live in src/record/hub (private module) | be1a3d6 |
| 7 | src/cli/help.rs | COMMANDS state line, help_json ops list, state help RECORDING block (8 lines) | document the two ops | be1a3d6 |
| 7 | src/mcp/server/boot.rs | `shutdown_browser`: `hub::finish_best_effort().await` first line (2 lines; also covers the MCP post-loop exit, no serve.rs edit needed) | finalize an active recording before Chrome goes away | be1a3d6 |
| 7 | src/cli/daemon.rs | `hub::finish_best_effort().await` before the post-loop browser shutdown (1 line) | finalize an active recording on daemon exit | be1a3d6 |
| 7 | src/mcp/tools.rs | state `op` enum +`record-start`,`record-stop`; one RECORDING description line; `name` description suffix (3 places, +233 collapsed chars ~58 tokens) | CTRL-04: record ops through the existing state tool, no sixth tool (Phase 1 D-01 Option A) | 381e0c3 |
| 7 | src/state.rs | `StateOp::ListTabs`: `let own = hub::own_target(cdp).await;` + marker via `hub::is_current(...)` (formatted, ~6 lines; `is_current` removed in 8.1, the marker line is now the 8.1 row below) | recorder's own CDP attach made the recorded tab report attached; current tab now from the page session's target id while recording, unchanged otherwise (07-04 gap) | 761910a |

Phase 8: no new hooks (zero edits to upstream files; verified by the opus verifier against `git diff --name-only bb9e7dd..HEAD` and `git cat-file -e upstream/main:<path>`).
| 9 | src/cli/daemon.rs | drift/lost relaunch branch: `crate::record::hub::finish_best_effort().await;` before `b.shutdown()` (1 line) | finalize an active recording before Chrome is relaunched; report goes to the next record op (LIFE-01) | 488c0e0 |
| 9 | src/cli/daemon.rs | `Err(BladeError::Closed)` relaunch: same one-line hook before `b.shutdown()` | same (LIFE-01) | 488c0e0 |
| 9 | src/cli/daemon.rs | idle timeout block: same one-line hook directly above `b.shutdown()`, after the login snapshot | a recording idle for BLADE_IDLE_TIMEOUT is finalized, not dropped with Chrome (LIFE-01) | 488c0e0 |
| 9 | src/cli/daemon.rs | post-loop: the Phase 7 hook line moved above the login snapshot block, below the upstream comment (net 0 lines, no upstream line changed) | finalize before anything that talks to Chrome, so a dead Chrome cannot delay the finalize (LIFE-01) | 488c0e0 |
| 9 | src/session_profile/reap.rs | first line of `reap_orphans()`: `crate::record::orphans::sweep();` (1 line) | sweep orphaned bladebro ffmpeg on every Chrome launch (LIFE-03); logic in src/record/orphans.rs (4-part safety rule) | b95e023 |

Phase 9 ledger verified by the opus verifier (2026-10-26) against `git diff 9756923..HEAD` and `git cat-file -e upstream/main:<path>`: only src/cli/daemon.rs and src/session_profile/reap.rs touch upstream files.
| 8.1 | src/page/mod.rs | Page::act new-tab branch: `self.activate()` -> `crate::record::hub::activate_opener(self)` (1 line) | while a recording is active a popup opened by a click keeps the front (stock Chrome behaviour) so it can be recorded; recording off: identical call (CAPT-06, D1 option A) | 35ea894 |
| 8.1 | src/state.rs | ListTabs: marker via `hub::tab_marker(...)` (replaces the `is_current` if/else) + `out.push_str(hub::tabs_footer(...))` (2 lines added, 6 removed: the `is_current` if/else) | mark agent (*) and video (R) tabs while recording; byte-identical when not recording (CAPT-06 SC3) | 6d0dba4 |
| 10 | src/mcp/server/proto.rs | handle_tools_call: comment + `let tl = crate::record::timeline::begin(name, &args);` directly above the handler match, `crate::record::timeline::end(tl, &result);` after it (3 lines) | action timeline from the verdicts every MCP call already returns; recording off: one atomic load, output and CDP calls unchanged (OUT-07) | 6f01bb1 |
| 10 | src/cli.rs | dispatch: same 3 lines with `begin(tool, args)` above `let result = match tool {` and `end` after it | same for the daemon lane (one-shot cannot record) (OUT-07) | 6f01bb1 |
| 10 | src/mcp/tools.rs | state RECORDING line (`record-stop [value=<marker note>] ... poster PNG, timeline JSON`) + `value` description (`... or record-stop marker note.`); no new property or op | tells agents the marker, poster and timeline exist; +77 chars (~19 tokens; recording total 307 chars, budget 330, D10-6) (OUT-06/07) | 14d6f61 |
| 10 | src/cli/help.rs | RECORDING block: `record-stop [--value <note>]` line (changed) + one `(writes <video>.poster.png and <video>.timeline.json ...)` line (added) | help text for the marker and the two sidecar files; help_json and other blocks untouched (OUT-06) | 14d6f61 |
| 11 | README.md | "Gotchas & limits" table: the `**Session video recording**` row (1 line changed) | the row said "Not supported"; recording exists in this fork, the row now points to docs/recording-and-launch-config.md | b709ead |
| 11 | README.md | fork-owned block (Phase 3 row): "## Recording (in development)" retitled "## Recording", body rewritten as a short pointer (no upstream line touched) | the "Not functional yet" note was false after Phases 5-10 (DOC-02 pointer rule) | b709ead |

Phase 11: the only upstream line changed is the README Gotchas row above; no source file changed besides fork-owned src/record/hub/** (11-01).

Phase 8.1 ledger verified (2026-10-26, gate run) against `git diff 62cb476..HEAD -- src Cargo.toml Cargo.lock npm CHANGELOG.md .github` and `git cat-file -e upstream/main:<path>`: exactly two upstream files changed (src/page/mod.rs 1 line, src/state.rs 2 added + 6 removed); all other changed src paths are src/record/** (fork-owned). Cargo, npm, CHANGELOG, .github and src/updater, src/mcp, src/cli: no diff.
