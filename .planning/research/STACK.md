# Stack Research: Session Video Recording

**Domain:** Opt-in session video recording for a Rust CDP browser driver (Bladebro v4.6.5)
**Researched:** 2026-10-21
**Confidence:** MEDIUM overall (protocol mechanics HIGH from long-stable CDP behaviour plus community corroboration; the CDP reference page could not be fetched live, and ffmpeg build contents were confirmed only via Homebrew formula dependencies, not configure flags)

## Recommended Stack

### Capture: CDP `Page.startScreencast`

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `Page.startScreencast` / `Page.screencastFrame` / `Page.screencastFrameAck` | Stock CDP, any current Chrome (project baseline Chrome 151; floor 154 per SUPPORT-MATRIX) | Per-tab frame capture over the existing CDP session | Per-tab (not whole-screen), works headful and headless, needs no extension, no new flags, no Chromium changes. Stays inside the existing Page domain that bladebro already drives. |

**Usage rules (prescriptive):**
- Use `format: "jpeg"`, `quality` 60-80, and `maxWidth`/`maxHeight` set to the viewport (or 1280x720 cap). Do not use `png`: frames are several times larger, which starves the WebSocket and the encoder.
- **Ack every frame.** Send `Page.screencastFrameAck { sessionId }` for every `screencastFrame`, promptly, even if the frame is dropped. Community reports show Chrome stalls after a couple of unacknowledged frames on animating pages (3 unacked vs 178 acked over 3 s). Ack from the reader loop, not after the ffmpeg write completes; if the write blocks, the frame goes to a bounded queue and is dropped with a counter, never allowed to stall the ack.
- **Do not use `everyNthFrame` as an FPS control.** Frames are event-driven (emitted when the compositor produces a new frame), so a static page yields few frames. Let the timestamps, not a nominal rate, define the timeline.
- **Do not trust `metadata.sessionId` as a frame counter.** It is constant per `startScreencast` call. Use your own monotonic counter.
- Stop with `Page.stopScreencast`. Recording is per target session, so tab-following means re-issuing `startScreencast` on the new target when the active tab changes (this belongs in the sidecar prototype).
- Keep recording on the WebSocket transport. `BLADE_TRANSPORT=pipe` gives no second client path for a sidecar; the in-tree recorder should use the same session the driver already holds.

### Encoding: external `ffmpeg` binary (default, no new crates)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `ffmpeg` (external, from PATH or override) | Minimum tested: 5.1 (for `-fps_mode`); test against current 8.1.x and 9.0.x | Encode JPEG frames to WebM (default) or MP4 | Zero binary-size cost, no new Rust dependency (ask-first is avoided), and it is the tool the user's Charlotte workflow already relied on. |

**Input method:** pipe frames to ffmpeg stdin as an image sequence, `-f image2pipe -c:v mjpeg -i -` (JPEG frames match the CDP output, so no re-encode before the encoder). Stdin is written only from the recorder task; stderr is captured to a log file, not inherited, so it can't corrupt MCP stdout.

**Timing (the main correctness trap):** frames arrive irregularly, so a fixed `-framerate` plays too fast or too slow. Recommended options, in order:
1. Timestamp each frame on arrival (or from `metadata.timestamp`, which is optional and in seconds) and feed ffmpeg with wall-clock timestamps: `-use_wallclock_as_timestamps 1` on the input, then `-fps_mode vfr` (or `-vsync vfr` on ffmpeg < 5.1, avoid if possible). Validate in the sidecar that the output duration matches elapsed wall time.
2. Fallback if (1) misbehaves on a given build: write frames plus a concat-demuxer list with per-frame `duration` lines to a temp dir, then encode with `-vsync cfr` at the measured average FPS.
3. Never leave `-r` at a guessed value.

**Encoder choices (default first):**

| Output | ffmpeg args (starting point) | Why | Confidence |
|--------|------------------------------|-----|------------|
| WebM (default) | `-c:v libvpx-vp9 -deadline realtime -cpu-used 8 -row-mt 1 -crf 36 -b:v 0 -pix_fmt yuv420p` | Royalty-free, plays everywhere that matters, realtime settings keep CPU sane | HIGH (flags are long-standing libvpx options; exact speed needs measuring) |
| WebM (fallback if VP9 slow on low-end CI) | `-c:v libvpx -deadline realtime -cpu-used 8 -crf 10 -b:v 2M` | VP8 encodes faster than VP9 | MEDIUM |
| MP4 (opt-in format) | `-c:v libx264 -preset veryfast -crf 28 -pix_fmt yuv420p -movflags +faststart` | Widest player compatibility | HIGH that flags are valid; MEDIUM on availability (see below) |

Always `-y`, `-loglevel error` (plus stderr capture), `-an` (no audio), and `-pix_fmt yuv420p` for player compatibility. Make the output name predictable and report the path in the tool result.

**ffmpeg availability per build (the real constraint):**
- **Homebrew (macOS):** `ffmpeg` depends on `x264`, `libvpx`, `opus`, `svt-av1`, `x265` (confirmed from formulae.brew.sh; the current stable formula is 9.0.2, and `ffmpeg@8` still ships 8.1.x). libx264 and libvpx are therefore available in a default `brew install ffmpeg`. Confidence: HIGH for dependency presence, MEDIUM for the exact enabled-encoder list (not checked from configure output).
- **Windows:** gyan.dev "essentials" and "full" builds (what `winget install Gyan.FFmpeg` installs) include libx264 and libvpx. Confidence: MEDIUM (secondary source; not verified from the gyan.dev build page).
- **Linux:** distro ffmpeg packages (Debian/Ubuntu/Fedora) ship libx264 and libvpx. Static builds from johnvansickle also do. Confidence: MEDIUM.
- **Rule:** at discovery time run `ffmpeg -hide_banner -encoders` once, cache the result, and check for the encoder the requested format needs (`libvpx-vp9` / `libvpx` / `libx264`). If missing, return an honest error naming the missing encoder and the install command for the platform. Never silently fall back to a different format.

**Discovery (no new crates, `src/platform.rs`):**
1. Explicit override env var (e.g. `BLADE_FFMPEG=/path/to/ffmpeg`), validated as an executable.
2. `PATH` lookup (`which` on Unix, `where` on Windows; or `Command::new("ffmpeg")` plus a spawn-probe `-version`).
3. Known install locations where PATH is often not inherited by daemons or GUI launchers:
   - macOS: `/opt/homebrew/bin/ffmpeg` (Apple Silicon), `/usr/local/bin/ffmpeg` (Intel Homebrew)
   - Windows: `%LOCALAPPDATA%\Microsoft\WinGet\Links\ffmpeg.exe`, plus `C:\ffmpeg\bin\ffmpeg.exe`
   - Linux: `/usr/bin/ffmpeg`, `/usr/local/bin/ffmpeg`
4. Probe `-version` and `-encoders`; record the version in `doctor` output.
- Do not auto-install, do not download, do not bundle (no npm install scripts; binary stays ~7 MB).

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Frame source | CDP `Page.startScreencast` | `HeadlessExperimental.beginFrame` | Requires old headless mode plus `--enable-begin-frame-control`, which is an automation-style flag and violates non-negotiable #7. Not available on new headless. Reject. |
| Frame source | CDP screencast | Xvfb `x11grab` (Linux) / `avfoundation` (macOS) / `gdigrab` (Windows) | Captures the whole screen, not the tab: other windows, the Xvfb WM and the user's desktop leak into the video. macOS needs Screen Recording permission per host app; Linux-only path for Xvfb. Use only as a documented last resort for the real-browser lane. |
| Frame source | CDP screencast | `chrome.tabCapture` / extension | Requires shipping a Chrome extension and altering the launch profile; adds a page-visible and profile-visible footprint. Reject. |
| Frame source | CDP screencast | Polling `Page.captureScreenshot` | Works as a fallback for a stalled screencast, but roughly 1-3 fps at high cost and slower than the event stream. Use only as a fallback if screencast proves unreliable in the sidecar. |
| Frame format | JPEG q60-80 | PNG | Several times the bytes per frame; too heavy over the WebSocket and for the encoder. |
| Encoder | external ffmpeg | Pure-Rust encoder (`rav1e`, AV1) | Credible in principle (pure Rust, royalty-free), but AV1 realtime encoding at QA-acceptable speed is CPU-heavy, the crate adds binary size and supply-chain surface, and it is an ask-first dependency. Revisit only if ffmpeg is unavailable for a user and the project agrees to add a crate. |
| Encoder | external ffmpeg | `openh264` crate / Cisco binary | H.264 baseline only; the Cisco binary is downloaded at runtime (network + supply chain). Reject. |
| Encoder | external ffmpeg | `vpx-encode` / `vpx-sys` C bindings | Adds a C toolchain requirement on every target including Windows; ffmpeg already gives you libvpx for free. Reject. |
| Encoder | external ffmpeg | `image` / `gif` crates | GIF only (large, banded), or lossless WebP (not a video). Not a video format. Reject. |
| Muxer | ffmpeg | `mp4` / `matroska` crates | Muxers only; still need an encoder, and bladebro would have to write its own timing and finalisation. Reject. |
| Ffmpeg timing | wall-clock timestamps / `-fps_mode vfr` | fixed `-framerate` | Fixed rate desyncs with event-driven frames (static pages produce few frames). |
| Format default | WebM (VP9) | MP4 (H.264) default | MP4/H.264 depends on a GPL-enabled ffmpeg build; WebM with libvpx is present in the same builds and avoids the GPL question for the default path. MP4 stays opt-in. |

## What NOT to Use

- `HeadlessExperimental.beginFrame`: needs an automation flag and legacy headless (violates non-negotiable #7).
- Any `--enable-automation`-class or `--disable-blink-features=AutomationControlled` flag to "help" recording.
- `everyNthFrame` as a frame-rate control.
- Un-acked screencast frames (stalls the stream).
- A bundled ffmpeg or any npm install script that downloads one.
- Patching `Function.prototype.toString` or injecting JS into the page to read frames: screencast does not need page-side code.
- A new sixth MCP tool for start/stop: recording is a parameter on `state` (per PROJECT.md).

## Installation

No new Rust crates. Runtime requirement only:

```bash
# macOS
brew install ffmpeg
# Windows
winget install Gyan.FFmpeg
# Debian/Ubuntu
sudo apt install ffmpeg
```

## Open Questions for Phase Research (flagged for the sidecar prototype)

1. Exact `metadata.timestamp` semantics on the installed Chrome (optional field; may need wall-clock fallback). MEDIUM.
2. Whether `-use_wallclock_as_timestamps` with `image2pipe` yields correct durations on ffmpeg 8/9 and older 5.1 builds. LOW until measured.
3. Realtime VP9 CPU cost on the CI runners (ubuntu, macos, windows) at 1280x720 and 30 fps-equivalent. Measure before picking VP9 vs VP8 as the default. LOW.
4. Whether screencast triggers any page-visible difference (should not, but `bladebro audit` 111/111 and `diff_oracle` divergent: 0 must confirm). MEDIUM.
5. Behaviour on a backgrounded/occluded headful tab: screencast may emit no frames; the output would then be near-empty. Must be surfaced in the verdict. LOW.

## Sources

- Chrome DevTools Protocol, Page domain reference: https://chromedevtools.github.io/devtools-protocol/tot/Page/ (redirect only on fetch; the method list and event field names above are from long-stable CDP behaviour, MEDIUM confidence, to be confirmed against the pinned Chrome version in the sidecar).
- Homebrew formula `ffmpeg` (current stable 9.0.2; runtime deps x264, libvpx, opus, svt-av1, x265): https://formulae.brew.sh/formula/ffmpeg
- Homebrew `ffmpeg@8` (8.1.x line): https://formulae.brew.sh/formula/ffmpeg%408
- Community corroboration on screencast ack stalls and event-driven frame rate (MEDIUM, not official): https://github.com/browser-use/browsercode/pull/124 and https://cloud.browserless.io/blog/screencast
- Puppeteer ScreenRecorder (reference implementation of the same pattern: screencast frames piped to ffmpeg): https://app.unpkg.com/chrome-devtools-frontend@1.0.1666631/files/front_end/third_party/puppeteer/package/src/node/ScreenRecorder.ts
- Project context: `.planning/PROJECT.md`, `.planning/codebase/STACK.md`, `AGENTS.md` non-negotiables #1, #2, #7, #8, #9.
