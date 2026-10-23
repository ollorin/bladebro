---
phase: 08-tab-following
verified: 2026-10-23T02:30:00Z
verifier: opus
status: human_needed
score: 11/11 must-haves verified (automated); 1 owner decision open (popup wording)
re_verification:
  previous_status: gaps_found
  previous_score: 3/4 success criteria fully verified
  gaps_closed:
    - "Test suite green: a_stop_is_bounded_when_one_encoder_ignores_eof now polls (until) for the hung encoder; passed in the full suite"
    - "hub_live.sh line 126 now expects the Phase 8 segments listing instead of the removed Phase 7 note"
    - "Window-feature popup path measured live (popup-measure scenario), re-run by this verifier with the same result shape"
  gaps_remaining: []
  regressions: []
human_verification:
  - test: "Decide on ROADMAP Phase 8 criterion 1 popup wording"
    expected: "Owner accepts the amended criterion (click popups become a segment after switch-tab; window-feature popups followed as measured) or asks for click-popup following"
    why_human: "The original criterion said 'a popup rebinds capture'. The fix round (commit 16cf41a) rewrote the success criterion itself to match the implementation. An executor-amended criterion needs the owner's sign-off; it is not something a verifier can approve."
  - test: "Accept the window-feature popup divergence"
    expected: "Owner accepts that with a window-feature popup the video shows the popup for as long as Chrome keeps it first in /json order, while `state tabs` (the agent's current tab) still names the opener (measured: popup segment 3.7 s to the stop, return to the opener only at record-stop catch-up)"
    why_human: "Product judgement on 'capture what the agent is looking at' versus 'what is in front'; documented honestly in docs/recording-and-launch-config.md"
  - test: "Windows and Linux runs (CI) of the phase"
    expected: "cargo fmt/clippy/test green on ubuntu and windows; live follow scenarios on Linux Xvfb"
    why_human: "Only macOS (arm64) was executed here; no CI was run (no push / GitHub by instruction)"
---

# Phase 8: Tab Following Verification Report (final, opus)

**Phase Goal:** A recording keeps capturing what the agent is looking at when it switches tabs or a popup opens, and users who want separate files per tab can have them
**Verified:** 2026-10-23 (macOS arm64, Chrome 154, headless, git c641e43)
**Status:** human_needed: every automated check is green; owner decisions are open on the self-amended popup criterion and the window-feature popup divergence; Windows/Linux not executed
**Re-verification:** Yes, after fix round 08-03 (commits 242bbe0..10eb624). All three previous gaps re-checked directly.

## Observable Truths

| # | Truth | Status | Evidence (this verifier's own runs) |
|---|-------|--------|------|
| 1 | Default mode: switch-tab/open-tab rebind into the SAME file and encoder; record-stop lists `[<offset> s tab <id>]` | VERIFIED | follow_live daemon: one ffmpeg pid 50081 across switches, segments A,B,A with ids matching `state tabs`, vp9 1280x720, clean full decode, 80 decoded == 80 reported, colours red/blue/red. Own timed script (A, open B, open C, switch A), run 1: one ffmpeg pid at all three samples, 1 file, 12.1 s against a 12.834 s wall window (start call to stop return; docs predict about +0.55 s), mean RGB at offset+0.4 s = (251,0,0) (0,1,254) (0,157,0) (251,0,0) = A/B/C/A. Run 2: 256.2 s against 256.883 s, 2562 frames, same colours |
| 2 | Popup reading | VERIFIED as now documented; see human items | popup: link popup gives exactly one segment (the opener), video yellow. Window popup plus switch-tab: segments opener then popup. popup-measure (no switch-tab): followed the popup, consistent with evidence/08-02-popup-measure.txt. MCP follow_popup: p1 one segment, p2 two segments. ROADMAP criterion 1 was rewritten by the executor (16cf41a); the original "a popup rebinds capture" is met only for window-feature popups |
| 3 | Closing the recorded tab / target-id change / browser loss never silently end recording and never claim clean when frames are lost | VERIFIED | close (CLI) and follow_close_recorded (MCP): saved with note `tab B gone at 5.5 s; following tab A`, last segment A, colours blue then red, 74/74 frames. Unit tests (substantive, read): closing_the_recorded_tab_rebinds_with_a_note, a_target_id_change_rebinds_to_the_new_id, no_tab_left_is_not_clean_and_keeps_the_file (asserts "recording not clean", never "saved"), a_browser_that_stops_answering_is_not_clean, a_failed_rebind_is_named..., a_long_zero_frame_segment_is_not_clean_a_short_one_is_labelled (background/hidden). xorigin 127.0.0.1 to localhost: clean, blue frames after the navigation, 49/49 |
| 4 | Frame-size change through real ffmpeg | VERIFIED | size_change_tests ran with real ~/.local/ffmpeg (not skipped): webm and mp4 1280x720, read_frames 40 == frames_written 40 |
| 5 | Per-tab opt-in: one file and encoder per tab, cap 8 with note, all files listed, error isolation, bounded stop, no orphans | VERIFIED | Own per-tab runs x2: ffmpeg count 1, then 2, then 3; 3 files, each vp9 1280x720, clean decode; stop header `recording saved (per-tab, 3 files)`. per-tab-close: closed tab's file finalized while recording continued (2.4 s, clean decode), note `closed at`. MCP per_tab_cap: 8 files plus `not recorded from 19.3 s: per-tab cap of 8 open files reached`. Isolation/bounded-stop unit tests pass in the full suite. No process left after any run |
| 6 | Layered resolver per-call > env > project > user > default, plus project-file trust | VERIFIED | src/record/layered.rs:188-192 order; every layer parsed before precedence; tabs_parse_table / tabs_precedence_default_and_errors pass. Project layer goes through the existing vet (dir confined); live: .bladebro.json `tabs: per-tab` enabled per-tab, BLADE_RECORD_TABS=follow overrode it to one file |
| 7 | Docs complete, stale Phase 7 text removed, drift test passing | VERIFIED | docs: Tab following + Per-tab files sections, env/key/precedence rows for tabs; no "agent is now on tab" anywhere; hub_live.sh updated; registry drift tests (7) pass |
| 8 | MCP schema unchanged | VERIFIED | `git diff bb9e7dd..HEAD -- src/mcp src/cli src/cli.rs` is empty |
| 9 | Zero upstream-file edits; no forbidden paths; trailers; no junk tracked | VERIFIED | `git cat-file -e upstream/main:<path>` (6b524d2) fails for all 54 changed paths, so all are fork-owned and no HOOKS.md entry is needed. No Cargo.toml/lock, npm/, CHANGELOG, .github. 21/21 commits in bb9e7dd..HEAD carry `Co-authored-by: Claude <claude@anthropic.com>`. No EXECUTION_LOG/.DS_Store/target tracked |
| 10 | Fork code quality | VERIFIED (1 warning) | No unwrap/expect in non-test changed files (mutex uses unwrap_or_else(into_inner)); thin module split under follow/. run.rs is 506 lines (cohesive follower loop; slightly over the ~500 ceiling) |
| 11 | Suite and instruments green | VERIFIED | see Commands |

**Score:** 11/11 automated truths verified.

## Commands run (raw output in evidence/08-opus-*.txt)

- `cargo fmt --check`: clean. `cargo clippy --release --all-targets -- -D warnings`: exit 0.
- `cargo test --release --no-fail-fast`: lib 675 passed. All integration targets ok except the known flake concurrent_cold_starts_have_one_owner (NotFound at daemon_lifecycle.rs:204). It leaked daemon pid 49170 (BLADE_HOME bladebro-daemon-race-49163, confirmed by env, killed). Rerun of daemon_lifecycle: 5/5 ok. Total 720 harness tests passed plus hermes_cli 81 PASS / 0 FAIL. The previously flaky a_stop_is_bounded_when_one_encoder_ignores_eof passed in the full run. (08-opus-cargo.txt)
- follow_live.sh scenarios daemon, close, popup, popup-measure, xorigin, per-tab, per-tab-close, mcp, off: every scenario exit 0 except per-tab (2 FAILs, a harness bug, see Anti-patterns). mcp: record_follow 5/5 passed (MCP stdio). off: base bb9e7dd vs new, 13 identical distinct WS methods, no screencast method, HTTP /json 10 vs 10, no recordings dir, no ffmpeg. (08-opus-follow-live.txt)
- Own timed checks (CLI daemon lane, default and per-tab, two runs each). (08-opus-timed-checks.txt)
- `bladebro audit` (headless, scratch home): SCORE 100/100, VERDICT PASS. (08-opus-audit.txt)
- `tools/qol_probe/run.py` (headless): 120 passed, 2 failed, the two known pre-existing ('chord cleared the live editor', 'nested scroller movement is reported as such'). (08-opus-qol.txt)
- lane_matrix_local.py daemon/oneshot/mcp/mcp-pipe x5: all exit 0, all rounds gl-hw/workarea/decorations/webdriver; real lane skipped. (08-opus-lanes.txt)
- Not run: tools/diff_oracle (python websockets module missing), unmodified tools/lane_matrix.py, the real lane, live external sites, Windows/Linux, CI.
- After every run: no bladebro daemon/mcp, scratch Chrome, ffmpeg, cdp_tap or fixture server left (final ps count 0).

## Observations (not product gaps)

- Host stalls: in several runs a 90 to 330 s pause occurred (second timed runs between open-tab B and open-tab C; MCP follow_close_recorded 96.5 s; mcp lane round 1 202.9 s with recording off). Every bladebro CLI command in the timed runs returned in 78 to 638 ms (one per-tab record-stop took 9.4 s while finalizing three ~330 s files), so the pauses were outside bladebro, on a host at load 50 to 137 with endpoint-security agents busy. Recordings covered them honestly (duration equals the wall window minus stop time; drops reported, e.g. `dropped 7`).

## Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| tools/record_sidecar/scenarios/follow_live.sh | 107, 405-416 | ffmpeg without `-nostdin` inside `while read ... done <<<"$lines"` | Warning (harness) | ffmpeg consumed the leading `/` of the next file line, giving a false "output file missing" for per-tab file 2. Re-parsing the same stop line gives correct paths; own per-tab runs confirm file 2 exists and decodes. Fix: add `-nostdin` or `</dev/null` |
| src/record/hub/follow/run.rs | - | 506 lines | Info | Cohesive (one follower state machine); slightly over the ~500 LOC guideline. No logical_cohesion finding |

## Human Verification Required

1. Popup criterion: ROADMAP Phase 8 criterion 1 was changed by the executor in 16cf41a from "switch-tab, open-tab and a popup rebind capture" to a narrower wording. Click-opened popups (`target=_blank`, plain `window.open`) are NOT followed until `switch-tab`. Owner: accept the amendment or request click-popup following.
2. Window-feature popup: the video follows the popup, but the agent's current tab (`state tabs`) stays the opener; the video returns to the opener only at the record-stop catch-up or a switch. Documented in docs/recording-and-launch-config.md; owner to accept.
3. Windows/Linux: not executed; CI not run.

## Gaps Summary

No automated gaps. The three previous gaps are closed and verified. The status is human_needed only because the popup success criterion was rewritten by the executor and the window-feature popup behaviour differs from the original goal wording. Both need the fork owner's decision, and other platforms are unexecuted.

---

_Verified: 2026-10-23_
_Verifier: Claude (gsd-verifier, opus)_
