# record_sidecar

Supported probe for the session-recording work. Not shipped in the bladebro binary; nothing in `src/` imports it.

A standalone CDP screencast recorder (Node ESM, zero npm dependencies) that attaches to a bladebro Chrome page
over the browser WebSocket, acks every `Page.screencastFrame`, and pipes the JPEGs into ffmpeg. Phase 2 used it to
decide whether CDP screencast + ffmpeg is a viable recording path before any product code is written. The verdict
and every measurement are in [RESULTS.md](RESULTS.md) (look for the `## Gate verdict` line: currently
`PASS-WITH-CAVEATS`).

## Requirements

- Node >= 22 (global `WebSocket` and `fetch`); tested on Node 24. Python 3.9+ stdlib only for the python helpers.
- ffmpeg via `BLADE_FFMPEG` (libvpx-vp9, libvpx, libx264) and `BLADE_FFPROBE`; tested on ffmpeg n9.0.2.
- A bladebro agent-lane Chrome on the WebSocket transport. The sidecar finds the endpoint from the main Chrome
  process line (`--remote-debugging-port=<N>`), because the agent lane does not write `DevToolsActivePort`.
- Does NOT work with `BLADE_TRANSPORT=pipe`: pipe mode has no browser WebSocket endpoint. Never set it here.
- macOS or Linux. Xvfb cadence is only measurable on Linux; `tools/diff_oracle/oracle.py` needs Linux + Xvfb +
  the python `websockets` module and refuses on a plain macOS host (see results/diff_oracle_attempt.txt).

## Safety

- Every run uses a scratch `BLADE_HOME` (`mktemp -d`), `BLADE_NO_UPDATE_CHECK=1`, `BLADE_NO_WARMING=1`,
  `BLADE_LANE=agent`; it never touches a real browser profile or `~/.blade`.
- Fixtures are local static HTML served on `127.0.0.1`. No live sites, no live stealth-test pages.
- `env.sh` provides `bb_cleanup` (stops the daemon, kills the fixture server, removes scratch dirs); every scenario
  installs `trap bb_cleanup EXIT`. Run `bladebro stop` yourself after manual experiments.

## Layout

- `sidecar.mjs` - the recorder CLI (attach, screencast, ack, bounded queue, ffmpeg writer, stats JSON).
- `lib/cdp.mjs` CDP client + percentiles; `lib/encoder.mjs` ffmpeg args, frame queue, spool, probe parsing;
  `lib/follow.mjs` tab-follow policy and `--script` parser; `lib/actions.mjs` harness-only window/viewport actions;
  `lib/attach.mjs` `--attach-wait` discovery.
- `measure.mjs` - `probe` / `merge` / `encode-bench` helpers that turn run stats into result JSON.
- `battery_probe.py` - runs the diff_oracle battery against the current tab and classifies run-to-run noise.
- `env.sh` - scratch home, fixture server, `bb_port`, `bb_cleanup`. `fixtures/` - `anim` (~100 fps repaint),
  `idle`, `tick` (one repaint per second), `a`, `b`.
- `scenarios/` - the measurement scripts below. `results/` - recorded outputs. `test/` - unit tests.

## sidecar.mjs flags

Required: `--out FILE` and one of `--port N` / `--attach-wait S --blade-home DIR`.

| flag | default | meaning |
|---|---|---|
| `--port N` | | browser debugging port (see `bb_port`) |
| `--attach-wait S --blade-home DIR` | | wait for a `DevToolsActivePort` under DIR (does not work on the agent lane) |
| `--target ID` | first real page | page target to record |
| `--duration S` | until SIGINT/browser gone | stop after S seconds |
| `--codec vp9\|vp8\|x264` | vp9 | encoder (x264 writes mp4) |
| `--ts wallclock\|concat\|cfr` | wallclock | timestamp strategy |
| `--fps N` | 10 | output rate for cfr and for concat/size tables |
| `--quality N` | 70 | JPEG quality |
| `--max-width N` / `--max-height N` | 1280 / 720 | screencast bound |
| `--every-nth N` | 1 | `everyNthFrame` |
| `--queue N` | 16 | frame queue length (drops the incoming frame when full) |
| `--slow-writer-ms N` | 0 | artificial writer delay (slow-consumer test) |
| `--size-mode fixed\|even` | fixed | one output size vs per-frame even sizes |
| `--follow off\|newest\|poll` | off | follow the active tab |
| `--poll-ms N` | 250 | poll interval for `poll` |
| `--script SPEC` | | harness actions, e.g. `5:emulate=1001x641;12:emulate-clear;18:window=1101x701` |
| `--no-page-enable` | off | skip `Page.enable` |
| `--no-tail` | off | do not hold the last frame to the stop time |
| `--stats FILE` | `<out>.stats.json` | stats output |
| `--finalize-timeout MS` | 10000 | wait for ffmpeg to finish |
| `--keep-frames` | off | keep the concat spool |
| `--ffmpeg PATH` | `$BLADE_FFMPEG` | ffmpeg binary |

Port: `source tools/record_sidecar/env.sh; $BB nav "$FIX/anim.html"; P=$(bb_port)` (reads the Chrome process line,
falls back to `DevToolsActivePort` if one exists). Quick run:

```bash
source tools/record_sidecar/env.sh; trap bb_cleanup EXIT; fixture_up
$BB nav "$FIX/anim.html"; P=$(bb_port)
node $RS/sidecar.mjs --port $P --duration 8 --codec vp9 --ts wallclock --out $SCRATCH/out.webm
```

## Scenarios

Each sources `env.sh`, uses fresh scratch homes and tears down its own browser. Run from the repo root with a built
`target/release/bladebro`.

| command | runtime | writes |
|---|---|---|
| `bash tools/record_sidecar/scenarios/tabswitch.sh` | minutes | results/tabswitch.json, tabswitch_sample.webm |
| `bash tools/record_sidecar/scenarios/core.sh` | minutes | results/core.json (fps, acks, drops, drift) |
| `bash tools/record_sidecar/scenarios/extra.sh` | longest (minutes) | results/extra.json (viewport, headless, background, codecs, coexistence) |
| `bash tools/record_sidecar/scenarios/stall_probe.sh` | 2 min of capture | results/stall_probe.json |
| `bash tools/record_sidecar/scenarios/stealth.sh` | 20 s to 2 min | results/stealth.json, audit_*.txt, diff_oracle_attempt.txt, stats/audit_*.stats.json |
| `bash tools/record_sidecar/scenarios/source_live.sh [dur-idle dur-tick dur-anim resize bg]` | about 4 min (headless, local fixtures) | .planning/phases/06-frame-source/evidence/06-02-live.txt (REC_LIVE_REPORT, ffprobe, decode, drift) |
| `bash tools/record_sidecar/scenarios/source_live.sh trace\|slow\|all` | about 1 min (headless, local fixtures) | .planning/phases/06-frame-source/evidence/06-03-trace.txt (wire method trace through a test-only WS proxy, Chrome argv before/during/after, slow-consumer counters, concurrent `see`/`state tabs`/`nav`, PASS/FAIL lines computed by script) |
| `bash tools/record_sidecar/scenarios/source_stealth.sh` | about 1 min (headless, local fixtures) | .planning/phases/06-frame-source/evidence/06-03-stealth.txt (`bladebro audit` without / with the product recorder on the audit's own browser with attempts, diff_oracle NOT RUN note, battery A / B (recorder attached) / A2 classify) |
| `python3 tools/record_sidecar/scenarios/sample_ts.py` | seconds | results/sample_timestamps.json |
| `bash tools/record_sidecar/scenarios/hub_live.sh [--only ID]` | about 3 min (headless, local fixtures; builds the pre-phase binary once into target/hub-base unless `HUB_BASE_BIN` is set) | .planning/phases/07-recordhub-and-state-ops/evidence/07-03-live.txt (MCP stdio tests of `tests/record_hub.rs`, CLI daemon record-start/record-stop/double-start/one-shot/off-mode, base-vs-new CDP method comparison through `lib/cdp_tap.py`, audit, qol_probe, lane_matrix; PASS/FAIL lines computed by script, cleanup check after every scenario) |
| `bash tools/record_sidecar/scenarios/follow_live.sh [--only ID]` | about 4 min (headless, local fixtures; builds the pre-phase binary once into target/follow-base unless `FOLLOW_BASE_BIN` is set) | .planning/phases/08-tab-following/evidence/08-01-live.txt (ids `mcp` = `tests/record_follow.rs`, `daemon` switch/open-tab, `popup`, `close` recorded tab closed, `xorigin`, `off` = no follower and base-vs-new WS method set and `/json` count, `perf` informational, `audit`, plus (08-02) `per-tab` = project `.bladebro.json` `record.tabs` gives one file per tab and `BLADE_RECORD_TABS=follow` overrides it, `per-tab-close` = a closed tab's file is finished while recording continues, and `mcp` also runs the per-tab live tests; ffprobe, decoded-frame and colour checks, cleanup check after every scenario) |
| `POPUP_BASE_BIN=<binary built from a92ad77> bash tools/record_sidecar/scenarios/popup_live.sh [--only ID]` | about 8 min (headless, local fixtures `popper.html` red opener, `b.html`/`selfclose.html`/`chain.html` blue popups, `c.html` green; scratch home per scenario) | .planning/phases/08.1-follow-every-popup/evidence/08.1-02-live.txt (ids `kinds` = blank/js/window/xorigin popups, 3 runs each, follow delay min/median/max, `close`, `close-opener`, `chain`, `burst`, `blocked`, `tabs` = state tabs markers + footer, `per-tab`, `exit` = MCP stdin EOF / MCP SIGTERM / daemon stop / daemon SIGTERM with a popup segment active (lib/popup_exit.py), `off` = cdp_tap method set and `Page.bringToFront` / `Target.getTargets` counts vs the a92ad77 binary, `mcp` = `tests/record_popup.rs`; `spike` only with `--only spike`; ffprobe, full decode, decoded-frame count and midpoint colour checks, cleanup check after every scenario) |
| `bash tools/record_sidecar/scenarios/exit_live.sh [--only ID]` | about 3.5 min (headless, local fixtures; idle scenarios wait about 30 s each) | .planning/phases/09-finalization-on-every-exit-path/evidence/09-03-live.txt (`lib/exit_live.py` drives `bladebro mcp` over stdio and the CLI daemon through one exit each: ids `mcp_eof`, `mcp_sigterm`, `mcp_sigint`, `mcp_idle` (BLADE_IDLE_TIMEOUT=20), `daemon_stop`, `daemon_sigterm`, `daemon_sigint`, `daemon_idle`, `mcp_kill9`, `daemon_kill9`, `mcp_kill_chrome`, `daemon_kill_chrome`, `mcp_kill_ffmpeg`, `daemon_kill_ffmpeg`, `oneshot`; each: ffprobe + full decode, exact result text, `ps` sweep for ffmpeg/bladebro/Chrome under the scratch home within 10 s; only scratch-home pids are signalled; exit code = number of FAIL lines) |
| `REC_HUB_FIX=http://127.0.0.1:8765 EXIT_BIN=target/release/bladebro python3 -I tools/record_sidecar/lib/sizes_live.py [SECONDS]` | 6 settings x (SECONDS + about 5 s), default 60 | file size per minute at default / mp4 / 15 fps / 1080p / max size (stdout; output of the 09-03 run: evidence/09-03-sizes.txt) |
| `python3 -I tools/record_sidecar/lib/cdp_tap.py --self-test` | seconds | stdlib logging TCP proxy for Chrome's debug port (`cdp_tap.py LISTEN CHROME LOGFILE`): rewrites `/json*` ws URLs, unmasks client frames, logs CDP method names. No `websockets` module needed |
| `REC_HUB_FIX=http://127.0.0.1:8765 python3 -I tools/record_sidecar/lib/lane_matrix_local.py <lane> 5` | about 20 s per lane | `tools/lane_matrix.py` run unchanged except a `ps` process sweep (the original needs `/proc`, Linux only) and the local `idle.html` fixture instead of example.com |

`source_live.sh` drives the product `FrameSource` + `Encoder` (not the Node sidecar) through the `#[ignore]` test `tests/record_live.rs` (inputs `REC_LIVE_PORT/TARGET/OUT/SECS/MODE`, one `REC_LIVE_REPORT <json>` stdout line); it is test code, not a CLI command or tool. Always `BLADE_HEADLESS=1`. Modes of `REC_LIVE_MODE`: `plain`, `resize`, `trace` (adds `wire_methods`/`wire_first` from the proxy), `slow` (300 ms/frame SlowSink, queue 16; adds `slow_sink {accepted, refused, max_len}`); every mode reports `frame_sizes` (distinct JPEG pixel sizes seen).

Runtimes were not timed except the stealth scenario (20 s to 2 min across my runs). The stealth scenario retries the audit-attached sidecar
up to 8 times per variant because the attach races the audit's own startup; the attempt count is recorded.

## Tests

```bash
node --test tools/record_sidecar/test/*.test.mjs
python3 -m unittest discover -s tools/record_sidecar/test -p 'test_*.py'
```

## Reading results

Every `results/*.json` is written by a script from run stats, not typed by hand; a value that was not measured is the
string `not_measured_because`. `stats/*.stats.json` are the raw per-run sidecar stats. Start with `RESULTS.md`: its
measurement table cites the source file and key for each number, and `## Gate verdict` carries the decision.
