# Bladebro Session Recording

## What This Is

Opt-in session video recording for Bladebro (the Rust agentic browser driver, v4.6.5), so autonomous UI/UX QA agents leave watchable evidence of passing and failing test runs. Tests are written in plain English and executed by a small model (Haiku today, via Charlotte MCP; moving to Bladebro). It must work in both the Claude Code GSD flow (`get-shit-done`, agent `gsd-charlotte-qa` / `gsd-test-writer`) and the DeepSeek Harness port (`gsd-dsh`). Work lives on the `ollorin/bladebro` fork (branch `gsd/browser-recording`, `upstream` = dondai44423/bladebro) and is designed to be upstreamable.

## Core Value

A QA run with recording enabled always leaves a playable video file at a predictable path that the tool result reports, without touching the stealth path or breaking Bladebro's five-tool contract.

## Requirements

### Validated

<!-- Inferred from the existing codebase map (.planning/codebase/). -->

- ✓ Agentic browser driver over MCP (stdio) and CLI with a persistent daemon — existing
- ✓ Exactly five tools: `act`, `see`, `state`, `run`, `vision` (schemas in `src/mcp/tools.rs`) — existing
- ✓ Own CDP client over WebSocket (optional pipe transport); MCP lane defaults to WebSocket — existing
- ✓ Live Page Model with stable refs and per-action deltas — existing
- ✓ Stealth injection and biometrics (audit 111 vectors, diff oracle) — existing
- ✓ Screenshots via `vision` (PNG, optional `marks`), console/network logs via `see`, tabs/cookies/sessions via `state` — existing
- ✓ Large outputs offloaded to an artifacts directory with path + preview — existing
- ✓ Linux/macOS/Windows first-class, CI on all three — existing

### Active

- [ ] Opt-in video recording of a browser session via CDP `Page.startScreencast` piped to an ffmpeg binary
- [ ] Default on/off and output directory configurable via environment/config (e.g. `BLADE_RECORD=<dir>`)
- [ ] Per-session start/stop control through a parameter on the existing `state` tool (no sixth tool), enabling per-test-case recordings with caller-chosen names
- [ ] Output format configurable, WebM (via ffmpeg) as the default
- [ ] Every recording is kept; the tool result reports the file path; the gsd flow decides pruning
- [ ] Graceful, honest failure when ffmpeg is missing or encoding fails (verdict says so; browsing is unaffected)
- [ ] Recording stays out of the stealth path (no new automation flags or page-visible differences; verified with `bladebro audit` 111/111 and `diff_oracle` divergent: 0)
- [ ] Works on Linux, macOS and Windows (platform differences behind `src/platform.rs`)
- [ ] Sidecar prototype validated first (separate CDP client + ffmpeg over the WebSocket transport) to prove frame capture, ffmpeg piping and tab-following before in-tree work
- [ ] Diff kept additive (new module plus minimal hook lines) so daily upstream releases merge cleanly
- [ ] Integration docs/snippets so the Claude Code QA agent and the DSH plugin can enable recording per test case
- [ ] Regression tests and live verification per AGENTS.md (clippy `-D warnings`, `cargo test --release`, relevant tools/ instruments)

### Out of Scope

- Viewport/device emulation — known QA gap, deferred to a later milestone
- Network throttling — known QA gap, deferred to a later milestone
- Accessibility/SEO/performance audits — known QA gap, deferred to a later milestone
- Keep-only-on-failure retention inside Bladebro — Bladebro cannot compute pass/fail (the agent decides); the harness prunes
- A sixth MCP tool — forbidden by AGENTS.md non-negotiable #1
- Any LLM call inside the driver — forbidden (non-negotiable #2)
- Updater, npm packaging, version bumps, releases — maintainer territory; changes to `npm/` need maintainer agreement
- Bundling ffmpeg or adding a Rust crate dependency for encoding — new dependencies are "ask first"; ffmpeg is used as an external binary
- Rewriting the `gsd-charlotte-qa` persona for Bladebro — separate follow-up in the get-shit-done repo, only enablement docs are in scope here

## Context

- **Origin:** QA runs through Charlotte MCP are slow and resource heavy; Bladebro is faster and lighter (5 tools, delta responses, ~1.9k-token tool definitions) but has no video, which the user's Charlotte fork provided via ffmpeg (that fork was not located; behaviour taken from the user's description: per-test enable/disable via config).
- **Research:** `gsd-dsh/docs/browser-automation-research/` holds the Charlotte vs Bladebro analysis, persona analysis and upstream update log. The README of Bladebro states "Session video recording: Not supported".
- **Consumers:** `get-shit-done/agents/gsd-charlotte-qa.md` (10 Charlotte tools, ~4-5 calls per interaction, Haiku) and the DSH plugin path (`dsh plugin --profile web add bladebro`, tools exposed as `mcp__bladebro__*`). Plain-English test flows are interpreted by the model, so recording must not require extra model effort.
- **DSH reference:** deepseek-harness has a Playwright recordVideo/ffmpeg recording note (`.agents/notes/implemented/process/2026-09-08-playwright-video-gif.md`) whose output conventions may be worth matching.
- **Upstream cadence:** ~one release per day, multiple contributors; the fork should track `upstream/main` with minimal conflict surface.
- **Codebase map:** `.planning/codebase/` (committed e307fa0). Known concerns relevant here: `validate_write_path` in `src/platform.rs` is a denylist with gaps (a feature that writes files passes through it), unwrap/expect discipline, CI uses unpinned Chrome.
- **Transport caveat:** a second CDP client (sidecar) only works on the WebSocket transport; `BLADE_TRANSPORT=pipe` exposes no port.

## Constraints

- **Tool surface**: five tools only — recording is a parameter/behaviour of existing tools — AGENTS.md non-negotiable #1
- **Schema change**: changing MCP tool schemas is "ask first" — open an upstream issue/draft PR before the final `state` parameter shape
- **Dependencies**: no new Rust dependency without discussion — ~7 MB binary and supply-chain posture; use external ffmpeg
- **Stealth**: no automation flags, no page-visible differences; must keep audit 111/111 and diff_oracle divergent: 0
- **Platforms**: Linux x64/arm64, macOS x64/arm64, Windows must all build and pass CI
- **Verification**: `cargo clippy --release -- -D warnings`, `cargo test --release`, scratch `BLADE_HOME`, regression tests, live readback
- **Release machinery**: no version bumps, tags, CHANGELOG curation or publishing
- **Maintainability**: additive diff and conventional commits so upstream merges stay clean

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Fork on `ollorin/bladebro`, GSD work on branch `gsd/browser-recording`, `main` mirrors upstream | Keep upstream sync trivial | — Pending |
| Sidecar prototype first, then in-tree opt-in feature | Validate CDP screencast + ffmpeg and the transport question before touching the codebase | — Pending |
| Control = env/config default + per-session parameter on `state` | Mirrors Charlotte's per-test toggle without a sixth tool | — Pending |
| Format configurable, WebM default | Plays anywhere, no H.264 encoder dependency | — Pending |
| Always keep recordings; report path in result; harness prunes | Bladebro cannot know pass/fail | — Pending |
| ffmpeg as external binary, not a crate | Avoid new dependency / binary size | — Pending |
| v1 = video only | Focused, upstreamable PR; gaps become later milestones | — Pending |
| Claude Code GSD flow is the primary consumer now; DSH port must also work | DSH version not ready yet, but design for both | — Pending |

---
*Last updated: 2026-10-21 after initialization*
