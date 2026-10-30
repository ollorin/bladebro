# Codebase Concerns

**Analysis Date:** 2026-10-20

Scope: Rust crate `bladebro` v4.6.5 (`Cargo.toml`), with the npm distribution wrapper in `npm/`, release scripts in `release.sh` and `scripts/publish-npm.sh`, and CI in `.github/workflows/ci.yml`. Findings are ordered by risk within each section.

## Security Considerations

**Self-update integrity rests on same-origin checksums; no signature:**
- Risk: The checksum file (`<asset>.sha256`) is fetched from the same release as the binary it protects. A compromised release (maintainer account, CI, or asset upload) can publish a matching binary and checksum, and the updater then executes the downloaded binary during `verify_binary_runs` before the swap. There is no detached signature, no embedded public key, and no provenance check.
- Files: `src/updater/download.rs` (`verify_sha256`, `verify_binary_runs`, `checksum_required`), `src/updater/mod.rs` (`update`, Steps 3 and following), `release.sh` (lines ~283-289 generate `.sha256` on the maintainer's machine), `Cargo.toml` (comment on the `sha2` dependency acknowledges this).
- Current mitigation: Fail-closed checksum for tags >= `3.3.0` (`FIRST_CHECKSUMMED_TAG`); bounded checksum body size (16 KiB); randomized temp filename to prevent symlink pre-placement; `verify_backup` refuses symlinks and non-files in `src/updater/swap.rs`.
- Gap: Tags below `3.3.0` take a warn-and-skip path (`download.rs`, "legacy release"), so an explicit `--force` downgrade to a legacy tag performs no integrity check at all and still executes the binary.
- Recommendations: Sign release artifacts (minisign or cosign) with the public key compiled into the binary, verify the signature before `verify_binary_runs`, and generate artifacts and checksums in CI rather than on a maintainer machine (`release.sh`, `scripts/publish-npm.sh` publish directly with `npm publish --access public`, no provenance attestation found). Consider refusing `--force` downgrades that skip checksum verification.

**Write-path validation is a denylist with gaps:**
- Risk: `validate_write_path` is the guard against prompt-injected writes (PDF export, downloads). It blocks a fixed set of system prefixes, credential path components, and a list of shell rc files, but it does not block macOS per-user launch agents or several other persistence and config locations. A prompt-injected page could steer a write to such a path.
- Files: `src/platform.rs` (`validate_write_path`, `BLOCKED_COMPONENTS`, `RC_FILES`, Unix `blocked_prefixes`, Windows `blocked_win`), `src/mcp/server/files.rs` (`handle_pdf`, `ensure_new_output`), `tests/security_audit.rs`.
- Specific gaps observed:
  - No entry for `~/Library/LaunchAgents` (macOS login persistence). `grep` for `LaunchAgents` across `src/` returns nothing.
  - `RC_FILES` only matches a file directly inside `$HOME`; rc-like files in subdirectories are not blocked.
  - Unix prefix list omits `/opt`, `/Library`, `/System`, `/Applications`, and most of `/var` (only `/var/log` and `/var/spool` are listed).
  - Windows only blocks `C:/Windows`, `C:/Program Files`, `C:/ProgramData/...start menu`, and a Startup-folder substring; no `%APPDATA%` coverage beyond npmrc.
  - Check-then-write: `ensure_new_output` and `validate_write_path` run before the write, so a symlink swapped into place between check and write is not caught (TOCTOU).
- Recommendations: Invert to an allowlist for the PDF and download destinations (for example, only the artifacts directory or an explicit user-provided path already inside an allowed root), or at minimum add `Library/LaunchAgents`, `Library/LaunchDaemons`, `/Library`, `/System`, `/opt`, and rc-like names at any depth. Open the destination with `O_NOFOLLOW`/`create_new` at write time instead of relying on a prior check.

**Daemon socket is bound before it is restricted:**
- Risk: `UnixListener::bind` creates `cli.sock` with the process umask, and only then is mode 0600 applied. The chmod result is discarded with `let _ =`. Between those calls, a loosened umask exposes the socket to other local users.
- Files: `src/cli/daemon.rs` (lines ~121-131).
- Current mitigation: `peer_uid_matches_self` (SO_PEERCRED / getpeereid) rejects other-uid peers in `src/cli/daemon.rs` (~line 177), and `socket_dir_trusted` in `src/cli.rs` requires a real, owner-owned, non-group/world-writable parent dir before the client trusts the socket. Both are strong, so this is a hardening gap rather than an open hole.
- Recommendations: Set a restrictive umask around `bind` (or bind inside a 0700 directory) and treat chmod failure as fatal.

**Credentials stored at rest in plaintext:**
- Risk: Live session cookies are snapshotted to a sidecar file (`src/logins.rs`, `logins_path`, `snapshot`), profile directories hold a full Chrome profile, and `src/fingerprint.rs` writes the `BLADE_PROXY` URL into a persisted file, which the code comment says "may embed proxy credentials". Anyone who can read the user's home directory can replay sessions.
- Files: `src/logins.rs`, `src/fingerprint.rs` (lines ~47-90), `src/session_profile.rs`, `src/state.rs`, `src/knowledge.rs` (`~/.blade/knowledge/`).
- Current mitigation: Files are written through `platform::secure_write_file` (mode 0600 on Unix, `src/platform.rs` ~line 207) and profile dirs are 0700 (`src/session_profile.rs` ~lines 104-205).
- Gap: On Windows, no ACL handling exists anywhere in `src/` (`icacls`, `SetNamedSecurityInfo`, DACL searches returned nothing), so files inherit default profile ACLs.
- Recommendations: Redact credentials from proxy URLs before persisting (store a reference to an env var instead), document the at-rest threat model in README, and apply an explicit user-only DACL on Windows.

**Silent security downgrades on launch:**
- Risk: If sandboxed Chrome dies at startup, the launcher retries with `--no-sandbox` automatically (`src/browser/launch.rs` ~lines 29-53 and ~409, `src/browser/pipe.rs` ~26-39). `src/browser/flags.rs` also always adds `--enable-unsafe-swiftshader` (lines ~319, ~414). The downgrade is reported only as a note (`src/browser/alerts.rs`), and an agent may not read it.
- Current mitigation: The code comment in `src/browser/flags.rs` (line 7) records the policy; the alert is surfaced in the output.
- Recommendations: Make the no-sandbox fallback opt-in via an env var for root or container cases, or emit it in the structured verdict rather than as a note.

**Arbitrary Chrome flags from the environment:**
- Risk: `BLADE_CHROME_FLAGS` is split on whitespace and appended to the Chrome command line unchecked (`src/browser/launch.rs` ~line 399, `src/browser/compat.rs` ~line 74). Any flag, including ones that disable site isolation or web security, is accepted.
- Current mitigation: Environment variable only; not reachable from page content.
- Recommendations: Keep as-is for power users, but document the risk and add a denylist for `--no-sandbox`, `--disable-web-security`, and `--disable-site-isolation-trials`.

**Audit writeup referenced but not tracked:**
- Risk: `tests/security_audit.rs`, `src/updater/download.rs`, and `AGENTS.md` cite `SECURITY-AUDIT-2026-08-16.md` as the source of truth for fixes. `.gitignore` excludes `SECURITY-AUDIT-*.md`, so the record is absent from the repository and the regression tests cannot be traced to their rationale by someone cloning the repo.
- Recommendations: Publish a sanitized audit summary under `docs/` or in `SECURITY.md`, or stop referencing the ignored file.

## Tech Debt

**Production `unwrap()` / `expect()` against AGENTS.md rule:**
- Issue: `AGENTS.md` ("Code conventions") says no `unwrap`/`expect` in non-test code outside provably-guarded cases. Non-test sites that depend on an invariant rather than a checked branch include:
  - `src/mcp/server/serve.rs` (lines ~311, ~360, ~415, ~421) and `src/cli/daemon.rs` (lines ~304, ~364): `page.as_mut().unwrap()`.
  - `src/action/perform.rs` (lines ~132, ~330, ~403, ~502, ~529, ~752, ~807): `sig_frame.as_ref().unwrap()` in per-action arms, relying on `sig_frame` being populated earlier (`match action.ref_id()` at line ~87).
  - `src/page/mod.rs` (line ~557), `src/action/verdict.rs` (lines ~201, ~244): `toggle.unwrap()`.
  - `src/main.rs` (line ~254): `browser.as_ref().unwrap()` (guarded by the same condition that created it, so low risk).
- Files: as listed above.
- Impact: A broken invariant panics the process. The MCP serve loop has the only `catch_unwind` in `src/` (`src/mcp/server/serve.rs` ~line 312); the daemon path (`src/cli/daemon.rs`) has no equivalent, so a panic there takes down the daemon and every connected session.
- Fix approach: Convert the `page` and `sig_frame` lookups to `ok_or_else(...)` returning an actionable error with page state (as AGENTS.md requires for action errors). Add `catch_unwind` or a per-request guard to the daemon loop.

**Heavy use of discarded results:**
- Issue: 306 occurrences of `let _ = ` across `src/`. Most are best-effort cleanup (acceptable), but several discard security-relevant results, including the socket chmod (`src/cli/daemon.rs` ~line 130), knowledge-base writes (`src/knowledge.rs` `sync`, which ignores `secure_write_file` and `rename` failures), and `taskkill` results (`src/platform.rs` ~lines 902, 923).
- Files: `src/knowledge.rs` (`sync`), `src/platform.rs`, `src/cli/daemon.rs`, `src/session_profile.rs`.
- Impact: Failures are invisible to the agent. For example, a failed knowledge write silently loses learned site data, and a failed chmod leaves the socket looser than intended.
- Fix approach: Log failures to stderr at minimum (stdout is a machine contract per AGENTS.md) and propagate errors on security-relevant paths.

**Oversized modules:**
- Issue: 38 Rust source files exceed 500 lines. The largest are `src/platform.rs` (1394), `src/updater/download.rs` (1128), `src/knowledge.rs` (1092), `src/page/attach.rs` (1046), `src/session_profile.rs` (1028), `src/action/perform.rs` (974), `src/mcp/server/extract.rs` (973), `src/cli/help.rs` (962).
- Impact: `src/platform.rs` mixes OS abstraction, path validation, process inspection, and file I/O, so a change to one concern touches code that is reviewed for another. AGENTS.md already asks for thin cores with focused children; several of these files have not been split.
- Fix approach: Split `src/platform.rs` into `platform/paths.rs` (validation), `platform/process.rs`, and `platform/fs.rs` following the existing `foo.rs` + `foo/` pattern.

**Duplicated subprocess and path logic:**
- Issue: `df` is invoked in two places (`src/updater/download.rs` ~line 486 and `src/updater/doctor.rs` ~line 651), `which` and `ps` are spawned from multiple modules (`src/updater/doctor.rs`, `src/session_profile/reap.rs` ~line 212, `src/platform.rs` ~lines 815, 861, 873), and `libc::geteuid()` ownership checks are repeated across `src/cli.rs`, `src/cli/hermes.rs`, `src/session_profile.rs`, and `src/updater/swap.rs`.
- Fix approach: Centralize in `platform.rs` helpers (for example `platform::owned_by_me(path)` and `platform::run_capture(cmd)`), so security checks live in one place.

**Repository hygiene:**
- `.intentionally-empty-file.o` is a tracked zero-byte file with no references anywhere in the repo.
- `Assets/video/demo.gif` (79k lines, tracked) makes `.git` 44 MB; `CHANGELOG.md` (~189 KB) and `README.md` (~47 KB) are large and partly duplicated.
- Fix approach: Remove the stray file; move the demo GIF to release assets or Git LFS.

## Known Bugs and Fragile Behavior

**Test temp-file collisions:**
- Symptoms: Tests write to fixed names in the shared temp dir (for example `std::env::temp_dir().join("bladebro-pdf-tests")` and `pdf-state.js` in `src/mcp/server/see.rs` ~lines 517-530). Parallel test runs or a pre-existing directory owned by another user can cause spurious failures.
- Files: `src/mcp/server/see.rs`, `src/mcp/server/extract.rs` (~lines 823-870), `src/browser/display.rs` (~lines 519-528).
- Trigger: `cargo test` run concurrently by two processes (for example two CI matrix jobs on one host, or a developer running tests while a background job runs).
- Workaround: Set `TMPDIR` to a per-run directory.

**Test pid-based Xvfb cleanup:**
- Symptoms: `src/session_profile/reap.rs` tests spawn a real `Xvfb` via `sh -c` on displays 150-199 and clean up by PID. If the test panics before cleanup, the display and its lock file persist.
- Files: `src/session_profile/reap.rs` (`orphan_display`, ~lines 395-440).
- Workaround: Run `bladebro doctor` (`src/updater/doctor.rs`, `check_stale_locks`) and remove `/tmp/.X*-lock` manually.

## Performance Bottlenecks

**Per-command CDP timeout is long:**
- Problem: `DEFAULT_TIMEOUT` is 30 seconds (`src/cdp/client.rs` line 34), and the daemon client read timeout defaults to 300 seconds (`BLADE_CMD_TIMEOUT`, `src/cli.rs` ~line 327).
- Files: `src/cdp/client.rs`, `src/cli.rs`.
- Cause: A wedged page can block one command for up to 5 minutes before the agent sees an error.
- Improvement path: Shorten the default for read-only commands (`see`, `state`) and surface a "still running" heartbeat instead of waiting for the full timeout.

**Blocking subprocess calls in the updater and doctor:**
- Problem: `std::process::Command` is used for `df` (`src/updater/download.rs` ~line 486) and `doctor` checks. These are synchronous calls in functions that run inside a tokio runtime.
- Files: `src/updater/download.rs`, `src/updater/doctor.rs`.
- Cause: Synchronous process spawn on the runtime thread.
- Improvement path: Use `tokio::process::Command` or `spawn_blocking` for these calls; they are short today, so the impact is low.

**Knowledge base rewrites whole files:**
- Problem: `KnowledgeBase::sync` (`src/knowledge.rs` ~line 401) rewrites each dirty domain JSON file in full and then rewrites `stats.json`, on every dirty flush in the daemon and MCP loops (`src/cli/daemon.rs` ~lines 221, 427, 450; `src/mcp/server/serve.rs` ~lines 561, 623).
- Cause: Full-file serialization with no batching window.
- Improvement path: Debounce flushes (for example, at most once per N seconds) and write only changed domains.

## Fragile Areas

**Injected stealth JavaScript:**
- Files: `src/stealth/inject.rs`, `src/stealth/inject/gpu.rs` (920 lines), `src/stealth/js/`, `src/page/js/`.
- Why fragile: Patches are assembled as strings. One syntax error silently disables every patch (AGENTS.md calls this out). The `Function.prototype.toString` rule and the Proxy-first rule (AGENTS.md non-negotiable #7) are enforced by review only.
- Safe modification: Always run `BLADE_DUMP_INJECT=... node --check` on the assembled output, and re-run `bladebro audit` (111 vectors) and `tools/diff_oracle/oracle.py` before merging.
- Test coverage: `tests/stealth_behavior.rs` and `tests/fixtures/native_behavior.js` cover behavior; full fidelity depends on live sites (bot.sannysoft.com, incolumitas.com, creepjs), which are not reproducible in CI.

**Stealth baseline is Chrome-version-sensitive:**
- Files: `src/browser/flags.rs`, `src/stealth/`, `STEALTH_BENCH.md`, `SUPPORT-MATRIX.md`.
- Why fragile: AGENTS.md states the audited numbers were measured against Chrome 151 (v4.0.0). CI installs `google-chrome-stable` from the apt repo unpinned (`.github/workflows/ci.yml` line 44), so the Linux stealth floor can change between runs without any code change.
- Safe modification: Pin the Chrome major version in CI and record the version in the audit output.

**Page-invariant unwraps in action dispatch:**
- Files: `src/action/perform.rs`, `src/mcp/server/serve.rs`, `src/cli/daemon.rs`.
- Why fragile: Changing the order of `sig_frame` construction or the `page` lifecycle (relaunch on idle, see `src/mcp/server/serve.rs` ~lines 237-249 and 344) can turn a previously-safe path into a panic.
- Safe modification: Add a regression test for each action arm that exercises a missing ref or a closed page before changing the setup code.

**Unix-only daemon and peer credential check:**
- Files: `src/cli/daemon.rs` (`run_daemon` and `stop_daemon` are `#[cfg(not(unix))]` stubs at ~lines 464 and 623), `src/cli.rs` (`peer_uid_matches_self` returns `true` on platforms other than Linux, Android, and macOS, ~lines 118-124; `daemon_running` returns `false` off Unix, ~line 319).
- Why fragile: Behavior diverges by platform. On Windows, every command is one-shot and cold-launches Chrome, which affects the lifecycle and stealth characteristics documented for the daemon lane.
- Safe modification: Keep platform differences behind `src/platform.rs` as AGENTS.md requires, and test the one-shot path on Windows in CI.

## Scaling Limits

**Single-page model per daemon:**
- Current capacity: The daemon holds one `Option<Browser>` and one `Option<Page>` (`src/cli/daemon.rs` ~lines 136-137). Requests are serialized through one socket loop.
- Limit: Parallel agents on one machine must each run a separate daemon or fall back to one-shot launches, each of which starts a fresh Chrome.
- Scaling path: Multiplex pages per daemon with a per-session map keyed by session name (`src/session_profile.rs`), or document the one-daemon-per-agent constraint in `README.md`.

**CDP event bus capacity:**
- Current capacity: Events are fanned out through a `tokio::sync::broadcast` channel (`src/cdp/client.rs`, `subscribe`), with oversized payloads shrunk in `shrink_oversized_event` (~line 391).
- Limit: A slow subscriber drops events (broadcast lag), and the code does not surface lag to the caller as an error.
- Scaling path: Count lagged receives and include them in the verdict so an agent knows a wait may have missed a page event.

## Dependencies at Risk

**Supply-chain gating is thin:**
- Risk: `deny.toml` only sets `[advisories] yanked = "deny"`. No `cargo audit` or `cargo deny check` step runs in `.github/workflows/ci.yml`. `release.sh` requires `cargo-deny` locally (line 59) but CI does not.
- Impact: Known RustSec advisories in the crates resolved in `Cargo.lock` (for example via `reqwest` 0.12 with rustls, `tokio-tungstenite` 0.30) are not blocked by CI.
- Migration plan: Add a `cargo deny check advisories licenses` job to CI and keep `dependabot.yml` active (it is present at `.github/dependabot.yml`).

**Node-based distribution wrapper:**
- Risk: `npm/bladebro/bin.js` and `npm/bladebro/binary.cjs` spawn the platform binary via `spawnSync`, and the optional-dependency packages (`npm/bladebro-*`) are published by hand through `scripts/publish-npm.sh` and `release.sh`.
- Impact: A compromised npm publish token can ship a different binary to every npm user with no CI attestation.
- Migration plan: Publish from CI with npm provenance (`--provenance`) and tie each optional package to a signed release.

## Missing Critical Features

**Release signing and reproducible builds:**
- Problem: No signing, provenance, or SBOM is produced for binaries (see the self-update section above).
- Blocks: Users cannot independently verify that a binary came from this source tree.

**Windows verification depth:**
- Problem: `SUPPORT-MATRIX.md` (line 14) records that the Windows stealth audit ran in CI, but "full local floor not run". Windows parity for the daemon, ACLs, and job-object process handling (`src/platform/windows_job.rs`) is verified only in CI.
- Blocks: Confident Windows fixes without a Windows host; regressions in the Windows-only paths show up late.

## Test Coverage Gaps

**Real-browser behavior is not covered by `cargo test` alone:**
- What's not tested: Live-site behavior (Reddit, X, consent banners, bot challenges) and the full stealth floor. CI runs `tools/reliability_probe/*.py`, `tools/dsh_probe/*.mjs`, and `tools/qol_probe/run.py` against local fixtures, plus a live-site audit, but these depend on a real Chrome and on external sites that change.
- Files: `tools/reliability_probe/`, `tools/qol_probe/`, `tools/dsh_probe/`, `tools/rb_live/`, `src/x.rs`, `src/reddit.rs`, `src/x/`, `src/reddit/`.
- Risk: Adapter DOM assumptions in `src/x.rs` and `src/reddit.rs` break silently when the sites change; the adapters are tested only against stored JSON fixtures (`tests/fixtures/x_tweet_detail.json`, `tests/fixtures/reddit_*.json`).
- Priority: High for `src/x.rs` and `src/reddit.rs` (user-visible `extract=auto` paths); Medium for the real-browser lane.

**Write-path validation has limited adversarial coverage:**
- What's not tested: `tests/security_audit.rs` covers `/etc`, `.ssh`, `.aws`, `.config/autostart`, and systemd user units, but not macOS `Library/LaunchAgents`, rc files in subdirectories, or a symlink swapped between validation and write.
- Files: `src/platform.rs` (`validate_write_path`), `tests/security_audit.rs`.
- Risk: The gaps listed in the Security section stay open without a failing test.
- Priority: High.

**Updater downgrade path:**
- What's not tested: Installing a legacy (pre-3.3.0) tag via `--force`, which skips checksum verification by design (`src/updater/download.rs`, `checksum_required`).
- Files: `src/updater/download.rs`, `src/updater/mod.rs`, `src/updater/swap.rs`.
- Risk: The warn-and-skip path executes an unverified binary and nothing asserts that the warning is shown or that the binary is run only after explicit confirmation.
- Priority: Medium.

---

*Concerns audit: 2026-10-20*
