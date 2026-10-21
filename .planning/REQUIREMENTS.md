# Requirements: Bladebro Session Recording

**Defined:** 2026-10-21
**Core Value:** A QA run with recording enabled always leaves a playable video file at a predictable path that the tool result reports, without touching the stealth path or breaking Bladebro's five-tool contract.

## v1 Requirements

Requirements for the first milestone. Each maps to a roadmap phase.

### Control & Configuration

- [ ] **CTRL-01**: With recording not enabled, Bladebro sends the same CDP calls as today (no screencast, no new domains) and creates no recording files
- [ ] **CTRL-02**: User can enable recording and set the output directory with `BLADE_RECORD=<dir>`; unset means off
- [ ] **CTRL-03**: User can persist recording settings in a config file under `BLADE_HOME`, with precedence per-call parameter > env var > config file > built-in defaults
- [ ] **CTRL-04**: Agent can start a named recording with a `state` `record-start` op and stop it with `record-stop`, without adding a sixth tool
- [ ] **CTRL-05**: A second `record-start` during an active recording returns an error that names the active file
- [ ] **CTRL-06**: User can choose the output format (WebM default, MP4 optional) via config, env or per-call parameter
- [ ] **CTRL-07**: User can configure maximum frame size and frame rate, with defaults chosen from prototype measurements

### Capture

- [ ] **CAPT-01**: The recorder captures frames with CDP `Page.startScreencast` as JPEG and acknowledges every `screencastFrame` immediately on receipt
- [ ] **CAPT-02**: Capture never blocks the CDP reader: frames go through a bounded queue, newest frames are dropped under backpressure, and drops are counted
- [ ] **CAPT-03**: In the default mode one recording follows the active tab: `switch-tab` and popups rebind capture into the same file, and each switch is reported as a segment boundary
- [ ] **CAPT-04**: User can opt in to a per-tab-files mode that writes one file per tab
- [ ] **CAPT-05**: If a loaded page produces no frames for a configurable period, the result says so instead of reporting a blank video as success

### Output & Reporting

- [ ] **OUT-01**: Bladebro finds ffmpeg via `BLADE_FFMPEG`, then PATH, then known install locations, probes version and encoders once, and caches the result
- [ ] **OUT-02**: `bladebro doctor` reports the ffmpeg path, version and available WebM/MP4 encoders
- [ ] **OUT-03**: Video duration matches wall-clock time within 5% on an idle page, and odd or changing frame sizes still produce a playable file
- [ ] **OUT-04**: Recordings are written to a dedicated directory (default `~/.blade/recordings/`, never auto-rotated) with unique, sanitized file names
- [ ] **OUT-05**: `record-stop` returns the absolute path, size in bytes, duration, frame count, dropped-frame count and segment boundaries
- [ ] **OUT-06**: The result includes a poster frame (PNG) path, and the agent can pass an optional marker (such as a failure note) whose time offset is recorded
- [ ] **OUT-07**: Bladebro writes an action timeline sidecar (JSON) aligned to the video, built from existing action verdicts

### Lifecycle & Reliability

- [ ] **LIFE-01**: Recordings are finalized on `record-stop`, tab close, daemon shutdown or idle timeout, `bladebro stop`, one-shot CLI exit and termination signals, by closing ffmpeg's stdin and waiting with a bounded timeout
- [ ] **LIFE-02**: A browser crash or relaunch mid-recording produces an honest "interrupted/unfinalized" result and the partial file is kept
- [ ] **LIFE-03**: No ffmpeg process is left running after normal exits or crashes, via the reaper in `src/session_profile/reap.rs`
- [ ] **LIFE-04**: Recording stops with an honest "disk low" result when free space falls below a threshold, and size-per-minute numbers are documented
- [ ] **LIFE-05**: Finalization works on Windows (stdin-EOF path, no console window) and is covered by the `windows-latest` CI job

### Safety & Honesty

- [ ] **SAFE-01**: Recording uses only the `Page` screencast methods: no `Runtime.enable`, no `Emulation.*`, no DOM overlays, no new launch flags, and `bladebro audit` stays 111/111 with `diff_oracle` divergent 0
- [ ] **SAFE-02**: Caller-supplied names are sanitized to a safe basename, created with `create_new` under the canonical recordings directory, and ffmpeg is invoked with an argv array
- [ ] **SAFE-03**: Files are created 0600 in a 0700 directory on Unix, the Windows ACL decision is documented, and the README warns that recordings can contain sensitive content
- [ ] **SAFE-04**: Success is reported only after a finished stop with ffmpeg exit 0 and a non-empty file; a missing ffmpeg or encoder yields an honest error and browsing continues unaffected

### Verification & Fork Sync

- [ ] **VER-01**: A sidecar prototype in `tools/record_sidecar/` demonstrates capture across a tab switch with recorded measurements (fps, ack latency, drops, duration drift) and is kept as a supported probe
- [x] **VER-02**: The `state` op shape, recordings directory and multi-tab policy are decided by us and recorded in a decision record before any schema code is written (no upstream approval needed; the fork is the main repo)
- [ ] **VER-03**: Tests cover config precedence, name sanitization, ffmpeg arguments, verdict strings, fake-encoder lifecycle, and a 2 MB `Page.screencastFrame` passing through `shrink_oversized_event` unchanged
- [ ] **SYNC-01**: Upstream updates keep merging cleanly into the fork: a documented, scripted procedure merges `upstream/main`, the recording code is additive (new `src/record/` module plus a short, listed set of hook lines), and a trial merge plus the instrument gate passes after each merge
- [ ] **VER-04**: The full instrument gate passes: clippy `-D warnings`, `cargo test --release` on ubuntu/macos/windows, audit 111/111, diff_oracle divergent 0, `qol_probe` 122/122, `lane_matrix`, and the live stealth sites with recording on

### Integration

- [ ] **INT-01**: Documentation shows how the Claude Code GSD flow (`gsd-charlotte-qa`, `gsd-test-writer`) enables recording per test case
- [ ] **INT-02**: Documentation shows how the DSH plugin flow enables recording per test case

## v2 Requirements

Deferred to future milestones. Tracked but not in the current roadmap.

### QA Gaps

- **QAX-01**: Viewport/device emulation
- **QAX-02**: Network throttling
- **QAX-03**: Accessibility, SEO and performance audits

### Recording Extras

- **RECX-01**: Post-capture overlays (cursor, click highlights) drawn by ffmpeg filters from the timeline, never in the DOM
- **RECX-02**: Post-hoc trimming of recordings
- **RECX-03**: Recording over the pipe transport (`BLADE_TRANSPORT=pipe`) for the sidecar prototype

## Out of Scope

| Feature | Reason |
|---------|--------|
| A sixth MCP tool | AGENTS.md non-negotiable #1 |
| Keep-only-on-failure retention in the driver | Bladebro cannot compute pass/fail; the harness prunes |
| Injected DOM cursor or highlights | Page-visible change; stealth |
| Bundling ffmpeg or adding an encoder crate | New dependency / binary size; ffmpeg stays an external binary |
| Recording on by default | Privacy and stealth; must be opt-in |
| Inline video bytes in tool results | Token cost |
| Audio capture, live-view URL, rrweb DOM replay | Not needed for QA evidence |
| Auto speed-up / idle-cut editing, pixel-diff verdicts | Out of the driver's role |
| Updater, npm packaging, version bumps, releases | Left as upstream has them to keep merges clean (the fork's updater still points at upstream releases; revisit separately) |
| Rewriting the `gsd-charlotte-qa` persona | Separate follow-up in the get-shit-done repo |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| CTRL-01 | Phase 7 | Pending |
| CTRL-02 | Phase 3 | Pending |
| CTRL-03 | Phase 3 | Pending |
| CTRL-04 | Phase 7 | Pending |
| CTRL-05 | Phase 7 | Pending |
| CTRL-06 | Phase 3 | Pending |
| CTRL-07 | Phase 3 | Pending |
| CAPT-01 | Phase 6 | Pending |
| CAPT-02 | Phase 6 | Pending |
| CAPT-03 | Phase 8 | Pending |
| CAPT-04 | Phase 8 | Pending |
| CAPT-05 | Phase 6 | Pending |
| OUT-01 | Phase 4 | Pending |
| OUT-02 | Phase 4 | Pending |
| OUT-03 | Phase 6 | Pending |
| OUT-04 | Phase 3 | Pending |
| OUT-05 | Phase 7 | Pending |
| OUT-06 | Phase 10 | Pending |
| OUT-07 | Phase 10 | Pending |
| LIFE-01 | Phase 9 | Pending |
| LIFE-02 | Phase 9 | Pending |
| LIFE-03 | Phase 9 | Pending |
| LIFE-04 | Phase 9 | Pending |
| LIFE-05 | Phase 5 | Pending |
| SAFE-01 | Phase 6 | Pending |
| SAFE-02 | Phase 3 | Pending |
| SAFE-03 | Phase 3 | Pending |
| SAFE-04 | Phase 5 | Pending |
| VER-01 | Phase 2 | Pending |
| VER-02 | Phase 1 | Complete |
| SYNC-01 | Phase 12 | Pending |
| VER-03 | Phase 12 | Pending |
| VER-04 | Phase 12 | Pending |
| INT-01 | Phase 11 | Pending |
| INT-02 | Phase 11 | Pending |

**Coverage:**
- v1 requirements: 35 total
- Mapped to phases: 35
- Unmapped: 0 ✓

---
*Requirements defined: 2026-10-21*
*Last updated: 2026-10-21 after roadmap creation*
