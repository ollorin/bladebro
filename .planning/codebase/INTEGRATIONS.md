# External Integrations

**Analysis Date:** 2026-10-20

## APIs & External Services

**GitHub (release distribution and update checks):**
- Repository constant: `dondai44423/bladebro` (`GITHUB_REPO` in `src/updater/mod.rs`).
- `https://github.com/dondai44423/bladebro/releases/latest` - Unauthenticated 301 redirect, tag parsed from `Location` (`src/updater/version.rs`, client built with redirects disabled).
- GitHub release asset downloads - Binary plus `<asset>.sha256` checksum (`src/updater/download.rs`). Checksum verification is fail-closed for releases at or above the threshold in `checksum_required` (`src/updater/download.rs`).
  - SDK/Client: `reqwest` (rustls)
  - Auth: None (public releases)
- Git clone URL shown in install guidance: `https://github.com/dondai44423/bladebro.git` (`src/updater/download.rs`).

**npm Registry:**
- `https://registry.npmjs.org/bladebro/latest` - Version check fallback for updates (`src/updater/version.rs`).
- Publishing: `scripts/publish-npm.sh` publishes the main `bladebro` package and five platform packages.
  - SDK/Client: `reqwest` (update check); `npm` CLI (publish)
  - Auth: npm credentials supplied outside the repo (not stored here)

**Google Chrome download page (doctor hint only):**
- `https://www.google.com/chrome/` - Printed as an install hint by `bladebro doctor` (`src/updater/doctor.rs`). No API call.

**Chrome / Chromium-family browsers (primary runtime dependency, local):**
- Launched as a local child process, not a network API. Discovered via `CHROME_PATH`, `PATH`, or known install paths (`src/browser/discover.rs`). Known brands: Google Chrome, Chromium, Brave, Helium, Microsoft Edge (`src/realbrowser/browsers.rs`).
- Control channel: Chrome DevTools Protocol.
  - WebSocket: `ws://127.0.0.1:<port>` using discovery at `http://127.0.0.1:<port>/json/...` (`src/cdp/discovery.rs`, `src/browser/launch.rs`).
  - Pipe: `--remote-debugging-pipe` (`src/cdp/client.rs`, `BLADE_TRANSPORT=pipe`).

**Target websites (the tool's purpose - driven, not integrated):**
- Arbitrary sites are navigated and automated through the browser. Site-specific fast paths:
  - Reddit: `src/reddit.rs`, `src/reddit/net.rs`, `src/reddit/search.rs`, `src/reddit/tree.rs` - Calls Reddit's own JSON endpoints from inside the page context: `<post>.json`, `/comments/<post>/comment/<parent>.json`, `/api/info.json?id=...`. Requires Reddit's `loid` client token set by page JS; HTTP 403 "network-security wall" responses are treated as stop-signals.
  - X.com / Twitter: `src/x.rs`, `src/x/gql.rs`, `src/x/fetch.rs`, `src/x/dom.rs`, `src/x/parse.rs` - Replays captured `https://x.com/i/api/graphql/<qid>/<Op>` requests (TweetDetail, UserTweets, SearchTimeline, HomeTimeline) from page context. Query IDs are captured from live traffic, not hard-coded.
  - Web tracker/ad blocking list: `src/page/intercept.rs` - Hostnames for analytics and ad networks (e.g. `google-analytics.com`, `analytics.google.com`, `sentry.io`, `heapanalytics.com`) are blocked when the `trackers` mask is set.
  - Hostnames for Cloudflare challenges (`challenges.cloudflare.com`), DataDome (`js.datadome.co`), and PerimeterX (`px-cdn.net`) are listed in `src/page/intercept.rs`. They are matched against request hosts only; no API calls are made to them.

**Proxies (user-provided, optional):**
- `BLADE_PROXY` - Passed to Chrome as `--proxy-server` and, for `body` downloads only, to the `reqwest` client (`src/mcp/server/files.rs`). HTTP(S) proxies only for the `body` download path; SOCKS and QUIC are browser-side only.
  - Auth: credentials embedded in the URL (`scheme://user:pass@host:port`). Validated and redacted in logs (`src/browser/proxy.rs`).

**Agent hosts (MCP clients that consume the binary):**
- Hermes Agent - Setup integration via its public CLI (`src/cli/hermes.rs`) that edits `mcp_servers.bladebro` and `agent.disabled_toolsets` through the `hermes` command. Subprocess only; no network call.
- DSH (DeepSeek host runtime) - Native plugin `npm/bladebro/dsh-plugin.mjs` imports `@deepseek-ai/dsh-mcp-client` and runs `bladebro mcp` over stdio. Declared through `npm/bladebro/cordis.patch.yml` (`dsh.bundle.patch`). CI tests this with `@deepseek-ai/dsh@0.2.0-rc.2` (`.github/workflows/ci.yml`).
- pi (Earendil) coding agent - Extension `npm/bladebro/pi-extension.ts` imports `@earendil-works/pi-coding-agent` types (optional peer dependency) and registers the binary as a stdio MCP server.
- Claude Code / other MCP clients - Stdio MCP server via `bladebro mcp` (`src/mcp/server/serve.rs`). Exits when stdin closes.

## Data Storage

**Databases:**
- None. No SQL or NoSQL database client is declared in `Cargo.toml`.

**Local persistent state (filesystem only):**
- `~/.blade/` (overridable with `BLADE_HOME`):
  - `~/.blade/sessions/<name>.json` - Cookies and localStorage snapshots (`src/state.rs`).
  - `~/.blade/knowledge/` - Per-domain consent selectors, block configs, timing, bot risk (`src/knowledge.rs`).
  - Browser profile directories (`BLADE_PROFILE_DIR`), copied and reaped by `src/session_profile/copy.rs` and `src/session_profile/reap.rs`.
- Real-browser profile discovery reads existing Chrome, Chromium, Brave, Helium, and Edge profile directories on the local machine (`src/realbrowser/profiles.rs`, `src/realbrowser/browsers.rs`) using the `HOME`, `LOCALAPPDATA`, and `XDG_CONFIG_HOME` variables. `HELIUM_CONFIG_HOME` is honored for Helium.
- Update staging and rollback binaries are written next to the installed binary (`src/updater/swap.rs`).

**File Storage:**
- Local filesystem only. Artifacts (e.g. PDFs from `Page.printToPDF`) are written by `src/artifacts.rs`.

**Caching:**
- None external. In-process caches only (e.g. label cache in `src/page/js/label_cache.js`).

## Authentication & Identity

**Auth Provider:**
- None for the tool itself. No user accounts, API keys, or OAuth flows are implemented.
- Site login state is carried through browser cookies, persisted under `~/.blade/sessions/` and via the real-browser profile path (`src/logins.rs`).
  - Implementation: Cookie store copy/flush handling for logins that must survive restarts (`src/logins.rs`, `src/session_profile/`).
- X.com and Reddit requests reuse the page's own session cookies and headers. No credentials are stored by bladebro for these sites.

**Release integrity:**
- SHA-256 checksum verification for update downloads (`sha2`). Release signing is not implemented; the code comments state that full compromise protection requires signing.

## Monitoring & Observability

**Error Tracking:**
- None. No Sentry, Datadog, or similar SDK in `Cargo.toml` or `npm/`.

**Logs:**
- `tracing` with `tracing-subscriber` (env-filter) to stderr. Debug output gated by `BLADE_RB_DEBUG`.
- Proxy credentials are redacted before logging (`src/browser/proxy.rs`).
- `bladebro doctor` (`src/updater/doctor.rs`) prints a local diagnostic report.

**Telemetry:**
- None detected. Update checks are the only outbound calls the binary makes on its own, and they can be disabled with `BLADE_NO_UPDATE_CHECK`.

## CI/CD & Deployment

**Hosting:**
- GitHub repository `dondai44423/bladebro`. Release binaries are published to GitHub Releases. npm packages are published to the npm registry.

**CI Pipeline:**
- GitHub Actions, `.github/workflows/ci.yml`.
  - Runners: `ubuntu-latest`, `macos-latest`, `windows-latest`, plus a separate MSRV job (Rust 1.86.0) and an Android cross-compile job.
  - Actions used: `actions/checkout@v7`, `dtolnay/rust-toolchain@master`, `actions/cache@v6`, `actions/setup-node@v7`.
  - Steps reach external hosts: `https://dl.google.com/linux/...` apt repo for Chrome on Linux; npm registry for `@deepseek-ai/dsh@0.2.0-rc.2` and the packed local packages.
- Dependabot - `.github/dependabot.yml` for Cargo and GitHub Actions, weekly.
- Local release tooling: `release.sh`, `scripts/publish-npm.sh`.

## Environment Configuration

**Required env vars:**
- None are strictly required for normal operation. Optional runtime controls are listed in `STACK.md` under Configuration.
- `CHROME_PATH` - Needed only when the Chrome binary is not in a standard location.
- `BLADE_PROXY` - Only when routing through a proxy.

**Secrets location:**
- No secrets are stored in the repository. `.gitignore` excludes `.env` and `.env.*`. Proxy credentials and npm publish tokens are supplied at runtime or by the operator environment.

## Webhooks & Callbacks

**Incoming:**
- None. The binary exposes no HTTP server. The only listeners are the local Unix-socket daemon (`src/cli/daemon.rs`) and the stdio MCP server. Test code binds `127.0.0.1:0` (`src/cdp/client.rs`, tests only).

**Outgoing:**
- None. No webhook callbacks are sent. Outbound HTTP is limited to the update check and release download described above, plus the `body` download mode, which fetches a caller-supplied URL (HTTP(S), 32 MiB and 30 s limits per `src/mcp/tools.rs`) through the optional `BLADE_PROXY`.

---

*Integration audit: 2026-10-20*
