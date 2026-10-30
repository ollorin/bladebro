# Project Research Summary

**Project:** Bladebro v4.6.5, milestone: opt-in session video recording
**Domain:** Agent-driven UI/UX QA evidence capture inside a Rust CDP browser driver (no LLM, five tools)
**Researched:** 2026-10-21
**Confidence:** MEDIUM

## Executive Summary

Bladebro needs a per-test video of what the agent did, so a QA harness can attach, review, or prune it. The established pattern (Playwright `page.screencast`, Puppeteer `page.screencast()`) is explicit start/stop per page, a caller-chosen path, and WebM encoded from compositor frames. Bladebro can match it with stock Chrome `Page.startScreencast` over the existing CDP session, JPEG frames piped to an external `ffmpeg`, and no new Rust crates. Everything is additive and opt-in, so the default path keeps identical CDP calls and the stealth gates (audit 111/111, diff_oracle divergent 0) should be unaffected.

Recommended approach: prove the risky parts first with a throwaway sidecar prototype, then build the in-tree module in this order: pure config and argument code, then the encoder process manager, then the frame source, then the hub wired into MCP/daemon/one-shot surfaces. The recorder must be owned by the surface loop and bound by target id, not stored on `Page`, because `Page::switch_tab` replaces the `Page` value and would kill the recording on every tab switch. Recordings go to a dedicated directory, not `~/.blade/artifacts/`, because rotation at 300 files would silently delete "keep everything" evidence.

Main risks are silent failures: a missing or late `screencastFrameAck` freezes the video with no error; blocking the CDP reader on the ffmpeg pipe deadlocks the session; skipping stdin EOF leaves an unfinalized WebM; blank video from occluded tabs looks like success. Stealth risk sits in any call that could change page-visible state (`Runtime.enable`, viewport overrides, `Target.activateTarget`/`Page.bringToFront` if they alter focus, DOM overlays). The verdict claims "recorded" only after a finished stop with ffmpeg exit 0 and a non-empty file. Open design points for the prototype and schema issue: `state` schema shape, multi-tab policy, timestamp strategy, codec default, fps cap.

## Key Findings

### Recommended Stack
- CDP `Page.startScreencast` / `screencastFrame` / `screencastFrameAck` / `stopScreencast` on the existing page session. No extension, flags, or injected JS.
- JPEG frames (q60-80, bounded `maxWidth`/`maxHeight`), never PNG.
- External `ffmpeg` (min tested 5.1 for `-fps_mode`; test 8.x and 9.x) via `-f image2pipe -c:v mjpeg` on stdin. WebM default (VP9 realtime; VP8 fallback), MP4/libx264 opt-in.
- Discovery in `src/platform.rs`: `BLADE_FFMPEG`, then PATH, then known install paths; probe `-version`/`-encoders` once, cache, report in `doctor`. No auto-install, download, or bundling.
- Rejected: `HeadlessExperimental.beginFrame` (automation flag), x11grab/avfoundation/gdigrab (whole-screen), `chrome.tabCapture` (extension), rav1e/openh264/vpx-sys/image/gif crates (size, supply chain, C toolchain, wrong format), polling `captureScreenshot` (fallback only).

### Expected Features
**Table stakes:** opt-in via `BLADE_RECORD=<dir>` env default and per-call `state` start/stop with a caller name; sanitized, collision-proof names with timestamp or short-ID suffix; absolute path returned in the result; WebM default, MP4 optional; sane size and fps ceiling; finalize on stop, tab close, daemon shutdown, `bladebro stop`; no orphan ffmpeg; honest ffmpeg-missing failure with browsing continuing; keep all recordings and report path/bytes/duration; stealth-neutral capture; active-tab following so popups don't silently end the recording.
**Differentiators (v1.x):** action timeline sidecar JSON; failure timestamp; poster frame PNG; post-hoc trim if asked.
**Defer (v2+):** post-capture overlays via ffmpeg filters from the timeline (never DOM); per-tab files mode; viewport emulation.
**Do not build:** sixth tool; keep-only-on-failure retention; injected DOM cursor/highlights; bundled ffmpeg; rrweb replay; recording on by default; inline video bytes; auto speed-up or idle cut; audio; live stream URL; pixel-diff judgement; automation flags.

### Architecture Approach
New `src/record/` module: `config.rs`, `session.rs` (RecordHub), `source.rs`, `encoder.rs`, `report.rs`, plus `record.rs` header. Upstream edits limited to hook lines in `mcp/server/state.rs`, teardown calls in `mcp/server/serve.rs`, `cli/daemon.rs`, `cli.rs`, and helpers in `platform.rs`.
Data flow: source task receives `Page.screencastFrame`, immediately acks on the same session, base64-decodes, `try_send`s JPEG bytes into a bounded channel (~64 frames, drop-newest with counter). Encoder task drains into ffmpeg stdin; stderr is a bounded tail; stdout null. Stop: `Page.stopScreencast`, drop producer, close stdin, bounded wait, kill as last resort and mark unfinalized. Tab switch: `rebind(new_target)` stops and restarts screencast on the new target, keeping the same ffmpeg process and file.
Schema: Option A (recommended) two new `state` ops `record-start`/`record-stop` reusing `name`; Option B (object property) rejected for token cost. Both ask-first; open upstream issue before in-tree PR.

### Critical Pitfalls
1. Forgotten or late ack freezes video silently. Ack on receipt, using the event's `sessionId`, on the CDP transport directly. Count frames vs acks.
2. Blocking CDP reader on ffmpeg pipe deadlocks and grows memory. Reader only acks and enqueues; bounded queue; writer task; drop counter surfaced in verdict.
3. Variable frame rate gives wrong speed. Use wallclock timestamps with `-fps_mode vfr`, or concat durations from `metadata.timestamp`; never a guessed `-r`. Acceptance: duration within 5% of wall clock on an idle page.
4. Size changes and odd dimensions break ffmpeg. Even-dimension `scale`, bounded max size, log device size changes; test 1001x641.
5. Wrong kill: unfinalized WebM, zombies. Close stdin and wait ~10 s before any kill; always `wait()`; finalize on SIGTERM, Ctrl-C, `bladebro stop`; extend `session_profile/reap.rs` for ffmpeg children.
6. Stealth regression from helpful calls (`Runtime.enable`, `Emulation.*`, overlays, launch flags). Only the three screencast methods; no viewport changes; no DOM indicator; CDP trace check.
7. Occluded/background tabs yield no frames. Detect "no frames for N seconds on loaded page" and report it; measure any focus call before use; no occlusion launch flags without diff_oracle.
8. Caller name as path (traversal, leading `-`, drive/UNC). Sanitize to `[A-Za-z0-9._-]`, reject leading `-`/`.`, `PathBuf::join` under canonical dir, `create_new`, argv arrays only; tests on all three OSes.
9. Sensitive content kept forever. 0600 files, 0700 dir; Windows ACL decision; opt-in; docs warning; never log payloads.
10. Dishonest verdicts. Success text only after finished stop with exit 0 and non-empty file; otherwise "unfinalized".

## Implications for Roadmap

**Phase 0: Decisions and upstream issues (no code).** Open the `state` op schema issue (Option A); decide dedicated recordings dir (`BLADE_RECORD`, default `~/.blade/recordings/`); decide multi-tab policy; mark codec and fps defaults provisional. Ask-first items need feedback first.

**Phase 1: Sidecar prototype (throwaway, `tools/record_sidecar/`, not shipped).** Second CDP client on a page target over WS (`--remote-debugging-port`), enable only `Page`, screencast with per-frame ack, pipe to ffmpeg, follow tab switches. Measure: effective fps; ack latency; drops under slow consumer; playability after EOF; mid-recording viewport change; whether bladebro's own events keep flowing; frame count equals ack count over 60 s; duration within 5% of wall clock on idle page; output across a tab switch; headed vs headless frames; Chrome 151 behavior; `Page.enable` impact on audit and diff_oracle. Exit gate: playable 30 s WebM across a tab switch with numbers recorded. Output decides in-tree (a) on the existing `Page` session (preferred) or (b) second client only if (a) conflicts.

**Phase 2: Pure in-tree pieces.** `record/config.rs` (precedence, sanitization), `encoder.rs::ffmpeg_args()`, `report.rs` verdicts, `platform::find_ffmpeg()`, `doctor` reporting path and version. Tests: traversal and name regressions on all three OSes. Can run in parallel with Phase 1.

**Phase 3: Encoder process manager.** argv-array spawn; stdin writer; bounded stderr tail; stdout null; EOF finalize with bounded wait; kill last resort marking unfinalized; Windows no-console flag; missing-binary and non-zero-exit handling. Needs research: Windows process lifecycle (no SIGINT; `taskkill` status discarded in `platform.rs`) and a cross-OS fake-encoder test helper.

**Phase 4: Frame source on a real page.** `record/source.rs`: ack-first, bounded `try_send`, drop and lag counters, dedicated or early-handled path so frames don't compete on the shared broadcast. Unit test: `shrink_oversized_event` passes `Page.screencastFrame` through unchanged at 2 MB. Gate: frames flow on fixture server; slow consumer shows counted drops; `tools/qol_probe/run.py` still 122/122. Needs research on `src/cdp/client.rs` broadcast handling.

**Phase 5: RecordHub and surface hooks.** `record/session.rs` state machine; `record-start`/`record-stop` ops (after schema feedback); rebind on `switch-tab`/`open-tab` into the same file with a segment marker; one active recording per session (second start errors naming the active file); `finish()` on every exit path (MCP teardown, idle and crash relaunch, daemon shutdown, `bladebro stop`, one-shot CLI before `Browser::shutdown()`). Needs research: popup and cross-origin swap fixtures; relaunch points in `mcp/server/serve.rs`.

**Phase 6: Hardening and verification gate.** 0600 files and 0700 dir; documented Windows ACL decision; free-space check reusing `df` logic from `updater/download.rs` with "stopped: disk low" verdict; orphan-process integration test; size-per-minute numbers per size/fps; README sensitive-content warning. Gates: clippy `-D warnings`; tests on ubuntu, macos, windows; audit 111/111; diff_oracle divergent 0; `tools/lane_matrix.py all 5`; live sites (sannysoft, incolumitas, creepjs) with recording on; CDP trace shows no new `Runtime.*` or `Emulation.*`.

**Post-MVP:** v1.x: timeline sidecar, poster frame, failure timestamp, trim. v2+: overlays via ffmpeg filters (never DOM); per-tab files mode.

**Phase ordering rationale:** Phase 1 answers architecture-changing questions (second client, ack, timing, occlusion, `Page.enable`) before in-tree rework. Phases 2 and 3 are browser-independent and carry the most silent-failure risk, testable with a fake encoder. Phase 4 isolates backpressure from lifecycle. Phase 5 touches shared surface loops and depends on the schema decision. Phase 6 makes stealth and cross-platform gates an explicit exit.

**Research flags:** Needs measurement or research: Phase 1 (Chrome 151 behavior not verified against docs), Phase 3 (Windows process lifecycle), Phase 4 (broadcast and `shrink_oversized_event`), Phase 5 (popups, site isolation, relaunch points, schema). Standard: Phase 2 (pure config and strings), Phase 6 (permissions, gates; document headed-occlusion results per platform).

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM | CDP behavior from long-stable knowledge, not verified against Chrome 151 docs or a live run; ffmpeg flags not checked against configure output; Homebrew deps confirmed. |
| Features | MEDIUM | Playwright/Puppeteer MEDIUM; Browserbase changelog MEDIUM; Cypress per-spec LOW (training data); Charlotte fork LOW (user description). |
| Architecture | HIGH for codebase integration, MEDIUM for CDP runtime | Integration points read from source (`switch_tab`, artifact rotation at 300, `base64` present); second-client, rebind, and pipe behavior unproven until Phase 1. |
| Pitfalls | MEDIUM | Repo facts verified (broadcast fan-out, `validate_write_path` denylist, `taskkill` discarded, reaper location); CDP and ffmpeg behavior to be reproduced in sidecar. |

**Overall: MEDIUM.** Direction and integration points are solid; protocol and encoder timing need the Phase 1 prototype before in-tree commitment.

### Gaps to Address
- **Schema shape (A vs B) and op names:** resolve via upstream feedback in Phase 0.
- **Multi-tab policy:** FEATURES asks for a choice; ARCHITECTURE rebinds into one file; PITFALLS requires segment markers, no silent merge. Recommended: one file per recording that follows the active tab, with each switch recorded as a segment boundary in the verdict. Confirm in Phase 0/1.
- **Recordings dir:** PITFALLS says "per-session directory under artifacts", which conflicts with ARCHITECTURE's rotation warning. Recommended: dedicated dir, never rotated. Decide in Phase 0.
- **Timestamp strategy:** STACK prefers wallclock + `-fps_mode vfr`; PITFALLS prefers concat from `metadata.timestamp` (semantics unverified). Decide by measurement in Phase 1.
- **Codec default:** VP9 (STACK) vs VP8-or-VP9 by prototype (ARCHITECTURE). Measure CPU on CI runners.
- **FPS cap:** 5 to 15 fps across documents. Pick from measured size-per-minute in Phase 1.
- **Chrome floor:** STACK cites "baseline Chrome 151; floor 154 per SUPPORT-MATRIX". Verify against the repo's support matrix before docs state a minimum.
- **Occlusion/focus:** whether `Target.activateTarget` or `Page.bringToFront` is needed and whether it changes page-visible focus. Measure with diff_oracle first.
- **`Page.enable` / event-bus impact:** unmeasured; first gate.
- **Pipe transport:** no port, so no sidecar; in-tree source must work on pipe or the verdict states the limit. Decide in Phase 4.
- **Headless/Xvfb cadence:** matrix must record which modes produce frames.
- **Windows ACLs:** no ACL handling in `src/`; explicit decision needed.

## Sources
- Codebase (read directly): `src/page/tabs.rs`, `src/page/mod.rs`, `src/browser.rs`, `src/artifacts.rs`, `src/cdp/client.rs`, `src/cdp/session.rs`, `src/mcp/tools.rs`, `src/mcp/server/state.rs`, `src/mcp/server/serve.rs`, `src/platform.rs`, `src/browser/flags.rs`, `Cargo.toml`, `AGENTS.md`.
- Secondary: Playwright video and screencast docs (.NET); Puppeteer `page.screencast` API and ScreenRecorder reference; Browserbase changelogs (May and July 2026); Homebrew ffmpeg formula; community screencast ack reports (browser-use/browsercode#124, browserless blog); CDP `Page` reference (not fetched live).
- Tertiary (LOW): Cypress per-spec behavior (training data); Charlotte fork behavior (user description); Puppeteer `page.record()` status (unconfirmed).

*Research completed: 2026-10-21. Ready for roadmap: yes.*
