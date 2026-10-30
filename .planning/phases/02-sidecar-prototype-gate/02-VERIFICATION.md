---
phase: 02-sidecar-prototype-gate
verified: 2026-10-21
status: passed
verifier: gsd-verifier-opus
first_pass_verifier: gsd-verifier-sonnet
score: 5/5 success criteria met (with honestly documented caveats)
requirement: VER-01
gate_verdict: PASS-WITH-CAVEATS
---

# Phase 2 Verification

## Checks run
- Deliverables exist: README.md (107 lines), RESULTS.md, results/*.json (core, extra, stall_probe, smoke, stealth, tabswitch, sample_timestamps), sidecar.mjs, measure.mjs, test/ (4 .mjs + 1 py).
- `node --test tools/record_sidecar/test/*.test.mjs`: 35 tests, 35 pass, 0 fail.
- `git diff 7a738ad..HEAD --name-only`: nothing outside `tools/record_sidecar/` and `.planning/` (src/, Cargo, npm untouched).
- `pgrep ffmpeg|bladebro`: no processes left.
- Largest file in results/ is 508 KB (tabswitch_sample.webm); dir is 680 KB. Nothing over 2 MB.
- 20+ RESULTS.md numbers spot-checked against JSON, all matched: 99.31 and 49.54 fps, ack p95 44.9 us / 0.316 ms, drop 94.63 %, 5994 = 5994 frames and acks, max gap 19 ms, drift tick -0.355/-0.242/-1.902, idle -0.273/-0.143/-1.902, notail -99.99, tabswitch 31.982 s / 1916 frames / 3 segments 599/615/701 / drift -0.246, 65 warning lines, vp9 10 fps 870,778 B / 4.545 cpu-s, vp8 25.74, 5 fps 396,855 B, 15 fps 1,150,851 B, 1080p 1,124,821 B / 6.18, minimized max gap 8976 ms, audit 100/100 x4, audit_changed false, battery divergent [] with noise `voices`.

## Success criteria
1. Playable 30 s WebM spanning a tab switch (WS transport, scratch BLADE_HOME): MET with caveat. 31.982 s, 3 non-empty segments, ffmpeg decode exit 0, all 1916 frames read. The probe's own `playable` field is False because strict "no error output" fails (65 non-monotonic dts warning lines, no corruption). RESULTS.md says so openly and counts it as a caveat. The fixed-fps concat re-encode decoded clean on all 11 matrix rows. The run was headless (stated).
2. Results file with fps, ack latency, drops, frames vs acks, drift, headed vs headless/Xvfb, viewport change: MET. Every item is present. Xvfb is marked "not measured because macOS host has no Xvfb". Per-frame JPEG bytes is also marked not measured. The wording "Chrome 151" is handled honestly: RESULTS.md states Chrome 154.0.8037.58 was the baseline, not 151 (the roadmap wording is stale; consider updating it).
3. Bladebro CDP events kept flowing; audit and diff_oracle: MET as a statement. Coexistence is 8/8 commands, with events flowing. Audit is 100/100 with and without screencast (AGENTS.md's 111 is not reproduced on macOS; documented, cause unverified). diff_oracle did NOT run (exit 2, then missing `websockets`; it needs Linux plus Xvfb). RESULTS.md records this and substitutes a 99-key battery A/B (divergent 0). The substitute is not the oracle. Real `divergent: 0` is still owed on Linux CI before Phase 6. Audit evidence is thin (5 to 6 frames).
4. Written verdict: MET. PASS-WITH-CAVEATS, with a rule, numbers and 7 caveats. In-tree approach: a separate recorder with its own CDP attachment, not the `Page` session, backed by code facts (`shrink_oversized_event` 16 KiB strip, shared event bus, `switch_tab` replaces `Page`). Timestamp strategy: concat with a held tail. Codec: VP9. fps: 10 (range 5 to 15). Size: 1280x720 fixed. The gate did not fail, so no roadmap revision is needed. The in-tree choice contradicts the roadmap's stated preference and is reasoned, not measured in-tree.
5. README plus supported probe, not shipped: MET (README line 3 states it, and nothing in `src/` imports it).

## Caveats to carry forward (not blockers)
- diff_oracle, the Xvfb cadence and Windows are unmeasured. Run them on Linux CI before Phase 6.
- One unexplained 37 s screencast silence in 1 of 7 long runs. Phase 6 needs a watchdog.
- Background and minimized tabs deliver 0 or about 0.3 fps.
- Phase 6 must exempt `Page.screencastFrame` from `shrink_oversized_event`.
- Update the roadmap and AGENTS.md "Chrome 151" and "111/111" wording, or note that they are platform and version specific.
- Untracked `.planning/EXECUTION_LOG.md` is pre-existing and not part of the phase.

## Final pass (gsd-verifier-opus, 2026-10-21)

Final status: **passed** (gate verdict PASS-WITH-CAVEATS upheld). Every check below was read-only and run independently.

1. **SC1 playability.** `ffprobe` reports vp9 1280x720 webm, duration 31.982 s. `ffmpeg -v error -i tabswitch_sample.webm -f null -` exits 0 with 65 lines. All 65 come from the `[null @ ...]` output muxer ("non monotonically increasing dts ... 1223 >= 1223") or are "Last message repeated" lines. After filtering those out, there are 0 decoder lines. Packet dts has exactly 9 non-increasing steps, which matches sample_timestamps.json. A `framemd5` passthrough decode writes 1916 frames with no error output. The cause is duplicate 1 ms timestamps from about 100 fps input, not corruption, so calling the file playable is justified. The strict criterion is still not met for live wallclock files, and the capped-fps live path has not been measured. RESULTS.md says both of these, and the rule belongs in Phase 5/6.
2. **Defaults and approach.** In `src/cdp/client.rs`: `MAX_EVENT_BYTES = 16 * 1024` (L52), `EVENT_BUS_CAPACITY = 16384` (L41), and the reader calls `shrink_oversized_event` when `text.len() > MAX_EVENT_BYTES` (L489). The `_ => {}` arm replaces `params` with an empty object (L446-448). As a result, an oversized `Page.screencastFrame` loses `data` and `params.sessionId`, which is the ack key. That is correct, so the exemption is required whichever client is used. The separate recorder attachment follows from the shared bus and from `switch_tab` replacing `Page`. RESULTS.md marks this choice as reasoned rather than measured in-tree. Spot checks against the JSON files: VP9 is 4.545 cpu-s/min and 870,778 B/min (extra.json matrix). VP8 live is 44.49 utime and 55 drops. Concat finalize is 562 ms (stats/vp9_concat). These numbers support the VP9, 10 fps, 1280x720 fixed, concat-plus-tail defaults. JPEG q70 is labeled as unproven.
3. **Invented numbers.** None found. 12 more figures were traced to files: 562, 79 written, 8976, 1884, 0.3156→0.316, 2,785,919, 44.49, 59.79, switch 7/10 ms, and first-frame 5..38 ms. One small mismatch: RESULTS says first frame is "8 to 43 ms", but the stored first_frame_ms range is 5..38. It does not affect any conclusion.
4. **Scope.** `git diff 7a738ad..HEAD --name-only` shows nothing outside `tools/record_sidecar/` and `.planning/`. There are no `src/`, Cargo or npm changes. `node --test test/*.test.mjs` gives 35 tests, 35 pass.
5. **Honesty about the stall and the oracle.** The 37 s stall is recorded as 1 of 7 runs with its cause not established. RESULTS also says the run's metrics were overwritten (they exist only in deferred-items.md) and that two 60 s stall probes did not reproduce it (5993 and 5994 frames, stall 0). A Phase 6 watchdog is listed. On diff_oracle, one claim is overstated. RESULTS says the oracle "requires Linux + Xvfb per its README". However, `oracle.py` only spawns Xvfb `if sys.platform.startswith("linux")`, and the actual blocker on macOS was the missing `websockets` Python module, which a scratch venv could have provided. This is not a gap because the substitute A/B/A2 runs the oracle's own `tools/diff_oracle/battery.js` on the same browser with and without the screencast. That directly answers SC3's question (did the screencast change any page-visible probe): 0 divergent of 99 keys. The literal `divergent: 0` oracle run is owed at the Phase 12 instrument gate, or on Linux before Phase 6.

Other gates. The repo ships no `scripts/fp-gate.ts`, migration, CVE or P0-tier scripts (`scripts/` holds only publish-npm.sh), so Steps 8g-8i do not apply. The phase has no UI, so Charlotte QA does not apply. There were no Rust changes, so cargo test/clippy are unaffected.

Carry-forward items (non-blocking): run diff_oracle literally on Linux CI, along with Xvfb cadence and Windows. Fix the "8 to 43 ms" figure (stored range is 5..38). Soften "requires Linux + Xvfb" to "not run: websockets missing; Linux+Xvfb is the supported setup". Update the roadmap wording for Chrome 151 and 111/111.
