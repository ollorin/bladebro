# Technology Stack

**Analysis Date:** 2026-10-20

## Languages

**Primary:**
- Rust (edition 2021, `rust-version = "1.86"`) - Entire driver: CLI, MCP server, CDP client, stealth layer, updater. Sources in `src/` (about 53k lines including `tools/` and `scripts/`). Crate manifest: `Cargo.toml`.

**Secondary:**
- JavaScript (browser-injected) - Page-context scripts embedded into the binary via `include_str!`-style loading: `src/page/js/*.js`, `src/stealth/js/*.js` (e.g. `src/stealth/js/stealth_core.js`, `src/stealth/js/webdriver_patch.js`), `src/mcp/server/js/*.js`.
- JavaScript (ES modules, Node wrapper) - npm distribution shim: `npm/bladebro/bin.js`, `npm/bladebro/binary.cjs`, `npm/bladebro/dsh-plugin.mjs`.
- TypeScript - pi extension shim: `npm/bladebro/pi-extension.ts`.
- Python 3 (stdlib only) - Live reliability and QA probes: `tools/diff_oracle/`, `tools/reliability_probe/`, `tools/qol_probe/`, `tools/gap44/`, `tools/hermes_probe/`, `tools/rb_live/`, `tools/lane_matrix.py`. Imports observed are standard library only (`json`, `subprocess`, `http`, `urllib`, `tempfile`, etc.). No Python requirements file was detected.
- Node.js (probes) - `tools/dsh_probe/*.mjs`, `tools/reliability_probe/pi-stdio.mjs`.
- YAML - `.github/workflows/ci.yml`, `.github/dependabot.yml`, `npm/bladebro/cordis.patch.yml`.
- Shell - `release.sh`, `scripts/publish-npm.sh`.

## Runtime

**Environment:**
- Native compiled binary (no managed runtime). Async runtime is Tokio (`tokio` 1.53.2 in `Cargo.lock`).
- Node.js `>=14` declared as engine in `npm/bladebro/package.json`. CI uses Node 24 (`.github/workflows/ci.yml`, `actions/setup-node@v7`) for the DSH integration job.
- Target browser: Google Chrome / Chromium-family browser discovered at runtime (see Configuration). Chrome 154 is the verified floor per `SUPPORT-MATRIX.md`.

**Platforms shipped as binaries (npm optional dependencies):**
- `bladebro-linux-x64`, `bladebro-linux-arm64`, `bladebro-darwin-x64`, `bladebro-darwin-arm64`, `bladebro-windows-x64` (packages under `npm/`).
- Android `aarch64-linux-android` is compile-checked in CI only (source-level support, no published artifact).

**Package Manager:**
- Cargo (Rust). Lockfile: present (`Cargo.lock`, 160 packages). CI uses `--locked` in the MSRV and Android jobs.
- npm (distribution). `package-lock.json`: not detected under `npm/`.

## Frameworks

**Core:**
- Tokio 1.53.2 - Async runtime (`rt-multi-thread`, `process`, `net`, `signal`, `io-std`). Declared in `Cargo.toml`.
- tokio-tungstenite 0.30.0 - WebSocket CDP transport to `ws://127.0.0.1:<port>` (plain ws, no TLS feature). Declared in `Cargo.toml`.
- Model Context Protocol (hand-rolled JSON-RPC over stdio) - MCP server in `src/mcp/server.rs`, `src/mcp/server/serve.rs`, `src/mcp/tools.rs`. Protocol version constant `2025-06-18` in `src/mcp/server.rs`, with legacy dialects `2024-11-05` and `2025-03-26` accepted.
- Chrome DevTools Protocol (CDP) - Browser control, implemented in `src/cdp/` (`client.rs`, `session.rs`, `discovery.rs`, `protocol.rs`).

**Testing:**
- Rust built-in test harness (`cargo test`). `tests/` contains integration tests such as `tests/cdp_mock.rs`, `tests/daemon_lifecycle.rs`, `tests/security_audit.rs`, `tests/stealth_behavior.rs`, `tests/xdg_home.rs`.
- `tests/hermes_cli.rs` is declared as a custom test with `harness = false` in `Cargo.toml`.
- Live browser QA: `bladebro audit` subcommand (stealth vector suite, exit-code gated in CI) and Python probes under `tools/`.

**Build/Dev:**
- `build.rs` - Embeds git short SHA and dirty flag as `BLADE_BUILD_ID` via `git rev-parse` and `git status --porcelain`; falls back to `unknown` when git is absent.
- `rustfmt` (`cargo fmt --check`) and Clippy (`cargo clippy --release -- -D warnings`) run in CI.
- `release.sh` and `scripts/publish-npm.sh` - Release pipeline (build five target binaries, sync versions from `Cargo.toml` into `npm/*/package.json`, publish to npm).

## Key Dependencies

**Critical (from `Cargo.toml`, versions from `Cargo.lock`):**
- `reqwest` 0.12.28 (`default-features = false`, features `json`, `rustls-tls`) - HTTP client for CDP `/json` discovery (`src/cdp/discovery.rs`), updater checks and downloads (`src/updater/`), and the `body` download mode (`src/mcp/server/files.rs`). TLS is rustls; no system OpenSSL.
- `tokio-tungstenite` 0.30.0 - CDP WebSocket transport (`features = ["connect", "handshake"]`).
- `serde` 1.0.229 / `serde_json` 1.0.152 - All JSON-RPC, CDP, and knowledge-store serialization.
- `sha2` 0.11.0 - SHA-256 verification of downloaded update binaries against the release `.sha256` asset (`src/updater/download.rs`). Fail-closed for releases >= the threshold defined there.
- `base64` 0.22.1 (declared as `0.23` in `Cargo.toml`) - Decoding `Page.printToPDF` output into PDF artifacts (`src/artifacts.rs`).
- `url` 2.5.8 - URL parsing and origin checks.
- `tracing` 0.1.44 + `tracing-subscriber` (env-filter) - Structured logging.
- `anyhow` 1 / `thiserror` 2 - Error handling (`src/error.rs`).
- `libc` 0.2 - Unix process/signal handling. Windows job objects are handled in `src/platform/windows_job.rs`.
- `futures-util` 0.3 - Stream/sink helpers for the WebSocket client.

**TLS/crypto transitive (not declared directly):**
- `rustls` 0.23.46, `ring` (present in lock), `tokio-rustls`, `hyper-rustls`. `openssl-sys` and `native-tls` are absent from `Cargo.lock`.

**Dev/Ops:**
- `deny.toml` - `cargo-deny` config, currently only `[advisories] yanked = "deny"`.
- `.github/dependabot.yml` - Weekly updates for the `cargo` and `github-actions` ecosystems.

## Configuration

**Environment variables read by the binary (non-exhaustive, from `src/`):**
- `BLADE_HOME` - Root for bladebro state (sessions, knowledge, profiles). Default location is under the user home (`~/.blade/`).
- `BLADE_PROFILE_DIR` - Browser profile directory override.
- `BLADE_PROXY` - Proxy spec; schemes `http`, `https`, `socks4`, `socks5`, `quic` (`src/browser/proxy.rs`). Credentials are redacted in logs.
- `BLADE_TZ`, `BLADE_LOCALE`, `BLADE_GPU`, `BLADE_WEBGL`, `BLADE_NOISE`, `BLADE_PERMS`, `BLADE_MEDIA`, `BLADE_INCOGNITO`, `BLADE_FRESH`, `BLADE_LANE`, `BLADE_PACE` - Stealth and fingerprint controls (`src/fingerprint.rs`, `src/stealth/`).
- `BLADE_TRANSPORT` - `pipe` selects the zero-port `--remote-debugging-pipe` transport; default is WebSocket.
- `BLADE_CHROME_FLAGS` - Extra Chrome command-line flags (`src/browser/flags.rs`).
- `BLADE_IDLE_TIMEOUT`, `BLADE_CMD_TIMEOUT` - Timeouts.
- `BLADE_NO_UPDATE_CHECK`, `BLADE_NO_COMPRESS`, `BLADE_NO_WARMING`, `BLADE_CONSENT`, `BLADE_RB_DEBUG`, `BLADE_DUMP_INJECT` - Feature and debug toggles.
- `CHROME_PATH` - Explicit Chrome/Chromium binary (`src/browser/discover.rs`).
- Standard environment: `HOME`, `USERPROFILE`, `LOCALAPPDATA`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `PATH`, `DISPLAY`, `WAYLAND_DISPLAY`, `TERM`, `HELIUM_CONFIG_HOME`.
- CI-only: `DSH_RUNTIME`, `BLADEBRO` (path to binary for probes), `POLISH_OUT`, `BLADE_INCOGNITO`, `RUNNER_OS`, `RUNNER_TEMP`.

**Secrets:**
- No `.env` file is present at the repo root. Secrets are not read or committed by this repository; proxy credentials are supplied at runtime via `BLADE_PROXY`.

**Build:**
- `Cargo.toml` - Release profile: `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`. A `quick` profile (no LTO, incremental) exists for local iteration.
- `build.rs` - Git metadata embedding.
- `.gitattributes` - `* text=auto eol=lf`; PNG/JPG/GIF/ICO marked binary.
- `.gitignore` - Excludes `target/`, `dist/`, `build/`, `node_modules/`, `.env*`, `npm/bladebro-*/bladebro` binaries, and private working docs.

## Platform Requirements

**Development:**
- Rust toolchain >= 1.86.0 (MSRV job pins `1.86.0`); `stable` used for the main CI matrix.
- Chrome or Chromium-family browser installed locally (or set `CHROME_PATH`). Linux CI installs `google-chrome-stable` and `xvfb`.
- Node.js (any version >=14 for the npm wrapper; Node 24 for DSH integration tests).
- Python 3 for the `tools/` probes (stdlib only).
- Android NDK only for the `android` CI job (`aarch64-linux-android` compile check).

**Production:**
- Distributed as prebuilt native binaries through npm (`bladebro` main package plus per-platform optional dependencies) or built from source with `cargo build --release`. GitHub Releases host the binaries and `.sha256` checksum files (`src/updater/`).
- Runs as a local process: MCP server over stdio, a Unix-socket daemon (`src/cli/daemon.rs`), and Chrome launched locally with CDP on `127.0.0.1`.

---

*Stack analysis: 2026-10-20*
