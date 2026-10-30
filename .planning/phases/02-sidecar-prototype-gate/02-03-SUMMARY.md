---
phase: 02-sidecar-prototype-gate
plan: 03
subsystem: testing
tags: [stealth, audit, screencast, verdict, gate]

requires:
  - phase: 02-sidecar-prototype-gate
    provides: 02-01 recorder, 02-02 measurements
provides:
  - results/stealth.json (audit with and without screencast, diff_oracle outcome, battery A/B diff)
  - RESULTS.md with all measurements, in-tree approach, Phase 3 defaults and the gate verdict
  - finished README.md
affects: [phase 3 defaults, phase 6 frame source]

key-decisions:
  - "Gate verdict: PASS-WITH-CAVEATS"
  - "In-tree: recorder outside Page with its own CDP attachment, plus a Page.screencastFrame exemption from shrink_oversized_event"
  - "Defaults: VP9/WebM, 10 fps cap, 1280x720 fixed, q70, concat timestamps, Page.enable on"

requirements-completed: [VER-01]

completed: 2026-10-21
---

# Phase 2 Plan 03: Stealth check, results and gate verdict

**Verdict: PASS-WITH-CAVEATS.** The screencast does not change the audit (100/100 with and without) or the oracle battery (0 divergent of 99 keys). All hard criteria hold; the caveats below remain open.

## Accomplishments

- `battery_probe.py` (test first, 4 unit tests) and `scenarios/stealth.sh`; `results/stealth.json` is assembled by `stealth_assemble.mjs` with scores read by regex.
- Audit without screencast 100/100 exit 0; with the sidecar attached to audit's own browser 100/100 exit 0 (5 frames, 5 acks, 0 errors, attached for the whole 5.2 s run); with `--no-page-enable` 100/100 (6 frames, 2 attempts). `audit_changed` false.
- diff_oracle run verbatim: exit 2 "no Chrome/Chromium found (set CHROME_PATH)"; with CHROME_PATH set, exit 1 `ModuleNotFoundError: No module named 'websockets'` (needs Linux + Xvfb). Substitute battery in the bladebro browser: A, B (sidecar attached, 296 to 299 frames), A2 gave 99 keys, divergent 0, noise `voices`.
- RESULTS.md: full measurement table with sources, in-tree reasoning from `src/cdp/client.rs` (`MAX_EVENT_BYTES` 16 KiB strips `Page.screencastFrame` params via `shrink_oversized_event`'s default arm, `EVENT_BUS_CAPACITY` 16384) and `src/page/tabs.rs` (`switch_tab` replaces `Page`).

## Recommended Phase 3 defaults

| setting | value |
|---|---|
| codec | VP9 WebM (x264 MP4 opt-in; VP8 dropped: 25.74 cpu-s/min, 55 live drops) |
| fps cap | 10 (range 5 to 15) |
| max frame size | 1280x720, fixed output size |
| JPEG quality | 70 (not compared with 60/80) |
| timestamps | concat durations at the capped fps with held final frame; wallclock as fallback; vfr rejected |
| queue / follow / Page.enable | 16, drop at queue after ack / poll the page list / send Page.enable |

In-tree approach: recorder task outside `Page` with its own attachment (second WS client, or flat child session on pipe), re-bound by target id; exempt `Page.screencastFrame` from `shrink_oversized_event`.

## Caveats (each is in RESULTS.md)

1. SC1 strict decode only partial: 65 "non monotonically increasing dts" lines (exit 0, all 1916 frames read; 9 of 1916 packets share a timestamp, results/sample_timestamps.json). Concat at fixed fps decodes clean on all 11 matrix rows; a live capped-fps wallclock file was not measured.
2. diff_oracle cannot run on macOS; substitute used; real run needed on Linux.
3. Xvfb cadence not measured.
4. Background and minimized tabs yield 0 or about 0.3 fps frames.
5. One 37 s screencast stall in 1 of 7 long runs, unexplained.
6. Audit evidence is 5 to 6 frames; audit prints 100 vectors here, not 111; Chrome is 154, not 151.
7. Attaching without `Page.enable` to audit's browser while it was starting often failed (-32000); only the final run is stored, earlier attempt counts were console-only.

## Deviations

- Plan said `--attach-wait` for audit's browser; the agent lane does not write `DevToolsActivePort`, so the scenario polls the Chrome process line and attaches with `--port --follow poll` (Rule 3).
- Added a retry loop (up to 8 audits) for the attach race, `stealth_assemble.mjs`, `scenarios/sample_ts.py` and `results/sample_timestamps.json` (to characterize the decode warnings), and used `anim.html` for the battery B run so frames really flow.
- vfr replaced by passthrough (02-01).

## Task commits

1. Task 1 stealth check: `8cd5504`
2. Task 2 RESULTS.md: `82b0ccc`
3. Task 3 README: `f964a95`
4. This summary: see metadata commit

## Self-Check: PASSED

Files exist (battery_probe.py, test, stealth.sh, stealth.json, audit_*.txt, diff_oracle_attempt.txt, RESULTS.md, README.md); commits present; 35 node tests and 4 python tests pass; no file over 2 MB; no leftover processes; scratch dirs removed; ROADMAP.md and STATE.md untouched; `.planning/EXECUTION_LOG.md` not committed.
