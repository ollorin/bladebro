---
phase: 04-ffmpeg-discovery-and-doctor
plan: 02
requirements: [OUT-02]
commits: [ac9e960, f7e51f1, 1b0813d]
---

# 04-02 Summary: doctor reports ffmpeg

Fork-only (SYNC-01). New `src/record/ffmpeg/report.rs` (`Level`, `DoctorLine`, `CHECK_NAME`, `install_hint`, pure `doctor_line`, impure `doctor_check`), re-exported via `record::ffmpeg` and `record`. The only pre-existing source file edited is `src/updater/doctor.rs` (+16 lines, 0 removed: one comment + `checks.push(check_ffmpeg());` after disk space, and `fn check_ffmpeg() -> Check`). Ledgered in `.planning/HOOKS.md` (row 4, commit f7e51f1). Never `Status::Fail`.

Also touched (own module): `parse.rs` got a `#[cfg(test)] EncoderSet::from_names`.

## Commits
- ac9e960 feat: ffmpeg doctor report with configured-format decision
- f7e51f1 feat: doctor reports ffmpeg for recording (hook)
- 1b0813d docs: ledger the doctor.rs hook

## Tests
`record::ffmpeg`: 35 -> 42 (18-row `doctor_table`, exact all-well line, name width, hints, no-probe, no-vp8-suggestion, `doctor_hook_is_wired`). Full suite: lib 401, total 442 across binaries (was 435), 0 failed. fmt and `clippy --release --all-targets -D warnings` clean.

## Live doctor output (macOS, scratch BLADE_HOME, BLADE_NO_UPDATE_CHECK=1)
Exit 1 in every run is from the unrelated "Version ... could not check for updates" warning/other checks; ffmpeg is never the cause (its level is only Pass/Warn).

(a) explicit override
```
  ✓ ffmpeg (recording)       /Users/sokolov.st@brainrocket.com/.local/ffmpeg/bin/ffmpeg (n9.0.2 via BLADE_FFMPEG; WebM/VP9 ok, MP4/x264 ok)
--- exit 1
```
(b) unusable override
```
  ⚠ ffmpeg (recording)       BLADE_FFMPEG=/nonexistent/ffmpeg unusable: not found; PATH not searched
    fix: fix or unset BLADE_FFMPEG (path to a working ffmpeg)
--- exit 1
```
(c) known-location fallback (PATH=/usr/bin:/bin:/usr/sbin:/sbin, BLADE_FFMPEG unset; /opt/homebrew/bin/ffmpeg and /usr/local/bin/ffmpeg absent, verified)
```
  ✓ ffmpeg (recording)       /Users/sokolov.st@brainrocket.com/.local/ffmpeg/bin/ffmpeg (n9.0.2 via known location; WebM/VP9 ok, MP4/x264 ok)
--- exit 1
```
(d) bad config (BLADE_RECORD_FORMAT=vp8)
```
  ⚠ ffmpeg (recording)       /Users/sokolov.st@brainrocket.com/.local/ffmpeg/bin/ffmpeg (n9.0.2 via BLADE_FFMPEG; WebM/VP9 ok, MP4/x264 ok); record config unreadable, assumed webm
    fix: BLADE_RECORD_FORMAT: format "vp8" is not supported; use webm or mp4 (default webm)
--- exit 1
```
(e) mp4 configured
```
  ✓ ffmpeg (recording)       /Users/sokolov.st@brainrocket.com/.local/ffmpeg/bin/ffmpeg (n9.0.2 via BLADE_FFMPEG; WebM/VP9 ok, MP4/x264 ok)
--- exit 1
```
Observation for (c): the known-location list found ffmpeg at `~/.local/ffmpeg/bin/ffmpeg` as expected. Cleanup: `bladebro stop` run; `pgrep -fl "bladebro|ffmpeg"` empty; no ~/.blade/recordings; git status shows only EXECUTION_LOG.md untracked.

## Notes for the coordinator
- D4-9: an old ffmpeg (major < 4) stays PASS with "(old; 4+ recommended)"; flip to WARN if preferred.
- `src/cli/help.rs` "13 checks" text is now stale (14 on macOS/Windows, 15 on Linux). Not edited, to keep upstream files untouched (SYNC-01).
- Row 16/D4-10: bad config yields WARN with the resolver error as the fix (no BLADE_FFMPEG mention, config-only issue).
- Not executed on Windows/Linux (no CI run; nothing is pushed).

## Self-Check: PASSED
Commits ac9e960, f7e51f1, 1b0813d and report.rs exist.
