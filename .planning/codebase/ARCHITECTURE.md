# Architecture

**Analysis Date:** 2026-10-20

## Pattern Overview

**Overall:** Single-binary Rust library-plus-binary crate (`bladebro` v4.6.5) with layered, module-per-concern design. One stateful "Live Page Model" (LPM) per browser session is the core abstraction; two agent-facing surfaces (MCP stdio server and CLI) share the same handler functions.

**Key Characteristics:**
- Library crate (`src/lib.rs`) plus a thin binary (`src/main.rs`). `main.rs` contains only process wiring: logging, exit-code contract, global `--host`/`--port` parsing, and dispatch to library modules.
- Few tools, full control: five agent tools (`act`, `see`, `state`, `run`, `vision`) defined once in `src/mcp/tools.rs`, with schemas derived from `src/action` (`ACT_ACTIONS`) so the MCP schema, CLI parser, and unknown-action error cannot drift.
- Own CDP client, no Playwright/Puppeteer: `src/cdp/` multiplexes JSON-RPC over a WebSocket.
- Perceive-as-side-effect: every action returns a delta from the LPM rather than requiring a fresh snapshot (`src/page/model.rs`, `src/page/refs.rs`).
- Embedded JavaScript: all in-page payloads live in `src/**/js/*.js` and are pulled in with `include_str!`. Rust never holds JS as string literals.
- Stealth in two layers: page-side injection (`src/stealth/inject.rs`, `src/stealth/js/`) and driver-side behavior (`src/stealth/biometrics.rs`, `src/stealth/hum.rs`). A separate "real-browser lane" (`src/realbrowser/`) turns page injection off entirely.
- Process isolation: each bladebro process gets its own session profile (`~/.blade/profiles/sess-<pid>`) and artifact namespace (`~/.blade/artifacts/blade-<pid>-<seq>.*`).
- Lazy lifecycle: the MCP server and CLI daemon start with no Chrome process and launch it on the first `tools/call`, shutting down after an idle timeout.

## Layers

Layers are listed bottom-up. Each higher layer depends only on layers below it.

**Platform and transport:**
- Purpose: OS-specific process/path helpers and the raw Chrome DevTools Protocol client.
- Location: `src/platform.rs`, `src/platform/windows_job.rs`, `src/cdp/`
- Contains: `CdpClient` (request/response multiplexer, event fan-out), `CdpSession` (per-target session), HTTP target discovery (`/json`, `/json/version`), generic `serde_json::Value` framing in `src/cdp/protocol.rs`.
- Depends on: `tokio`, `tokio-tungstenite`, `reqwest` (rustls), `libc`.
- Used by: everything in `src/page/`, `src/browser/`, `src/stealth/`.

**Browser process management:**
- Purpose: find a Chrome binary, launch it with stealth flags (on Xvfb when available, `--headless=new` otherwise), wait for the debug endpoint, and tear it down.
- Location: `src/browser.rs` + `src/browser/` (`launch`, `discover`, `flags`, `probe`, `pipe`, `proxy`, `alerts`, `compat`, `display`)
- Contains: `Browser` handle with Drop-based cleanup; discovery scans PATH, common install paths, the NixOS store, and `CHROME_PATH`.
- Depends on: `platform`, `cdp`, `session_profile`, `fingerprint`.
- Used by: `src/mcp/server/boot.rs`, `src/cli.rs` one-shot and daemon paths, `src/audit.rs`.

**Session and profile management:**
- Purpose: per-process Chrome profiles, seasoning (a warmed profile), orphan reaping, login persistence, cross-session knowledge.
- Location: `src/session_profile.rs` (+ `copy.rs`, `reap.rs`), `src/logins.rs`, `src/state.rs`, `src/knowledge.rs`, `src/fingerprint.rs`
- Contains: profile copy, reaper for leaked Chrome/Xvfb, cookie persistence, domain knowledge DB in `~/.blade/knowledge/`, stable fingerprint seed.
- Depends on: `platform`, `cdp`.
- Used by: `browser`, `mcp/server/boot.rs`, `cli/daemon.rs`.

**Page layer (Live Page Model):**
- Purpose: the `Page` handle owns one CDP connection and one `LivePageModel` for the lifetime of a session. It keeps refs stable across captures and computes deltas.
- Location: `src/page/mod.rs` (struct and accessors), `src/page/attach.rs`, `src/page/navigate.rs`, `src/page/tabs.rs`, `src/page/heal.rs`, `src/page/logs.rs`, `src/page/route.rs`, `src/page/intercept.rs`, `src/page/model.rs`, `src/page/refs.rs`, `src/page/perception.rs` + `src/page/perception/` (`wait`, `consent`, `block`, `content`), `src/page/gl.rs`
- Contains: `Page` struct (CDP session, LPM, dialog queue, in-flight counter, route epoch, network and XHR ring buffers), capture pipeline, settle detection, consent dismissal, block detection, markdown extraction, ref tables.
- Depends on: `cdp`, `stealth`, `fingerprint`.
- Used by: `src/action/`, `src/mcp/server/*`, `src/cli.rs`, site adapters.

**Action layer:**
- Purpose: implements the `act` tool: resolve a stable ref to its signature, re-locate the element in the live DOM, dispatch the input, wait for settle, recapture, return the delta.
- Location: `src/action.rs` (the `Action` enum and predicates, `ACT_ACTIONS`), `src/action/perform.rs` (dispatch engine), `find.rs`, `input.rs`, `edit.rs`, `verdict.rs`
- Contains: click via `Input.dispatchMouseEvent` at element center, type via `Input.insertText`, select via in-page value set plus change event, press via key events, scroll via `window.scrollBy`.
- Depends on: `page`, `stealth::biometrics` (timing, click targets, typing cadence).
- Used by: `src/mcp/server/act.rs`, `src/cli/args/act.rs`.

**Stealth and behavior layer:**
- Purpose: make the browser look and act like a human browser.
- Location: `src/stealth/mod.rs`, `src/stealth/inject.rs` (+ `inject/gpu.rs`), `src/stealth/biometrics.rs`, `src/stealth/hum.rs`, `src/stealth/js/*.js`
- Contains: ordered injection scripts assembled from `stealth_core.js`, `proxy_helpers.js`, `media_patch.js`, `permissions_patch.js`, `webdriver_patch.js`, `locale_override.js`, `noise.js`, `rtc_patch.js`, `stealth_tail.js`. Bezier mouse paths with overshoot, log-normal typing cadence, and an idle "hum" task.
- Depends on: `cdp`, `fingerprint`.
- Used by: `page::attach` (injection), `action` (biometrics), `browser` (launch flags).

**Real-browser lane:**
- Purpose: drive the user's own Chromium-family browser with no page-side injection. Driver-side features still apply.
- Location: `src/realbrowser.rs`, `src/realbrowser/` (`browsers`, `config`, `controls`, `profiles`, `select`, `template`)
- Contains: lane selection (`init_lane`, `input_paused`), profile clone into a blade-owned template, browser picker.
- Depends on: `browser`, `session_profile`, `cli/rb.rs` for the command surface.
- Used by: `src/main.rs` (`init_lane`), `src/cli.rs` (`run_rb`), `src/mcp/server/serve.rs` (pause checks).

**Tool handlers (MCP and CLI shared):**
- Purpose: one handler per tool. Both the MCP server and CLI call these functions, so changes propagate to both surfaces.
- Location: `src/mcp/server/act.rs`, `see.rs`, `state.rs`, `run.rs`, `vision.rs`, `eval.rs`, `extract.rs`, `files.rs`, `paginate.rs`, `resolve.rs`, `validate.rs`
- Contains: `handle_act`, `handle_fill`, `handle_see`, `handle_state`, `handle_run`, `handle_vision`, `handle_eval`, `handle_collect`, `handle_pdf`, `handle_download`. Argument validation runs before any navigation or input.
- Depends on: `page`, `action`, `artifacts`, `reddit`, `x`.
- Used by: `src/mcp/server/proto.rs` (`tools/call`), `src/cli.rs` (`dispatch`).

**Agent surfaces:**
- MCP server: stdio JSON-RPC 2.0. Dual dialect: legacy `initialize` handshake (≤ 2025-11-25) and the 2026-07-28 stateless revision (`server/discover`, per-request `_meta` protocol version). Code in `src/mcp/server.rs` (protocol core), `src/mcp/server/boot.rs` (lifecycle, transports), `src/mcp/server/serve.rs` (JSON-RPC loop), `src/mcp/server/proto.rs` (verbs), `src/mcp/tools.rs` (tool schemas).
- CLI: `src/cli.rs` plus `src/cli/args/` (one parser per command), `src/cli/daemon.rs` (Unix socket server), `src/cli/help.rs`, `src/cli/hermes.rs`. The CLI can run in one-shot mode or talk to a daemon over `cli.sock` in the bladebro state dir.
- Stdout is a machine contract: one JSON object for `--json`, pure JSON-RPC for `mcp`. Logs go to stderr.

**Site adapters:**
- Purpose: `extract auto` fast paths that replay the page's own API traffic instead of scraping the DOM.
- Location: `src/reddit.rs` + `src/reddit/` (`net`, `search`, `tree`), `src/x.rs` + `src/x/` (`dom`, `fetch`, `gql`, `parse`)
- Contains: Reddit comment-tree and search extraction via same-origin JSON endpoints; X.com GraphQL templates captured from live XHR traffic (`Page::xhr_log`).
- Depends on: `page`.
- Used by: `src/mcp/server/extract.rs`.

**Artifacts, audit, and updater:**
- `src/artifacts.rs`: large outputs (eval, extract, console logs, PDFs) are written to `~/.blade/artifacts/`; the agent gets a path and a preview.
- `src/audit.rs`: `bladebro audit`, a stealth gate that runs the vector suite in `tests/vectors.html` and exits non-zero on any FAIL.
- `src/updater/`: self-update (`mod.rs`, `download.rs`, `version.rs`), atomic binary swap (`swap.rs`), rollback, and `doctor` diagnostics. Checksums are verified fail-closed for releases ≥ 3.3.0.

## Data Flow

**MCP tool call (`tools/call`):**

1. stdin newline-delimited JSON-RPC is read by `src/mcp/server/serve.rs`.
2. On the first `tools/call`, `boot::launch_browser` starts Chrome (`src/browser.rs`) and attaches a `Page` (`src/page/attach.rs`). Subsequent calls reuse the same page.
3. `proto::handle_tools_call` routes to the handler (`act`, `see`, `state`, `run`, `vision`).
4. The handler validates arguments (`src/mcp/server/validate.rs`), resolves targets (`resolve.rs`), and performs the work through `Page` and `action`.
5. Action path: resolve ref → signature → re-locate in DOM → dispatch via CDP → `wait_for_settle_with_network` → recapture → `PageDelta`.
6. Large results go through `artifacts`, and the response carries a path plus preview.
7. The response is written to stdout as JSON-RPC. Dialogs captured by the dialog task are appended to the result.
8. Idle timer (default checked every 15 s) shuts Chrome down after `BLADE_IDLE_TIMEOUT`. Stdin EOF or SIGTERM/SIGINT/SIGHUP triggers graceful teardown: Chrome killed, profile synced, session dir removed.

**CLI command (`bladebro act --ref ...`):**

1. `src/main.rs` parses global flags (`--host`, `--port`, `--version`, `--rollback`), calls `realbrowser::init_lane`, and sets the SIGPIPE default for CLI output commands.
2. Commands in the `is_cli_cmd` list are routed to `cli::run_cli` (`src/cli.rs`). Update and doctor commands go to `updater`. Others (for example `targets`, `probe`) use raw CDP from `main.rs`.
3. `cli/args/*.rs` parses flags into the same JSON args object the MCP handlers expect.
4. If a daemon is running and the socket dir is trusted (`socket_dir_trusted`, `peer_uid_matches_self`), the request goes to `cli/daemon.rs`. Otherwise a one-shot Chrome launches, runs, and exits.
5. Output: human text by default, or one JSON object with `--json` (`{"ok","is_error","text"}`). Exit codes: 0 ok, 1 failure, 2 usage.

**Perception (capture):**

1. `page::perception::capture` injects the JS in `src/page/js/` (`deep_all.js`, `read_tree.js`, `vis_fn.js`, `role_fn.js`, `name_fn.js`, `state_fn.js`, and others) through `Runtime.evaluate`.
2. Raw elements go to `LivePageModel` (`src/page/model.rs`), which assigns refs (`src/page/refs.rs`) and diffs against the prior capture.
3. Content extraction (`content.rs`, `markdown.js`, `content_text.js`) is budget-bounded; `detect_block.js` flags bot walls and consent overlays.

**Stealth injection:**

1. `stealth::inject::full_script` assembles the page-side script from `src/stealth/js/*.js` in order.
2. `page::attach` installs it before navigation. `fingerprint.rs` keeps the canvas and audio noise seed stable across sessions.
3. `stealth::hum` runs while the page is idle; `biometrics` shapes actions.

**State Management:**
- Per-process state lives in `Page` and `Browser` in memory. `Page` holds the LPM, refs, dialog queue, in-flight count, route epoch, and network/XHR ring buffers (last 50 network entries, last 128 XHR entries).
- Cross-session state is on disk under `~/.blade/`: `profiles/`, `artifacts/`, `knowledge/`, `cli.sock`, pidfile, and login snapshots.
- The `state` tool exposes cookies, localStorage, sessionStorage, and tabs. `src/state.rs` owns session save and load.

## Key Abstractions

**`Page`:**
- Purpose: one connection to one browser tab, plus the Live Page Model.
- Examples: `src/page/mod.rs` (struct), `src/page/attach.rs` (construction), `src/page/navigate.rs`
- Pattern: long-lived handle with `Drop` that aborts the dialog task so the CDP client clone is released.

**`LivePageModel` and refs:**
- Purpose: stable element references that survive re-captures, with deltas on every action.
- Examples: `src/page/model.rs`, `src/page/refs.rs` (`RefEntry`, `StateChange`, `StateProbe`)
- Pattern: ref → element signature; act re-locates by signature before dispatch.

**`Action` enum:**
- Purpose: one enum for every `act` verb, shared by the MCP schema, CLI parser, and batch steps.
- Examples: `src/action.rs`, `src/mcp/tools.rs` (`batch_step_actions`)
- Pattern: schema derived from the enum constant, never hand-maintained.

**`CdpClient` / `CdpSession`:**
- Purpose: request/response multiplexing with monotonic ids, event fan-out to subscribers.
- Examples: `src/cdp/client.rs`, `src/cdp/session.rs`, `src/cdp/protocol.rs`
- Pattern: generic `Value` framing, no generated CDP bindings.

**`Browser`:**
- Purpose: owns a launched Chrome process and its profile and display; cleanup on Drop.
- Examples: `src/browser.rs`, `src/browser/launch.rs`
- Pattern: RAII teardown, with a Windows job object (`src/platform/windows_job.rs`) for descendant cleanup.

**`ToolResult` / handler return:**
- Purpose: the handler returns text, optionally with an image (vision). The CLI and MCP layers format it differently.
- Examples: `src/cli.rs` (`dispatch`), `src/mcp/server/proto.rs`
- Pattern: handlers return `Result<String>` or a JSON object, and the surface wraps it.

**Error types:**
- Purpose: one crate-wide error enum with exit-code mapping.
- Examples: `src/error.rs` (`BladeError`, `Result`), `src/main.rs` (`print_error`, exit codes)
- Pattern: `BladeError::Usage` maps to exit 2; other errors map to exit 1.

## Entry Points

**Binary `bladebro` (`src/main.rs`):**
- Location: `src/main.rs`
- Triggers: shell invocation, or `npm` launcher `npm/bladebro/bin.js`, which resolves a per-platform binary via `npm/bladebro/binary.cjs`.
- Responsibilities: logging setup, global flags, lane init, dispatch to MCP, CLI, updater, audit, or legacy debug commands.

**MCP server (`bladebro mcp`):**
- Location: `src/mcp/server.rs` → `src/mcp/server/boot.rs` (`run`, `run_pipe`) → `src/mcp/server/serve.rs` (`serve`)
- Triggers: an MCP client (for example Claude Code, pi, Hermes) starts the process and speaks stdio JSON-RPC.
- Responsibilities: protocol negotiation, lazy Chrome launch, tool dispatch, idle and crash recovery, teardown.

**CLI daemon (`bladebro daemon`):**
- Location: `src/cli/daemon.rs` (`run_daemon`)
- Triggers: explicit `bladebro daemon`.
- Responsibilities: persistent Chrome, Unix socket server at `cli.sock`, idle timeout, login snapshot, knowledge sync.

**Library API:**
- Location: `src/lib.rs` (re-exports `cdp`, `page`, `action`, `cli`, `mcp`, `stealth`, and others)
- Triggers: `tests/*.rs` integration tests and external crate consumers.

**Plugin and host integrations (npm):**
- `npm/bladebro/pi-extension.ts`: pi extension (declared in `npm/bladebro/package.json`), talks to the binary over stdio.
- `npm/bladebro/dsh-plugin.mjs`: DSH plugin; imports the running host's bridge rather than a private SDK copy.
- `npm/bladebro/skills/bladebro/SKILL.md`: agent skill document.

## Error Handling

**Strategy:** Typed errors at the library boundary (`BladeError`, via `thiserror`), `anyhow` for wrapping internal failures. Errors become JSON-RPC errors on the MCP surface or text plus exit code on the CLI surface. Tool failures are returned as `isError` results so the agent can react.

**Patterns:**
- Usage errors (unknown flags, missing values) are loud and exit 2. Unknown flags are never silently dropped (`src/cli/args.rs`).
- Self-healing: a dead Chrome connection on `tools/call` triggers relaunch and one retry (`src/mcp/server/serve.rs`). A dead tab triggers dead-tab recovery.
- Validation before side effects: `src/mcp/server/validate.rs` checks argument types before navigation or input.
- Fail-closed updates: a missing or mismatched checksum aborts the update (`src/updater/download.rs`).
- Process-death cleanup: the next launch reaps orphaned Chrome and Xvfb processes (`src/session_profile/reap.rs`).

## Cross-Cutting Concerns

**Logging:** `tracing` with `tracing-subscriber` (`EnvFilter`). Default `warn,bladebro=info`; `warn` in MCP mode. `RUST_LOG` overrides. Output goes to stderr only (`src/main.rs`). Human styling is in `src/ui.rs`.

**Validation:** Argument validation in `src/mcp/server/validate.rs` and per-command parsers in `src/cli/args/`. Action vocabulary is validated against `ACT_ACTIONS` in `src/action.rs`.

**Authentication:** None for the MCP surface. Site login state is held in Chrome profiles and cookies (`src/logins.rs`, `src/state.rs`). The CLI daemon socket is trusted only when its directory is owned by the current user with no group or world write bits, and the peer UID matches (`src/cli.rs`).

**Stealth verification:** `bladebro audit` (`src/audit.rs`) and the `tests/stealth_behavior.rs` and `tests/vectors.html` suite gate changes to page-side injection.

**Tests:** Unit tests inline as `#[cfg(test)]`. Integration tests in `tests/`. Verification instruments in `tools/`. The stealth JS is `node --check`ed by the test suite.

---

*Architecture analysis: 2026-10-20*
