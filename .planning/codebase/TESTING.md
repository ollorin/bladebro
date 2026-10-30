# Testing Patterns

**Analysis Date:** 2026-10-20

## Test Framework

**Runner:**
- Rust built-in test harness (`cargo test`) for unit and integration tests.
- `tokio` test macro (`#[tokio::test]`) for async tests. Roughly 29 `#[tokio::test]` attributes and 297 `#[test]`-attributed functions across `src/` and `tests/` (counted by grep, so treat as approximate).
- One integration target uses a custom harness: `[[test]] name = "hermes_cli" harness = false` in `Cargo.toml`. Its `fn main()` is at `tests/hermes_cli.rs:119`.
- Config: no `.cargo/config.toml`, `nextest.toml`, or `rustfmt.toml` in the repo. Test profile is the built-in one, plus a `quick` profile in `Cargo.toml` (`[profile.quick]`, inherits release, no LTO, for local iteration).
- Python tools: `unittest` (stdlib). Example: `tools/diff_oracle/test_oracle.py` imports `parse_bladebro_eval` from `oracle.py`.
- Node: injected-JS regression tests spawn `node` (see below).

**Assertion Library:**
- Standard `assert!`, `assert_eq!`, `assert!(matches!(...))`, and `.expect("...")` for setup failures.
- `serde_json::json!` for building expected payloads.

**Run Commands:**
```bash
cargo test --release                          # All tests (CI command)
cargo test --profile quick                    # Fast local iteration (no LTO)
cargo test --release --test cdp_mock          # One integration test file
cargo test --release <name_substring>         # Filter by test name
cargo clippy --release -- -D warnings         # Lint (required before commit)
cargo fmt --check                             # Format check (CI-enforced)
python3 tools/diff_oracle/test_oracle.py      # Oracle evidence parser (CI runs this)
python3 tools/qol_probe/run.py                # Input/action behavior suite, 122 checks (needs browser)
python3 tools/diff_oracle/oracle.py           # Stock Chrome vs bladebro (needs browser)
./target/release/bladebro audit               # 111 stealth vectors, must stay 111/111
```

No coverage tool is configured. No `cargo-llvm-cov`, `tarpaulin`, or coverage CI step was found.

## Test File Organization

**Location:**
- Unit tests: inline in the source file in a `#[cfg(test)] mod tests { ... }` block at the bottom. About 65 `#[cfg(test)]` blocks exist under `src/`. Examples: `src/action.rs` (block starts near line 39 per a line-position check; verify before relying on the exact line), `src/session_profile.rs`, `src/updater/download.rs`, `src/platform.rs`.
- Integration tests: separate files in `tests/` (top-level only, no nested folders). Each file is its own crate and imports the library as `bladebro::`.
- Test fixtures: `tests/fixtures/` (JSON captures, a stealth behavior script, a native behavior script, and `tests/vectors.html`).
- Browser fixtures for the verification tools: `tools/qol_probe/*.html` (`editor.html`, `shadow.html`, `extract.html`, `universal.html`, `router.html`, `reads.html`).

**Naming:**
- Test functions are descriptive snake_case sentences stating the behavior, not the method name:
  - `truncate_utf8_never_panics_on_multibyte_boundaries` (`tests/security_audit.rs`)
  - `permissions_preserve_native_arguments_promises_and_receiver_errors` (`tests/stealth_behavior.rs`)
  - `event_between_wait_creation_and_command_response_is_not_lost` (`tests/cdp_mock.rs`)
- Python test methods: `test_<behavior>` inside `unittest.TestCase` subclasses (`tools/diff_oracle/test_oracle.py`).

**Structure:**
```
src/<module>.rs                 # #[cfg(test)] mod tests { ... } at bottom
tests/<topic>.rs                # One integration test crate per topic
tests/fixtures/                 # Shared JSON / JS / HTML inputs
tools/<tool>/README.md          # Each verification instrument documents itself
tools/<tool>/test_*.py          # Unit tests for Python tools (only test_oracle.py exists)
```

## Test Structure

**Inline unit test pattern:**
- Helpers defined at the top of the test module build fixtures (`el`, `cap`, `el_fp` in `tests/page_unit.rs`).
- Assertions state the invariant in the message when non-obvious:

```rust
assert!(
    matches!(unrelated.await, Err(BladeError::Timeout(_))),
    "session filters must still reject root events"
);
```

**Integration test pattern (real protocol, no mocks of the client):**
- `tests/cdp_mock.rs` binds a real `TcpListener` on `127.0.0.1:0`, accepts a WebSocket with `tokio_tungstenite::accept_async`, and speaks minimal CDP JSON. It then drives the real `CdpClient`. The file header explains that this is deliberately a real-protocol test rather than a mock of the client.

**Subprocess pattern (daemon and CLI):**
- `tests/daemon_lifecycle.rs`, `tests/cli_endpoint.rs`, `tests/xdg_home.rs`, `tests/hermes_cli.rs`, `tests/rb_discovery.rs` run the built `bladebro` binary with an isolated home directory via `.env("BLADE_HOME", home)`. Never point these at a real browser profile.

**Node-backed injected-script pattern:**
- `tests/stealth_behavior.rs` shells out to `node tests/fixtures/stealth_behavior.cjs <scenario>` and asserts the exit status. Helper shape:

```rust
fn regression(scenario: &str) {
    let result = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/stealth_behavior.cjs"))
        .arg(scenario)
        .output()
        .expect("node is required for injected-script tests");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}
```

- Tests that require Node fail loudly with `expect("node is required ...")`. Node must be installed to run `cargo test`.

**Security regression pattern:**
- `tests/security_audit.rs` locks in fixes from a dated audit. Each test has a `// ── FIX: <title> ──` banner and a doc comment naming the exploit or PoC. Add a regression test here or in the matching module when fixing a security defect.

**Setup and teardown:**
- Isolation is by scratch directory and env var, not by global fixtures. Use `BLADE_HOME` set to a temp directory for anything that touches daemon state, sessions, or profiles.
- Servers spawned in tests are cancelled explicitly: `server.abort()` in `tests/cdp_mock.rs`. Drop the client before aborting.

## Mocking

**Framework:** None. No `mockall`, `mockito`, or similar crate is in `Cargo.toml`. Mocks are hand-written.

**Patterns:**
- Replace the transport with a local WebSocket server (`tests/cdp_mock.rs`). Prefer this over mocking traits.
- Construct synthetic inputs as plain structs. `tests/page_unit.rs` builds `RawElement` and `PageCapture` directly to test the Live Page Model without a browser.
- Build the test fixture with a helper function, and set only the fields under test (`el_fp` overrides `fingerprint` on top of `el`).

**What to Mock:**
- The CDP wire (use a local WebSocket server).
- Page captures and element lists (use synthetic `PageCapture`/`RawElement` values).

**What NOT to Mock:**
- The CDP client itself. Tests exercise the real multiplexer over a real socket.
- Native browser behavior for stealth. Stealth claims are proven against real Chrome by `tools/diff_oracle/oracle.py` and `bladebro audit`, not by unit tests with fake browsers. The regression tests exist to prevent injected JS from breaking native semantics.

## Browser-Dependent Verification

Unit and integration tests run without Chrome, except that `CHROME_PATH` is set in CI on Linux for browser-backed steps. Behavioral validation is carried by the tools in `tools/`, run by hand and in the "Live report regressions" CI steps (`.github/workflows/ci.yml`):

- `tools/qol_probe/run.py`: input/action behavior suite, 122 checks, local HTTP fixtures, isolated `BLADE_HOME`.
- `tools/diff_oracle/oracle.py`: differential oracle. `divergent: 0` is required. `--lane real` checks the real-browser lane.
- `tools/lane_matrix.py all 5`: cold-start matrix across five lanes, five rounds each.
- `tools/rb_live/attach_drift.py`: real-lane attach drift; run after touching `src/realbrowser.rs`.
- `tools/dsh_probe/run.mjs`, `tools/reliability_probe/*.py`: packaging, download, and polish checks in CI.

Each tool has a `README.md` in its folder. Read it before running.

## Fixtures and Factories

**Test Data:**
- Static JSON captures from live sites live in `tests/fixtures/` (`reddit_search.json`, `reddit_post_comments.json`, `x_tweet_detail.json`). Use these for site-adapter parser tests (`src/reddit/`, `src/x/`).
- Synthetic builders are inline in the test file (see `el`, `cap`, `el_fp` in `tests/page_unit.rs`).

**Location:**
- `tests/fixtures/` for shared inputs used by integration tests.
- Inline in `#[cfg(test)]` modules for test-only helpers.
- Browser HTML fixtures in `tools/qol_probe/`.

**Python fixture pattern:**
```python
with tempfile.TemporaryDirectory() as home:
    path = pathlib.Path(home) / "observations.json"
    path.write_text(json.dumps(json.dumps(battery)), encoding="utf-8")
```

## Coverage

**Requirements:** None enforced. No coverage threshold or CI coverage job exists.

**View Coverage:**
```bash
# Not configured. To measure ad hoc, install cargo-llvm-cov and run:
cargo llvm-cov --release
```

## Test Types

**Unit Tests:**
- Inline `#[cfg(test)]` modules next to the code they cover. Scope: pure logic (parsers, verdict classification, ref stabilization, path validation, UTF-8 truncation).

**Integration Tests:**
- `tests/*.rs`: the library through its public API (`tests/cdp_mock.rs`, `tests/page_unit.rs`), the built binary as a subprocess (`tests/daemon_lifecycle.rs`, `tests/cli_endpoint.rs`, `tests/hermes_cli.rs`), and Node-backed injected JS (`tests/stealth_behavior.rs`).

**Live Browser Verification (manual and CI):**
- The `tools/` instruments. Scope: end-to-end behavior against real Chrome and real sites. Required for stealth, input, and adapter changes. Live sites named in AGENTS.md: bot.sannysoft.com, incolumitas.com, abrahamjuliot.github.io/creepjs.

**E2E Tests:**
- Not used as a framework. The `tools/` suites serve this role.

## Common Patterns

**Async Testing:**
```rust
#[tokio::test]
async fn roundtrip_command_and_event() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // Spawn the mock server, then drive the real client.
    tokio::spawn(async move { /* accept_async, send frames */ });
    let client = CdpClient::connect(&format!("ws://{addr}")).await.unwrap();
    // ...
}
```
- Bind to port `0` to get a free port. Never hard-code ports in tests.
- Race-sensitive ordering (event before response) is tested explicitly. See `event_between_wait_creation_and_command_response_is_not_lost` in `tests/cdp_mock.rs`.
- Use `tokio::time::timeout` or the client's own `Duration` argument rather than sleeping.

**Error Testing:**
```rust
assert!(matches!(unrelated.await, Err(BladeError::Timeout(_))));
```
- Match on the concrete `BladeError` variant. Do not compare strings. The `degraded:true` tag is a contract, so a test asserting it should check the variant.
- For UTF-8 and parser boundaries, assert the invariant (`cut.len() <= 77`, `evil.starts_with(cut)`), not an exact output.

**Regression tests for bugs:**
- AGENTS.md requires a regression test in any bugfix PR ("a test that would have caught the bug"). Reproduce the failure first, then fix.
- Stealth and input fixes additionally need the `tools/` instruments run green and before/after numbers in the PR description.

**Python tool tests:**
- `unittest.TestCase` subclasses, run as scripts (`python tools/diff_oracle/test_oracle.py`). CI runs this before clippy (`.github/workflows/ci.yml`, "Oracle evidence parser" step).

## CI Test Gates

From `.github/workflows/ci.yml` (runs on ubuntu, macos, windows for PRs touching non-docs paths):
1. `cargo fmt --check`
2. `python tools/diff_oracle/test_oracle.py`
3. `cargo clippy --release -- -D warnings`
4. `cargo test --release`
5. `cargo build --release`
6. Native DSH and browser lifecycle, downloads, polish, and live report regressions (Linux runs Chrome and Xvfb).

Docs-only changes skip CI (`paths-ignore` for `**.md`, `Assets/**`, and others).

## Gaps

- No coverage measurement or threshold.
- Only `tools/diff_oracle/` has a Python unit-test file. Other `tools/` scripts are verified by running them against a browser.
- Some `#[tokio::test]` integration tests depend on loopback sockets and timing windows. Keep timeouts generous and avoid sleeps.

---

*Testing analysis: 2026-10-20*
