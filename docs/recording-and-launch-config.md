# Recording and launch configuration

Single reference for every bladebro-fork option: session recording, the ffmpeg override and the managed browser's launch mode. The README only points here.

## Quick answers

- Separate file per tab? `BLADE_RECORD_TABS=per-tab` (see [Per-tab files](#per-tab-files)).
- Hide the browser for tests: `BLADE_HEADLESS=1` (one run), `bladebro headless on` (your machine) or `"headless": true` in a committed `.bladebro.json` (a suite).
- Keep it visible: leave everything unset (macOS and Windows show a window; Linux uses its private virtual display), or set `BLADE_HEADLESS=0` to override a lower layer.
- Where settings live: environment, then `.bladebro.json` found from the current directory upward, then `<data root>/launch.json` and `<data root>/record.json`. See [Configuration layers and precedence](#configuration-layers-and-precedence).

## Environment variables

| Var | Default | Values | What it does |
|---|---|---|---|
| `BLADE_HEADLESS` | unset (platform default) | `1`/`on`/`true`/`yes`: hidden browser on every OS. `0`/`off`/`false`/`no`: visible browser. Anything else is an error naming the value | Forces the managed browser hidden or visible. Managed lane only |
| `BLADE_RECORD` | off | `1`/`on`/`true`/`yes`: on, default dir. `0`/`off`/`false`/`no`: off. Absolute path: on, record into that dir. Relative paths are rejected | Session recording switch |
| `BLADE_RECORD_FORMAT` | `webm` | `webm` (VP9) or `mp4` (H.264) | Recording container |
| `BLADE_RECORD_FPS` | `10` | Whole frames per second, 5-15 | Recording frame rate |
| `BLADE_RECORD_SIZE` | `1280x720` | `WIDTHxHEIGHT`, both sides even, 160x120 to 3840x2160 | Recording frame size |
| `BLADE_RECORD_TABS` | `follow` | `follow`: one file follows the agent's tab. `per-tab`: one file per tab (at most 8 open). Anything else is an error naming the value | Multi-tab recording mode |
| `BLADE_FFMPEG` | auto | Absolute path to the ffmpeg binary used for recording. Set but unusable is reported, never silently ignored. Otherwise PATH, then known install locations. `bladebro doctor` shows what was found | ffmpeg override |

## Browser launch mode (BLADE_HEADLESS)

What each setting does to the managed browser, per OS:

| OS | unset or `0` | `1` |
|---|---|---|
| macOS | native window (visible) | `--headless=new`, no window |
| Windows | native window (visible) | `--headless=new`, no window |
| Linux | headful on bladebro's private Xvfb display (invisible to you by design, best stealth); if Xvfb is missing, the existing honest headless fallback with its warning | `--headless=new`, Xvfb is never started and no Xvfb warning is printed |

Notes:

- Unset and `0` behave identically on every OS. `0` exists to override a lower layer (a later config file) that says headless.
- On Linux, "headed" means headful on bladebro's own Xvfb display, not a window on your desktop. Showing a window on your own Linux desktop is not offered: the managed lane strips Wayland and pins X11 to Xvfb.
- The mode is decided once per process. The daemon and the MCP server keep one mode for their lifetime, so restart them (`bladebro stop`) to change it. A GL-ladder or crash relaunch never flips the mode.
- Launch note: one stderr line `[bladebro] browser mode: <mode> (source: <source>)` per managed launch process (MCP server, one-shot, `audit`). On the default CLI path the browser lives in a background daemon whose stderr is discarded, so the CLI itself prints the note, once, on the invocation that started the daemon (never on later commands that reuse it; stderr only, so `--json` stdout is untouched). On Linux that client note states the requested mode (`headed (virtual display)`); if Xvfb is missing the daemon still falls back to headless with its own warning, visible in `bladebro doctor` and the MCP server's stderr. Modes: `headless`, `headed`, `headed (virtual display)`, `headless (no Xvfb; fallback)`. Source: `env BLADE_HEADLESS`, `project file <path>`, `user config <path>` or `default`. stdout is untouched, so `--json` and MCP output stay machine-clean.
- Stealth: a forced-headless launch sets the launched-headless flag, so the headless-only masks apply. The "no Xvfb display was available" advisory is not emitted when headless was requested, since it would be false.
- Audit parity: forced-headless scores the same stealth audit as headed (100/100 on macOS, Chrome 154). On macOS/Windows a headless launch pins a 1920x1080 screen and a 1880x1010 window (otherwise Chrome reports its 800x600 default), and the headless UA override carries client-hints metadata so `navigator.userAgentData` has the same brand list as headed Chrome (`HeadlessChrome` is never exposed). The metadata is only attached when headless was requested (`BLADE_HEADLESS` / config); the unset Linux no-Xvfb fallback keeps its previous behavior until it is measured.
- Client hints: `fullVersionList` and `uaFullVersion` use the browser's real full version (CDP `Browser.getVersion`, e.g. `154.0.8037.58`) and `platformVersion` comes from the OS API (macOS `sysctl kern.osproductversion`, Linux `uname` release; read in-process, no program is spawned), so they equal what headed Chrome reports (measured on macOS: headless and headed `getHighEntropyValues` and the `Sec-CH-UA-Full-Version-List` / `Sec-CH-UA-Platform-Version` headers match). Still differs or unmeasured: Windows `platformVersion` is left empty (headed Chrome reports a build-derived value); Linux headless geometry and `platformVersion` are by code reading only; the stealth diff oracle has not been run for headless on Linux.
- Window geometry (macOS/Windows headless): the screen is pinned to 1920x1080 and the window to 1880x1010 (headless adds a 16 px frame, so `outerWidth` is 1896 and `outerHeight` 1010; at the `screenX`/`screenY` 22 headless uses, the outer window ends at 1918x1032, inside the screen). Still differs: there is no menu-bar work area (`availHeight` equals `height`); the device pixel ratio is 1.
- Measured: `bladebro audit` on macOS 26.6.2, Chrome 154.0.8037.58: SCORE 100/100 with `BLADE_HEADLESS=1` and SCORE 100/100 with `BLADE_HEADLESS=0`, VERDICT: PASS in both.
- The real-browser lane ignores `BLADE_HEADLESS`; use `bladebro rb visible on|off` there.
- Older workaround: `BLADE_CHROME_FLAGS="--headless=new"`. Replace it with `BLADE_HEADLESS=1`: with raw flags, stealth believed the launch was headed. If `BLADE_CHROME_FLAGS` contains `--headless` and `BLADE_HEADLESS` does not force headless, the launch note and `bladebro doctor` say `headless via BLADE_CHROME_FLAGS` and recommend `BLADE_HEADLESS=1`; they never say headed.
- Invalid setting: a bad `BLADE_HEADLESS` (or project file) is refused by the CLI before any daemon starts, so nothing is left running. A daemon that was started with an invalid setting by another route (an older build) answers every command with `this daemon was started with an invalid launch setting ...; run bladebro stop`. The MCP server reports the real message on the first tool call (`Could not launch Chrome: BLADE_HEADLESS: ...`) without the "Check that Chromium is installed" hint; it does not fail at startup.

## Recording settings

Recording is controlled by the `state` ops [below](#recording-with-the-state-tool) and is off unless the operator enables it.

**Recordings contain whatever was on screen**: passwords as typed into visible fields, session tokens, personal data, private pages. Treat every recording as sensitive. Do not enable recording for sessions that use real credentials. bladebro never prunes recordings; delete them in your harness.

Config file: `<data root>/record.json` (data root: `~/.blade`, or `BLADE_HOME` when set; on Windows `%USERPROFILE%\.blade`). Fields are listed in [Config keys](#config-keys); `quality` is JPEG 1-100 (default `70`, file only) and `dir` must be absolute in this file. Unknown fields are an error. Precedence: per-call > env > project file > `record.json` > default (see [Configuration layers and precedence](#configuration-layers-and-precedence)). Through the `state` tool the only per-call input is the recording `name`; `format`, `fps` and `size` come from env, the project file, `record.json` or the defaults (the library API keeps a per-call layer for them). `enabled`, `dir` and `quality` have no per-call layer. A bad value in any layer is an error, even when a higher layer overrides it. Invalid values are rejected, never clamped. Errors name their source (`BLADE_RECORD_FPS:`, `record.json fps:`, `per-call fps:`).

Where files go:
- Default dir: `<data root>/recordings/`. A dir inside `<data root>/artifacts/` is refused (artifacts rotate at 300 files). Nothing in bladebro rotates, prunes or deletes recordings.
- File names: `<name>-<UTC timestamp>-<id>.webm` (or `.mp4`). The name keeps only `A-Z a-z 0-9 . _ -`, is capped at 80 characters, and falls back to `recording`. Existing files are never overwritten.
- Unix: a new or empty dir is set to `0700`; files are `0600`. A non-empty dir that is group- or other-accessible is refused (run `chmod 700` on it, or use a dedicated dir). A symlinked dir and a dir owned by another user are refused.
- Windows: files inherit the directory's permissions. The default under your profile is private to your account; keep `BLADE_RECORD` inside your profile. bladebro sets no Windows ACLs.

### Recording time

The video clock starts at the first frame, not at the `record-start` call, and ends when the stop request reaches the capture loop. Video duration is frames divided by fps. `record-stop` then takes about 0.5 s (a fixed 500 ms stop grace) before it returns, so the wall-clock time from the `record-start` call to the `record-stop` return is about 0.55 s longer than the video duration. A late first frame (for example a hidden tab that starts painting later) makes the video shorter by that delay. Segment offsets are accurate to about one tab-list check (about 0.25 s).

### How recording is encoded

- ffmpeg is run directly with an argument list (no shell); JPEG frames go to its stdin, its stdout is discarded and the last 4 KiB of its error output is kept for the result.
- WebM uses VP9 (`libvpx-vp9`, realtime, crf 40); MP4 uses H.264 (`libx264`, veryfast, crf 28, faststart). Output is always the configured size (default 1280x720), letterboxed, with even dimensions.
- Timing: frames are written at a constant rate equal to the configured fps; a frame is repeated for as long as it stayed on screen (from the capture timestamps), and the last frame is held until the stop, so video length matches the recording time.
- Stop: bladebro closes ffmpeg's input and waits for it to finish: normally well under 10 s, at most about 18 s in the worst case (10 s drain, 2 s minimum wait for ffmpeg, 5 s to reap after a kill, 1 s for its error output). Success is reported only when ffmpeg exits 0, the file is non-empty and every accepted frame was written. If ffmpeg does not finish in time it is killed, the result says "unfinalized", and the partial file is kept. If frames were still unwritten at the 10 s deadline, the result says "truncated" with the number of frames not written.
- Errors name the cause and the fix: ffmpeg missing, `BLADE_FFMPEG` unusable, encoder missing for the format (switch with `BLADE_RECORD_FORMAT`), or the ffmpeg exit code with its last error lines. Browsing continues either way.
- Requires ffmpeg 4.3 or newer (`bladebro doctor` shows the version and encoders); this is not checked in code, so an older ffmpeg fails when encoding starts and the result carries its exit error. These limits are fixed; there is no setting for them.

### How frames are captured

- The recorder opens its own connection to the recorded tab and sends only `Page.enable`, `Page.startScreencast` (JPEG, quality 70, at most the configured size), `Page.screencastFrameAck` and `Page.stopScreencast`. It injects no script, changes no viewport or emulation, draws no overlay and adds no launch flag.
- Every frame is acknowledged as soon as it arrives. Headless Chrome sent about 60 frames/s on an animated page in testing (1200 frames in 20 s) and about 100 headed; the recorder keeps at most the configured fps (newest frame wins, the last one is never lost; 200 frames reached the encoder in those 20 s at 10 fps) and hands frames to the encoder through a queue of 16. When the encoder falls behind, the newest frame is dropped and counted. The report gives frames received, acknowledged, sent to the encoder, skipped by the fps cap, dropped and lost.
- No frames: if not a single frame arrives, the result is "no frames for N s since recording start", never a finished video, however short the recording. After 5 s without a first frame a warning is logged and recording continues. A background tab behaved this way in testing: 0 frames in 8 s, and Chrome reported the tab hidden.
- Limits: a background tab, a minimized window (about 0.3 frames/s) or a covered window delivers no new frames. A static page sends about one frame and then nothing (a 20 s recording of a still page received 1 frame), which looks the same as a stalled capture, so after the first frame silence is reported (time since the last frame, longest gap), not treated as failure, and the video holds the last frame. A one-off 37 s silence was once seen in testing (cause unknown). The video used to come out about 0.4 to 0.5 s longer than the recording time (the last frame was held through the 0.5 s stop grace); it now ends at the stop request. Measured on 8 s recordings (8 runs, including stops off a whole second) the video length differed from the recording time by -0.74 % to +0.16 % (about 60 ms at most on that slow-ticking page, since the video is built from whole frame slots at the chosen fps). On animated pages the video ran about 100 ms long (+0.6 % to +1.2 % on 8 to 12 s recordings): frames that arrive during the stop grace are still added, and the last frame is always written at least once. The theoretical worst case is about 0.5 s plus one frame slot. Recording needs the default WebSocket transport: with `BLADE_TRANSPORT=pipe` it reports that it cannot record (unset it and run `bladebro stop`).
- These limits are fixed; there is no setting for them.

## Recording with the state tool

Two ops on the existing `state` tool (there is no sixth tool):

- `record-start` with `name` (required): starts a video of the tab the agent is on; the video follows the agent's tab (see [Tab following](#tab-following)). The file is `<sanitized name>-<UTC>-<id>.<ext>` in the recordings dir.
- `record-stop`: finalizes the file and reports it.

```json
{"op": "record-start", "name": "login-flow"}
{"op": "record-stop"}
```

CLI: `bladebro state record-start login-flow` and `bladebro state record-stop`. In a `batch` or `run` step: `{"action": "state", "op": "record-start", "name": "x"}`. Recording is off unless enabled (for example `BLADE_RECORD=1`), and it needs ffmpeg (`bladebro doctor` shows what was found).

Start result (the first frame is not awaited):

```
recording started: /abs/path/smoke-20261022T122754Z-c1f8f8.webm (tab A89A...); first frame not awaited. record-stop saves it.
```

Stop result, from a real run:

```
recording saved: /abs/path/smoke-20261022T122754Z-c1f8f8.webm (bytes 51945, duration 4.1 s, frames 41, dropped 0, segments [0.0 s tab A89A...])
```

| Field | Meaning |
|---|---|
| path | absolute path of the file |
| bytes | file size |
| duration | frames written / fps (not re-measured from the file) |
| frames | frames written to the video |
| captured | frames passed to the encoder after the fps cap (Chrome may send many more); shown only when different from `frames`. It can exceed `frames`, because a frame on screen for less than one 1/fps slot gets no video slot, and it can be lower, because a held frame is repeated to fill its time |
| dropped | frames dropped by the encoder queue plus frames lost on the CDP connection |
| segments | one entry per tab the video shows, `[<offset> s tab <id>]`; the offset is the time of that tab's first frame on the video clock (first frame of the video = 0.0), and the id is the full target id that `state tabs` prints. A tab with no frames is labelled `<id> (no frames)` |
| notes | "no new frames for N s" (static page or stalled tab; the video holds the last frame); `tab <A> gone at <T> s; following tab <B>` when the recorded tab was closed or replaced (see [Tab following](#tab-following)) |

The stop is `ok` only when capture and encoder both finished cleanly. Otherwise it is an error (MCP `isError`, CLI exit 1; in `batch`/`run` an error step stops the batch) whose text starts with `recording not clean:` and still carries the path and numbers.

A stop can take up to about 18 s in the worst case (500 ms capture grace, up to 10 s encoder drain, then the trailer). Set your client's tool timeout above that.

Refusals (nothing is recorded, an error is returned):

| Case | Text starts with |
|---|---|
| recording off | `recording is off: the operator has not enabled it. Set BLADE_RECORD=1 ...` |
| already active | `recording already active: <path> (started N s ago, ...). record-stop it first.` |
| none active (stop) | `no active recording; record-start <name> first.` |
| pipe transport | `recording unavailable: BLADE_TRANSPORT=pipe has no debugging port ...` |
| one-shot CLI (`--no-daemon`, `--host/--port`; every CLI call on Windows, so record through MCP there) | `recording needs a long-lived session: use the MCP server or the daemon ...` |
| real-browser lane | `recording is not available in the real-browser lane ...` |
| ffmpeg missing or unusable | the ffmpeg error (see [How recording is encoded](#how-recording-is-encoded)) |

### Tab following

One recording is one file and one ffmpeg process for the whole session; the video follows the tab the agent is on and each tab it shows is a segment in the stop result. Real results (headless Chrome, local fixtures; ids shortened):

```
recording saved: /abs/path/f-20261022T140820Z-527648.webm (bytes 30013, duration 8.0 s, frames 80, dropped 0, segments [0.0 s tab F249..., 3.9 s tab 746A..., 6.2 s tab F249...], captured 63)
recording saved: /abs/path/c-20261022T141401Z-e248c8.webm (bytes 27994, duration 7.4 s, frames 74, dropped 0, segments [0.0 s tab B59C..., 3.3 s tab 3AA1..., 5.3 s tab B59C...], captured 59). Note: tab 3AA1... gone at 5.3 s; following tab B59C...
```

How it works:

- The recorder reads Chrome's tab list over HTTP (`/json` on the debugging port) every 200 ms. It sends no CDP command for this and nothing is visible to the page. The agent is on the first page of that list (Chrome orders it most recently activated first; Chrome did not report an `attached` flag, so that flag is not used). A switch is made when the same tab is first on two samples about 50 ms apart.
- `switch-tab` to an open tab is followed within about 0.25 s (allow up to about 0.7 s on a busy machine or when the agent goes back and forth right away): the recorder connects to the new tab first, then releases the old one. `open-tab` is followed after the new tab has stayed in front for 1.5 s (about 1.7 s in total), so the segment offset is that much later than the call; this delay is what keeps a popup that is put back behind its opener from becoming a segment.
- Popups: a click that opens a tab (`target=_blank`, or `window.open` without window features) leaves the opener as the agent's page (the opener is re-activated after the click), so such a popup is not in the video until the agent runs `switch-tab` to it, and then it becomes a segment. A popup that opens as its own window with window features (`window.open(url, '', 'width=500,height=400')`) is different, and this was measured live (headless, local fixture, the agent never ran `switch-tab`): the recording followed it, with a segment starting 3.7 s after `record-start` for a click made about 1.5 s in (it has to be in front for 1.5 s first), while `state tabs` kept the opener as the current tab. It did not go back to the opener at the next input action: after a `press Tab` at about 7 s the video stayed on the popup, and the return segment appeared only at 10.7 s, at `record-stop` (11 s after the start). Only `press` was measured; `click` and `type` were not. So with window-feature popups the video can show the popup while the agent works on the opener; run `switch-tab` back to the opener if that matters. A popup that was already first when `record-start` ran is ignored until the front tab changes once.
- Several switches inside one check are not all recorded: only the last tab is followed and the tabs in between get no segment. A switch made less than one check before `record-stop` is caught up at the stop, so the last tab is in the video.
- The recorder opens its own connection for every tab it records, with the same four methods as above.
- Tabs of a different size are letterboxed into the configured size (the file keeps one width and height; tested with 1280x720 and 500x400 frames into one WebM and one MP4).
- Closing the recorded tab, or a tab replaced by a new target id, continues on the agent's next tab with the note `tab <A> gone at <T> s; following tab <B>` and the result stays "recording saved". Cross-site navigation of the recorded tab kept the same tab id and frames in testing (127.0.0.1 to localhost).
- Not clean (`recording not clean: ...`, the file is kept): no tab left (`no tab left at <T> s`), the browser stopped answering the tab list three times in a row (`browser connection lost at <T> s`), or a tab that could not be recorded (`tab <B> not recorded from <T> s: ...`). Never `recording saved` when the agent's tab went unrecorded.
- A segment with no frames for at least 5 s is not clean (`tab <B>: no frames for N s (hidden or background tab)`); a shorter one is shown as `<B> (no frames)` and stays clean. A video with no frame at all keeps the "no frames" failure.
- Only one recording runs at a time; a second `record-start` is refused while the first is following.

The environment is read by the long-lived process. A daemon or MCP server started without `BLADE_RECORD` keeps answering "recording is off" until `bladebro stop` (or an MCP restart). The project file `.bladebro.json` is found from that process's working directory.

Browser shutdown finalizes an active recording on a best-effort basis when it comes from MCP shutdown (stdin EOF, signal, idle) or the daemon's normal exit; the next record op returns that report prefixed `previous recording ended with the browser: `. Not covered yet (owned by Phase 9): daemon idle-timeout shutdown, the daemon drift relaunch and Closed-relaunch paths, and a daemon relaunch after a crash. On those the hub may still look active, and the next `record-stop` reports a lost connection. A killed process (SIGKILL) can leave an unfinalized file.

### Per-tab files

Set `BLADE_RECORD_TABS=per-tab` (or `record.tabs` in `record.json` / `.bladebro.json`) to get one file per tab instead of one file that follows the agent. There is no `state` parameter for it. Real results (headless Chrome, local fixtures; directories and ids shortened; the second result is a run where tab 2 was closed while recording):

```
recording saved (per-tab, 3 files): /abs/recordings/pt-tab1-20261022T145420Z-dd5570.webm (bytes 18877, duration 10.7 s, frames 107, dropped 0, segments [0.0 s tab 182C..., 8.7 s tab 182C...], captured 42); /abs/recordings/pt-tab2-20261022T145424Z-905bd9.webm (bytes 13282, duration 6.6 s, frames 66, dropped 0, segments [0.0 s tab B397...], captured 16); /abs/recordings/pt-tab3-20261022T145427Z-196942.webm (bytes 11114, duration 3.4 s, frames 34, dropped 0, segments [0.0 s tab 583E...], captured 14)
recording saved (per-tab, 2 files): /abs/recordings/ptc-tab1-20261022T145446Z-6a8fd0.webm (bytes 15579, duration 7.2 s, frames 72, dropped 0, segments [0.0 s tab FC01..., 5.5 s tab FC01...], captured 33); /abs/recordings/ptc-tab2-20261022T145450Z-3ee0a9.webm (bytes 11919, duration 2.4 s, frames 24, dropped 0, segments [0.0 s tab 45CC...], captured 21). Note: tab 45CC... gone at 5.5 s; following tab FC01...; tab 45CC... closed at 5.7 s
```

How it works:

- Files are named `<name>-tab<N>-<UTC>-<id>.<ext>`; `N` is the order of first visit (the tab you started on is `tab1`). Every file goes through the same folder, permission and no-overwrite rules as a single recording.
- Returning to a tab continues its file (a new segment in that file; offsets are from that file's first frame). A file lasts from the tab's first visit until the tab closes or `record-stop`; while the agent is on another tab it holds its last frame (a gap longer than 10 minutes is clamped).
- A closed tab's file is finished at once (its ffmpeg ends while recording continues) and its slot is free again. The result notes `tab <id> closed at <T> s`.
- At most 8 files are open at once (one ffmpeg each, so CPU and disk grow with open tabs). The next tab is not recorded and the result says so: `tab <id> not recorded from <T> s: per-tab cap of 8 open files reached (close a tab to free one)`. That note does not make the result not clean. Close a tab to free a slot; a tab that went unrecorded is tried again when the agent returns to it.
- Each file is finished on its own. One that fails (ffmpeg exits, the file cannot be created) makes the result `recording not clean (per-tab, N files): tab <id>: <reason>; <every file>`; the other files are finished and listed. A file that cannot be created is named `tab <id> not recorded from <T> s: <error>`.
- `record-stop` finishes all files at the same time, each bounded; a file that cannot be finished in time is listed as `not finalized`. A second `record-start` is refused and names the first file.

### Example: Claude Code (.mcp.json)

```json
{
  "mcpServers": {
    "bladebro": {
      "command": "bladebro",
      "args": ["mcp"],
      "env": { "BLADE_RECORD": "1" }
    }
  }
}
```

`record-stop` can take about 18 s, so your client's MCP tool timeout must be longer than that.

### Example: DSH plugin

The plugin (`npm/bladebro/dsh-plugin.mjs`) runs `bladebro mcp` over stdio and accepts the options `binaryPath`, `env`, `cwd`, `toolCallTimeoutMs` (default 180000 ms, which covers the stop) and `reconnect`:

```js
{ env: { BLADE_RECORD: "1" }, cwd: "/path/to/project" }
```

`cwd` decides which `.bladebro.json` applies.

## Configuration layers and precedence

Highest layer first. A layer that does not set a value falls through to the next one.

| Setting | per-call | env | project `.bladebro.json` | user file | default |
|---|---|---|---|---|---|
| headless | none (suite-level by design) | `BLADE_HEADLESS` | `headless` | `<data root>/launch.json` | platform default |
| recording on/off | none | `BLADE_RECORD` | `record.enabled` | `record.json` `enabled` | off |
| recording dir | none | `BLADE_RECORD` (absolute path) | `record.dir` (vetted, see below) | `record.json` `dir` | `<data root>/recordings` |
| format, fps, size | none via the `state` tool (library API only) | `BLADE_RECORD_FORMAT`, `BLADE_RECORD_FPS`, `BLADE_RECORD_SIZE` | `record.format`, `record.fps`, `record.size` | `record.json` | `webm`, 10, 1280x720 |
| quality | none | none | `record.quality` | `record.json` `quality` | 70 |
| tab mode | none via the `state` tool (library API only) | `BLADE_RECORD_TABS` | `record.tabs` | `record.json` `tabs` | `follow` |

Through the `state` tool the only per-call recording input is `name`. Headless has no per-call layer: it is a property of the whole browser launch, not of a call.

## Config keys

Every key each config file accepts. A test compares this table with the code's own key lists in both directions, so a key cannot be added to or removed from either side without the other.

| File | Key | Meaning |
|---|---|---|
| `record.json` | `enabled` | Recording on/off (bool) |
| `record.json` | `dir` | Recording directory (absolute path) |
| `record.json` | `format` | `webm` or `mp4` |
| `record.json` | `fps` | Frames per second, 5-15 |
| `record.json` | `size` | `WIDTHxHEIGHT` |
| `record.json` | `quality` | JPEG quality 1-100 |
| `record.json` | `tabs` | `follow` or `per-tab` |
| `.bladebro.json` | `headless` | Hidden browser (bool) |
| `.bladebro.json` | `record` | Recording section; its keys follow |
| `.bladebro.json record` | `enabled` | As `record.json` |
| `.bladebro.json record` | `dir` | Relative to the project, vetted (see below) |
| `.bladebro.json record` | `format` | As `record.json` |
| `.bladebro.json record` | `fps` | As `record.json` |
| `.bladebro.json record` | `size` | As `record.json` |
| `.bladebro.json record` | `quality` | As `record.json` |
| `.bladebro.json record` | `tabs` | As `record.json` |
| `launch.json` | `headless` | Written by `bladebro headless on` / `off` (bool) |

## Project file (.bladebro.json)

A JSON file that travels with a project (commit it, or keep it local):

```json
{
  "headless": true,
  "record": { "enabled": true, "dir": "recordings", "fps": 8 }
}
```

Keys are listed in [Config keys](#config-keys). Both levels are strict: an unknown key, a bad value, invalid JSON or a file over 64 KiB is an error that names the file.

Discovery (from the current directory upward):
- The first `.bladebro.json` found wins.
- The walk stops after a directory that contains `.git`: a file in a parent folder cannot steer a checkout.
- The home directory is never read as a project (user settings belong in `launch.json`), and the walk stops at the filesystem root or after 64 levels.

Trust rules (Unix): the file must be a regular file (not a symlink), owned by the current user and not group/other-writable. Otherwise it is ignored with a stderr warning naming the path and the reason. Windows has no owner or mode check.

`record.dir`: a relative path is joined to the project directory. The resolved path (symlinks followed) must be inside the project directory or `<data root>/recordings`. Anything else (`..`, an absolute path elsewhere) is dropped with a warning and the next layer or default applies. Recordings may contain secrets: add a project-local recordings dir to `.gitignore`. Any symlink component below the project directory is refused too (including a dangling one, and one that points back inside the project): use a real directory. The artifacts-folder guard still applies afterwards.

## User config files

- `<data root>/launch.json`: `{ "headless": true|false }`, written by `bladebro headless on|off`. Atomic write, mode `0600` on Unix. Unknown keys are an error.
- `<data root>/record.json`: recording settings, see Recording settings.

## bladebro headless on|off|status

- `bladebro headless status` prints `browser mode: <headless|headed|platform default> (source: <where>)`. When `launch.json` is overridden by the environment or a project file, a second line says `user config <path> says on|off but <source> takes precedence`.
- `bladebro headless on|off` writes `launch.json`, prints `headless: on (saved to <path>)`, the fresh status line, and `applies at the next browser launch; run bladebro stop first if a daemon is running (MCP servers: restart the client's server)`. If the current setting is invalid (bad `BLADE_HEADLESS`, broken `.bladebro.json`), the toggle is rejected without writing: `launch.json was not changed: ...`.
- `--json` prints one object `{"ok":true,"is_error":false,"text":"..."}`. An unknown subcommand is a usage error (exit 2).
- `bladebro doctor` has one `Browser launch mode` line with the mode and its source; an invalid setting fails the check with the error.

## Daemon and MCP behavior

The browser mode is pinned per process. A CLI command whose mode (headless or not) differs from the running daemon's is refused with exit 1:

```
browser mode mismatch: the running daemon's browser is headless (source: ...) but this command resolves to headed (source: ...). Run `bladebro stop`, then retry; the next command starts the browser in the requested mode.
```

Run `bladebro stop` and retry. There is no automatic restart: it would kill another session's browser. `bladebro stop` always works. Different sources with the same resulting behavior are not a mismatch. The real-browser lane ignores headless, so it is never refused. The MCP server reads its mode from its own environment and directory at first launch; restart it to change the mode. Windows has no daemon (commands run one-shot), so no mismatch arises there. A daemon started by a previous build ignores the new field: run `bladebro stop` once after upgrading.

## Threat model: project files

A cloned repository controls its `.bladebro.json`, so the file is treated as untrusted input.
- It can set `headless` and recording `enabled`, `format`, `fps`, `size`, `quality`, `tabs` (per-tab writes more files, at most 8 open at once, into the same vetted dir).
- It cannot redirect recordings outside the project directory or `<data root>/recordings`.
- It cannot run commands: no key in the schema executes anything, and no external program is ever executed from project-controlled input (not the working directory, not a PATH entry like `.`): `platformVersion` is read from the OS API and the ffmpeg lookup skips relative PATH entries.
- A symlinked, foreign-owned or group/other-writable file is ignored with a warning (Unix).
- A file in a parent folder cannot cross a `.git` boundary, and a file in your home directory is never read.
- Size is capped at 64 KiB and unknown keys are rejected.

## Examples

### 1. QA server, hidden browser

Claude Code `.mcp.json`, a second server entry next to the normal one:

```json
{
  "mcpServers": {
    "bladebro-qa": {
      "command": "bladebro",
      "args": ["mcp"],
      "env": { "BLADE_HEADLESS": "1" }
    }
  }
}
```

DSH profile `cordis.patch.yml` (the same `env` block the README shows for `CHROME_PATH`):

```yaml
- id: bladebro
  config:
    env:
      BLADE_HEADLESS: "1"
```

DSH accepts one `bladebro` namespace, so a DSH profile is either QA (hidden) or interactive, not both.

### 2. Normal interactive entry

The README `mcpServers` entry, unchanged and without `env`:

```json
{
  "mcpServers": {
    "bladebro": {
      "command": "bladebro",
      "args": ["mcp"]
    }
  }
}
```

Unset means a visible browser on macOS and Windows.

### 3. Suite repo with a committed project file

Commit `.bladebro.json` at the repo root:

```json
{ "headless": true, "record": { "format": "webm", "fps": 10 } }
```

Everyone who runs bladebro from inside that repo gets a hidden browser and these recording settings. `record.dir` may only point inside the repo or `<data root>/recordings`. A repo-local recordings dir belongs in `.gitignore`, because recordings may contain secrets.

### 4. Toggle on your machine

```bash
bladebro headless status   # browser mode: platform default (source: default)
bladebro headless on       # saved to <data root>/launch.json
bladebro stop              # a running daemon keeps its old mode
bladebro nav http://127.0.0.1:8000/   # now hidden
bladebro headless off
```

One-off, without touching any file:

```bash
BLADE_HEADLESS=1 bladebro nav http://127.0.0.1:8000/
```

## Troubleshooting

- `browser mode mismatch`: a daemon from another mode is running. Run `bladebro stop`, then retry.
- `BLADE_HEADLESS: "<value>" is not valid`: use `1`, `on`, `true`, `yes`, `0`, `off`, `false` or `no`, or unset it. The same spellings apply to `BLADE_RECORD`.
- A window still appears: an old daemon from a previous build is still running (`bladebro stop`); you are on the real-browser lane (`bladebro rb visible off`); or a higher layer says headed. `bladebro doctor` shows the mode and its source.
- No window with `BLADE_HEADLESS=0` on Linux: by design, the browser runs on bladebro's private Xvfb display.
- `.bladebro.json` ignored with a warning: it is a symlink (replace it with a regular file), owned by another user, or group/other-writable (`chmod 644 .bladebro.json`).
- `record.dir ... ignored`: the path resolves outside the project and `<data root>/recordings`. Use a path inside the project.
- Old `BLADE_CHROME_FLAGS=--headless=new` workaround: remove it and use `BLADE_HEADLESS=1`.
