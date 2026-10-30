# Windows ACL decision - recordings dir

Date: 2026-10-21. Status: Decided (Phase 3). Requirement: SAFE-03 (Windows part).

## Context

- On Unix the recordings dir is 0700 and files are 0600 (`src/record/dir.rs`, `create_recording_file`).
- Windows has no mode bits. `src/` has no ACL handling anywhere (`.planning/codebase/CONCERNS.md`, L37: no `icacls`, `SetNamedSecurityInfo` or DACL code).
- New files and dirs inherit the parent directory's DACL.
- Default location is `%USERPROFILE%\.blade\recordings` (Windows data root is `%USERPROFILE%\.blade`, `src/platform.rs` `resolve_blade_dir`). A default user profile DACL grants access to the owner, SYSTEM and Administrators only, so the default dir is private by inheritance.
- A `BLADE_RECORD` dir elsewhere (e.g. a shared folder) inherits whatever that location grants.

## Options

| Option | How | Cost | Verdict |
|---|---|---|---|
| A. Inherit the parent ACL | Do nothing; rely on inheritance | No code, no runtime cost. Default dir stays user-private. A shared `BLADE_RECORD` location stays shared | CHOSEN |
| B. `icacls` via `std::process::Command` | `icacls <dir> /inheritance:r /grant:r *<SID>:(OI)(CI)F` | Process spawn per dir prep. Needs the current user SID (by-name grants break on renamed or localized accounts). Removing inheritance also drops SYSTEM and Administrators (backup, AV). Failure semantics unclear (warn or refuse?). Cannot be tested on the macOS host | Rejected for now |
| C. Raw Win32 FFI | `SetNamedSecurityInfoW` with a protected DACL, in the style of `src/platform/windows_job.rs` (hand-declared `extern "system"`, no windows crate) | Hand-written unsafe ACL/ACE building. No Windows CI run available before merge to verify it | Rejected for now |
| D. Crate (`windows-acl` / `windows-sys`) | Add a dependency | New dependency: binary size and supply chain (CLAUDE.md, "ask first") | Rejected: no new dependencies in Phase 3 (Cargo.toml unchanged) |

## Decision

Option A: inherit the parent ACL.

Implemented now: nothing for Windows. The `cfg(not(unix))` branch of `src/record/dir.rs` keeps the portable checks: symlink refusal, `is_dir`, `create_new` (O_EXCL) file creation, and the artifacts-prefix guard in `src/record/config.rs` resolve.

The README tells Windows users that recordings inherit the directory's permissions and to keep `BLADE_RECORD` inside their user profile.

## Revisit when

- A Windows CI runner is available to verify. Then prefer C over B: no process spawn and no SID-by-name handling.
- A user reports a recording leaked from a shared location.

## Verification status

Not executed on Windows (no CI run; nothing is pushed). The decision relies on the documented default profile DACL and on Windows inheritance behaviour. It is not a measured result.
