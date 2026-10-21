# record_sidecar

Phase 2 gate probe for opt-in session recording. A standalone CDP screencast recorder
(Node ESM, zero npm dependencies) that attaches to a bladebro Chrome page over the browser
WebSocket, acks every screencast frame, and pipes the JPEGs into ffmpeg.

This is NOT shipped in the bladebro binary and is NOT used by `src/`. It exists only to
measure whether CDP screencast + ffmpeg is a viable recording path before any product
code is written.

## Requirements

- Node >= 22 (global `WebSocket` and `fetch`). Tested on Node 24.
- ffmpeg via `BLADE_FFMPEG` (and `BLADE_FFPROBE` for the smoke probe). Tested on ffmpeg n9.0.2.
- A bladebro agent-lane Chrome on the WebSocket transport. The sidecar finds the endpoint from
  the Chrome process command line (`--remote-debugging-port=<N>`); the agent lane does not write
  `DevToolsActivePort`.
- Does NOT work with `BLADE_TRANSPORT=pipe`: pipe mode has no browser WebSocket endpoint.

## Safety

- Every run uses a scratch `BLADE_HOME` (`mktemp -d`), `BLADE_NO_UPDATE_CHECK=1`,
  `BLADE_NO_WARMING=1`, `BLADE_LANE=agent`, and never touches a real browser profile or `~/.blade`.
- Fixtures are local static HTML served on `127.0.0.1` only. No internet sites.
- `env.sh` provides `bb_cleanup`, which stops the daemon, kills the fixture server, and removes
  the scratch directories. Scripts that source it install `trap bb_cleanup EXIT`.

## Usage

Smoke commands (headed Chrome, under 2 minutes total). Each sidecar run attaches a NEW session
to the same tab:

```bash
source tools/record_sidecar/env.sh; trap bb_cleanup EXIT; fixture_up
$BB nav "$FIX/anim.html"; P=$(bb_port)
S="node $RS/sidecar.mjs --port $P --duration 8"
$S --codec vp9  --ts wallclock --out $SCRATCH/vp9_wall.webm
$S --codec vp8  --ts wallclock --out $SCRATCH/vp8_wall.webm
$S --codec x264 --ts wallclock --out $SCRATCH/x264_wall.mp4
$S --codec vp9  --ts concat    --out $SCRATCH/vp9_concat.webm
$S --codec vp9  --ts cfr --fps 10 --out $SCRATCH/vp9_cfr.webm
$S --codec vp9  --ts wallclock --size-mode even --out $SCRATCH/vp9_even.webm
$S --codec vp9  --ts wallclock --no-page-enable --out $SCRATCH/vp9_nopage.webm
```

Unit tests:

```bash
node --test tools/record_sidecar/test/*.test.mjs
```

Each run writes `<out>.stats.json` (frames, acks, queue drops, encoder exit, bench line).

## Status

Skeleton: the recorder and smoke runs exist; the verdict and the measurement matrix are in plan
02-02 and 02-03 (`.planning/phases/02-sidecar-prototype-gate/`).
