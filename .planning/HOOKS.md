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
