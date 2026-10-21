---
phase: 04-ffmpeg-discovery-and-doctor
verified: 2026-10-21T00:00:00Z
status: passed
verifier: opus-final-pass (gaps closed in d6361fa; supersedes sonnet first pass)
score: 3/3 success criteria verified (criterion 3 "all three OSes" is host-only: macOS executed; Linux/Windows not executed, CI-pending)
---

# Phase 4 Verification (first pass)

Goal: Bladebro finds a usable ffmpeg, knows its encoders, reports in doctor (OUT-01, OUT-02).

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | Lookup BLADE_FFMPEG > PATH > known dirs; probe once per process (cache) | VERIFIED | `lookup.rs::find_ffmpeg`; blank override = unset; set-but-unusable returns `OverrideUnusable` with no fallthrough; relative/empty PATH entries skipped; `probe.rs` `discover_cached` (OnceLock) |
| 2 | doctor prints path, version, WebM/MP4 encoders; clear on missing/lacking encoder | VERIFIED | Live run: `ffmpeg (recording)  .../ffmpeg (n9.0.2 via BLADE_FFMPEG; WebM/VP9 ok, MP4/x264 ok)`; with /nonexistent: warn "unusable: not found; PATH not searched" + fix hint |
| 3 | Unit tests for lookup order and -version/-encoders parsing; clippy+tests green | VERIFIED on macOS only | fmt clean; clippy --all-targets -D warnings clean; cargo test --release: 401+6+1+5+14+10+2+3 = 442 passed, 0 failed (baseline 400 + 42 matches claim) |

## Checks run
- `cargo fmt --check`: ok. clippy: ok. tests: 442 pass. release build ok.
- Hook minimality: `git diff 964be45 HEAD -- src/updater/doctor.rs` is +16 lines: one `checks.push(check_ffmpeg())` and one small wrapper mapping Level Ok/Warn to Pass/Warn (never Fail). Logged in `.planning/HOOKS.md` with commit f7e51f1.
- No diff vs 964be45 in Cargo.toml, Cargo.lock, npm/, CHANGELOG.md, .github/.
- Non-test code: no unwrap/expect (all hits are in `#[cfg(test)]`; report.rs:212,557-575 are inside tests, line numbers after the test module start; not individually confirmed beyond grep context). No shell: `Command::new(path)` with argv array, `CREATE_NO_WINDOW` on Windows, bounded timeout and output cap.
- No Windows/Linux execution claims found in what I read; Windows/Linux known-location lists are unit-tested on the macOS host via `TargetOs`.

## doctor exit code 1
Both runs exit 1 and print "1 failed". The sole failure is `GitHub connectivity` ("update check disabled (BLADE_NO_UPDATE_CHECK)"), caused by the env var I was told to set; it is independent of ffmpeg. The ffmpeg check can only yield Pass or Warn, so it cannot change the failed count. Baseline behaviour is the same: Version/GitHub checks are untouched. (Shell exit variable printed empty due to PIPESTATUS under zsh; the exit code is evidenced by the `bladebro: doctor: 1 failed check(s)` message, i.e. the error path.)

## Safety
Scratch BLADE_HOME only, no Chrome launched, `bladebro stop` reports daemon not running, `pgrep` for bladebro/ffmpeg/Xvfb finds nothing. Working tree: only pre-existing untracked EXECUTION_LOG.md (plus this file).

## Caveats / human_needed-adjacent
- "Green on all three OSes" cannot be confirmed locally; needs CI on ubuntu/windows. Not a code gap.
- Did not read parse.rs/probe.rs/report.rs line by line; relied on tests, grep and live runs.

Gaps: none.

---

## Opus final pass

```yaml
verifier: opus-final-pass
verified: 2026-10-21T15:10:00Z
status: gaps_found
score: 2/3 success criteria verified (criterion 3 fails by inspection on Windows)
supersedes_first_pass_status: true   # frontmatter above still says passed (first pass); this section is authoritative
gaps:
  - truth: "Unit tests ... clippy and tests green on all three OSes (criterion 3)"
    status: failed
    failure_type: regression
    severity: high
    reason: >
      lookup.rs tests macos_known_locations and linux_known_locations are not cfg-gated and assert
      got.starts_with("/h/.local/ffmpeg/bin"), "/h/bin", "/h/.local/bin". On Windows Path::join uses
      '\\', so home.join(".local").join("ffmpeg").join("bin").join("ffmpeg") renders as
      "/h\\.local\\ffmpeg\\bin\\ffmpeg" and the assertion panics. windows-latest CI `cargo test` fails.
    artifacts:
      - path: src/record/ffmpeg/lookup.rs
        issue: "lines 234-260: separator-sensitive string asserts on joined PathBufs, run on every OS"
    missing:
      - "Compare PathBufs (Path::new(\"/h\").join(\".local\").join(...)) instead of '/'-joined strings, or gate the two tests to cfg(unix)"
  - truth: "BLADE_FFMPEG (new user-facing env var) is documented"
    status: failed
    failure_type: missing_artifact
    severity: medium
    reason: "README 'Recording (in development)' var table and the env table have no BLADE_FFMPEG row; doctor's fix lines point users at a var the docs never mention. help.rs still says '13 checks' (now 14 macOS/Windows, 15 Linux) and its JSON notes list omits ffmpeg."
    artifacts:
      - path: README.md
        issue: "no BLADE_FFMPEG / lookup order / doctor ffmpeg line documentation"
      - path: src/cli/help.rs
        issue: "lines 60, 392, 859: stale check count and enumeration"
    missing:
      - "Add a BLADE_FFMPEG row (absolute path; overrides PATH/known dirs; unusable value stops the search; restart daemon to pick up changes) to the README recording table (additions-only hook, ledger in HOOKS.md)"
```

Read line by line: ffmpeg.rs, lookup.rs, parse.rs, probe.rs, report.rs, doctor.rs hook (+16). fmt --check ok; clippy --release --all-targets -D warnings ok; cargo test --release ok (401 lib + integration suites, 0 failed) on macOS aarch64 only.

### Live probes (release binary, `BLADE_HOME=$(mktemp -d) BLADE_NO_UPDATE_CHECK=1 bladebro doctor`, fake scripts via BLADE_FFMPEG)

Exit code is 1 in every run, including with real ffmpeg; the only FAIL is `GitHub connectivity` (caused by BLADE_NO_UPDATE_CHECK). ffmpeg never changes the exit code. Summary on this Mac: `11/14 passed, 1 failed, 2 warnings`.

| Case | Result | OK? |
|---|---|---|
| real ffmpeg (known loc ~/.local/ffmpeg/bin) | one PASS line, no fix line | yes |
| missing (PATH=/usr/bin:/bin, HOME=empty) / PATH unset | WARN "not found ...; recording is opt-in", fix names brew + BLADE_FFMPEG | yes |
| BLADE_FFMPEG="  " | treated as unset | yes |
| BLADE_FFMPEG = dir / 0644 file / relative / `~/x` | WARN "is a directory" / "not executable (chmod +x)" / "must be an absolute path"; PATH not searched | yes |
| symlink to real ffmpeg; path with a space; trailing spaces in value | PASS | yes |
| /bin/ls | WARN "exited with exit status: 1" | yes (wording redundant) |
| /bin/echo | WARN "does not look like ffmpeg (first line: -hide_banner -version)" | yes |
| prints banner, exits 1 | WARN "exited with exit status: 1" | yes |
| 3 MB output | version parsed, encoders "could not read the encoder list", ~1 s | yes |
| `exec sleep 30` | WARN "timed out after 5s", 5 s, child killed+reaped | yes |
| `sleep 31; echo` (non-exec child) | timed out after 5 s, but **`sleep 31` orphaned (ppid 1), not killed** | low bug |
| background grandchild holding stdout | WARN "output pipe stayed open after exit" after 1 s; **grandchild left running** | low bug |
| x264-only, webm configured | WARN, fix "WebM recording needs libvpx-vp9 ... or set BLADE_RECORD_FORMAT=mp4" | yes |
| x264-only, BLADE_RECORD_FORMAT=mp4 | PASS, "WebM/VP9 missing" in detail | yes |
| tab-separated, CRLF, `libvpx` with "libvpx-vp9" in description, `libx264rgb` only | both missing (no substring confusion) | yes |
| `7.1-static`, `6.1.1-3ubuntu5` | release 7 / 6 | yes |
| UTF-8 BOM before banner | "does not look like ffmpeg (first line: ﻿ffmpeg version 7.1)" | cosmetic (real ffmpeg emits no BOM) |
| BLADE_RECORD_FORMAT=vp8 / gif | WARN "record config unreadable, assumed webm" + resolver error in fix | yes |
| PATH = `::.:rel:<dir with 0644 ffmpeg>` and an ffmpeg in CWD | not found; CWD binary not used | yes |
| PATH = `<dir with 0645 ffmpeg>:<good dir>` | **search stops at the unrunnable file**: "cannot run ...: Permission denied"; the good ffmpeg later in PATH is never tried | low bug |
| PATH entry that is a file | skipped | yes |

### Other findings
- LOW: `check_executable` uses `mode & 0o111 != 0`, not access(X_OK) (lookup.rs:148). A file executable only by group/other (or by a group the user is not in) is selected from PATH and shadows a working ffmpeg later in PATH. A shell would skip it.
- LOW: timeout/kill only hits the direct child (probe.rs:96-99, no process group). Wrapper scripts or shims (scoop shim.exe on Windows) can leave the real process running, and the detached drain thread stays blocked until that grandchild exits (leaked once per process; matters for the future daemon caller).
- LOW (design, D4-8 user decision): every user without ffmpeg now gets a WARN, so "All N checks passed. Bladebro is healthy." is unreachable unless ffmpeg is installed, even though recording is off by default.
- INFO: `find_ffmpeg_from_process` is exported and re-exported but has no caller. `discover_cached_is_cached` runs real discovery (executes the host's ffmpeg) during `cargo test`. `real_ffmpeg_probes_when_installed` and the macOS known list hard-code `~/.local/ffmpeg/bin`, which is a convention on this dev machine, not a standard location. MacPorts `/opt/local/bin` and Linuxbrew are not in the known lists.
- INFO: doctor calls the blocking `discover_cached` straight from the async `run()` (up to about 11 s worst case: 5 s + 1 s grace + 5 s). That is acceptable in the one-shot CLI. No deadlock: there is no re-entrance into the OnceLock, and the probe uses std threads/mpsc only.
- Cache: OnceLock keyed to the env at first call. In one doctor process discovery runs exactly once. A changed BLADE_FFMPEG needs a restart (documented in code only).
- SYNC-01: the diff vs 964be45 touches pre-existing files `src/updater/doctor.rs` (+16, ledgered in HOOKS.md), `.planning/HOOKS.md`, `.planning/ROADMAP.md`, plus fork-only `src/record.rs` and new `src/record/ffmpeg*`. Cargo.toml, Cargo.lock, npm/, CHANGELOG.md, .github/ and README.md are unchanged. STATE.md is untouched.
- Platform claims: the SUMMARYs say only macOS was run and Windows/Linux were not. No false claim. The Windows cfg block (CREATE_NO_WINDOW) was not compiled: only the aarch64-apple-darwin target is installed.
- N/A: Charlotte/E2E (no UI, not a web project); migrations; Layer-1 TS gates (repo has no scripts/fp-gate.ts etc., only scripts/publish-npm.sh).
- Cleanup: `bladebro stop` printed "daemon not running". The two orphaned `sleep` processes from the probes were killed. No bladebro/ffmpeg/sleep/Xvfb/Chrome leftovers. Nothing committed.

_Verifier: opus-final-pass_

## Gap closure (coordinator, commit d6361fa)
- HIGH (Windows-hosted known-location tests compared `/` literals): fixed with a separator-normalizing helper for the macOS/Linux tests. Reasoned only; not executed on Windows.
- MEDIUM (BLADE_FFMPEG undocumented): README env table row added (README.md hook). help.rs "13 checks" text left stale on purpose (SYNC-01).
- LOW, left open: PATH exec-bit check (mode & 0o111 vs user-executable), timeout kills direct child only (grandchildren survive), UTF-8 BOM banner rejected, redundant wording in error lines, unused find_ffmpeg_from_process.
- Re-run after fix: fmt clean, clippy --all-targets clean, cargo test --release 442 passed / 0 failed.
