# Recording and launch configuration

Single reference for every bladebro-fork option: session recording, the ffmpeg override and the managed browser's launch mode. The README only points here. Every command and config block on this page was run against local fixtures in Phase 11 (headless, macOS); see [Not verified](#not-verified) for what was not.

## Quick answers

- Separate file per tab? `BLADE_RECORD_TABS=per-tab` (see [Per-tab files](#per-tab-files)).
- Hide the browser for tests: `BLADE_HEADLESS=1` (one run), `bladebro headless on` (your machine) or `"headless": true` in a committed `.bladebro.json` (a suite).
- Keep it visible: leave everything unset (macOS and Windows show a window; Linux uses its private virtual display), or set `BLADE_HEADLESS=0` to override a lower layer.
- Where settings live: environment, then `.bladebro.json` found from the current directory upward, then `<data root>/launch.json` and `<data root>/record.json`. See [Configuration layers and precedence](#configuration-layers-and-precedence).
- Record each QA test case (Claude Code GSD agents, DSH): see [QA flows: recording per test case](#qa-flows-recording-per-test-case).

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

The video clock starts at the first frame, not at the `record-start` call, and ends when the stop request reaches the capture loop. If `record-stop` comes before the first frame, it first waits up to 2 s for it (see [How frames are captured](#how-frames-are-captured)). Video duration is frames divided by fps. `record-stop` then takes about 0.5 s (a fixed 500 ms stop grace) before it returns, so the wall-clock time from the `record-start` call to the `record-stop` return is about 0.6 s longer than the video duration. A late first frame (for example a hidden tab that starts painting later) makes the video shorter by that delay. Segment offsets are accurate to about one tab-list check (about 0.25 s).

### How recording is encoded

- ffmpeg is run directly with an argument list (no shell); JPEG frames go to its stdin, its stdout is discarded and the last 4 KiB of its error output is kept for the result.
- WebM uses VP9 (`libvpx-vp9`, realtime, crf 40); MP4 uses H.264 (`libx264`, veryfast, crf 28, faststart). Output is always the configured size (default 1280x720), letterboxed, with even dimensions. Frames are converted from Chrome's full-range JPEGs to limited (tv) range: full-range VP9 plays only its first frame in Chrome (fixed 2026-10-30).
- Timing: frames are written at a constant rate equal to the configured fps; a frame is repeated for as long as it stayed on screen (from the capture timestamps), and the last frame is held until the stop, so video length matches the recording time.
- Stop: bladebro closes ffmpeg's input and waits for it to finish: normally well under 10 s, at most about 18 s in the worst case (10 s drain, 2 s minimum wait for ffmpeg, 5 s to reap after a kill, 1 s for its error output). After a finished video, the poster frame adds at most 3 s (see [Poster, marker and timeline](#poster-marker-and-timeline)). Success is reported only when ffmpeg exits 0, the file is non-empty and every accepted frame was written. If ffmpeg does not finish in time it is killed, the result says "unfinalized", and the partial file is kept. If frames were still unwritten at the 10 s deadline, the result says "truncated" with the number of frames not written.
- Errors name the cause and the fix: ffmpeg missing, `BLADE_FFMPEG` unusable, encoder missing for the format (switch with `BLADE_RECORD_FORMAT`), or the ffmpeg exit code with its last error lines. Browsing continues either way.
- Requires ffmpeg 4.3 or newer (`bladebro doctor` shows the version and encoders); this is not checked in code, so an older ffmpeg fails when encoding starts and the result carries its exit error. These limits are fixed; there is no setting for them.

### How frames are captured

- The recorder opens its own connection to the recorded tab and sends only `Page.enable`, `Page.startScreencast` (JPEG, quality 70, at most the configured size), `Page.screencastFrameAck`, `Page.stopScreencast`, and `Target.getTargets` (only when a new tab appears, to read which tab opened it). It injects no script, changes no viewport or emulation, draws no overlay and adds no launch flag.
- Every frame is acknowledged as soon as it arrives. Headless Chrome sent about 60 frames/s on an animated page in testing (1200 frames in 20 s) and about 100 headed; the recorder keeps at most the configured fps (newest frame wins, the last one is never lost; 200 frames reached the encoder in those 20 s at 10 fps) and hands frames to the encoder through a queue of 16. When the encoder falls behind, the newest frame is dropped and counted. The report gives frames received, acknowledged, sent to the encoder, skipped by the fps cap, dropped and lost.
- First frame at stop: if no frame has reached the encoder yet, `record-stop` waits up to 2 s for the first frame (`FIRST_FRAME_WAIT`, fixed), so a case stopped right after `record-start` on a static page still gets a video of at least one frame. Measured in 11.1 on the static fixture, headless: 18 of 18 stops 0.1 s and 0.5 s after `record-start` saved, over MCP, the CLI daemon and per-tab. The wait ends as soon as the frame arrives; a recording that already has a frame does not wait. It never runs past the 5 s no-frames threshold, and only `record-stop` waits: the exit paths in [When a session ends](#when-a-session-ends) do not.
- No frames: if no frame arrives even within that wait (a background, minimized or covered tab), there is no video, however short the recording, and the stop is `recording not clean`. Follow mode prints `recording not clean: recording failed: no frames reached the encoder, so there is no video; <path> left in place` at stop. Per-tab mode prints `recording not clean (per-tab, N files): tab <id>: recording failed: no frames reached the encoder, so there is no video; <path> left in place` (measured with one file). In both the file is 0 bytes. Harnesses should match the substring `no frames reached the encoder`, which both forms contain. The capture-side wording, `no frames for N s since recording start`, is the source's own verdict in the code and is not the one a stop prints in these modes. After 5 s without a first frame a warning is logged and recording continues. A background tab behaved this way in testing: 0 frames in 8 s, and Chrome reported the tab hidden.
- Limits: a background tab, a minimized window (about 0.3 frames/s) or a covered window delivers no new frames. A static page sends about one frame and then nothing (a 20 s recording of a still page received 1 frame), which looks the same as a stalled capture, so after the first frame silence is reported (time since the last frame, longest gap), not treated as failure, and the video holds the last frame. A one-off 37 s silence was once seen in testing (cause unknown). The video used to come out about 0.4 to 0.5 s longer than the recording time (the last frame was held through the 0.5 s stop grace); it now ends at the stop request. Measured on 8 s recordings (8 runs, including stops off a whole second) the video length differed from the recording time by -0.74 % to +0.16 % (about 60 ms at most on that slow-ticking page, since the video is built from whole frame slots at the chosen fps). On animated pages the video ran about 100 ms long (+0.6 % to +1.2 % on 8 to 16 s recordings): frames that arrive during the stop grace are still added, and the last frame is always written at least once. The theoretical worst case is about 0.5 s plus one frame slot. Recording needs the default WebSocket transport: with `BLADE_TRANSPORT=pipe` it reports that it cannot record (unset it and run `bladebro stop`).
- These limits are fixed; there is no setting for them.

## Recording with the state tool

Two ops on the existing `state` tool (there is no sixth tool):

- `record-start` with `name` (required): starts a video of the tab the agent is on; the video follows the agent's tab (see [Tab following](#tab-following)). The file is `<sanitized name>-<UTC>-<id>.<ext>` in the recordings dir.
- `record-stop`: finalizes the file and reports it. An optional `value` is a marker note (what went wrong or right, at the moment of the stop); it is placed on the timeline (see [Poster, marker and timeline](#poster-marker-and-timeline)).

```json
{"op": "record-start", "name": "login-flow"}
{"op": "record-stop"}
{"op": "record-stop", "value": "FAIL: login button did nothing"}
```

CLI: `bladebro state record-start login-flow`, `bladebro state record-stop` and `bladebro state record-stop --value "FAIL: login button did nothing"`. In a `batch` or `run` step: `{"action": "state", "op": "record-start", "name": "x"}` and `{"action": "state", "op": "record-stop", "value": "FAIL: ..."}`. Recording is off unless enabled (for example `BLADE_RECORD=1`), and it needs ffmpeg (`bladebro doctor` shows what was found).

Start result (the first frame is not awaited):

```
recording started: /abs/path/login-flow-20261026T145206Z-6c82d8.webm (tab F376...); first frame not awaited. record-stop saves it.
```

Stop result, from the Phase 11 MCP run (11-01 evidence; paths shortened):

```
recording saved: /abs/path/login-flow-20261026T145206Z-6c82d8.webm (bytes 12952, duration 1.6 s, frames 16, dropped 0, segments [0.0 s tab F376...]). Poster: /abs/path/login-flow-20261026T145206Z-6c82d8.poster.png. Timeline: /abs/path/login-flow-20261026T145206Z-6c82d8.timeline.json (0 actions)
```

Stop result with a poster, a timeline and a marker (real MCP run, 11-01 evidence; paths shortened like the examples above):

```
recording saved: /abs/path/ev-20261026T145211Z-af4af0.webm (bytes 36204, duration 3.8 s, frames 38, dropped 0, segments [0.0 s tab A61A...]). Poster: /abs/path/ev-20261026T145211Z-af4af0.poster.png. Timeline: /abs/path/ev-20261026T145211Z-af4af0.timeline.json (1 action). Marker at 3.7 s
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
| poster | `. Poster: <abs path>.poster.png`: the last frame of the finished video as a PNG, next to it (see [Poster, marker and timeline](#poster-marker-and-timeline)). `. Poster: not made (<reason>)` when it could not be made; the recording is still saved |
| timeline | `. Timeline: <abs path>.timeline.json (N actions; 1 action for one)`, plus `, K not kept` when entries were dropped at the 1000-entry cap. `. Timeline: not written (<reason>)` when it could not be written. Only with `record-stop`, not the exit paths |
| marker | `. Marker at T s` when `value` was placed (T on the video clock, clamped to the length); `. Marker not placed: <reason>` when it was not (empty note, not a string, no active recording). The note itself is never echoed |
| notes | "no new frames for N s" (static page or stalled tab; the video holds the last frame); `tab <A> gone at <T> s; following tab <B>` when the recorded tab was closed or replaced; `popup <P> closed at <T> s; back on opener <O>` when a followed popup closed; `N popup(s) not recorded (never in front)` for popups that never reached the front; `agent was on tab <A> at stop; video followed tab <B>` when they differ at stop (see [Tab following](#tab-following)) |

The stop is `ok` only when capture and encoder both finished cleanly. Otherwise it is an error (MCP `isError`, CLI exit 1; in `batch`/`run` an error step stops the batch) whose text starts with `recording not clean:` and still carries the path and numbers.

A stop can take up to about 23 s in the worst case: up to 2 s waiting for a first frame when none has arrived yet, about 18 s for the video (500 ms capture grace, up to 10 s encoder drain, then the trailer) plus the 3 s poster budget. Set your client's tool timeout above that.

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

One recording is one file and one ffmpeg process for the whole session; the video follows the tab the agent is on and each tab it shows is a segment in the stop result. Real results from the Phase 11 run (headless Chrome, local fixtures; ids shortened):

```
recording saved: /abs/path/f-20261026T145300Z-b64fd0.webm (bytes 41830, duration 4.9 s, frames 49, dropped 0, segments [0.0 s tab EE14..., 1.7 s tab 0848..., 3.6 s tab EE14...]). Poster: /abs/path/f-20261026T145300Z-b64fd0.poster.png. Timeline: /abs/path/f-20261026T145300Z-b64fd0.timeline.json (2 actions)
recording saved: /abs/path/c-20261026T145305Z-a03e19.webm (bytes 37934, duration 4.8 s, frames 48, dropped 0, segments [0.0 s tab EE14..., 1.7 s tab 0AEC..., 3.3 s tab 0848...], captured 47). Note: tab 0AEC... gone at 3.3 s; following tab 0848.... Poster: /abs/path/c-20261026T145305Z-a03e19.poster.png. Timeline: /abs/path/c-20261026T145305Z-a03e19.timeline.json (2 actions)
```

How it works:

- The recorder reads Chrome's tab list over HTTP (`/json` on the debugging port) every 200 ms. It sends no CDP command for this and nothing is visible to the page. The agent is on the first page of that list (Chrome orders it most recently activated first; Chrome did not report an `attached` flag, so that flag is not used). A switch is made when the same tab is first on two samples about 50 ms apart.
- `switch-tab` to an open tab is followed within about 0.25 s (allow up to about 0.7 s on a busy machine or when the agent goes back and forth right away): the recorder connects to the new tab first, then releases the old one. `open-tab` (a tab nobody opened) takes the same two-sample check, so it is followed within about 0.25 s as well (by code reading; the Phase 8.1 live run did not time it separately).
- Popups: a tab opened by a tab of the recording is followed by itself, without `switch-tab`, once it has stayed in front for 300 ms. This covers a `target=_blank` link, `window.open` with or without window features, and popups of popups. The recorder reads which tab opened each new tab with one `Target.getTargets` call when a new tab appears. While recording, bladebro skips the opener re-front after a popup click (one line in `src/page/mod.rs`, active only while a recording runs). That skip is what keeps a tab-style popup in front; without it the popup is hidden at once and never becomes a segment.
  - Follow delay, measured live (headless, macOS arm64, local fixtures, 3 runs per kind, no `switch-tab`; delay = seconds from the click returning until `state tabs` marks the popup `R`): blank link 0.05-0.06 s, `window.open` without features 0.06 s, cross-origin link 0.05-0.21 s, window-feature popup 0.20-0.21 s. The MCP lane measured 0.00-0.17 s on the same kinds. In the Phase 8.1 spike (click at about 1 s) the popup segment started at 1.8 s (blank link) and 2.0 s (`window.open`) on the video clock.
  - Return: the agent's next `act`, `vision` or `nav` re-fronts the opener, so for tab-style popups the video returns to the opener at that input. Measured with `act press Tab` on the opener, in every tab-style kind.
  - Window-feature popup (`window.open(url, '', 'width=500,height=400')`): it is a separate window. Input on the opener does not re-front it (measured: `press Tab` did not return the video), so the video stays on the popup. The click leaves the agent on the opener, and `switch-tab` to the opener while the agent is already there does not bring the video back (measured in 11-01: the popup row kept ` R`). It returns when the popup is closed (`state close-tab <popup id>`; measured: the opener row became `*R`) or after `switch-tab` to another tab and then back to the opener (measured). It does not return by itself.
  - Closing a followed popup returns the video to its opener with the note `popup <P> closed at <T> s; back on opener <O>` (measured: segments opener, popup, opener). Closing the last tab leaves no tab: Chrome exits, and stop reports `no tab left at <T> s` as `recording not clean`. The file is kept and decodes in full (measured).
  - Bursts: a popup becomes a segment only if it stays in front for 300 ms. One that never does is not recorded and is named once in the stop note: `N popup(s) not recorded (never in front): <ids>; switch-tab to record one` (at most 3 ids named). Timing decides which popups of a burst are segments. A live burst of three clicks gave six segments and no not-recorded note; an earlier live run named one popup in that note.
  - Blocked popups: a popup that Chrome blocks is never a tab, so it produces no segment. bladebro's Chrome runs with `--disable-popup-blocking`, so a `window.open` without a user gesture is a real tab and is followed (measured). A `rel=noopener` popup has no known opener: it is followed after the same 300 ms, but it is never counted into the popup family or the stop note.
  - A popup that was already first when `record-start` ran is ignored until the front tab changes once.
- Several switches inside one check are not all recorded: only the last tab is followed and the tabs in between get no segment. A switch made less than one check before `record-stop` is caught up at the stop, so the last tab is in the video.
- The recorder opens its own connection for every tab it records, with the same five methods as above (`Target.getTargets` only when a new tab appears).

#### state tabs while recording

While a recording runs, each `state tabs` row has two marker columns: `*` for the agent's tab (the one `act` and `see` act on) and `R` for the tab the video shows.

| Row starts with | Meaning |
|---|---|
| `*R` | agent and video are on the same tab |
| `* ` | agent only |
| ` R` | video only (the agent is on another tab) |
| `  ` | neither |

The footer line `(* agent, R video: differ; switch-tab to move the agent)` appears only when the agent and the video are on different tabs. Without a recording the output is unchanged: `*` marks the attached tab, a space otherwise, and there is no differ footer. Real output from the Phase 11 window-popup run (11-01 evidence, the `state tabs` after `act click "open window"`; ids shortened):

```
   EE14...  A — http://127.0.0.1:8765/a.html
*  0848...  Popper — http://127.0.0.1:8765/popper.html
 R B425...  B — http://127.0.0.1:8765/b.html
(* agent, R video: differ; switch-tab to move the agent)
(* = current tab; switch-tab <target_id> to change)
```

When the agent and the video differ at stop, the stop result adds the note `agent was on tab <A> at stop; video followed tab <B>`. For example, a window-feature popup still in front at stop gives this note with the popup as `<B>`.
- Tabs of a different size are letterboxed into the configured size (the file keeps one width and height; tested with 1280x720 and 500x400 frames into one WebM and one MP4).
- Closing the recorded tab, or a tab replaced by a new target id, continues on the agent's next tab with the note `tab <A> gone at <T> s; following tab <B>` and the result stays "recording saved". Cross-site navigation of the recorded tab kept the same tab id and frames in testing (127.0.0.1 to localhost).
- Not clean (`recording not clean: ...`, the file is kept): no tab left (`no tab left at <T> s`), the browser stopped answering the tab list three times in a row (`browser connection lost at <T> s`), or a tab that could not be recorded (`tab <B> not recorded from <T> s: ...`). Never `recording saved` when the agent's tab went unrecorded.
- A segment with no frames for at least 5 s is not clean (`tab <B>: no frames for N s (hidden or background tab)`); a shorter one is shown as `<B> (no frames)` and stays clean. A video with no frame at all keeps the "no frames" failure. At `record-stop` the tab the video is on gets up to 2 s for its first frame (never past the 5 s threshold).
- Only one recording runs at a time; a second `record-start` is refused while the first is following.

The environment is read by the long-lived process. A daemon or MCP server started without `BLADE_RECORD` keeps answering "recording is off" until `bladebro stop` (or an MCP restart). The project file `.bladebro.json` is found from that process's working directory.

### When a session ends

Every way a session can end finalizes an active recording the same way: stop feeding frames, close ffmpeg's input, wait a bounded time (worst case about 16 s: 10 s for ffmpeg, 5 s for a last-resort kill, 1 s for its log), and only if ffmpeg does not exit is it killed and the result says so (`unfinalized`). The result is kept for the next record op and returned prefixed `previous recording ended with the browser: `. In the MCP lane the first call after Chrome died may start with a one-line `note: Chrome was restarted ...` before the recording text. The partial file stays at the path `record-start` reported. After any of these exits a new `record-start` works (the recorder is never "active forever"). These exits do not wait for a first frame (only `record-stop` does), so the bound is unchanged.

| Exit path | MCP | Daemon | One-shot CLI |
|---|---|---|---|
| `record-stop` | finalized | finalized | n/a (record-start is refused) |
| recorded tab closed, follow mode | continues on the next tab, finalized at stop | same | n/a |
| recorded tab closed, per-tab mode | that tab's file finalized, the others keep recording | same | n/a |
| stdin EOF | finalized, file plays in full (live: process exited in about 0.8 s); the session is gone, so nothing reports it | n/a | n/a |
| SIGTERM / SIGINT | finalized, file plays in full (live: exited in about 0.8 s); nothing reports it | same (live: about 0.8 s) | n/a (record-start is refused) |
| `bladebro stop` | n/a | finalized, file plays in full (live: returned in about 0.85 s, `daemon stopped`) | n/a |
| idle timeout (`BLADE_IDLE_TIMEOUT`, default 600 s, 0 disables; checked every 15 s) | finalized, reported at next record op (live: timeout 20 s, Chrome and ffmpeg gone after about 26 s) | same (live: about 26 s) | n/a |
| Chrome relaunch (lane or settings change, lost connection, `Closed`) | finalized, reported at next record op | finalized, reported at next record op | n/a |
| Chrome relaunch after the recorder's own connection was already lost | interrupted, partial kept (`recording not clean`) | interrupted, partial kept | n/a |
| one-shot CLI exit | n/a | n/a | refused (no recording possible): `record-start` needs a long-lived session |
| kill -9 of bladebro | self-finalized by ffmpeg on stdin EOF (live: ffmpeg gone 62 ms after the kill, file decodes in full; nothing reports it, the new session has no record of it) | same (live: 58 ms) | n/a |
| ffmpeg killed mid-recording | interrupted within 2 s, reported at next record op (`encoder exited`); a file that has no data yet is reported as empty (live: 0 bytes, VP9 writes at finalize); a new record-start works | same | n/a |
| Chrome killed mid-recording | interrupted, file finalized and playable up to the cut, reported at next record op (`browser connection lost`); live: ffmpeg gone about 2.1 s after the kill (the stop may instead say `ended with the browser` when the relaunch finds the dead Chrome first); browsing and a new record-start work afterwards | same | n/a |
| recorder connection lost, Chrome still alive | recording continues when the tab is still listed (the follower reconnects); otherwise stop reports it as not clean; a new start works | same | n/a |
| disk low (under 500 MiB free), 2 h maximum duration, 1 GiB per file | stopped and finalized, reported at next record op (see [Disk space and limits](#disk-space-and-limits)) | same | n/a |

What the next record op says after an interruption (exact text; `<T>` is seconds into the recording, `<path>` the file `record-start` reported). It is an error result (the recording did not end by your stop), and it is never `recording saved`:

- `previous recording interrupted: browser connection lost at <T> s; partial file kept: <path>. recording not clean: ...` (Chrome was killed or its debugging port stopped answering)
- `previous recording interrupted: encoder exited at <T> s; partial file kept: <path>. recording not clean: recording failed: ffmpeg exited with signal: 9 ...` (ffmpeg died)
- `previous recording interrupted: ...; file is empty, nothing playable: <path>. ...` when the file has no bytes at all (ffmpeg was killed before it wrote anything)
- `previous recording stopped: disk low (<N> MiB free); saved: <path> (bytes ..., duration ...)`, `previous recording stopped: maximum duration (2 h) reached; saved: ...`, `previous recording stopped: size limit (1 GiB) reached; saved: ...` (a limit stop; see [Disk space and limits](#disk-space-and-limits))
- `previous recording ended with the browser: ...` when a Chrome relaunch, idle timeout or shutdown finalized it first (the usual case when the next call is what finds the dead Chrome)

How it is noticed: while a recording runs, a small guard checks it every 2 s. It ends the recording early (closes ffmpeg's input so the file plays up to where it was cut) when the tab list stops answering for three reads in a row (about 0.6 s) or when ffmpeg is dead. The guard never talks to a page, so browsing is unaffected, and a guard from an earlier recording never stops a newer one. In per-tab mode only the browser check applies (every file is finalized); a killed ffmpeg of one tab file shows up in that file's line at stop.

Details:

- The idle timeout is not paused by a recording: a recording that sees no tool call for `BLADE_IDLE_TIMEOUT` seconds is finalized with Chrome and reported at the next record op.
- `bladebro stop` waits for the finalize (the client waits up to 30 s, the finalize is bounded by about 16 s for one file).
- On macOS and Linux ffmpeg runs in its own process group. A terminal Ctrl-C reaches bladebro only, which then closes ffmpeg's input and finalizes; ffmpeg is not interrupted halfway through writing its trailer. A second signal while finalizing is ignored until the bounded finalize ends. If bladebro itself is killed (SIGKILL), ffmpeg sees its input close and finalizes the file on its own.
- Only `record-stop` writes the poster and the timeline. Every exit, interrupt, limit and disk-low finalize above writes the video only: no ffmpeg poster run, no timeline file, no `Poster:` or `Timeline:` text, and no new file in the recordings folder. Their results are the same as before the poster and timeline existed.
- Live proof (headless Chrome, local fixtures, both lanes, every path above that can be driven): `tools/record_sidecar/scenarios/exit_live.sh`, output kept in `.planning/phases/09-finalization-on-every-exit-path/evidence/09-03-live.txt`. After each exit the file was decoded in full and no ffmpeg, bladebro or scratch Chrome was left within 10 s (they were gone within 0.1 s). One exception, not caused by recording: after `kill -9` of an MCP server its Chrome is not reaped by the next MCP launch (the same happens with no recording; the daemon lane does reap it).
- SIGHUP: the MCP server treats it like SIGTERM; the daemon ignores it on purpose.
- The one-shot CLI cannot hold a recording because it exits after one command, so `record-start` is refused there with a message to use MCP or the daemon.
- Windows: Ctrl-C is handled. Closing the console, logoff and shutdown are not handled (that needs a console control handler). If bladebro dies, ffmpeg sees its input close and the job object that contains it ends it; a hard kill can leave an unfinalized file. This was reasoned from the code and not tested on Windows.
- The matrix above is `MATRIX` in `src/record/exit.rs`; every row is backed by a test in `src/record/exit_tests.rs` or by a live scenario.

### Disk space and limits

These are fixed values, not settings (no environment variable, no config key). To record longer, stop and start again.

- **Before recording:** `record-start` is refused when the recordings folder's disk has less than 500 MiB free: `recording not started: disk low (<N> MiB free in <dir>, need 500 MiB)`. Nothing is created and no ffmpeg is started.
- **While recording:** the guard (every 2 s) checks free space. Under 500 MiB it stops the recording and finalizes the file; the next record op says `previous recording stopped: disk low (<N> MiB free); saved: <path> (...)`. The file plays in full up to that point. A free-space reading that fails is never treated as low.
- **Maximum duration:** 2 h per recording: `previous recording stopped: maximum duration (2 h) reached; saved: ...`.
- **Maximum file size:** 1 GiB per file (each file in per-tab mode): `previous recording stopped: size limit (1 GiB) reached; saved: ...` (per-tab: `... reached by <file name>; saved (per-tab, N files): ...`).
- The recording is finalized in every case, so the result is `saved`, not an error. A new `record-start` works afterwards (it is refused only while the disk is still low).
- Windows: the free-space check is not available (the disk guard is off); the duration and size limits still apply.
- Why 500 MiB: two minutes at the largest setting (3840x2160, 15 fps) is about 5 MB in the measurements below, so 500 MiB leaves a very large margin, including for busier pages. 2 h costs about 80 MB at the defaults and about 320 MB at the largest setting, so the 2 h limit bites before 1 GiB in practice.
- Verified with injected free-space values and short limits in unit tests (`guard_tests`, `disk_tests`). A real disk was never filled and a real 2 h recording was never made.

### File size per minute

| Setting | Phase 2 measurement | Phase 9 measurement |
|---|---|---|
| default: webm (VP9), 10 fps, 1280x720 | about 0.87 MB/min | 0.67 MB/min |
| mp4 (H.264), 10 fps, 1280x720 | about 0.54 MB/min | 0.42 MB/min |
| webm, 15 fps, 1280x720 | about 1.15 MB/min | 1.05 MB/min |
| webm, 10 fps, 1920x1080 | about 1.12 MB/min | 0.84 MB/min |
| largest: webm, 15 fps, 3840x2160 | not measured | 2.67 MB/min |
| largest: mp4, 15 fps, 3840x2160 | not measured | 1.78 MB/min |

Phase 9 numbers: one 60 s recording per setting, headless Chrome on macOS arm64, a local moving-box fixture (`tools/record_sidecar/fixtures/anim.html`), 1 MB = 10^6 bytes, duration from ffprobe; raw lines in `.planning/phases/09-finalization-on-every-exit-path/evidence/09-03-sizes.txt`. It is one fixture and one run each: real pages move more or less, so treat the figures as an order of magnitude (a mostly static page is smaller, a video or busy page larger). Two hours is about 80 MB at the defaults and about 320 MB at the largest setting (webm).

### Leftover encoders

ffmpeg exits by itself when bladebro dies (its input closes), so leftovers are rare. As a backstop bladebro removes an ffmpeg only when all four hold: the program is named `ffmpeg`, its command line has bladebro's own input flags (`-f image2pipe ... -i pipe:0`), its output is a file in the recordings folder with a bladebro-style name (`<name>-<UTC stamp>-<6 hex>.webm` or `.mp4`), and its parent process is gone (or is pid 1 and that pid 1 is not bladebro itself: a bladebro running as pid 1 in a container keeps its own encoders). Your own ffmpeg, an ffmpeg in another folder, and an ffmpeg whose bladebro is still running are never touched. It sends SIGTERM, waits up to 2 s, and uses SIGKILL only if the same process (same command line) is still there.

When: at every Chrome launch (the same place stale browsers are cleaned up; the scan runs in the background so launch time is unchanged) and at every `record-start`, for the recordings folder in use. macOS and Linux only: on Windows nothing is swept, because the job object that holds ffmpeg ends it together with bladebro (reasoned, not tested on Windows). The scan reads the process list with `ps`, so a very unusual install path of ffmpeg that contains ` -` is not recognized (it is left alone).

### Per-tab files

Set `BLADE_RECORD_TABS=per-tab` (or `record.tabs` in `record.json` / `.bladebro.json`) to get one file per tab instead of one file that follows the agent. There is no `state` parameter for it. Real result from the Phase 11 run (headless Chrome, local fixtures; directories and ids shortened):

```
recording saved (per-tab, 2 files): /abs/path/pt-tab1-20261026T145327Z-0901e8.webm (bytes 8976, duration 3.2 s, frames 32, dropped 0, segments [0.0 s tab BF81...], captured 18); /abs/path/pt-tab2-20261026T145328Z-2389e9.webm (bytes 10793, duration 1.7 s, frames 17, dropped 0, segments [0.0 s tab 176C...]). Poster: /abs/path/pt-tab2-20261026T145328Z-2389e9.poster.png. Timeline: /abs/path/pt-tab1-20261026T145327Z-0901e8.timeline.json (1 action)
```

How it works:

- Files are named `<name>-tab<N>-<UTC>-<id>.<ext>`; `N` is the order of first visit (the tab you started on is `tab1`). Every file goes through the same folder, permission and no-overwrite rules as a single recording.
- Returning to a tab continues its file (a new segment in that file; offsets are from that file's first frame). A file lasts from the tab's first visit until the tab closes or `record-stop`; while the agent is on another tab it holds its last frame (a gap longer than 10 minutes is clamped).
- A closed tab's file is finished at once (its ffmpeg ends while recording continues) and its slot is free again. The result notes `tab <id> closed at <T> s`.
- At most 8 files are open at once (one ffmpeg each, so CPU and disk grow with open tabs). The next tab is not recorded and the result says so: `tab <id> not recorded from <T> s: per-tab cap of 8 open files reached (close a tab to free one)`. That note does not make the result not clean. Close a tab to free a slot; a tab that went unrecorded is tried again when the agent returns to it.
- Each file is finished on its own. One that fails (ffmpeg exits, the file cannot be created) makes the result `recording not clean (per-tab, N files): tab <id>: <reason>; <every file>`; the other files are finished and listed. A file that cannot be created is named `tab <id> not recorded from <T> s: <error>`.
- `record-stop` finishes all files at the same time, each bounded; a file that cannot be finished in time is listed as `not finalized`. A second `record-start` is refused and names the first file.
- A followed popup gets its own file, numbered like any tab; a popup that never reaches the front gets none. When the popup closes its file is finished and the opener's file continues, with the note `popup <P> closed at <T> s; back on opener <O>` (measured: a popup closed at 4.1 s got a 2.4 s file of its own). The cap of 8 open files applies to popups too.

### Poster, marker and timeline

Three extras that `record-stop` adds next to the video. They are made from what the tools already returned and from one ffmpeg run on the finished file; no page is queried for them and no CDP call is added.

**Poster frame.** One short ffmpeg run reads the finished video and writes its last frame as `<video stem>.poster.png` next to the video. It decodes at most the last second and succeeds only when ffmpeg exits 0 and the file starts with the PNG signature; otherwise the file is removed and the result says `. Poster: not made (<reason>)`. The poster never changes `ok` or `err`. The run has a 3 s budget (one retry without the seek when the container cannot seek from the end, inside the same budget). Measured in 10-01: the ffmpeg run takes about 0.07 s (15 runs, max 0.074 s), and the wall time of `record-stop` moved from a median of 0.632 s to 0.654 s over 3 runs each on the same scenario, inside run-to-run noise. The poster file is created new with mode 0600 (a planted symlink is never followed; on a name collision it is numbered), and it is removed again when the run fails. In per-tab mode it comes from the file of the tab the video followed at stop, named after that file (see below).

**Marker.** `record-stop` with `value` places a labelled point on the timeline at the moment the stop was received, on the video clock. The text is cleaned first: every control character (tab, newline included) becomes a space, zero-width and bidi-control characters are dropped (they can disguise text for a reviewer), whitespace runs collapse, and it is cut to 200 characters with `...` when longer. The result says `. Marker at T s` and does not echo the note (the agent wrote it). A marker never costs the recording:

| Case | Result text |
|---|---|
| placed | `. Marker at 3.7 s` (clamped to 0..duration) |
| empty after cleaning | `. Marker not placed: empty note` |
| not a string | `. Marker not placed: value must be a string` |
| no active recording | `. Marker not placed: no active recording` (full text: `no active recording; record-start <name> first. Marker not placed: no active recording`) |

**Timeline.** `<video stem>.timeline.json` next to the video, 0600, created once. It lists what the agent did, in order, with the same clock as the marker:

- Listed: every `act` call; every `run` call (one entry per call, with `steps` = the number of top-level steps, and the first line of the result as its verdict); `state` only for `open-tab`, `close-tab` and `switch-tab`.
- Not listed: `see`, `vision`, other `state` ops, `record-start`, `record-stop`. A navigation made by `see url=` is not an entry.
- Each entry: `at_s` (call start), `end_s` (result produced), `tool`, `action`, `ok` (false for an error result), and `verdict` (the first non-empty line of the result or error text, cleaned and cut to 200 characters). A failed action keeps its verdict.
- At most 1000 entries. Later ones are counted in `dropped` and reported as `K not kept`.
- Written only by `record-stop`. The exit, interrupt, limit and disk-low finalizes do not write it (see the note under [When a session ends](#when-a-session-ends)).
- The file holds verdict lines, which can contain text the agent typed or text from the page, and the marker note. Treat it with the same care as the video: a recording folder can hold sensitive content.

Real file from the MCP run above (pretty-printed here; basenames only, no directories):

```json
{
  "clock": "seconds from the first video frame",
  "dropped": 0,
  "duration_s": 3.8,
  "entries": [
    {"action": "click", "at_s": 1.49, "end_s": 2.19, "kind": "action", "ok": true, "tool": "act",
     "verdict": "outcome: clicked (no observable DOM change) via mouse on button [] (topmost=true,disabled=false) - dispatched once; verify state before re-clicking"},
    {"at_s": 3.69, "kind": "marker", "note": "x"}
  ],
  "files": [
    {"duration_s": 3.8, "file": "ev-20261026T145211Z-af4af0.webm", "start_s": 0.0}
  ],
  "version": 1,
  "video": "ev-20261026T145211Z-af4af0.webm"
}
```

The example has two entries because that run made one action and one marker. Keys are in the order the file stores them (alphabetical). A `run` with three steps is one entry (`"tool": "run", "action": "run", "steps": 3`), as the 10-02 timeline run showed.

**Clock and alignment.** `0 s` is the arrival of the first video frame, the same clock the existing notes (`tab X gone at T s`) use. An entry's `at_s`/`end_s` and the marker are on that clock, clamped to `[0, duration]`. If no frame arrived at all, the clock is the `record-start` instant and the JSON `clock` text says so. Measured in 10-02 (headless Chrome, local fixtures, five clicks over the webm, mp4 and per-tab runs, 0.1 s scan steps): the frame where a clicked button's colour changed appeared about 0.3 s before the entry's `end_s` (the same value for every click in the official run; exploratory runs gave -0.2 to -0.3 s on the same grid). So `end_s` + 0.4 s always showed the new colour, and `at_s` - 0.3 s showed the previous colour. Every first new-colour frame lay inside its own action's `[at_s, end_s]` window. The measured offset is the error bound to use: the scan step is 0.1 s, and a frame is 0.1 s at 10 fps, so read the offsets as good to about 0.1 s, not as exact frame times.

**Per-tab files.** With `BLADE_RECORD_TABS=per-tab` the result has one poster and one timeline. The poster comes from the file of the tab followed at stop (else the first finalized file) and is named after it. The timeline is named after the `-tab1-` file, and all times are on the recording clock, which is the earliest file's first frame. `files[]` gives every file's `start_s` on that clock (null when a file got no frame), so a file-local time is `at_s - start_s`. A marker in per-tab mode is on the recording clock too, so it can be later than the end of file 1. Measured in 10-02: file 2 started at 3.91 s on the recording clock; a click at local 0.67 s of file 2 showed its colour at local 1.03 s.

**Recording off.** With recording off the timeline hooks cost one atomic load and nothing else, and no poster, timeline or video file is created. Measured in 10-02 against the pre-phase binary (d18dae1, two runs each, the same fixed CLI session): the set of CDP methods is the same (11 distinct), every method except `Input.dispatchMouseEvent` has equal per-method counts, and the normalized stdout is byte-identical. `Input.dispatchMouseEvent` is not a stable count even for one binary (the base binary gave 23 and 30 in two runs, the new one 34 and 30), because the humanized mouse paths are random by design (`src/stealth/biometrics.rs`). Read that count as noise, not as a regression signal.

## QA flows: recording per test case

A QA agent records one video per test case with two extra `state` calls; nothing else changes in how it drives the browser. Recording is switched on once, for the long-lived process (the MCP server or the daemon), not per call.

Per test case:

1. `state` with `{"op": "record-start", "name": "<test id>"}`. The result's text starts `recording started: <path>`. Over MCP read every content item, not only the first: on a fresh profile the first item can be a `note: first run — the profile was warmed (...) before this call.` block (the upstream first-run note, from the server's content[0]), and `recording started:` is then content[1]. That note was not seen in the Phase 11 runs, which used scratch profiles.
2. The test steps, as their own `act` / `see` / `vision` / `run` calls. Do not put `record-start` or `record-stop` inside a `run`: the timeline would be empty (see [Known issues](#known-issues)).
3. `state` with `{"op": "record-stop", "value": "PASS: <test id>"}` or `{"op": "record-stop", "value": "FAIL: <what went wrong>"}`. The note becomes the marker on the timeline.
4. Copy the paths from the stop result into the test report.

Where the paths appear: the stop result is text (MCP: the tool result's text content, every item of it, since a first-run note can come before the recording text; CLI: stdout; CLI `--json`: the `text` field of `{"is_error":false,"ok":true,"text":"..."}`) of the form `recording saved: <video> (bytes ..., duration ... s, ...). Poster: <stem>.poster.png. Timeline: <stem>.timeline.json (N actions). Marker at T s`. The video path is the text between `recording saved: ` and ` (bytes`; the poster and timeline paths follow `Poster: ` and `Timeline: `. If a poster or timeline could not be made the text says `not made` or `not written` instead, and the video is still saved. A recording with a problem starts `recording not clean:` and still names the file; per-tab mode starts `recording saved (per-tab, N files):` and lists each file, separated by `; `.

Rules that keep one video per test case:

- One recording at a time: stop the previous test's recording before the next `record-start` (a second start is refused and names the active file).
- Close popups a test opened before the next test (`state close-tab <id>`); a window-feature popup otherwise keeps the video (see [Tab following](#tab-following)).
- A stop can take up to about 23 s; the client's tool timeout must be longer.
- Use the test id as `name`: it is sanitized (see [Per-call parameters](#per-call-parameters)) and gets a UTC stamp, so reruns never overwrite.
- Recordings are sensitive (see [Recording settings](#recording-settings)): use test accounts, and delete recordings in your harness.

### A test case that ends before its first frame

Since 11.1 `record-stop` waits up to 2 s for the first frame, so an assertion-only case (`record-start`, a `see`, `record-stop`, with no wait and no repaint) on a static page saves a short video. No harness wait or repaint is needed. Measured in 11.1 on the static fixture, headless: 6 of 6 saved over MCP (follow mode), 6 of 6 over the CLI daemon and 6 of 6 per-tab, each at a stop 0.1 s and 0.5 s after `record-start` (2 frames at 0.1 s, 6 at 0.5 s). The sample below is the assertion-only case from the same run (1 frame, 0.1 s).

```jsonl
{"op": "record-start", "name": "07-03-assert-only"}
{"tool": "see", "arguments": {"mode": "model"}}
{"op": "record-stop", "value": "PASS: assertion-only case"}
```

```output
recording saved: .../07-03-assert-only-20261026T165648Z-36a198.webm (bytes 9590, duration 0.1 s, frames 1, dropped 0, segments [0.0 s tab 6CF2...], captured 2). Poster: .../07-03-assert-only-20261026T165648Z-36a198.poster.png. Timeline: .../07-03-assert-only-20261026T165648Z-36a198.timeline.json (0 actions). Marker at 0.0 s
```

A tab that never paints within the 2 s (hidden or in the background) still fails with `no frames reached the encoder` and a 0-byte file; bring it to the front (`switch-tab`) before stopping.

### Claude Code GSD flow (gsd-charlotte-qa, gsd-test-writer)

Server entry (project `.mcp.json`):

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

`command` must start this fork's build (put it first on PATH or use its absolute path): the npm-published `bladebro` has no recording. Add `"BLADE_HEADLESS": "1"` to `env` to hide the browser (see [Example 1](#1-qa-server-hidden-browser)). The tools are then `mcp__bladebro__act`, `mcp__bladebro__see`, `mcp__bladebro__state`, `mcp__bladebro__run` and `mcp__bladebro__vision`.

**gsd-charlotte-qa (MCP).** The agent definition in get-shit-done lists only Charlotte tools today, and bladebro does not ship or change it. For your copy of the agent to drive bladebro, its `tools:` line needs the bladebro tools, for example:

```yaml
tools: Read, Bash, mcp__bladebro__act, mcp__bladebro__see, mcp__bladebro__state, mcp__bladebro__run, mcp__bladebro__vision
```

Each flow in its `test_flows` / `e2e_flows` is then one test case. The calls of one case, as run in 11-02 (the `name` is phase-plan-flow):

```json
{"op": "record-start", "name": "07-02-login-invalid-password"}
{"action": "navigate", "url": "http://127.0.0.1:8765/mark.html"}
{"action": "click", "text": "turn blue"}
{"op": "record-stop", "value": "FAIL: error banner never appeared"}
```

Result of the last call (11-02 evidence; paths shortened):

```
recording saved: /abs/path/07-02-login-invalid-password-20261026T150308Z-2505f0.webm (bytes 53535, duration 4.6 s, frames 46, dropped 0, segments [0.0 s tab 0B65...], captured 35). Poster: /abs/path/07-02-login-invalid-password-20261026T150308Z-2505f0.poster.png. Timeline: /abs/path/07-02-login-invalid-password-20261026T150308Z-2505f0.timeline.json (2 actions). Marker at 4.6 s
```

Have the agent report the video, poster and timeline paths with each flow's result.

**gsd-test-writer (Bash).** This agent has no MCP tools; the tests it writes can call the CLI. The daemon reads `BLADE_RECORD` when it starts, so the suite's first `bladebro` command runs with it (see [Example 5](#5-per-test-recording-from-a-shell-cli-daemon)). After a test's steps, the test takes the paths from the stop line:

```bash
out=$(bladebro state record-stop --value "PASS: t03")
video=$(printf '%s\n' "$out" | sed -nE 's/^recording saved: (.+\.(webm|mp4)) \(bytes .*/\1/p')
poster=$(printf '%s\n' "$out" | sed -nE 's/.*\. Poster: (.+\.poster\.png)\..*/\1/p')
timeline=$(printf '%s\n' "$out" | sed -nE 's/.*\. Timeline: (.+\.timeline\.json) \(.*/\1/p')
echo "video: $video"; echo "poster: $poster"; echo "timeline: $timeline"
```

Output from that run (11-02 evidence; paths shortened):

```
video: /abs/path/t03-20261026T150315Z-2f3988.webm
poster: /abs/path/t03-20261026T150315Z-2f3988.poster.png
timeline: /abs/path/t03-20261026T150315Z-2f3988.timeline.json
```

On Windows the CLI cannot record (no daemon); use the MCP server there.

### DSH plugin flow

Install the plugin as the npm package README says: `dsh plugin --profile web add bladebro` (Desktop: `dsh plugin --profile desktop add bladebro`). DSH exposes the same five tools as `mcp__bladebro__act`, `mcp__bladebro__see`, `mcp__bladebro__state`, `mcp__bladebro__run` and `mcp__bladebro__vision`, so the per-test-case calls are the ones above.

The plugin (`npm/bladebro/dsh-plugin.mjs`) runs `bladebro mcp` over stdio and accepts the options `binaryPath`, `env`, `cwd`, `toolCallTimeoutMs` (default 180000 ms, which covers the stop) and `reconnect`. Without `binaryPath` the plugin looks for the npm platform package for this machine, and its source throws `bladebro: platform package "<pkg>" not installed` when that package is missing. Point `binaryPath` at this fork's build:

```json
{ "binaryPath": "/abs/path/to/bladebro", "env": { "BLADE_RECORD": "1" }, "cwd": "/path/to/project" }
```

`cwd` decides which `.bladebro.json` applies.

As a profile patch (the `bladebro` row exists once `dsh plugin add` has run; per the gsd-dsh profile notes a patch row replaces that row's whole `config`, so list every option you need):

```yaml
- id: bladebro
  config:
    binaryPath: /abs/path/to/bladebro
    env:
      BLADE_HEADLESS: "1"
      BLADE_RECORD: "1"
```

DSH accepts one `bladebro` namespace per profile (see [Example 1](#1-qa-server-hidden-browser)), and its default `toolCallTimeoutMs` (180000 ms) covers the stop. Run in 11-02: the plugin file with a stub DSH bridge, its options started as the MCP server and driven through the same test case, which saved a recording; a DSH host itself was not run (see [Not verified](#not-verified)). The GSD port in gsd-dsh has no `bladebro` entry in its tool map (`tool-map.ts` maps the Charlotte tools to `drop`), so this flow applies to a DSH chat or agent that has the plugin enabled.

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

## Per-call parameters

The only per-call inputs are the arguments of the two `state` ops (MCP `state` tool, CLI `bladebro state`, or a `batch`/`run` step). On/off, dir, format, fps, size, quality, tab mode and headless come from the environment and the config files.

| Op | Parameter | Required | Values | Effect |
|---|---|---|---|---|
| `record-start` | `name` | yes | a non-empty string. Kept: `A-Z a-z 0-9 . _ -`; every other character becomes `_` (runs collapse to one), leading and trailing `. _ -` are trimmed, cut to 80 characters, `recording` when nothing is left, Windows device names (`con`, `nul`, `com1`, ...) get a `rec_` prefix | file name `<name>-<UTC>-<id>.<ext>` (per-tab: `<name>-tab<N>-<UTC>-<id>.<ext>`) |
| `record-stop` | `value` | no | a string; see [Poster, marker and timeline](#poster-marker-and-timeline) for the cleaning (controls become spaces, zero-width and bidi characters dropped, 200 characters) | marker note at the stop moment; never echoed |

Measured (11-01 `name-rules`): `../../evil name%d` became `evil_name_d-<UTC>-<id>.webm`, a 100-character name was cut to 80, `...` became `recording-<UTC>-<id>.webm`, `ok.name_1-2` was kept. An empty name is refused: `usage: record-start needs name (a string), e.g. state record-start login-flow.`

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

### 5. Per-test recording from a shell (CLI daemon)

One video per test case: the first command starts the daemon with recording on (the daemon reads the environment once), each test is wrapped in record-start / record-stop, and every stop prints the file path. The URL is this repo's fixture page (tools/record_sidecar/fixtures served on port 8765); use your app's URL.

```bash
export BLADE_HEADLESS=1 BLADE_RECORD=1
bladebro nav http://127.0.0.1:8765/mark.html
for t in t01-blue t02-green; do
  bladebro state record-start "$t"
  bladebro act click "turn ${t#*-}"
  bladebro state record-stop --value "PASS: $t"
done
bladebro stop
```

Real stop line from that run (11-01 evidence; paths shortened):

```
recording saved: /abs/path/t01-blue-20261026T145222Z-1c9b50.webm (bytes 27424, duration 0.8 s, frames 8, dropped 0, segments [0.0 s tab D4E7...], captured 9). Poster: /abs/path/t01-blue-20261026T145222Z-1c9b50.poster.png. Timeline: /abs/path/t01-blue-20261026T145222Z-1c9b50.timeline.json (1 action). Marker at 0.7 s
```

On Windows the CLI has no daemon and refuses record-start; record through MCP there.

## Known issues

- **Recording inside one `run` gives an empty timeline.** A `run` that itself contains `record-start` ... `record-stop` produces a video, poster and marker, but the timeline lists no actions (`(0 actions)`): the timeline records whole tool calls, and that `run` call began before the recording existed. Measured in 11-01: the in-run timeline held only the marker; the same steps as separate calls gave an action and the marker. For a timeline, call `record-start` and `record-stop` as their own `state` calls and put the test steps between them.
- **`act click` can report `clicked (no observable DOM change)` for a click that did change the page.** The verdict readback did not report a background-colour change (src/action, upstream). The video and the poster show the change (11-01: after `click "turn blue"` the poster's 1x1 pixel was rgb(2, 65, 252), blue). Check the effect with `see` or the poster, not the verdict, in that case.
- **`switch-tab` to the tab the agent is already on does not move the video.** After a window-feature popup the agent is still on the opener; `switch-tab <opener>` reports success, but the video stays on the popup (11-01 `window-popup`: the popup row kept ` R`). Close the popup (`state close-tab <popup id>`) or switch to another tab and back (see [Tab following](#tab-following)).
- **A short test gives a short video.** The clock starts at the first frame (see [Recording time](#recording-time)). In 11-01 a test that was only `record-start`, one click and `record-stop` on an already loaded page gave 0.8 s of video (`ffprobe` duration 0.800000 for both runs). A case stopped before its first frame gets a video of about one frame (11.1: frames 1-6 from `11.1-01-live.txt`).

## Measurements

Not re-run in Phase 11. These come from earlier phases, on macOS. Phase 2 figures were measured headed, later phases headless:

- The size table in [File size per minute](#file-size-per-minute): Phase 2 and Phase 9 (Phase 9 source: `.planning/phases/09-finalization-on-every-exit-path/evidence/09-03-sizes.txt`).
- Phase 10 figures (labelled 10-01 or 10-02 where they appear).
- The unlabelled "in testing" figures in [How frames are captured](#how-frames-are-captured) and the cross-site note in [Tab following](#tab-following): earlier phases. The headless frame rate (the `dur-anim` run in `06-02-live.txt`) and the pre-fix drift (up to +6.24 %, also in `06-02-live.txt`) are from Phase 6. The tick-page drift (-0.74 % to +0.16 %) is from `06-05-live.txt`. The post-fix animated-page drift (+0.6 % to +1.2 %, 8 to 16 s) is from the Phase 6 verifier re-runs in `.planning/phases/06-frame-source/06-VERIFICATION.md` (lines 42-44), since no raw file holds it. The background-tab result (0 frames in 8 s, Chrome reported the tab hidden) is from Phase 6 (`06-02-live.txt`, the `[bg]` scenario, headless) result (0 frames in 8 s, Chrome reported the tab hidden) is from Phase 6 (`06-02-live.txt`, the `[bg]` scenario, headless); the Phase 8.1 popup spike is from `08.1-01-spike.txt`. The about-100 frames/s headed rate, the minimized window's about 0.3 frames/s and the 37 s silence (one run in seven, not reproduced) are from Phase 2: `tools/record_sidecar/RESULTS.md` and `tools/record_sidecar/results/core.json` (headed Chrome), with the minimized and background runs in `results/extra.json` and the silence in `.planning/phases/02-sidecar-prototype-gate/deferred-items.md`. Treat them as order-of-magnitude figures and re-measure before relying on them. The short-case figures are from 11.1 (`.planning/phases/11.1-short-case-frame-guarantee/evidence/11.1-01-live.txt`).

## Not verified

The Phase 11 runs were on macOS (arm64) with headless Chrome 154 against local fixtures. Every result sample on this page (each fenced output block) is output from the Phase 11 runs (11-01, 11-02 and 11-03 evidence in `.planning/phases/11-integration-docs/evidence/`, as mapped in `snippet-map.tsv`), not output carried over from earlier phases. The one sample from the 11.1 run (the assertion-only case) is from `.planning/phases/11.1-short-case-frame-guarantee/evidence/11.1-01-live.txt`. Figures quoted in prose that carry no 11-01, 11-02 or 11-03 label, and the size table, are earlier-phase measurements; see [Measurements](#measurements). Not run:

- Linux and Windows (no run of any example there; CI was not run for Phase 11). Windows lifecycle and the job object are by code reading only.
- Headed Chrome: no example was run with a visible browser (the interactive `.mcp.json` entry was started and listed its tools, but made no browser call).
- The record-stop first-frame wait in a headed browser (11.1 measured headless only), and a live zero-frame stop after the wait (unit tests only).
- The real-browser lane (`BLADE_LANE`): recording is refused there by design; the refusal was not run.
- A DSH host: `dsh` is not installed here. The plugin file was loaded with a stub bridge and the options it hands DSH were started as the MCP server; DSH's own loading of the profile YAML and its MCP bridge were not run.
- The Claude Code client itself: the `.mcp.json` entries were started exactly as a client starts them (command, args, env) and driven over stdio; a session inside Claude Code was not run (see Phase 11 evidence). An attempt with claude -p from the build sandbox failed before any tool call (Failed to authenticate: OAuth session expired and could not be refreshed).
- A real 2 h recording, a full disk and a 1 GiB file (see [Disk space and limits](#disk-space-and-limits)).

## Troubleshooting

- `browser mode mismatch`: a daemon from another mode is running. Run `bladebro stop`, then retry.
- `BLADE_HEADLESS: "<value>" is not valid`: use `1`, `on`, `true`, `yes`, `0`, `off`, `false` or `no`, or unset it. The same spellings apply to `BLADE_RECORD`.
- A window still appears: an old daemon from a previous build is still running (`bladebro stop`); you are on the real-browser lane (`bladebro rb visible off`); or a higher layer says headed. `bladebro doctor` shows the mode and its source.
- No window with `BLADE_HEADLESS=0` on Linux: by design, the browser runs on bladebro's private Xvfb display.
- `.bladebro.json` ignored with a warning: it is a symlink (replace it with a regular file), owned by another user, or group/other-writable (`chmod 644 .bladebro.json`).
- `record.dir ... ignored`: the path resolves outside the project and `<data root>/recordings`. Use a path inside the project.
- Old `BLADE_CHROME_FLAGS=--headless=new` workaround: remove it and use `BLADE_HEADLESS=1`.
- `recording is off: the operator has not enabled it ...` although you set `BLADE_RECORD=1`: the daemon or MCP server was started before you set it and reads the environment once. Run `bladebro stop` (or restart the MCP server), then retry.
- `recording unavailable: BLADE_FFMPEG=... is unusable`: fix or unset `BLADE_FFMPEG`; `bladebro doctor` shows which ffmpeg was found and its encoders.
- `BLADE_RECORD_FPS: fps 20 out of range 5-15`, `BLADE_RECORD_SIZE: ... must be even`, `BLADE_RECORD_FORMAT: format "avi" is not supported`, `BLADE_RECORD_TABS: must be follow or per-tab`, `BLADE_RECORD: ... must be an absolute directory path`: the message names the variable and the valid values; fix it and run `bladebro stop` so the daemon re-reads it.
- The timeline says `(0 actions)` although the test clicked: the recording was started inside the same `run` call (see [Known issues](#known-issues)).
- `no frames reached the encoder` (in `recording not clean: recording failed: ...` in follow mode, or `recording not clean (per-tab, N files): tab <id>: recording failed: ...` in per-tab mode): no frame arrived even within the up to 2 s that `record-stop` waits (the tab was hidden, in the background or minimized), and the file is 0 bytes. Harnesses should match this substring. Bring the tab to the front (`switch-tab`) before `record-stop`.
- The stop times out in your client: a stop can take about 23 s in the worst case; set the client's tool timeout above that (the DSH plugin's `toolCallTimeoutMs` default, 180000 ms, is enough).
