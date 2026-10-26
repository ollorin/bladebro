# 10-01 Summary: record-stop poster and marker

**Status:** complete (4/4 tasks). Executor model: haiku for every task. No task escalated to sonnet.

## Commits
- 4893ba8 feat(10-01): marker cleaning, sidecar files and ffmpeg poster (Task 1)
- 6bc8923 refactor(10-01): video clock instant on follow results; recordings helpers list videos only (Task 2)
- 94987e3 refactor(10-01): hub stop/finish paths in hub/finish.rs (Task 3, extraction only)
- 325d581 feat(10-01): record-stop returns a poster frame and places a marker (Task 3)
- c44dfbd test(10-01): live poster and marker checks (Task 4)
- docs(10-01): plan summary (this file, with evidence/10-01-live.txt and deferred-items.md)

## What changed
- `record-stop` (agent stop, follow and per-tab) returns `. Poster: <abs>.poster.png`: one bounded ffmpeg run on the finished file (`-sseof -1`, last frame, PNG, 3 s budget, PNG magic verified, file created 0600 next to the video). A poster that cannot be made says `. Poster: not made (<reason>)` and never changes ok/err.
- `record-stop` with `value` places a marker: `. Marker at T s` on the video clock, clamped to the recording length; the note is not echoed. Bad markers say `. Marker not placed: <reason>` and the stop still happens.
- Marker text is cleaned (controls to space, bidi/zero-width dropped, whitespace collapsed, <= 200 chars).
- Per-tab: one poster, from the file of the tab followed at stop (else the first finalized file).
- Orphan/interrupt/limit finalize paths are unchanged (no Poster/Marker text, no sidecar file).
- Test helpers ignore sidecar files (`.poster.png`, `.timeline.json`) when listing the recordings dir (D10-8).
- No upstream file edits, no new op/property/dependency, no CDP method added (RecMethod stays 5), no stealth-path change.

## Live numbers (Darwin 25.6.0, headless, Chrome 154, local fixtures, `evidence/10-01-live.txt`)
Build under test: 325d581 (Task 4 added no Rust). Baseline: d18dae1 binary built in a detached worktree.

| id | result |
|---|---|
| poster (webm) | PASS: `. Poster:` path next to the webm, `<stem>.poster.png`, mode 600, PNG magic, ffprobe `png,1280,720` = video `vp9,1280,720`, 1x1 colour of the poster rgb 2 66 251 (blue, the last frame), full decode exit 0 with no stderr, decoded frames 38 = reported 38 |
| poster-mp4 | PASS: same checks; video `h264,1280,720`, poster `png,1280,720`, rgb 2 65 252, decode exit 0, frames 39 = 39 |
| marker | PASS: `. Marker at 3.8 s` on duration 3.8 s (T in [duration-0.6, duration]); note `FAIL: demo note` absent from the output; poster checks pass on the same stop |
| marker-bad | PASS: `--value ""` gives `. Marker not placed: empty note`, file saved, no marker offset |
| per-tab | PASS: two files, exactly one Poster, named `-tab2-...poster.png` (the followed tab), PNG 0600, 1280x720, rgb 0 158 0 (green), decode exit 0, frames 30 = 30; file tab2 green at 2.7 s |
| latency | PASS: see below |

Stop latency (wall time of the `record-stop` call, 3 interleaved runs each, same mark.html scenario):
- final run: base (d18dae1) 0.655 / 0.632 / 0.620 s, median 0.632 s; new (325d581) 0.649 / 0.654 / 0.661 s, median 0.654 s; new - base = 0.022 s
- earlier run of the same script (before a timing-only change): base median 0.619 s, new median 0.651 s, delta 0.032 s
- The poster ffmpeg run alone (same argv as `make_poster`, 15 runs over 3 recordings): typ 0.07 s, max 0.074 s. The stop-level delta (about 0.02-0.03 s) is within run-to-run noise of the ~0.63 s baseline; the poster costs about 0.07 s of ffmpeg time, far inside the 3 s budget.

Cleanup: every scenario ended with a PASS cleanup line (no daemon, Chrome, ffmpeg or cdp_tap left under its scratch home); final sweep of our processes: none.

## Tests and gates
- `cargo fmt --check`: clean. `cargo clippy --release --all-targets -- -D warnings`: clean.
- `cargo test --release --no-fail-fast`: **864 passed, 0 failed, 16 ignored** (baseline 820 from 08.1-02; the flaky `concurrent_cold_starts_have_one_owner` did not fail in this run).
- Upstream check (`git diff --name-only d18dae1..HEAD` vs `upstream/main`): prints nothing.
- Scope check (`git diff d18dae1..HEAD -- src/mcp src/cli src/cli.rs src/state.rs src/page Cargo.toml Cargo.lock npm CHANGELOG.md .github`): empty.

## Deviations
- [Rule 3 - Blocking, scenario harness, not product code] The first live runs produced FAIL lines that were defects in `evidence_live.sh`, not in `src/record`: (1) the `poster_path` sed used `\S`, which BSD sed does not support, so the poster path came back empty; (2) `poster_case` ended the scenario (deleting its home) before the checks read the files; (3) the per-tab poster check hard-coded blue, but that poster correctly shows the green tab (fixed with an explicit expected colour, not weakened); (4) the poster timing ran after teardown, and `--only` appended to a stale evidence file. All four were fixed in the script; the final evidence file is from a full run that truncates it. No `src/record` change was needed, so no reproduction test was added for a product bug.
- [Rule 3 - Blocking, environment] An exploratory daemon from my own first manual run was left alive and was stopped by hand; its Chrome helpers were gone after the stop.
- Latency baseline uses a temporary `git worktree` at d18dae1 with its own target copy; removed after the run.
- The per-tab marker offset is on the recording clock (D10-5), so it can exceed the length of file 1 (clamped to the longest file instead). Not live-checked: the Task 4 spec covers the marker in follow mode only.

## Deferred
- [DEFERRED] `act click` verdict reads `clicked (no observable DOM change)` for a click that changed the page background (red to blue, proven by pixels in the video and poster). Logged in `.planning/phases/10-evidence-extras/deferred-items.md`; area `src/action/`, out of scope.

## Not verified
- Linux and Windows were not run. The `stat` mode check uses `stat -f` on Darwin and `stat -c` otherwise; only the Darwin branch ran.
- Live poster failure paths (no png encoder, ffmpeg error, 3 s timeout) were not exercised live; they are covered by the unit tests from Tasks 1 and 3 only.
- Chrome 154 on macOS only; the stealth baseline was not re-measured (this plan touches no stealth path).
- Recording-off output identity is covered by construction (no code outside `src/record` changed); the live off-mode diff is deferred to 10-02 per D10-7.
