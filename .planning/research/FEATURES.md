# Feature Research

**Domain:** Session video recording for agent-driven UI/UX QA in an agentic browser driver
**Researched:** 2026-10-21
**Confidence:** MEDIUM (competitor behaviour verified from search snippets and official pages where noted; Cypress and the Charlotte fork are from training data or the user's description and are LOW)

## Scope Constraints (from PROJECT.md)

These are not features to debate. They shape every row below:

- Five MCP tools only. Recording is a parameter on `state` plus an env/config default. No `record_*` tool.
- Opt-in. Output dir via env/config (`BLADE_RECORD=<dir>`). WebM default, ffmpeg as an external binary, no new crate.
- Bladebro cannot judge pass/fail, so it never prunes. Every recording is kept and its path is reported.
- Nothing page-visible. No injected DOM, no automation flags. `bladebro audit` 111/111 and `diff_oracle` divergent 0.
- Video only in v1.

## Competitor Reference (what the field does)

| Tool | Start/stop granularity | Naming / output | Format / size / fps | Multi-tab | Notes |
|------|------------------------|-----------------|---------------------|-----------|-------|
| Playwright `recordVideo` (context option) | Per browser context, lifetime of context | Auto-generated file per page, dir set by option | Default size is viewport scaled to fit 800x800, viewport top-left in output ([official .NET docs](https://playwright.dev/dotnet/docs/videos)) | Each page and popup gets its own file | Declarative. Restarting a context is the only way to split |
| Playwright `page.screencast` (1.59+, imperative) | Explicit start/stop per page | Caller-chosen `path` | `Size` option; frames keep aspect ratio; ignored if a trace or `recordVideo` is already active ([.NET API ref](https://playwright.dev/dotnet/docs/api/class-screencast)) | Per page | Imperative start/stop is the pattern that matches per-test-case use. JS docs not confirmed in this research |
| Puppeteer `page.screencast()` (v24.8+) | Explicit start/stop per page | `path` option | WebM, VP9, 30 fps default; needs ffmpeg; options `crop`, `scale`, `speed`, `ffmpegPath` ([pptr.dev](https://pptr.dev/api/puppeteer.page.screencast)) | Per page | Closest analogue to what Bladebro needs. Some third-party guides say it is limited and recommend `page.record()`; unverified, treat as LOW |
| Browserbase | Automatic per session | Dashboard and API; MP4 download added July 2026 ([changelog](https://browserbase.com/changelog/mp4-session-recordings-download)) | Dashboard playback 0.5x to 4x; HLS replay API May 2026 ([changelog](https://www.browserbase.com/changelog/session-replay-streaming)) | Up to 10 concurrent tabs per session, one HLS playlist per tab | Cloud product. Replay is a hosted viewer, not a local file. rrweb DOM replay being deprecated, video kept |
| Cypress | Per spec file, not per test (training data, LOW) | Per spec, under `videos/` | Fixed by runner | Single-window focus | Per-test recording is a known user request; default is per spec |
| Charlotte MCP fork (user's prior tool) | Per test, toggled by config | Behaviour from user description only | ffmpeg-based | Not known | The target parity baseline: per-test enable/disable via config |

## Feature Landscape

### Table Stakes (Users Expect These)

Missing any of these makes the feature feel broken to a QA agent or a harness.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Opt-in on/off via env/config default | Recording costs disk and CPU; must never be on silently | LOW | `BLADE_RECORD=<dir>` as the default; off when unset |
| Per-test start/stop via a `state` parameter | Per-test-case evidence is the whole point; Playwright `page.screencast` and Puppeteer `page.screencast()` both use explicit start/stop | MEDIUM | Parameter on existing `state`. Schema change is "ask first" per AGENTS.md |
| Caller-chosen recording name | Must map a file to a test case ID without parsing | LOW | Sanitize for all three OSes (Windows reserved chars, length cap). Append a timestamp or short ID so reruns never overwrite |
| Predictable output path, returned in the tool result | Harness needs the path to attach or prune | LOW | Absolute path in the result. Matches the existing artifacts-with-path convention |
| Output format WebM default, MP4 optional | WebM plays anywhere and needs no H.264 encoder; Puppeteer and PROJECT.md both default to WebM | LOW | Format is an ffmpeg flag. Keep MP4 as a single option, not a matrix |
| Sane frame size and fps ceiling | Unbounded resolution makes large files; Playwright defaults to a fit-in-800x800 viewport, Puppeteer to 30 fps | MEDIUM | Capture at viewport, scale with aspect preserved, cap width. Screencast only emits on change, so fps is a ceiling, not a constant |
| Finalize on stop, tab close, and daemon shutdown | A truncated WebM is unplayable; a leaked ffmpeg process is a bug | MEDIUM | Flush ffmpeg stdin, wait for exit, and reap on `bladebro stop` and orphan cases |
| Graceful, honest failure when ffmpeg is missing or encoding fails | PROJECT.md requirement; the browsing run must continue | LOW | Verdict says "recording unavailable: ffmpeg not found" with recovery path. Doctor check reports it too |
| Browsing unaffected by recording | Recording must never change page behaviour or timing | MEDIUM | Screencast frame ack and ffmpeg backpressure must not stall the action path. Verify with `qol_probe` and timing numbers |
| Keep every recording, report path and size | Harness prunes; the driver cannot know the verdict | LOW | Result states path, bytes, duration. No automatic deletion |
| Stealth-neutral capture | Non-negotiable; a detector-visible change defeats the product | MEDIUM | Uses CDP `Page.startScreencast` only. No flags, no injected JS, no extra targets. Gate: audit 111/111, diff_oracle divergent 0 |
| Recording follows the tab the agent is driving | Popups and `target=_blank` are routine in real sites; a recording that stops at the first click is useless | MEDIUM | Playwright and Puppeteer record per page. For v1, either follow the active tab or write one file per page; pick one and document it |

### Differentiators (Competitive Advantage)

Not expected by a generic user, but they make the output usable by an LLM agent or a human reviewer.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Action timeline sidecar (JSON, timestamps per `act` and verdict) | Lets a reviewer jump to the failing step and lets the agent cite a timestamp instead of re-watching | MEDIUM | Derived from existing action verdicts and delta output. Cheap text, token-light. Depends on the verdict stream |
| Failure timestamp in the result | "Failed at 00:12.4" tells the harness and the human where to look | LOW | Only if the existing `blocked:` or failed verdict is already timestamped. Do not add a new parameter for it |
| Post-capture overlays (click markers, action captions) | Visible proof of what was clicked, without touching the page | HIGH | Render from the timeline sidecar with ffmpeg (drawtext or subtitle filter) after capture. Must NOT be injected DOM. This is the only stealth-safe route |
| Poster frame PNG on stop | Agent can `vision`-check the last state or give a human a thumbnail without decoding video | LOW | One extra frame written at stop. Reuses the existing artifact convention |
| Per-test segmentation under one session | Several test cases in one browser session each get their own file, keyed by name | MEDIUM | Falls out of start/stop per name. Needs clean stop-then-start with no frame loss between |
| Trim to a named range (start/stop markers) | Removes setup noise from a file | MEDIUM | Explicit start/stop already trims. Post-hoc `-ss`/`-to` trimming is a v1.x item only if users ask |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| A sixth tool (`record_start`, `record_stop`, `video_export`) | Feels cleaner than a parameter | Violates non-negotiable #1; tool definitions are the whole interface budget (~1,900 tokens billed per agent) | Parameter on `state`; defaults via env |
| Keep-only-on-failure retention in the driver | Saves disk; matches Playwright's `retain-on-failure` | Bladebro cannot compute pass/fail; the agent decides. A wrong guess deletes the only evidence | Keep all, report path; harness prunes |
| Injected DOM cursor or click-highlight overlays | Easiest way to show clicks in a video | Page-visible DOM change; detectors read it; violates the stealth contract | Post-capture overlays from the timeline sidecar |
| Bundling ffmpeg or adding an encoder crate | Removes an install step | Binary size (~7 MB shipped) and supply-chain cost; "ask first" | External ffmpeg, detected and reported honestly |
| rrweb-style DOM replay recording | Replayable, inspectable DOM | Large payloads, PII in DOM snapshots, and Browserbase is deprecating its rrweb path; heavy token cost | Video plus the existing console and network logs via `see` |
| Recording on by default | "Never miss evidence" | Silent disk growth, privacy (cookies and form data appear on screen), CPU cost on every run | Opt-in via env or per-call parameter |
| Returning video bytes inline to the LLM | Agent can "see" the run | Huge token tax on every user; models do not benefit from raw video frames | Return path and metadata; `vision` on the poster frame or a specific screenshot |
| Auto-speed-up or idle-cut editing | Shorter, prettier files | Changes the evidence; a sped-up or cut video can hide a flicker or a hang | Keep the raw capture; the timeline sidecar gives the navigation |
| Audio capture | Some QA tools offer it | Not useful for UI/UX QA here, adds an audio device dependency per OS | Out of scope |
| Live streaming or a live view URL | Watch an agent in real time | Large new surface (server, auth, port); exposes a live session; not needed for the evidence use case | Finished file at the end of the run |
| Automatic visual or pixel-diff judgement inside the driver | Looks like free QA | Bladebro has no LLM and must not claim verdicts it cannot prove; becomes a heuristic pass/fail | The agent judges; the driver supplies evidence |
| Automation flags to get cleaner captures (e.g. headless-only tricks, `--enable-automation`) | Seems to smooth capture | Explicitly forbidden: detectors read these and Chrome shows an infobar | CDP screencast without flags |

## Feature Dependencies

```
Screencast pipeline (CDP Page.startScreencast + ack + ffmpeg stdin)
    ├──required by──> Caller-chosen naming + output path
    │                     └──required by──> Result reporting (path, size, duration)
    ├──required by──> Finalize on stop / shutdown
    │                     └──required by──> Poster frame PNG
    ├──required by──> Per-test segmentation (stop-then-start)
    └──required by──> Multi-tab following
                          └──depends on──> existing tab/attach handling (src/page/tabs, attach)

ffmpeg detection ──required by──> Graceful failure verdict
                  └──enhances──> doctor check

Verdict stream (existing act results)
    └──required by──> Action timeline sidecar
                          ├──required by──> Failure timestamp
                          └──required by──> Post-capture overlays (HIGH)
                                                └──requires──> ffmpeg filter support

Trim to range ──enhances──> Per-test segmentation (only if post-hoc trim is added)
```

### Dependency Notes

- **Overlays require the timeline sidecar:** overlays are drawn from timestamps, so the sidecar must exist first. Overlays are the highest-cost differentiator and can be deferred without blocking the core.
- **Multi-tab following depends on tab handling that already exists:** do not rebuild attach logic; hook the screencast into the current active-target path.
- **Graceful failure conflicts with silent fallback:** if ffmpeg is missing, the run must say so in the verdict. A silent skip would break the honest-verdict rule.
- **Overlays conflict with stealth if done in DOM:** the only compatible route is post-capture.

## MVP Definition

### Launch With (v1)

- [ ] Opt-in via `BLADE_RECORD` and a per-call `state` parameter for start/stop with a caller name. Essential: the core per-test-case use case.
- [ ] WebM default, MP4 optional, sane size and fps cap. Essential: playable everywhere, bounded size.
- [ ] Predictable absolute path returned in the result, with size and duration. Essential: the harness and the human need it.
- [ ] Finalize on stop and shutdown; no orphan ffmpeg. Essential: broken files and leaked processes are the most likely bugs.
- [ ] Honest failure when ffmpeg is missing; doctor check. Essential: PROJECT.md requirement.
- [ ] Follow the active tab, or one file per page, documented. Essential: real sites open popups.
- [ ] Stealth gate green: audit 111/111, diff_oracle divergent 0, live readback recorded. Essential: non-negotiable.

### Add After Validation (v1.x)

- [ ] Action timeline sidecar. Trigger: QA agents report difficulty finding the failing step in a video.
- [ ] Poster frame PNG. Trigger: harnesses want thumbnails in reports.
- [ ] Failure timestamp in the result. Trigger: timeline sidecar exists and failures are timestamped.
- [ ] Post-hoc trim to a range. Trigger: users ask to remove setup noise beyond what start/stop gives.

### Future Consideration (v2+)

- [ ] Post-capture overlays (click markers, captions). Defer: highest complexity; needs the sidecar and ffmpeg filter validation on all platforms.
- [ ] Per-tab separate files as a switchable mode if v1 follows the active tab only. Defer: depends on real demand.
- [ ] Viewport and device emulation affecting capture size. Defer: already listed as out of scope in PROJECT.md.

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Opt-in env + per-test `state` start/stop | HIGH | MEDIUM | P1 |
| Caller-chosen name + predictable path in result | HIGH | LOW | P1 |
| WebM default, MP4 option, size/fps cap | HIGH | LOW | P1 |
| Finalize on stop and shutdown, no orphans | HIGH | MEDIUM | P1 |
| Honest ffmpeg-missing verdict + doctor check | HIGH | LOW | P1 |
| Active-tab following (or per-page files) | HIGH | MEDIUM | P1 |
| Stealth-neutral capture gate | HIGH | LOW (verification) | P1 |
| Action timeline sidecar | MEDIUM | MEDIUM | P2 |
| Poster frame PNG | MEDIUM | LOW | P2 |
| Failure timestamp in result | MEDIUM | LOW | P2 |
| Post-capture overlays | MEDIUM | HIGH | P3 |
| Post-hoc trim | LOW | MEDIUM | P3 |

**Priority key:**
- P1: Must have for v1 launch
- P2: Should have, add after validation
- P3: Nice to have, future consideration

## Recommended Choices for the Requirements Step

- Mirror Puppeteer and Playwright `page.screencast` semantics (explicit start/stop, `path`, WebM VP9 default, scale over crop) rather than the declarative `recordVideo` model. It fits per-test naming.
- Default frame cap in the Playwright range (fit-in-800 class) with an override. Keep the default small; evidence is for review, not archival quality.
- Keep the result compact: path, bytes, duration, and a one-line status. No inline video, no per-frame data.
- Put every visual annotation outside the page. If overlays are ever built, they are ffmpeg-side.

## Open Questions

- **Multi-tab policy:** follow the active tab, or one file per page? Playwright does the latter. Decide before the `state` parameter shape is final, since the schema change needs an upstream issue first.
- **Cypress per-test behaviour:** the claim that Cypress records per spec, not per test, comes from training data. Verify against current Cypress docs before quoting it in user-facing docs.
- **Playwright JS screencast signature:** the `Size` option and 1.59 API were confirmed only from the .NET reference. Check the JS/TS docs before relying on exact option names.
- **Puppeteer `page.record()` status:** one low-quality source says `page.screencast()` is obsolete. Not confirmed against official Puppeteer docs.
- **Transport:** the sidecar prototype only works on the WebSocket transport (`BLADE_TRANSPORT=pipe` exposes no port). The in-tree feature runs inside the daemon, so this matters only for the prototype.

## Sources

- Playwright video docs (.NET): https://playwright.dev/dotnet/docs/videos
- Playwright `Screencast` API reference (.NET): https://playwright.dev/dotnet/docs/api/class-screencast
- Puppeteer `page.screencast` API: https://pptr.dev/api/puppeteer.page.screencast
- Browserbase session replay streaming changelog (May 14, 2026): https://www.browserbase.com/changelog/session-replay-streaming
- Browserbase MP4 downloads changelog (July 15, 2026): https://browserbase.com/changelog/mp4-session-recordings-download
- Browserbase session recording docs: https://docs.browserbase.com/features/session-recording
- PROJECT.md (Bladebro Session Recording): `.planning/PROJECT.md`
- AGENTS.md non-negotiables (five tools, no LLM, no automation flags, stealth gates): `AGENTS.md`

---
*Feature research for: Bladebro session recording (agent-driven UI/UX QA)*
*Researched: 2026-10-21*
