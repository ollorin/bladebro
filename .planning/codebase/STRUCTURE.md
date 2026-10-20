# Codebase Structure

**Analysis Date:** 2026-10-20

## Directory Layout

```
bladebro/                         # repo root (crate `bladebro` 4.6.5)
├── src/                          # Rust library + binary (~46k lines of .rs)
│   ├── main.rs                   # binary entry: flags, logging, dispatch
│   ├── lib.rs                    # library root: declares all public modules
│   ├── action.rs + action/       # `act` verbs: perform, find, input, edit, verdict
│   ├── page/                     # Live Page Model, perception, refs, navigation
│   │   ├── js/                   # in-page JS payloads (include_str!)
│   │   └── perception/           # capture pipeline: wait, consent, block, content
│   ├── cdp/                      # own Chrome DevTools Protocol client
│   ├── browser.rs + browser/     # Chrome discovery, launch, flags, pipe transport
│   ├── stealth/                  # page injection + biometrics + hum
│   │   ├── js/                   # stealth injection scripts (order matters)
│   │   └── inject/gpu.rs         # GPU spoof helper
│   ├── mcp/                      # MCP stdio server
│   │   ├── tools.rs              # tool schemas
│   │   ├── server.rs             # protocol core and shared helpers
│   │   └── server/               # boot, serve, proto, and per-tool handlers
│   │       └── js/               # auto_extract.js, template_extract.js
│   ├── cli.rs + cli/             # CLI surface, daemon, help, rb, hermes
│   │   └── args/                 # per-command argument parsers
│   ├── session_profile.rs + session_profile/  # per-process profiles, reaper
│   ├── realbrowser.rs + realbrowser/          # real-browser lane (`rb`)
│   ├── reddit.rs + reddit/       # Reddit extraction adapter
│   ├── x.rs + x/                 # X.com extraction adapter
│   ├── updater/                  # self-update, rollback, doctor
│   ├── platform.rs + platform/   # OS-specific helpers, Windows job object
│   ├── state.rs                  # cookies / storage / tabs state
│   ├── logins.rs                 # login persistence
│   ├── knowledge.rs              # cross-session domain knowledge
│   ├── fingerprint.rs            # stable fingerprint seed
│   ├── artifacts.rs              # large-output offloading
│   ├── framing.rs                # bounded stdio/socket/pipe framing
│   ├── audit.rs                  # `bladebro audit` stealth gate
│   ├── error.rs                  # BladeError
│   └── ui.rs                     # human CLI styling
├── tests/                        # integration tests + fixtures
│   ├── fixtures/                 # JSON and JS fixtures for reddit, x, native behavior
│   └── vectors.html              # stealth audit vector suite
├── tools/                        # verification instruments (each has a README.md)
│   ├── diff_oracle/              # stealth diff oracle (Python + JS)
│   ├── dsh_probe/                # DSH plugin probe (Node)
│   ├── gap44/                    # WebRTC proxy probe (Python)
│   ├── hermes_probe/             # Hermes integration probe (Python)
│   ├── qol_probe/                # HTML fixtures and runner for QoL checks
│   ├── rb_live/                  # real-browser live checks
│   ├── reliability_probe/        # release and reliability runs (Python + Node)
│   └── lane_matrix.py            # lane coverage matrix
├── npm/                          # npm distribution
│   ├── bladebro/                 # launcher package: bin.js, binary.cjs, dsh-plugin.mjs,
│   │                             #   pi-extension.ts, skills/bladebro/SKILL.md
│   └── bladebro-{platform}/      # per-platform binary packages (darwin, linux, windows)
├── scripts/publish-npm.sh        # npm publish helper
├── .github/                      # CI workflow, issue and PR templates, CODEOWNERS
├── Assets/                       # README images, SVGs, demo GIF
├── Cargo.toml / Cargo.lock       # crate manifest and lockfile
├── build.rs                      # embeds git SHA and dirty flag for `-v`
├── deny.toml                     # cargo-deny policy
├── release.sh                    # release automation
├── stealth-bench-sites.csv       # site list for stealth benchmark
├── AGENTS.md                     # source of truth for coding agents
├── CLAUDE.md                     # pointer that imports AGENTS.md
├── CONTRIBUTING.md               # contribution guide
├── README.md                     # user-facing behavior reference
├── CHANGELOG.md                  # release history
├── STEALTH_BENCH.md              # stealth benchmark notes
└── SUPPORT-MATRIX.md             # platform and browser support matrix
```

## Directory Purposes

**`src/`:**
- Purpose: all Rust code. `lib.rs` exposes every module as `pub mod` (except `framing`).
- Contains: `.rs` source files, plus `include_str!`-embedded `.js` in the `js/` subfolders.
- Key files: `src/main.rs` (binary), `src/lib.rs` (module root), `src/error.rs` (`BladeError`).

**`src/page/`:**
- Purpose: the Live Page Model and everything that observes a page.
- Contains: `mod.rs` (`Page` struct), `attach.rs`, `navigate.rs`, `tabs.rs`, `heal.rs`, `logs.rs`, `route.rs`, `intercept.rs`, `model.rs`, `refs.rs`, `perception.rs`, `gl.rs`.
- Key files: `src/page/mod.rs`, `src/page/model.rs`, `src/page/refs.rs`, `src/page/perception.rs`.
- Embedded JS: `src/page/js/*.js` (`deep_all.js`, `read_tree.js`, `vis_fn.js`, `role_fn.js`, `name_fn.js`, `state_fn.js`, `ctx_fn.js`, `esc_fn.js`, `landmark_fn.js`, `label_cache.js`, `label_proxy.js`, `fnv_fn.js`, `nc_fn.js`, `extract_basic.js`, `markdown.js`, `content_text.js`, `detect_block.js`).

**`src/page/perception/`:**
- Purpose: the capture pipeline split by concern.
- Contains: `wait.rs` (settle detection), `consent.rs` (cookie and consent dismissal), `block.rs` (bot wall detection), `content.rs` (budget-bounded markdown and text).

**`src/action/`:**
- Purpose: the `act` dispatch engine.
- Contains: `perform.rs` (perform and perform-with-network, `MUT_WATCH`), `find.rs` (locate by selector, text, or signature), `input.rs` (mouse and keyboard), `edit.rs` (type and clear ladders), `verdict.rs` (conditions and scroll reports).
- Key file: `src/action.rs` (the `Action` enum and `ACT_ACTIONS`).

**`src/cdp/`:**
- Purpose: raw CDP transport.
- Contains: `client.rs` (multiplexer), `session.rs` (per-target session and subscription), `discovery.rs` (HTTP target listing), `protocol.rs` (message framing).

**`src/browser/`:**
- Purpose: Chrome lifecycle.
- Contains: `launch.rs`, `discover.rs` (binary search, NixOS store), `flags.rs` (launch flags), `display.rs` (Xvfb), `pipe.rs` (pipe transport), `probe.rs`, `proxy.rs`, `alerts.rs`, `compat.rs`.

**`src/stealth/`:**
- Purpose: page-side and driver-side stealth.
- Contains: `inject.rs` (script assembly), `inject/gpu.rs`, `biometrics.rs` (bezier mouse, log-normal typing, action gap), `hum.rs` (idle activity task).
- Embedded JS (assembly order is significant): `proxy_helpers.js`, `stealth_core.js`, `media_patch.js`, `permissions_patch.js`, `webdriver_patch.js`, `locale_override.js`, `noise.js`, `rtc_patch.js`, `stealth_tail.js`.

**`src/mcp/`:**
- Purpose: MCP surface.
- Contains: `tools.rs` (five tool schemas, `batch_step_actions`), `server.rs` (protocol core and `artifact_hint`), `server/` children.
- `server/` children: `boot.rs` (`run`, `run_pipe`, lazy launch, warm profile), `serve.rs` (JSON-RPC loop), `proto.rs` (`initialize`, `server/discover`, `tools/list`, `tools/call`), and handlers `act.rs`, `see.rs`, `state.rs`, `run.rs`, `vision.rs`, `eval.rs`, `extract.rs`, `files.rs`, `paginate.rs`, `resolve.rs`, `validate.rs`.
- Embedded JS: `src/mcp/server/js/auto_extract.js`, `src/mcp/server/js/template_extract.js`.

**`src/cli/`:**
- Purpose: human and agent CLI.
- Contains: `daemon.rs` (Unix socket server and lifecycle), `help.rs` (help text and typo suggestions), `rb.rs` (real-browser lane commands), `hermes.rs` (Hermes setup), `args.rs` plus `args/` (`nav.rs`, `see.rs`, `act.rs`, `state.rs`, `run.rs`, `vision.rs`).
- Key file: `src/cli.rs` (dispatch, daemon client, one-shot run).

**`src/session_profile/`:**
- Purpose: profile lifecycle.
- Contains: `copy.rs` (profile copy and seasoning), `reap.rs` (orphan reaper).

**`src/realbrowser/`:**
- Purpose: real-browser lane internals.
- Contains: `browsers.rs`, `config.rs`, `controls.rs`, `profiles.rs`, `select.rs`, `template.rs`.

**`src/reddit/` and `src/x/`:**
- Purpose: site adapters. `reddit.rs` / `x.rs` hold the public surface; children are `net.rs`, `search.rs`, `tree.rs` (Reddit) and `dom.rs`, `fetch.rs`, `gql.rs`, `parse.rs` (X).

**`src/updater/`:**
- Purpose: update channel.
- Contains: `mod.rs` (commands), `download.rs` (checksum-verified download), `version.rs`, `swap.rs` (atomic binary replace), `doctor.rs` (diagnostics).

**`tests/`:**
- Purpose: integration tests (`cargo test`). Unit tests are inline `#[cfg(test)]` modules in `src/`.
- Contains: `cdp_mock.rs`, `cli_endpoint.rs`, `daemon_lifecycle.rs`, `hermes_cli.rs` (`harness = false`), `page_unit.rs`, `rb_discovery.rs`, `security_audit.rs`, `stealth_behavior.rs`, `xdg_home.rs`.
- Fixtures: `tests/fixtures/` (`native_behavior.js`, `stealth_behavior.cjs`, `reddit_post_comments.json`, `reddit_search.json`, `x_tweet_detail.json`), and `tests/vectors.html`.

**`tools/`:**
- Purpose: manual and live verification scripts, not part of `cargo test`. Each folder has a README.md.
- Contains: see the Directory Layout above.

**`npm/`:**
- Purpose: npm distribution. The `bladebro` package has `bin.js` (launcher), `binary.cjs` (resolves the platform package beside itself), `dsh-plugin.mjs`, `pi-extension.ts`, and `skills/`. The `bladebro-<os>-<arch>` packages carry only the binary and license.

## Key File Locations

**Entry Points:**
- `src/main.rs`: binary `main`, global flags, dispatch.
- `src/lib.rs`: library root.
- `src/mcp/server/boot.rs`: MCP `run` and `run_pipe`.
- `src/mcp/server/serve.rs`: JSON-RPC loop.
- `src/cli.rs`: `run_cli` and `dispatch`.
- `src/cli/daemon.rs`: `run_daemon`.
- `npm/bladebro/bin.js`: npm launcher.

**Configuration:**
- `Cargo.toml`: dependencies, release profile (`lto = "thin"`, `strip = "symbols"`), and a `quick` profile for local iteration.
- `build.rs`: git SHA embedding.
- `deny.toml`: cargo-deny policy.
- `.github/workflows/ci.yml`: CI pipeline.
- `.github/dependabot.yml`: dependency updates.
- `npm/bladebro/package.json`: npm metadata and pi extension manifest.

**Core Logic:**
- `src/page/mod.rs`: `Page`.
- `src/page/model.rs`: `LivePageModel`.
- `src/page/refs.rs`: ref tables.
- `src/action/perform.rs`: act dispatch engine.
- `src/mcp/tools.rs`: tool schemas.
- `src/mcp/server/proto.rs`: `tools/call` routing.
- `src/stealth/inject.rs`: injection assembly.
- `src/browser/launch.rs`: Chrome launch.

**Testing:**
- `tests/*.rs`: integration tests.
- `tests/vectors.html`: stealth vectors.
- `tools/*/README.md`: manual verification docs.

## Naming Conventions

**Files:**
- Rust modules: `snake_case.rs`. Each large module is a `foo.rs` file plus a `foo/` folder with children, not `foo/mod.rs`. Examples: `src/action.rs` + `src/action/perform.rs`, `src/mcp/server.rs` + `src/mcp/server/serve.rs`.
- `mod.rs` is used only where it is the sole file in a folder: `src/page/mod.rs`, `src/cdp/mod.rs`, `src/mcp/mod.rs`, `src/stealth/mod.rs`, `src/updater/mod.rs`.
- In-page JS: `snake_case.js`, with a `_fn.js` suffix for functions injected as helpers (`vis_fn.js`, `role_fn.js`, `ctx_fn.js`) and `_patch.js` for overrides (`media_patch.js`, `rtc_patch.js`).
- Integration tests: `snake_case.rs` in `tests/`.
- Probe scripts in `tools/`: `snake_case.py` or `.mjs`, plus `README.md` per folder.

**Directories:**
- Lowercase, single words or `snake_case` (`session_profile`, `realbrowser`).
- Each concern has its own folder; per-command folders match command names (`cli/args/nav.rs` for `nav`).

**Symbols:**
- Public tool and action names are lowercase verbs: `act`, `see`, `state`, `run`, `vision`; `click`, `type`, `fill`, `select`, `press`, `scroll`, `batch`.
- Version-tagged comments and doc-comments reference decision IDs (for example `D5`, `S18`, `G03`) and release versions (for example `v3.9.7`, `4.6.1`). Keep these when editing the surrounding code.

## Where to Add New Code

**New `act` action:**
- Enum variant and predicates: `src/action.rs` (add to `ACT_ACTIONS` so the schema and CLI pick it up).
- Dispatch: `src/action/perform.rs`; element location in `src/action/find.rs`; input in `src/action/input.rs`.
- Handler wiring: `src/mcp/server/act.rs`; CLI parser in `src/cli/args/act.rs`.
- Tests: inline `#[cfg(test)]` in the action module, plus `tests/page_unit.rs` or `tests/cdp_mock.rs` for CDP-level behavior.

**New MCP tool or handler:**
- Schema: `src/mcp/tools.rs`.
- Handler: new file in `src/mcp/server/` (for example `src/mcp/server/foo.rs`), declared in `src/mcp/server.rs` with `mod foo;` and re-exported as needed.
- Routing: `src/mcp/server/proto.rs` (`handle_tools_call`).
- CLI: parser in `src/cli/args/` (new file for a new command), dispatch in `src/cli.rs`, help in `src/cli/help.rs`.

**New CLI command:**
- Parser: `src/cli/args/<command>.rs`, re-exported from `src/cli/args.rs`.
- Dispatch: add to `is_cli_cmd` in `src/main.rs` and to `dispatch` in `src/cli.rs`.
- Help: `src/cli/help.rs`.

**New page-side observation:**
- JS: `src/page/js/<name>_fn.js` (embedded with `include_str!` from `src/page/perception.rs`).
- Rust caller: `src/page/perception.rs` or a child in `src/page/perception/`.

**New stealth override:**
- JS: `src/stealth/js/<name>_patch.js`.
- Registration and ordering: `src/stealth/inject.rs`. Order matters; `stealth_core.js` comes early and `stealth_tail.js` last.
- Verification: add vectors to `tests/vectors.html` and run `bladebro audit`.

**New site adapter:**
- Module: `src/<site>.rs` plus `src/<site>/` for children, following `src/reddit.rs` and `src/x.rs`.
- Registration: `pub mod` in `src/lib.rs`; wire into `src/mcp/server/extract.rs`.
- Fixtures: `tests/fixtures/<site>_*.json`.

**New browser or platform behavior:**
- Chrome discovery and flags: `src/browser/discover.rs`, `src/browser/flags.rs`.
- OS-specific code: `src/platform.rs` only. Do not add `#[cfg]` elsewhere except at transport boundaries.

**New shared utility:**
- No general `utils` module exists. Put helpers in the module that owns the concern. Use `src/ui.rs` for CLI styling and `src/framing.rs` for bounded I/O.

**New integration test:**
- `tests/<area>.rs`. Register `[[test]]` in `Cargo.toml` only if it needs a custom harness.

**New verification script:**
- `tools/<name>/` with a `README.md`.

## Special Directories

**`.planning/`:**
- Purpose: GSD planning and codebase analysis output (this document lives in `.planning/codebase/`).
- Generated: Yes (by GSD tooling).
- Committed: Not verified; check `.gitignore` before committing.

**`target/` (Cargo build output):**
- Purpose: compiled artifacts.
- Generated: Yes.
- Committed: No (ignored via `.gitignore`).

**`~/.blade/` (runtime, outside repo):**
- Purpose: runtime state: `profiles/`, `artifacts/`, `knowledge/`, `cli.sock`, pidfile, login snapshots.
- Generated: Yes at runtime.
- Committed: Not in repo.

**`Assets/`:**
- Purpose: README images and demo media.
- Generated: No.
- Committed: Yes.

**`npm/bladebro-*/`:**
- Purpose: per-platform npm packages. The `bladebro-<os>-<arch>` directories contain only `package.json` and `LICENSE`; binaries are added at publish time (`scripts/publish-npm.sh`).
- Generated: Partly (binaries inserted at release).
- Committed: Manifests yes.

---

*Structure analysis: 2026-10-20*
