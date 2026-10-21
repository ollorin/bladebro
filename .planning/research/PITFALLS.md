# Pitfalls Research

**Domain:** CDP screencast + external ffmpeg session recording inside a stealth-sensitive Rust browser driver (Bladebro v4.6.5)
**Researched:** 2026-10-21
**Confidence:** MEDIUM. Repo facts are verified against `src/` and `.planning/codebase/CONCERNS.md`. Chrome screencast behavior is from knowledge of the CDP `Page.startScreencast` contract and has NOT been re-verified against Chrome 151 docs or a live run in this session. Every item marked "verify" must be reproduced in the sidecar prototype before in-tree work.

## Critical Pitfalls

### Pitfall 1: Forgetting `Page.screencastFrameAck` (or acking late)

**What goes wrong:**
Chrome sends `Page.screencastFrame` events and then stops sending new frames until each one is acknowledged with `Page.screencastFrameAck { sessionId }`. A recorder that consumes frames but does not ack, or acks only after the ffmpeg write completes, produces a video that freezes after the first frame or a few frames. The browser keeps working, so nothing errors.

**Why it happens:**
The ack is an easy-to-miss protocol requirement. Sidecar prototypes often ack in the same loop that writes to ffmpeg, so a slow write delays the ack.

**How to avoid:**
- Ack immediately on receipt, before any processing. Copy the `sessionId` from the event, not a cached value.
- Ack on the CDP transport directly, never through a shared action lock that can wait behind a `state` or `act` command.
- Count frames sent and frames acked. A growing gap is the freeze signal.

**Warning signs:**
- Output duration far shorter than wall-clock time, or a video of only a few frames.
- Frames arrive in bursts after a long action (the acks were queued behind it).
- The recording stops at the first navigation or the first `act` call.

**Phase to address:** Sidecar prototype (frame capture). Must be a pass/fail check there.

---

### Pitfall 2: Blocking the CDP reader on the ffmpeg pipe (deadlock and backpressure)

**What goes wrong:**
If the code that reads the CDP websocket also writes JPEG/PNG bytes to ffmpeg stdin, a slow or stalled ffmpeg blocks the reader. The reader then cannot process the ack responses or any other command reply, and the whole session hangs. Memory also grows: an unbounded channel between capture and encode holds every frame when ffmpeg is slower than Chrome (a 1080p screencast can run faster than a software VP9 encode).

**Why it happens:**
The simple design is "on frame, write to stdin, then ack". It works until the encoder falls behind. Tokio `ChildStdin` writes are async, but `.await`-ing them inside the event loop still stalls the loop.

**How to avoid:**
- Split into three parts: CDP reader (ack and enqueue only), a bounded queue (for example 8 to 30 frames), and a writer task that feeds ffmpeg stdin.
- On a full queue, drop the oldest or newest frame and increment a `dropped_frames` counter. Never block the reader.
- Surface `dropped_frames` in the recording verdict so the agent knows the video has gaps.
- Use `max_width`/`max_height` and a JPEG `quality` in `Page.startScreencast` to cap the bytes per frame. Use `everyNthFrame` only if the cap is still too high.

**Warning signs:**
- Memory of the daemon or MCP process rising linearly during a recording.
- Action latency (the `act` round trip) rising only while recording is on.
- `Runtime`/`Page` commands timing out with the 30 s `DEFAULT_TIMEOUT` (`src/cdp/client.rs`) during recording.

**Phase to address:** Sidecar prototype, then the in-tree capture module. Add a load test with a fast-animating fixture page.

---

### Pitfall 3: Variable frame rate and timestamp drift (time-lapse or slow-motion video)

**What goes wrong:**
`Page.screencastFrame` fires only when the compositor produces a new frame. A static page can yield one frame per several seconds. A video encoded at an assumed constant rate (for example, ffmpeg's default for raw or image pipes) then plays back too fast or too slow relative to wall-clock time. Ack latency adds jitter on top.

**Why it happens:**
ffmpeg assumes a constant input frame rate unless told otherwise. Developers test on a busy page, see a plausible result, and never test an idle one.

**How to avoid:**
- Use `metadata.timestamp` from each frame (seconds, Chrome monotonic clock) to compute per-frame durations, and feed ffmpeg a timed input (for example, the concat demuxer with `duration` lines, or a `-use_wallclock_as_timestamps 1` input with `-vsync`/`-fps_mode vfr` for WebM output). Pick one approach and document it.
- Set an explicit output frame rate cap (for example 10 to 15 fps for QA evidence). This also limits file size.
- Duplicate or hold the last frame during an idle stretch if the QA reviewer needs a continuous timeline. Decide this explicitly; it affects size.

**Warning signs:**
- `ffprobe`-level duration differs from the measured wall-clock recording time by more than a few percent.
- Animations look sped up on the idle parts and smooth on the busy parts.

**Phase to address:** Sidecar prototype (timing acceptance test: a 30 s run with a static page and a 2-second animation; duration must match wall-clock within 5%).

---

### Pitfall 4: Frame size changes break the encoder

**What goes wrong:**
ffmpeg takes the input size from the first frame. If the viewport changes (window resize, a page that changes its device metrics, a new tab with a different window size, or DPR change), later frames have a different size and ffmpeg either errors out or silently scales/corrupts depending on the pipeline. Odd dimensions are a separate trap: yuv420p (used by most H.264 and many WebM encoder paths) requires even width and height, and fails with "width not divisible by 2".

**Why it happens:**
Screencast frame sizes come from the device's pixel size times DPR, not from the CSS viewport. Developers test with one fixed window and the default DPR of 1.

**How to avoid:**
- Force a fixed output size with a `scale` filter, and round to even: `scale=trunc(iw/2)*2:trunc(ih/2)*2` on the first frame, or `scale=W:H:force_original_aspect_ratio=decrease,pad=W:H` against a fixed canvas.
- Pass `maxWidth`/`maxHeight` to `Page.startScreencast` so Chrome returns frames at a bounded size.
- Read `metadata.deviceWidth`/`deviceHeight` per frame and log a note when they change; do not just rely on ffmpeg.
- Default to WebM with VP8 or VP9 (libvpx), which tolerates more shapes than H.264, but still enforce even dimensions.

**Warning signs:**
- ffmpeg stderr lines containing "not divisible by 2", "Input stream #0:0 frame changed from size", or "Changing video frame properties on the fly is not supported".
- Recording works in a headless default window but fails on a headed run with a different window size.

**Phase to address:** Sidecar prototype (window-size matrix: 1280x720, 1366x768 at DPR 1 and 2, and a deliberately odd size such as 1001x641).

---

### Pitfall 5: Killing ffmpeg wrong: unflushed or corrupt WebM and zombie processes

**What goes wrong:**
A WebM file is finalized only when ffmpeg receives EOF on stdin and writes its trailer (Cues, duration, segment size). SIGKILL, a dropped `Child`, or a daemon crash leaves a file with no trailer. Some players refuse it or show a zero duration. A child that is never `wait()`ed becomes a zombie on Unix. On Windows, `TerminateProcess` gives no chance to finalize at all.

**Why it happens:**
The obvious cleanup is `child.kill()`. Tokio `Child` with `kill_on_drop(true)` sends SIGKILL, which is the wrong signal for a finalizing encoder.

**How to avoid:**
- Stop sequence: stop screencast, drop the frame queue producer, flush the writer, close ffmpeg stdin, then `wait()` with a bounded timeout (for example 10 s). Only after that timeout, kill, and then report the file as "unfinalized" in the verdict.
- Do not use `kill_on_drop(true)` as the only guard for the normal path. Use it as the last-resort guard, and make sure the normal stop runs first.
- Always `wait()` the child (also in error paths) and record its exit status. A non-zero exit is a failed recording, not a silent success.
- Handle SIGTERM/SIGINT in the daemon and MCP server: finalize active recordings before exit. `bladebro stop` must do this too.

**Warning signs:**
- `ps` shows `ffmpeg` processes after the daemon has exited (orphans), or `<defunct>` entries.
- Output file exists with size > 0 but no duration in `ffprobe`, or the player shows 0:00.
- The next `bladebro stop` takes exactly the timeout length (ffmpeg was waiting on stdin that never closed).

**Phase to address:** Lifecycle and shutdown phase. Add an integration test that kills the parent mid-recording and checks for orphan ffmpeg processes. Extend the orphan reaper in `src/session_profile/reap.rs` to know about ffmpeg children.

---

### Pitfall 6: Stdin pipe deadlock from unread ffmpeg stdout/stderr

**What goes wrong:**
If ffmpeg writes progress or warnings to stderr (or stdout) and nothing drains those pipes, the ffmpeg process blocks once the OS pipe buffer fills (about 64 KB on Linux and macOS). ffmpeg then stops reading stdin, and the writer stalls. The symptom looks exactly like backpressure from Pitfall 2 but is caused by the stderr pipe.

**Why it happens:**
`Stdio::piped()` for stderr is set up for diagnostics and then never read.

**How to avoid:**
- Pass `-nostats -loglevel error` (or `warning`) and keep a bounded tail of stderr in a task that reads continuously. Use the tail in the failure verdict.
- Set stdout to `Stdio::null()`. Stdout is not needed for output (the file path is an argv element).
- Never let stderr go to the daemon's stdout (the MCP stdout is a JSON-RPC pipe; ffmpeg output there would corrupt the protocol). Logs go to stderr only, as AGENTS.md requires.

**Warning signs:**
- Recording stops after roughly the same number of frames on every run.
- ffmpeg is in state `S` with the pipe full (check with `lsof` or `/proc/<pid>/fd` on Linux).

**Phase to address:** Sidecar prototype.

---

### Pitfall 7: Background, occluded, and minimized tabs produce no frames

**What goes wrong:**
In headed Chrome, a window that is minimized, fully covered, or on another macOS Space is treated as occluded, and Chrome stops compositing it. Screencast then delivers no frames, and the video is blank or frozen. Background tabs in the same window behave the same way. Chrome's backgrounding and throttling is what causes this. The existing flags in `src/browser/flags.rs` should be checked for any occlusion behavior.

**Why it happens:**
Screencast captures what the compositor draws, so an unrendered tab has nothing to send. Developers test in a visible headed window or in headless mode, where occlusion does not happen.

**How to avoid:**
- Before recording starts, make the target visible: `Target.activateTarget` or `Page.bringToFront`. Check whether these calls change anything the stealth audit can see (see Pitfall 12). Prefer the call that does not change focus state if one exists; verify before relying on it.
- Detect "no frames for N seconds while the page is loaded" and report it in the verdict instead of writing a blank video silently.
- Do not add `--disable-backgrounding-occluded-windows` or similar launch flags without an explicit decision. Launch flags are reviewed for stealth (AGENTS.md non-negotiable #7 covers automation flags; any new flag still needs `diff_oracle` and `audit` runs).
- On macOS and Windows, a headed run on a locked or sleeping display produces no frames at all. Document this as an environment limit.

**Warning signs:**
- A recording of a tab that is not the frontmost window has many identical frames, or none.
- Frame count is zero for the first several seconds after `startScreencast`.

**Phase to address:** Sidecar prototype (headed macOS and headed Linux with Xvfb plus a window manager, and headless). Document results per platform in the PR.

---

### Pitfall 8: Headless vs headed differences (and the Xvfb path)

**What goes wrong:**
Screencast behavior differs between `--headless=new` and headed Chrome, and between old headless and new headless. On Linux, headless Bladebro runs Chrome under its own Xvfb (see AGENTS.md). A recorder tested only in one mode can fail in the other. Frame timing also differs: headless compositor cadence is not the same as a display at 60 Hz.

**Why it happens:**
The recorder is developed against the default mode and the platform-specific display path is assumed to be transparent.

**How to avoid:**
- Test the matrix explicitly: headless Linux (Xvfb), headed Linux (Xvfb plus window manager), headless macOS, headed macOS, headed Windows, and the pipe transport. Record which combinations produce frames.
- Do not depend on the Xvfb display number. Use the display and window handling already in `src/browser/display.rs`, and be careful that the sidecar's ffmpeg does not capture the X display directly (for example `x11grab`), which would bypass the CDP path and record the wrong thing.
- Keep the stealth lanes separate in the matrix. The real-browser lane (`rb`) has its own launch path and must be tested separately.

**Warning signs:**
- Works on the developer machine, empty video in CI or on Linux headless.
- Frame rate in headless is much lower than headed.

**Phase to address:** Platform test matrix phase; first appearance in the sidecar prototype.

---

### Pitfall 9: Popups, new tabs, navigations, and site-isolation process swaps

**What goes wrong:**
Screencast is per target session. When a page opens a popup (`window.open`), navigates cross-origin (which can swap the renderer process under site isolation), or Bladebro creates a new tab, the recording can keep pointing at the old target and show nothing new. It can also stop after a cross-origin navigation without any error. The `state`/tab logic in `src/page/tabs.rs` and `src/page/attach.rs` owns the active target, and the recorder must follow it.

**Why it happens:**
The recorder is attached once at `start` and never re-attached. Developers test single-tab flows.

**How to avoid:**
- Tie the recorder to the active page the daemon/MCP already tracks. On a page switch, stop the old screencast and start one on the new target, and mark a segment boundary in the output (a log line or a separate segment file). Do not silently merge segments.
- Re-issue `Page.startScreencast` after cross-origin navigations and renderer swaps. Verify this with a fixture that navigates to another origin mid-recording.
- Decide what a popup means: record the opener only (simple, honest), or record each target as its own file. Report the choice in the verdict.

**Warning signs:**
- The output ends at the first navigation or shows the wrong page.
- Two recordings for one test case, or a recording that contains frames from two different sites with no marker.

**Phase to address:** Tab-following phase. Add a fixture (`tests/` or `tools/qol_probe/` style) with popup and cross-origin navigation.

---

### Pitfall 10: Enabling domains or calls that change page-visible behavior (stealth regression)

**What goes wrong:**
Recording can quietly change what the page can see:
- `Runtime.enable` is the most-cited detection signal and must not be enabled just to support recording.
- `Emulation.setDeviceMetricsOverride` or any viewport override to get a consistent frame size is detectable (window and screen metrics change), and is a known stealth regression class.
- Injecting an overlay (cursor, "REC" badge, a click indicator) into the page with `Runtime.evaluate` or an injected script is a page-visible DOM change. It also breaks the "every page-side override is a documented mask" rule.
- A new launch flag for screencast or GPU capture is an unreviewed launch difference.

**Why it happens:**
Recording "needs" a fixed viewport, a visible cursor, or a debug overlay, and these look harmless in isolation.

**How to avoid:**
- Use only `Page.startScreencast`, `Page.screencastFrameAck`, and `Page.stopScreencast`. Do not enable `Runtime` for recording. Do not change viewport metrics. Do not draw overlays in the page. Show any "recording" indicator in the CLI/tool output, not in the page.
- Treat `Page.enable` as a possible change: measure before and after with `bladebro audit` and `tools/diff_oracle/oracle.py`. Record the result in the PR (AGENTS.md requires `divergent: 0`).
- Keep the feature off by default. The default path must be byte-identical in its CDP calls to today's, so that the stealth numbers are unchanged for users who do not opt in.
- Run the stealth live sites (bot.sannysoft.com, incolumitas.com, creepjs) with recording on, as required for stealth-touching PRs.

**Warning signs:**
- Any new `Runtime.*` or `Emulation.*` call appears in the CDP trace for a recording session.
- `diff_oracle` lines marked DIVERGENT, or a new EXPECTED line without a documented mask.
- `audit` drops below 111/111.

**Phase to address:** Every phase; this must be a gate in the sidecar prototype and the in-tree PR.

---

### Pitfall 11: Sensitive data captured in video and kept forever

**What goes wrong:**
A screencast records everything rendered: typed passwords (as they appear on screen, including visible password fields with the reveal toggle), session tokens shown in pages, personal data in forms and dashboards, and the content of the page the test agent logs into. The project's own decision is "always keep recordings", so these files accumulate and are never pruned by Bladebro. The existing `src/logins.rs` and session snapshots already treat cookies as sensitive; recordings are a new, less-protected copy.

**Why it happens:**
Video is treated as harmless "evidence". Nobody decides who can read it.

**How to avoid:**
- Create recording files with user-only permissions (0600 on Unix, through `platform::secure_write_file` or equivalent, and an explicit DACL decision on Windows; CONCERNS.md notes Windows has no ACL handling anywhere in `src/`).
- Put recordings in a per-session directory under the artifacts area, not in a shared temp directory.
- Document the risk in README and in the integration docs: recordings contain everything on screen; do not enable for pages with real credentials; prune in the harness.
- Consider skipping recording frames while the active element is a password field only if this can be done without page-side JS (for example, by reading the focus state from the existing CDP-side model). Do not add a page-side hook for this.
- Never log frame data or base64 payloads. Keep logs to counts and sizes.

**Warning signs:**
- Recording paths under `/tmp` or world-readable directories.
- Harness docs that enable recording by default for all test cases.

**Phase to address:** Output/storage phase and the docs phase.

---

### Pitfall 12: Output path handling and the `validate_write_path` denylist

**What goes wrong:**
The `state` parameter lets the caller choose a recording name. If that name is used as a path, the caller can write outside the recording directory (`../../..`, absolute paths, Windows drive-relative paths like `C:foo`, UNC paths, or a name starting with `-` that ffmpeg reads as an option). `validate_write_path` in `src/platform.rs` is a denylist with known gaps (CONCERNS.md lists LaunchAgents, `/opt`, `/Library`, and rc files in subdirectories). A recording writer that calls it once and then `create`s the file leaves a TOCTOU window for a symlink swap.

**Why it happens:**
The denylist is reused as the "safe path" check without thinking about what it does not cover. ffmpeg argv is built with string formatting.

**How to avoid:**
- Sanitize the recording name to a basename: allow only `[A-Za-z0-9._-]`, reject leading `-` and `.`, cap the length, and then join it with the recording directory. Build the path with `PathBuf::join`, never with format strings and never through a shell.
- Canonicalize the recording directory once and check the final path stays inside it.
- Create the output file with `create_new` (O_EXCL), so an existing symlink cannot be followed. Pass the path to ffmpeg as one argv element. Prefer `-y` only after you know the file is the one you created.
- Do not depend on `validate_write_path` alone. Add a regression test for each traversal form, on all three OSes, with a Windows-specific case (backslash, drive letter, UNC).
- Do not let a caller-chosen name overwrite an earlier recording of the same test. Either suffix it or refuse with an actionable error that names the existing path.

**Warning signs:**
- A recording name containing `/`, `\`, `:` or `..` succeeds.
- ffmpeg error "Unable to open" on Windows for paths with spaces or long paths (over 260 characters without the long-path prefix).

**Phase to address:** Tool parameter / output phase. Also a security regression test before merge (the repo has `tests/security_audit.rs`).

---

### Pitfall 13: Windows-specific failures (pipes, paths, process tree, file locks)

**What goes wrong:**
- ffmpeg is found on PATH only with `.exe` resolution handled by the OS; a bare `ffmpeg` name can fail when a user has a different ffmpeg earlier on PATH. Users may also have ffmpeg installed under a path with spaces.
- Paths with spaces and non-ASCII user names (common in `C:\Users\<name>\`) break naive argv building only if someone builds a command string. Passing argv arrays avoids this.
- Windows keeps an open handle on files being written, so antivirus or an indexing service can lock the WebM, and a later rename or delete fails.
- `taskkill` results are discarded in `src/platform.rs` (CONCERNS.md, ~lines 902, 923). If ffmpeg is killed via `taskkill /T` without a prior stdin close, the file is left unfinalized.
- Windows has no SIGTERM, so the only graceful path is closing stdin (Pitfall 5).
- Stdin pipes on Windows: do not wrap the pipe in text mode or add newline translation. Write raw bytes only.

**Why it happens:**
The Linux and macOS path is tested in CI and the Windows path is assumed to match.

**How to avoid:**
- Put ffmpeg discovery in `src/platform.rs`: honor an explicit `BLADE_FFMPEG` path first, then PATH lookup, and report the resolved absolute path in `doctor`.
- Use `\\?\` long-path handling or keep recording paths short (for example `%LOCALAPPDATA%\bladebro\rec\<id>`).
- Retry the final rename or read with a short bounded backoff and report a lock as a specific error, not a generic failure.
- Make the Windows stop path close stdin first and wait; `taskkill` only as the last resort, and check its exit status.
- Add `windows-latest` to the recording tests, even if they are skipped on the other OSes when ffmpeg is missing.

**Warning signs:**
- Recordings work on macOS and Linux CI but fail on Windows CI with access-denied or "cannot find the file".
- Orphan `ffmpeg.exe` processes in Task Manager after a daemon crash.

**Phase to address:** Platform phase. Tests on the Windows runner are required for merge (AGENTS.md non-negotiable #9).

---

### Pitfall 14: Disk fill and unbounded recordings

**What goes wrong:**
Screencast at high resolution with frequent frames writes large files fast. Keeping every recording (the project decision) with no size or count cap fills the disk, and the first failure is an ffmpeg write error or a daemon failure in an unrelated command. Pruning by the harness only works if the harness runs.

**Why it happens:**
"Keep everything, the harness prunes" is simple to build and the cost is invisible until a long run.

**How to avoid:**
- Cap recording duration (for example, a hard maximum with a clear verdict when reached) and cap the frame rate and maximum frame size.
- Check free space before starting and periodically during a recording (the updater already calls `df` in `src/updater/download.rs`, so reuse that logic in `platform.rs` rather than duplicating it). Stop with an honest "stopped: disk low" verdict instead of an ffmpeg error.
- Report file size and duration in the stop verdict so the agent and harness can prune.

**Warning signs:**
- A 10-minute test case produces a file larger than 500 MB.
- `ENOSPC` in ffmpeg stderr, or unrelated `act`/`see` commands failing with write errors.

**Phase to address:** Output/limits phase. Measure the size per minute at each size/fps setting in the sidecar and record it in the docs.

---

### Pitfall 15: Frame delivery lost to CDP event bus lag and payload shrinking

**What goes wrong:**
Events are fanned out through a `tokio::sync::broadcast` channel (`src/cdp/client.rs`). A slow subscriber gets a `Lagged` error and frames are dropped silently, which means video gaps with no message. Separately, `shrink_oversized_event` truncates oversized event params. Screencast frames are large base64 strings. If the shrink logic ever applies to `Page.screencastFrame` (its `match` currently lists other methods, and the default branch must be checked), frames get corrupted. Corrupted JPEG frames can show as artifacts or stop the encoder.

**Why it happens:**
The shared event bus was built for small events (network, dialogs). Screencast is the first high-volume consumer.

**How to avoid:**
- Give the recorder its own subscription path, or handle the screencast frame before it enters the shared broadcast, so it does not compete with other subscribers.
- Count `Lagged` and treat it as dropped frames in the verdict.
- Verify that `shrink_oversized_event` never touches `Page.screencastFrame`. Add a unit test with a large base64 payload (for example 2 MB) that asserts the payload is passed through unchanged.

**Warning signs:**
- Frame counts differ between two runs of the same deterministic page with no error logged.
- Corrupted or grey frames in the output.

**Phase to address:** Sidecar prototype (CDP plumbing) and the in-tree capture module.

---

### Pitfall 16: Daemon and MCP lifecycle: page relaunch, idle timeout, and multiple recordings

**What goes wrong:**
The MCP serve loop relaunches the page on idle (`src/mcp/server/serve.rs`, around lines 237-249 and 344), and the daemon holds one page at a time (CONCERNS.md). A recording can end silently when the page is relaunched, or the new page gets no recording while the state says "recording". A second `start` for the same session can leak the first ffmpeg. The daemon process can also exit (`bladebro stop`, crash, updater restart) with the recording still active.

**Why it happens:**
Recording state is stored in one place and the page lifecycle in another, and the two are updated independently.

**How to avoid:**
- Keep recording state keyed by session and tied to the page lifecycle. Any page relaunch either re-attaches the recorder with a segment marker or ends the recording with an honest verdict.
- Enforce one active recording per session. A second `start` returns an actionable error naming the active file, not a silent restart.
- Finalize all recordings on every exit path (normal exit, signal, `bladebro stop`, the updater's restart). Add an integration test for each.
- Note the stale-binary gotcha from AGENTS.md: after rebuilding, stop the daemon or restart the MCP server, or the test runs the old code.

**Warning signs:**
- `state` reports "recording: true" but the ffmpeg PID is gone.
- After `bladebro stop` the output file is missing its trailer (see Pitfall 5).

**Phase to address:** Lifecycle phase (after the sidecar, before the in-tree PR).

---

### Pitfall 17: Honest verdicts that claim more than the recording proves

**What goes wrong:**
A tool result that says "recorded to X" when ffmpeg exited non-zero, when zero frames were captured, or when the file has no duration is a false claim. AGENTS.md (non-negotiable #6) requires a readback that proves the behavior. Recording failures should not break browsing, but they must be reported.

**Why it happens:**
The path is returned as soon as `start` returns, before any frame arrives or any encode succeeds.

**How to avoid:**
- Report the state as it is: `recording: started` (no claim about content), `recorded: <path> frames=N dropped=M duration=Xs size=Y` only after a finished stop with ffmpeg exit 0 and a non-empty file.
- When ffmpeg is missing, the verdict says "recording unavailable: ffmpeg not found (set BLADE_FFMPEG)" and browsing continues.
- When the file was not finalized, say "unfinalized" with the path, not "recorded".
- Keep the verdict short. Per the output contract, do not add verbose fields for every frame.

**Warning signs:**
- Tests that only check that a path string was returned, not that the file exists with a valid duration.

**Phase to address:** Verdict and tool-output phase. Include in the regression test list.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Ack after the ffmpeg write | Simpler code | Freezes and backpressure deadlocks (Pitfall 1, 2) | Never |
| `kill_on_drop` as the only stop path | No shutdown code | Unfinalized WebM, orphans (Pitfall 5) | Last-resort guard only |
| Fixed `-r 25` input | Works in the demo | Wrong speed on idle pages (Pitfall 3) | Never for shipped code |
| Reusing `validate_write_path` as the only path check | One check | Denylist gaps, TOCTOU (Pitfall 12) | Never for recording names |
| Recording on by default | Evidence always present | Stealth diff, disk, and privacy risk (Pitfalls 10, 11, 14) | Never; opt-in only |
| Testing on Linux only | Fast CI | Windows and macOS failures (Pitfall 13) | Never; all three OSes are required |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Chrome CDP screencast | Forgetting `screencastFrameAck` | Ack first, process second (Pitfall 1) |
| ffmpeg stdin | Writing raw frames without knowing the codec | Use `image2pipe` with a declared input codec (`-c:v mjpeg` or `png`) and an explicit output codec (`libvpx-vp9` or `libvpx`) |
| ffmpeg stderr | Piping and never reading | Null stdout, bounded stderr tail (Pitfall 6) |
| Existing CDP client | Using the shared broadcast for high-volume frames | Dedicated path or early handling (Pitfall 15) |
| Tab and page lifecycle | Attaching once | Follow the active page with segment markers (Pitfall 9) |
| `doctor` | Not reporting ffmpeg | Report resolved path and version so the agent can self-diagnose |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Full-resolution JPEG frames over websocket | CPU spike, action latency rises | `maxWidth`/`maxHeight`, quality cap, fps cap | Large viewports, animated pages |
| Unbounded frame queue | Memory grows linearly | Bounded queue with drop counter | Any encode slower than capture |
| Software VP9 at high resolution | Dropped frames, long stop | Lower resolution, `-deadline realtime` or `-cpu-used` tuning, measure | Long recordings, slow CI runners |
| Full-file knowledge flush during recording | Latency spikes | Already a known issue (CONCERNS.md); do not add recording writes to that path | Frequent dirty flushes |
| Base64 decode on the CDP thread | Event loop stalls | Decode and write in the writer task | Every frame |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Caller-chosen name used as a path (Pitfall 12) | Write outside the recording directory | Basename sanitization, `create_new`, canonical directory check |
| Recordings readable by other local users | Sensitive page content leaks | 0600 files and 0700 directory; Windows DACL decision |
| ffmpeg argv built from strings | Argument injection (a name starting with `-`) | argv array, reject leading `-`, no shell |
| Logging frame payloads | Sensitive content in logs | Log counts and sizes only |
| Recording on by default | Silent capture of logged-in sessions | Opt-in, documented, with a warning in the docs |
| ffmpeg resolved from a writable PATH entry | Local binary hijack | Explicit `BLADE_FFMPEG` option; report the resolved path in `doctor` |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Silent blank video | QA agent trusts a video with no content | Verdict includes frames, dropped count, and a no-frames warning |
| Path only, no size or duration | Harness cannot decide what to prune | Report size and duration in the stop verdict |
| Verbose per-frame output | Token cost on every call (AGENTS.md "lean outputs") | One summary line at stop; nothing per frame |
| Error that stops browsing | Test run fails for a recording problem | Recording failures are non-fatal and reported |

## "Looks Done But Isn't" Checklist

- [ ] **Frame capture:** Often missing a stop-time flush. Verify the file has a duration (`ffprobe` or the sidecar's own check) after a normal stop.
- [ ] **Ack:** Often acks only the first frame. Verify frame count equals ack count over a 60 s run.
- [ ] **Timing:** Often tested only on a busy page. Verify duration matches wall-clock on an idle page.
- [ ] **Tab following:** Often tested on one tab. Verify popup and cross-origin navigation mid-recording.
- [ ] **Shutdown:** Often tested only with `stop`. Verify parent kill, `bladebro stop`, and Ctrl-C all leave no orphan ffmpeg and a finalized file.
- [ ] **Stealth:** Often only the feature path is tested. Verify `audit` 111/111, `diff_oracle` divergent 0, and the CDP trace has no new `Runtime.*` or `Emulation.*` calls.
- [ ] **Windows:** Often assumed to work. Verify on `windows-latest` CI.
- [ ] **Path safety:** Often tested with a plain name. Verify traversal, leading `-`, and Windows drive/UNC forms all fail with an actionable error.
- [ ] **Verdict:** Often returns a path on start. Verify the success text appears only after a successful finalize.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Unfinalized WebM | LOW | Try remux with ffmpeg (`-c copy`) from the partial file; if it fails, report unfinalized |
| Orphan ffmpeg | LOW | Reaper in `session_profile/reap.rs` kills tagged PIDs; `bladebro doctor` lists them |
| Stealth regression from a new domain call | MEDIUM | Revert the call; re-run audit and diff_oracle; add a documented mask only if unavoidable |
| Frame gaps from lag | LOW | Dedicated subscription; surface drop count |
| Disk full | MEDIUM | Prune via harness; add caps; stop with honest verdict |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| 1. Ack handling | Sidecar prototype | Frame count equals ack count over 60 s |
| 2. Reader blocked by pipe, memory growth | Sidecar prototype, in-tree capture | Load test with fast animation; memory flat |
| 3. VFR and timestamp drift | Sidecar prototype | Duration within 5% of wall-clock on idle page |
| 4. Size change, odd dimensions | Sidecar prototype | Window-size and DPR matrix including 1001x641 |
| 5. Unfinalized WebM, zombies | Lifecycle | Kill-parent test; no orphan ffmpeg; playable file on normal stop |
| 6. Unread stderr pipe | Sidecar prototype | Long run with verbose ffmpeg warnings completes |
| 7. Occluded or background tabs | Sidecar prototype | Headed macOS and Linux results documented |
| 8. Headless vs headed, Xvfb | Platform matrix | Frames on each mode in the matrix |
| 9. Popups, navigation, site isolation | Tab following | Fixture with popup and cross-origin navigation |
| 10. Stealth side effects | Every phase, gate in PR | audit 111/111, diff_oracle divergent 0, CDP trace check |
| 11. Sensitive data | Output/storage, docs | 0600 permissions; docs warning |
| 12. Path traversal, denylist gaps | Tool parameter/output | Traversal regression tests on all OSes |
| 13. Windows quirks | Platform | windows-latest CI green |
| 14. Disk fill | Output/limits | Size per minute documented; free-space check test |
| 15. Event bus lag, payload shrink | Sidecar, in-tree capture | Unit test with 2 MB payload; lag counted |
| 16. Lifecycle, relaunch, multiple recordings | Lifecycle | Relaunch fixture; second-start error test |
| 17. Dishonest verdicts | Verdict phase | Tests check file, duration, and exit status |

## Open Questions for Phase Research

- **Chrome 151 screencast behavior:** confirm the current `Page.startScreencast` parameters, frame cadence under `--headless=new`, and whether occlusion stops frames on macOS. Needs a live run, not docs alone.
- **`Page.enable` impact:** measure whether the existing `Page` enablement changes any audit or diff_oracle result. This is the first gate before any recording code lands.
- **`shrink_oversized_event` default branch:** confirm `Page.screencastFrame` is passed through unchanged (Pitfall 15).
- **Frame-to-timestamp approach:** choose between concat-demuxer timing and wallclock timestamps, based on sidecar results.
- **ffmpeg version floor:** pick the minimum ffmpeg version and codec availability (libvpx) per platform, and check the ffmpeg install path on each CI runner.

## Sources

- Repo: `.planning/PROJECT.md`, `.planning/codebase/CONCERNS.md`, `src/cdp/client.rs` (`shrink_oversized_event`, broadcast fan-out), `src/browser/flags.rs`, `src/platform.rs` (`validate_write_path`), `AGENTS.md` (non-negotiables and instruments)
- Chrome DevTools Protocol, `Page.startScreencast` / `Page.screencastFrameAck` / `Page.screencastFrame` (behavior as recalled; verify against the protocol docs for the target Chrome version before relying on details)
- ffmpeg behavior (stdin piping, WebM finalization, yuv420p even-dimension requirement, concat demuxer timing): from general knowledge; verify with the pinned ffmpeg version in the sidecar
- Confidence: repo facts HIGH; CDP and ffmpeg behavior MEDIUM until reproduced in the sidecar prototype
