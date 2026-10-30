# Coding Conventions

**Analysis Date:** 2026-10-20

## Scope

Bladebro is a Rust crate (edition 2021, MSRV 1.86) shipped as a single binary, with a library target at `src/lib.rs`. Conventions below cover the Rust core. Side tooling in `tools/` is Python (`tools/**/*.py`) and Node (`tools/**/*.mjs`, `tools/**/*.js`); embedded browser payloads are plain JavaScript in `src/**/js/*.js`. Those rules are noted where they differ.

Primary sources of truth: `AGENTS.md` (repo guide, code conventions) and `CONTRIBUTING.md` (binding PR contract). `CONTRIBUTING.md` wins on conflict.

## Naming Patterns

**Files:**
- Snake_case module files: `src/action/perform.rs`, `src/page/perception/wait.rs`, `src/session_profile/reap.rs`.
- Module pattern: a parent file holds the public surface and re-exports, with a sibling folder of the same name for children. Example: `src/action.rs` declares `perform`, `find`, `input`, `edit`, `verdict` from `src/action/`. Keep public paths stable through re-exports when moving code.
- Integration tests are snake_case files in `tests/` (`tests/cdp_mock.rs`, `tests/daemon_lifecycle.rs`, `tests/security_audit.rs`).
- Embedded JavaScript: `src/<area>/js/<name>.js`, loaded with `include_str!`. Example: `src/page/js/read_tree.js`, `src/stealth/js/stealth_core.js`.

**Functions:**
- `snake_case` throughout. Examples: `compute_verdict`, `eval_condition`, `hit_probe`, `reap_orphans_at`.
- Suffix `_at` marks the path-parameterized variant of a function that otherwise uses a default location: `reap_orphans()` delegates to `reap_orphans_at(blade_dir)` in `src/session_profile/reap.rs`.
- Expression builders return `String` and are named `*_expr`: `hit_probe_expr` in `src/action/verdict.rs`.
- Visibility is tight: `pub(super)` is used for helpers shared with sibling children (`src/action/verdict.rs`, `src/session_profile/reap.rs`); `pub` is reserved for the public surface re-exported from `src/lib.rs`.

**Variables:**
- `snake_case`. Short local names are common for tight scopes (`cdp`, `ws`, `el`).

**Types:**
- `UpperCamelCase`. Error enum: `BladeError` in `src/error.rs`. Test fixture type: `RawElement`, `PageCapture` (`src/page/perception.rs`, `src/page/mod.rs`).
- Field name `box_` is used where `box` is a Rust keyword (`src/page/perception.rs`, `RawElement::box_`).

**Constants:**
- `SCREAMING_SNAKE_CASE` for embedded JS payloads re-exported from modules: `JS_PREAMBLE`, `JS_READ_TREE` (imported in `src/action/verdict.rs`).

## Code Style

**Formatting:**
- `rustfmt` with default settings. No `rustfmt.toml` exists in the repo root. CI enforces `cargo fmt --check` (`.github/workflows/ci.yml`).
- Run before committing: `cargo fmt`.

**Linting:**
- `clippy` with warnings as errors. CI and AGENTS.md require: `cargo clippy --release -- -D warnings`. No `clippy.toml` is checked in.
- Crate-level attribute in `src/lib.rs`: `#![recursion_limit = "256"]`. No crate-wide `#![deny]` or `#![forbid(unsafe_code)]` was found, so `unsafe` is locally scoped (about 49 occurrences under `src/`).
- Python tools: no linter config checked in (no `ruff`, `pyproject.toml`, or `setup.cfg` at the root). Follow the style of the surrounding file in `tools/`.

**Editor/Whitespace:**
- No `.editorconfig` in the repo. Rust formatting comes from rustfmt.

## Import Organization

**Order (observed in `src/action/verdict.rs`, `src/action.rs`, `tests/cdp_mock.rs`):**
1. `std::` imports
2. External crates (`serde_json`, `tokio`, `futures_util`, `tokio_tungstenite`)
3. `crate::` internal paths
4. `super::` relative paths last

Example from `src/action/verdict.rs`:
```rust
use serde_json::json;
use std::time::Duration;

use crate::cdp::CdpSession;
use crate::error::{BladeError, Result};
use crate::page::perception::{JS_PREAMBLE, JS_READ_TREE};
use crate::page::{wait_for_settle, LivePageModel, PageDelta};

use super::edit::{clear_verdict_text, type_verdict_text, EditReport};
use super::Action;
```

**Path Aliases:**
- None. Imports use full `crate::` paths. Integration tests import the library as `bladebro::...` (`tests/cdp_mock.rs`).

**Module headers:**
- Larger modules begin with `//!` documentation and a `Module map:` paragraph listing children. Found in 12 files, including `src/action.rs`. Add this header when a module gains children.

## Error Handling

**Central type:**
- `src/error.rs` defines `BladeError` (a `thiserror` enum) and the alias `pub type Result<T> = std::result::Result<T, BladeError>;`. Use `crate::error::Result` in library code.
- Keep the enum as the concrete cause type. Do not box errors into `dyn Error` in library code (stated in the doc comment on `BladeError`).

**Error variants carry machine-readable tags:**
- Client-side transport failures include the literal tag `degraded:true` in the `#[error(...)]` string (`BladeError::Transport`, `BladeError::Timeout`). Agents branch on this tag, so preserve it when editing those messages.

```rust
#[error("CDP command timed out after {0:?} - degraded:true (client-side: ...)")]
Timeout(std::time::Duration),
```

**Error strings are product surface:**
- Messages are read by LLM agents. They must be specific, name the recovery path, and include page state for action errors (AGENTS.md "Code conventions"). Example pattern: the `Closed` and `NoTarget` variants in `src/error.rs`.

**Panics and unwraps:**
- AGENTS.md forbids `unwrap`/`expect` in non-test code "outside provably-guarded cases". A line-position count found about 44 `unwrap()`/`expect()` calls located before the first `#[cfg(test)]` marker in their file, across roughly 25 files. Most `unwrap` calls sit in `#[cfg(test)]` modules, where they are normal.
- Where a panic is intentional, use `expect("<reason>")` with a short reason (example: `src/cli/args.rs` test helpers use `.expect("valid endpoint")`).
- Bugs that could crash on untrusted input are fixed by avoiding byte slicing. See `truncate_utf8` in `src/platform.rs`, covered by `tests/security_audit.rs`.

**Errors in `bin` vs `lib`:**
- `anyhow` is a dependency (`Cargo.toml`) but `BladeError` is the error type used across the library. Prefer `BladeError` for new library code.

## Logging and Output

**stdout is a machine contract:**
- `--json` prints exactly one JSON object to stdout. MCP stdout is pure JSON-RPC. Do not add stray `println!` in these paths. About 304 `println!` calls exist in `src/`, mostly in CLI output paths.
- Diagnostics go to stderr. About 164 `eprintln!` calls exist in `src/`.

**Structured logging:**
- `tracing` with `tracing-subscriber` (`env-filter`). Usage is sparse: `tracing::debug!` (9 sites), `tracing::warn!` (1), `tracing::trace!` (1). Prefer `tracing::debug!` for diagnostics that should be opt-in.

**Human styling:**
- Use `src/ui.rs` helpers (`bold`, `enabled()`, `paint`) for human-facing styled output. Styling is TTY-gated and honors `NO_COLOR` and dumb terminals (`decide(tty, plain, forced, no_color, term_dumb)`). Piped output must stay plain.

## Comments

**When to comment:**
- AGENTS.md: "Comments explain why, not what." Document constraints, detection classes, and behaviors that break if a line changes. Example: the `hit_probe_expr` doc comment explains that `elementFromPoint` returns the shadow host, which is why the probe descends open shadow roots.
- Module-level `//!` docs describe design decisions and the reason for splitting the module (see `src/action.rs`, `src/session_profile/reap.rs`).

**Doc comments:**
- `///` on public types, variants, and non-obvious functions. Error variants document the caller-facing meaning (`src/error.rs`).
- Doc comments on tests describe the regression they lock in: "Each test locks in a fix from the 2026-08-16 security audit" (`tests/security_audit.rs`).
- Do not write temporal comments ("was changed", "previously"). Describe the current constraint.

**Raw-string hazard:**
- `src/cli/help.rs` help blocks are raw strings (`r#"..."#`). Do not put a `"#` sequence inside them (AGENTS.md).

## Function Design

**Size:** Large functions exist in dispatch and perception code. Prefer splitting into child modules following the `foo.rs` + `foo/` pattern rather than growing a single file.

**Parameters:** Pass `&CdpSession` (or `&Path`) explicitly. Session and client handles are not global. Use `Option<T>` for optional protocol fields (`Option<serde_json::Value>` for CDP params, as in `CdpClient::send("Page.navigate", None)`).

**Return values:** Return `crate::error::Result<T>` from fallible async functions. Return `Option<String>` for "may have nothing to report" diagnostics (`hit_probe` returns `Result<Option<String>>`).

**Async:** `tokio` runtime. Async functions use `async fn` directly. Timeouts are explicit `Duration` parameters (`client.wait_for("Page.frameNavigated", Duration::from_millis(100))`).

## Module Design

**Exports:**
- The crate root `src/lib.rs` re-exports the public surface. Integration tests use `bladebro::page::model::LivePageModel`, `bladebro::cdp::{CdpClient, CdpSession}`, and so on.
- `main.rs` is a thin binary entry that calls into the library.

**Barrel files:**
- `mod.rs` is used where a folder is a module (`src/page/mod.rs`, `src/cdp/mod.rs`, `src/mcp/mod.rs`, `src/stealth/mod.rs`). Newer code uses the `foo.rs` + `foo/` form instead of `foo/mod.rs`. Follow the form the surrounding module uses.

**Embedded JS rules (`src/**/js/*.js`):**
- Edit the `.js` file. Never paste a payload back into a Rust string (AGENTS.md).
- Injected JS must stay dependency-free and syntactically valid. The test suite runs `node --check` on assembled scripts. Debug with `BLADE_DUMP_INJECT=<path>` and `node --check <path>`.
- Stealth patches install functions as `Proxy` objects whose `apply` trap delegates to the original first. Never patch `Function.prototype.toString` (AGENTS.md non-negotiable #7).

## Dependency Rules

- Adding a dependency requires an issue or draft PR first (binary size and supply chain; AGENTS.md "Ask first").
- `deny.toml` exists at the root but is a stub (29 bytes). Do not assume license or advisory policy beyond what CI runs.

## Repo-Specific Rules

- Five MCP tools only (`act`, `see`, `state`, `run`, `vision`). New capabilities are parameters or behaviors of these tools.
- No LLM calls inside the driver.
- Honest verdicts: a success claim requires a readback. Phrase unverifiable outcomes as "readback unverified".
- Conventional Commits for commit messages: `feat:`, `fix:`, `stealth:`, `perf:`, `docs:`, `chore:`.
- Keep PRs scoped: a stealth fix and a refactor are two PRs.

---

*Convention analysis: 2026-10-20*
