# Upstream hook ledger (SYNC-01)

Every edit this project makes to a file that exists in `upstream/main` is listed here, so a merge of upstream can be
reviewed quickly and conflicts localized. New files (e.g. `src/record/**`, `tools/record_sidecar/**`) need no entry.

| Phase | File | Lines / symbol | Reason | Added by commit |
|-------|------|----------------|--------|-----------------|
| 3 | src/lib.rs | `pub mod record;` (between realbrowser and reddit) | expose the fork-only recording module | 5782a59 |
| 3 | README.md | new "## Recording (in development)" block before the Maintenance heading (additions only) | document recording config and the sensitive-content warning (SAFE-03) | 1f9e11d |
| 4 | src/updater/doctor.rs | `checks.push(check_ffmpeg());` after check_disk_space + `fn check_ffmpeg` before check_version | doctor reports ffmpeg path/version/encoders (OUT-02); logic in src/record/ffmpeg/report.rs | f7e51f1 |
| 4 | README.md | one `BLADE_FFMPEG` row after `BLADE_RECORD_SIZE` in the Recording env table (addition only) | document the ffmpeg override | d6361fa |
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
