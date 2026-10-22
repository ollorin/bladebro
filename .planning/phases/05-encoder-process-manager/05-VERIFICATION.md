---
phase: 05-encoder-process-manager
verified: 2026-10-22T08:16:11Z
verifier: opus-adversarial
status: gaps_found
score: 3/5 must-haves verified (SC1, SC2, SC3 verified on macOS; SC4 written-not-executed; derived truth T5 failed)
re_verification:
  previous_status: gaps_found
  previous_score: 3/4 (sonnet first pass)
  gaps_closed: []
  gaps_remaining:
    - "SC4 / LIFE-05 Windows CI run"
  regressions: []
gaps:
  - truth: "SC4 / LIFE-05: on Windows the stdin-EOF finalize path works with no console window, verified by a fake-encoder lifecycle test running in the windows-latest CI job"
    status: failed
    failure_type: missing_test
    reason: "src/record/encoder/windows_tests.rs is cfg(windows): never compiled or run (macOS only, nothing pushed). SC4 stays 'written, not executed'. By reasoning, w4 will probably FAIL: Rust std does not check that a .cmd/.bat program exists. It runs cmd.exe /c <path> instead, so Encoder::start returns Ok, cmd.exe then prints 'not recognized' and exits 1, and `.err().unwrap()` panics."
    artifacts:
      - path: "src/record/encoder/windows_tests.rs"
        issue: "never compiled or executed; w4_missing_binary_is_a_spawn_error likely panics on Windows (Ok instead of Spawn error)"
    missing:
      - "Push the branch and get a green windows-latest job (ci.yml already has windows-latest and runs `cargo test --release`)"
      - "w4: either point it at a non-batch missing path (e.g. no-such-ffmpeg.exe, which CreateProcessW rejects), or accept Ok + an ExitFailed outcome after finish"
  - truth: "T5 (derived from the goal 'reports success only when proven'): a Finalized verdict never hides frames that were accepted but not written"
    status: failed
    failure_type: semantic_stub
    reason: "When the writer still has more than FINISH_TIMEOUT (10 s) of writes pending at stop, finish() aborts it. ffmpeg then gets EOF and exits 0, so the outcome is Finalized. The frames still in the channel, and the rest of the pending repeat count, are silently discarded: they are counted in frames_queued but never as written or dropped. The verdict says '... N frames written, 0 dropped at the queue'. Reproduced with real ffmpeg: 64 frames 60 s apart (intended 63 min of video) gave a Finalized webm of 1787 s and an mp4 of 2092 s, with the verdict reporting 0 dropped. It is hard to trigger in normal use, because the hold flush keeps the backlog to about 1 s. Triggers are clock jumps (each gap is clamped to 10 min, not less) or a host where ffmpeg runs well below real time."
    artifacts:
      - path: "src/record/encoder/process.rs"
        issue: "finish(): the writer-timeout branch (abort) is not recorded anywhere in the report or the outcome"
    missing:
      - "Record the writer abort (e.g. a Shared flag or the remaining rx.len() + 1) and either classify it as a distinct non-Finalized outcome or make the Finalized verdict say 'last K accepted frames discarded at the 10 s stop deadline'. Add the frames that were never written to the dropped count, so queued == written-source + dropped holds."
      - "Regression test: fake `cat > $out` encoder + frames spaced 600 s apart + finish_timeout 500 ms -> verdict must not claim 0 dropped / complete"
  - truth: "The unix matrix is reliable under CI-like load (the suite is the evidence for SC2)"
    status: failed
    failure_type: missing_test
    reason: "matrix_tests::j_idle_hold_flushes_before_finish flakes under load: 'file only 0 bytes before finish' (matrix_tests.rs:239). It failed in 7/18 runs (3 parallel cargo test, 24 busy loops, 12 cores) and 15/15 runs (5 parallel, 48 busy loops). Root cause, measured with 36 concurrent encoders using new fake scripts: macOS takes up to 3.3 s to exec a freshly written #!/bin/sh script under concurrency, so `cat > $out` has not even created the file 2.5 s in, while the writer had already written to the pipe. It is a test-timing assumption, not an encoder bug. In the first loaded run, success_finalizes_and_reaps, non_zero_exit_reports_stderr, a_empty_precreated_output, b_missing_output and c_no_frames also failed once, at their outcome asserts (messages not captured). The likely cause is the same: the script did not start within finish_timeout + 2 s, so the encoder was killed and reported Unfinalized. 45 later loaded runs did not reproduce those. macos-latest CI runners are slow, so expect j to flake there."
    artifacts:
      - path: "src/record/encoder/matrix_tests.rs"
        issue: "j asserts on the fake script's output file after a fixed 2.5 s sleep, which depends on shell exec latency"
    missing:
      - "j: first wait (bounded poll) until the fake has created its output file, then sleep 2.5 s and assert; or assert on a writer-side counter"
      - "Optionally give the 5 s-timeout fake tests a larger finish_timeout (e.g. 15 s). They only wait that long when something is wrong."
---

# Phase 5: Encoder Process Manager, Verification Report (adversarial final)

**Phase goal:** Given a stream of JPEG frames, Bladebro produces a finalized video file and reports success only when that is proven, on Linux, macOS and Windows.
**Verified:** 2026-10-22T08:16:11Z, verifier opus-adversarial (this replaces the earlier sonnet first-pass report; its useful content is kept below)
**Status:** gaps_found
**Re-verification:** Yes. It re-checks the first pass, which only skimmed pace.rs and the matrix bodies and did no load testing.

**Verdict for the coordinator:** gaps_found. On macOS, SC1-SC3 hold, and every probe I threw at real ffmpeg either produced a correct video or an honest failure verdict, with nothing left running. Phase 5 still cannot be marked passed, for three reasons:
- SC4 (Windows) is written, not executed, and by reasoning one of its four tests (w4) will fail as written.
- There is one low-likelihood honesty hole: a writer abort at the 10 s stop deadline gives Finalized with "0 dropped" while frames were silently discarded.
- One matrix test (j) reliably flakes under load.

All three are small fixes. Only SC4 needs CI, which in turn needs a push (owner decision).

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1: argv spawn (no shell), frames on stdin, stdout discarded, bounded stderr tail; arg builder gives even-dim scaling + timestamp strategy, covered by arg tests | VERIFIED | Read process.rs/args.rs in full. `Command::new(path).args(argv)`, stdin piped, stdout null, stderr piped -> `push_tail` 4096 B. Exact-argv tests for webm/mp4, odd sizes, fps clamp, version gate (-vsync for major<=5), weird/non-UTF8 path as one OsStr element. Probed with real ffmpeg: a path with spaces, `;$(touch PWN)`, backticks, `&|<>*?'"` and ` -y` inside produced a valid vp9 file at the exact path, and no PWN file appeared anywhere. |
| 2 | SC2: stop closes stdin, bounded wait, kill only as last resort -> "unfinalized"; success only on exit 0 + non-empty file | VERIFIED (macOS) | Read writer.rs/process.rs/report.rs in full. The writer owns the only ChildStdin and drops it on every return. `finish` gives the writer one deadline, the child at least 2 s more (CHILD_WAIT_FLOOR), then `child.kill()` (kill + wait, 5 s), then 1 s stderr grace. `classify` returns Finalized only for Code(0) + frames>0 + no write error + bytes>0. Probes: a fake that ignores EOF ended Unfinalized{reaped:true} in 3.0 s with 0.8 s timeout + 2 s floor + 1 s stderr grace (a grandchild held stderr). Real ffmpeg SIGKILLed mid-run gave "exited with signal: 9 (SIGKILL) ... not a valid recording". See T5 for the one honesty hole. |
| 3 | SC3: missing ffmpeg / missing encoder / non-zero exit -> honest, actionable error; browsing keeps working | VERIFIED (unit level + probes) | Verdict and error tables in report.rs; missing encoder and bad path are rejected before any spawn (marker-file tests). Real-ffmpeg probes: garbage-only or random frames gave "exit code 69 ... Nothing was written into output file"; a nonexistent parent dir gave "exit code 254 ... No such file or directory"; a directory as output gave "exit code 235 ... Is a directory". "Browsing unaffected" holds by construction: nothing outside src/record references the encoder (grep). Integration proof belongs to Phase 7. |
| 4 | SC4: Windows stdin-EOF finalize, no console window, fake-encoder test in windows-latest CI | FAILED (written, not executed) | `creation_flags(CREATE_NO_WINDOW)` under cfg(windows) and the same EOF path. windows_tests.rs w1-w4 were never compiled or run. Predicted w4 failure (see gaps and the Windows review). |
| 5 | T5 (derived): a Finalized verdict never hides accepted-but-unwritten frames | FAILED | Reproduced (see gaps): 64 queued frames needing more than 10 s of writes gave Finalized, "0 dropped at the queue", and the video was about 30 of the intended 63 minutes. |

**Score:** 3/5

### Required Artifacts

| Artifact | Status | Details |
|---|---|---|
| src/record/encoder.rs | VERIFIED | Thin core, module map, re-exports; test modules cfg-gated (unix matrix, windows tests). |
| encoder/args.rs | VERIFIED | Pure argv builder + path checks (`-` prefix, absolute, extension, case-insensitive). |
| encoder/pace.rs | VERIFIED (read in full) | Cumulative round-half-up slots, so error never accumulates. Backwards ts counts 0 and follows the reset clock; gaps are clamped at 600 s; the final frame is written at least once; the hold flush (`advance_to`) does not double count (the next `on_frame` saturates to 0). |
| encoder/writer.rs | VERIFIED | biased select; the hold tick fires only when the queue is empty and a frame is pending; flush at the end. |
| encoder/process.rs | VERIFIED with gap T5 | Bounded finish, kill + reap, kill_on_drop backstop. |
| encoder/report.rs | VERIFIED | classify table, verdict strings. |
| encoder/resolve.rs | VERIFIED | spawn_blocking discovery, pure `resolve_from` table. |
| encoder/{lifecycle,matrix,real,windows}_tests.rs, test_util.rs | VERIFIED except j flake and the unexecuted windows tests | |
| docs/recording-and-launch-config.md "How recording is encoded" | VERIFIED with 2 nits | See the docs check. |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| send_frame | writer | bounded mpsc try_send (never blocks; full queue or dead writer counts as dropped) | WIRED. Probe: 8 tasks x 50 frames gave accepted 34 + dropped 366 = 400 exactly. |
| writer | ffmpeg stdin | Pacer -> write_n, drop of ChildStdin = EOF | WIRED. Real ffmpeg durations match the pacer exactly. |
| ffmpeg stderr | verdict | reader task -> tail mutex -> tail_for_verdict (last 3 lines, 300 chars) | WIRED. Real ffmpeg error lines appear in the verdicts. |
| exit/frames/bytes | Outcome | classify, first match wins | WIRED |
| Encoder -> browsing commands | n/a | not wired until Phase 7 (by design) | n/a |

### Requirements Coverage

| Requirement | Status | Note |
|---|---|---|
| SAFE-04 | SATISFIED on macOS (literal text). One honesty hole (T5) is outside the literal wording. | Linux is not executed locally (CI ubuntu would run the unix matrix and lifecycle tests on push). |
| LIFE-05 | NOT SATISFIED (pending a windows-latest CI run) | Leave unticked. |
| SYNC-01 (touch check) | OK | See below. |

## What I ran (executed) vs only reasoned

### Executed

1. `cargo fmt --check`: exit 0. `cargo clippy --release --all-targets -- -D warnings`: exit 0. `cargo test --release`: 564 passed, 0 failed (523 + 6 + 1 + 5 + 14 + 10 + 2 + 3). The known-flaky `concurrent_cold_starts_have_one_owner` passed.
2. `cargo test --release --lib record::encoder` 6x unloaded: 49/49 every time (about 3 s).
3. Under load (12 cores, 24-48 `yes` busy loops, 3-5 parallel `cargo test record::encoder`), 48 runs in total:
   - `j_idle_hold_flushes_before_finish` failed 7/18 runs (moderate load) and 15/15 runs (heavy load), always "file only 0 bytes before finish".
   - In the very first loaded batch, `success_finalizes_and_reaps`, `non_zero_exit_reports_stderr`, `a_empty_precreated_output`, `b_missing_output` and `c_no_frames` also failed once at their outcome asserts. I did not capture the messages; they did not reproduce in the next 45 loaded runs.
   - Root cause measured with my own probe (36 concurrent encoders, each a freshly written fake script, under load): the time until the fake shell even creates its output file was 0.6-3.3 s. The encoder writer had already pushed its bytes (all 36 still ended Finalized with the correct frame count). This is a test-timing issue, not a product bug.
4. External probe crate, outside the repo (`/private/tmp/claude-501/-Users-sokolov-st-brainrocket-com-claude-bladebro/692c5087-55f4-4f58-b674-e2a3d7ce3526/scratchpad/encprobe`, path dependency on bladebro, its own target dir, never added to the repo). It drives the public `Encoder` API against real ffmpeg n9.0.2 (`~/.local/ffmpeg/bin`) with ffprobe `-count_frames` checks. Results:
   - **(a) Timestamps**, identical for webm and mp4:
     - Irregular ts (0, 37, 180, 181, 650, 1400, 1405, 1406, 2900): 30 frames, 3.000 s.
     - 5 s idle gap sent instantly: 71 frames, 7.1 s.
     - 5 s real-time idle (real sleeps, exercises the hold flush): 70 frames, 7.0 s for 7.04 s of wall time.
     - Backwards ts (0, 100, 200, 300, 50, 150, 250, 1000, 900, 2000): 25 frames, 2.5 s, as the pacer predicts.
     - 1 frame: Finalized, 1 frame, 0.1 s.
     - 0 frames: NoFrames, "no frames reached the encoder".
     - Huge jump (ts to u64::MAX/2): clamped once, 600.3 s of frozen frame, finish 3.4 s, Finalized. The verdict does not mention the clamp; `gaps_clamped=1` is only in the report.
   - **(b) Corrupt frames:**
     - Garbage-only or random bytes: ExitFailed "exit code 69" with a useful tail, finish about 12 ms.
     - Every 5th frame garbage: Finalized, valid file, but 18 decoded frames and 2.4 s against "30 frames written". ffmpeg silently drops undecodable frames, and "frames written" counts stdin writes, not encoded frames.
     - One truncated JPEG: decoded, Finalized, 30 frames.
     - finish stayed bounded in every case.
   - **(c) Fake ignoring EOF that spawns a grandchild** (`sleep 301 &; cat; exec sleep 30`): Unfinalized{reaped:true} in 3.0 s, and the direct child is gone (`kill(pid,0)` gives ESRCH). The grandchild `sleep 301` survived, reparented to launchd (PPID 1). Honest description: the encoder reaps only its direct child, and ffmpeg never forks, so this matters only for wrapper scripts; Phase 9 owns orphan cleanup. A grandchild that only holds stderr open costs the 1 s stderr grace and nothing else. I killed the probe's leftover sleeps by pid.
   - **(d) Paths:**
     - Metacharacter path: correct.
     - `-rec.webm`, `rel/rec.webm`, `./rec.webm`: rejected before spawn.
     - Wrong or missing extension: rejected before spawn.
     - `/abs/-dash.webm`: allowed and works (only a leading `-` of the whole path matters).
     - `REC.WEBM`: works.
     - Missing parent dir, or output path is a directory: honest ExitFailed.
   - **(e) Drop without finish:** dropped inside the runtime, ffmpeg pid was gone (not even a zombie) 0.5 s later. Dropped after the spawning runtime had been shut down, also gone. The partial file is lost (webm buffers), which is expected; Phase 9 owns the drop path.
   - **(f) Concurrency and dead encoder:**
     - SIGKILL to ffmpeg mid-run: 7/30 later frames accepted (already queued), the rest counted as dropped; the verdict names SIGKILL.
     - 8 concurrent sender tasks: counters exact (34 + 366 = 400).
     - Full queue (2000 x 67 KB frames into queue 64): 1934 dropped, finish 0.17 s, Finalized with honest counts.
     - Backlog over the 10 s budget: this is T5, failed.
     - Extra: SIGINT to ffmpeg mid-run gives exit 255 and the verdict "is not a valid recording", although ffprobe shows a valid 17-frame file. That is conservative (never a false success), but the sentence is factually wrong; "may not be valid" would be exact.
5. `pgrep -fl "ffmpeg|fake-enc|sleep 30|encprobe"` after all of the above: nothing. No `bladebro-enc-*` scratch dirs left in the temp dir. `git status --short` shows the same entries as at session start; my probe crate lives only in the scratchpad.
6. SYNC-01: `git diff --stat fbad112 -- . ':!.planning' ':!docs'` touches only src/record.rs (+6: module-map line, `pub mod encoder`, re-exports) and src/record/encoder.rs plus encoder/* (12 files). Both are fork-only files, so HOOKS.md needs no entry. New `BLADE_*` strings in the diff: only the existing `BLADE_FFMPEG` and `BLADE_RECORD_FORMAT` inside error text, no new env var. No Cargo/npm/CHANGELOG/.github changes.
7. Layer-1 gates (8g/8h/8i): the repo ships none of `scripts/fp-gate.ts`, `migration-security-gate.ts`, `cve-scan-gate.ts`, `check-p0-tier-delta.ts`, and has no `apps/`. Not applicable. Charlotte QA and E2E are not applicable (no UI and not a web project); there are no migrations.

### Only reasoned (not compiled or run)

**Windows (cfg(windows) code and windows_tests.rs):**
- **Compile.** `tokio::process::Command::creation_flags` is an inherent method on Windows, so no trait import is needed and the call compiles. Nothing in non-test encoder code is cfg(unix)-only. In tests, `script`/`start_retry`/`pid_gone` are cfg(unix) and unused on Windows only where they are not compiled; `Scratch::cmd`, `cfg_for` and `fake_ffmpeg` are used by windows_tests. `real_tests` has `#[cfg(unix)] let pid`, which is fine. I see no dead_code or unused-import problem. In any case CI runs `cargo clippy --release -- -D warnings` without `--all-targets`, and `cargo test` does not deny warnings, so test-only warnings cannot fail Windows CI.
- **Batch escaping.** Rust >= 1.77.2 (`append_bat_arg`) quotes any argument that contains ASCII outside `[A-Za-z0-9#$*+-./:?@\_]`. It rewrites `%`, doubles `"`, and errors only on characters it cannot represent (NUL, newlines). The `-vf` argument (`=`, `,`, `(`, `)`) is therefore quoted, not refused. The "remaining uncertainty" in windows_tests.rs is very likely a non-issue for w1-w3.
- **w4 is likely wrong.** For a path ending in `.cmd`, std's `resolve_exe` returns the path without an existence check, and spawn runs `cmd.exe /e:ON /v:OFF /d /c "...no-such-ffmpeg.cmd"`. That spawn succeeds, so `Encoder::start` returns Ok and `.err().unwrap()` panics. Fix: use `no-such-ffmpeg.exe` (CreateProcessW fails with NotFound), or assert an ExitFailed outcome after finish.
- **w1-w3 semantics.** `more > file` drains the pipe, `echo ... 1>&2` lands in the stderr pipe, and `exit /b 3` as the last command propagates through `cmd /c`. They look plausible.
- **Windows abort-path comment.** The process.rs comment says aborting the writer delivers EOF even if ffmpeg stopped reading. On Windows, tokio's ChildStdin is a `Blocking` wrapper, so an in-flight blocking write keeps the handle open until the write completes. The kill fallback still bounds `finish`, so the outcome is the same (Unfinalized after the deadline) and the comment is unix-accurate only. Not a bug.
- **Console window.** CREATE_NO_WINDOW correctly prevents a console flash. No test can observe it; the SC wording relies on the CI job running at all.

**Other reasoning:**
- `Encoder::start` and the writer's `interval` panic (inside tokio) if called from a runtime built without IO/time drivers. The daemon and MCP use full runtimes, so this is informational. The "nothing here panics" header holds only for enable_all runtimes.

## Deviation judgements

- **crf 40 instead of the user's 36: acceptable, but the owner should confirm.** Every measured Phase 2 number (870,778 B/min, 4.55 cpu-s/min) was produced at crf 40 (sidecar encoder.mjs line 62). 36 comes from pre-measurement research (STACK.md) and was never measured. Switching is a one-constant change in `VP9_FLAGS`, plus the exact-argv test and the docs line. It is not a gap, but it overrides an explicit user value, so it needs an explicit yes.
- **Duplicate-on-write instead of concat: sound, and verified better for this design.** concat needs a spool of every frame on disk (sensitive frames left behind after a crash, plus cleanup) and encodes the whole recording at stop, which breaks the bounded finish. My real-ffmpeg probes show durations exact to the slot (3.0/7.1/7.0/2.5/0.1 s) for irregular, idle, real-time-idle and backwards streams, in both formats. Trade-offs: up to 15 fps duplicates the same JPEG over the pipe (cheap; idle 10 min = 6000 writes, flushed progressively), and a clock jump clamps at 10 min of frozen video, which is generous; 30-60 s would bound the damage of a wild timestamp better.
- **Verdict strings moved earlier (05-01): harmless.** It made classify/verdict pure and table-tested before any process code existed.
- **SC3 "browsing unaffected": true by construction only.** Nothing outside src/record calls the encoder; every error string ends "Browsing is unaffected." (except BadOutputPath, which is a caller bug). Phase 7 must prove it end to end.
- **SC4 honesty: kept as "written, not executed".** I strengthened it with the concrete w4 prediction.

## Docs check (docs/recording-and-launch-config.md)

Accurate on:
- argv/no shell, 4 KiB stderr tail
- VP9 realtime crf 40 / x264 veryfast crf 28 faststart
- default 1280x720, letterboxed, even dimensions
- constant-rate repeat timing, last frame held to the stop
- success = exit 0 + non-empty file; killed -> "unfinalized", partial kept
- error causes, "no setting for them"

Nits:
1. "waits up to 10 s" is the common case. The worst case is about 18 s: 10 s writer deadline + 2 s child floor + 5 s kill/reap + 1 s stderr grace. It is still bounded.
2. "Requires ffmpeg 4.3 or newer" is not enforced in code. An older ffmpeg fails honestly at encode time (unknown `force_divisible_by` option, shown in the verdict tail). Acceptable, but "needs" would be more accurate than implying a check.

Not mentioned, and fine for now: frames dropped by ffmpeg as undecodable are not reflected in "frames written".

## Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| src/record/encoder/process.rs | 244-249 | Writer abort at the deadline is not reported (T5) | Warning (gap) | Finalized verdict can hide a discarded tail |
| src/record/encoder/matrix_tests.rs | 236-239 | Fixed-sleep assertion on an external process's file | Warning (gap) | Flaky under load / slow CI |
| src/record/encoder/windows_tests.rs | 88-95 | Assumes a missing .cmd fails at spawn | Warning (gap) | Likely CI failure |
| src/record/encoder/report.rs | 148-149 | "is not a valid recording" on any non-zero exit | Info | Overstates (SIGINT case leaves a valid file) |
| src/record/encoder/report.rs | 145-146, 159 | "left in place" / "(kept)" said even when no file exists | Info | In production the caller pre-creates the file, so mostly true |
| (logical_cohesion) | - | none | - | Files are small and single-purpose; process.rs (lifecycle) and writer.rs (pacing I/O) are cleanly split |

No unwrap/expect/panic in non-test encoder code (mutex poisoning handled with `into_inner`).

## Human Verification Required

None beyond the CI run. A push or CI run needs the owner's go-ahead.

## Gaps Summary

1. **SC4 / LIFE-05.** Never compiled or run on Windows, and w4 is likely wrong. Fix w4, push, get a green windows-latest job.
2. **T5.** When the writer is aborted at the 10 s stop deadline, the result is Finalized with "0 dropped" while accepted frames are silently discarded. About a 5-10 line fix plus a regression test. Rare in practice, but it is exactly the class SAFE-04 exists to prevent.
3. **j flake.** It depends on macOS shell exec latency (up to 3.3 s measured under concurrency). Poll for the fake's output file before the timed sleep. The rare one-off failures of the 5 s-timeout matrix rows have the same root cause.

Everything else holds under adversarial probing on macOS with real ffmpeg n9.0.2, with no stray processes. Linux has not been executed locally; CI ubuntu would run the unix matrix on push.

---
_Verified: 2026-10-22T08:16:11Z_
_Verifier: Claude (opus-adversarial, gsd-verifier)_
