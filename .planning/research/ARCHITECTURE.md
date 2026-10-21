# Architecture Research: Session Video Recording

**Domain:** Opt-in CDP screencast → ffmpeg recorder inside an existing Rust browser driver (bladebro v4.6.5)
**Researched:** 2026-10-21
**Confidence:** HIGH for codebase integration points (read from source); MEDIUM for CDP screencast behavior under multiple clients and tab switches (must be proven by the sidecar prototype, not assumed)

## Standard Architecture

### System Overview

```
┌──────────────────────────────────────────────────────────────────────────┐
│ Surfaces (unchanged except one parameter path)                           │
│  MCP serve loop (serve.rs)   CLI one-shot (cli.rs)   daemon (daemon.rs)  │
│        │ state{op,record…}        │                       │              │
├────────┴──────────────────────────┴───────────────────────┴──────────────┤
│ Tool handler: src/mcp/server/state.rs  (+ thin hook: record ops)         │
├──────────────────────────────────────────────────────────────────────────┤
│ NEW  src/record/        RecordHub (owned by surface loop, NOT by Page)   │
│  ├─ config.rs      resolve dir/format/fps/name (arg > env > off)         │
│  ├─ session.rs     lifecycle state machine + target binding + finish()   │
│  ├─ source.rs      frame receiver: Page.screencastFrame → bounded chan   │
│  │                 + immediate Page.screencastFrameAck (pacing)          │
│  ├─ encoder.rs     ffmpeg process manager (stdin pipe, stderr tail,      │
│  │                 EOF-finalize, bounded wait, Windows no-console)       │
│  └─ report.rs      result text + honest verdict strings                  │
├──────────────────────────────────────────────────────────────────────────┤
│ Existing: Page (page/mod.rs) → CdpSession → CdpClient (cdp/)             │
│ Existing: Browser (browser.rs) Drop / shutdown()                         │
│ Existing: platform.rs (ffmpeg lookup, secure write, Windows spawn flags) │
└──────────────────────────────────────────────────────────────────────────┘
          │ stdin (JPEG frames, image2pipe)            ▲ exit status / stderr
          ▼                                            │
        ffmpeg (external binary, not a crate)  ────────┘
          │
          ▼
  <recordings dir>/<name>-<pid>-<seq>.webm   (NOT under artifacts/)
```

### Component Responsibilities

| Component | Responsibility | Communicates With | Typical Implementation |
|-----------|----------------|-------------------|------------------------|
| `record::config` | Resolve on/off, output dir, format (webm default), fps, file name. Pure functions, no I/O beyond env read | `state` handler, `session` | Pure struct + `resolve(args, env)` returning `Result`; sanitizes `name` |
| `record::session` (RecordHub) | Lifecycle: Idle → Starting → Recording(target, file) → Finalizing → Done/Failed. Rebinds on tab switch. Owns the source task and encoder | config, source, encoder, report, surface loop | State machine held in the MCP/daemon/one-shot loop next to the `Page` handle |
| `record::source` | Subscribes to `Page.screencastFrame` events for one target session; decodes base64; forwards frame bytes; acks every frame | `CdpSession` (existing session or sidecar client), encoder channel | tokio task + `broadcast::Receiver` filtered by `sessionId` + bounded `mpsc` |
| `record::encoder` | Spawns `ffmpeg` with image2pipe input, writes frames to stdin, captures stderr tail, closes stdin to finalize, bounded wait then reap | `platform` (binary lookup, spawn flags), `source` channel | `tokio::process::Command` or `std::process` on a blocking task |
| `record::report` | Produces the one-line result (path, duration, frames, format) or the honest failure verdict | session, encoder | Strings only; no extra JSON fields by default |
| `page::Page` (existing) | Unchanged responsibilities. Exposes the target's `CdpSession` (`cdp_ref()`) | source (read-only use of session) | — |
| `platform.rs` (existing, extended) | `find_ffmpeg()`, Windows `CREATE_NO_WINDOW` for child spawn, path helpers | encoder | Cfg-gated helpers, per AGENTS.md non-negotiable #9 |

## Recommended Project Structure

```
src/
├── record.rs              # Module map header + re-exports (public surface)
├── record/
│   ├── config.rs          # resolve(args, env) -> RecordConfig; name sanitizing
│   ├── session.rs         # RecordHub state machine, start/stop/rebind/finish
│   ├── source.rs          # screencast frame receiver + ack pacing
│   ├── encoder.rs         # ffmpeg spawn/stdin/finalize; ffmpeg_args() builder
│   └── report.rs          # verdict + result text
├── page/...               # UNCHANGED except nothing (recorder reads cdp_ref())
├── mcp/server/state.rs    # +1 hook: record ops routed to RecordHub
├── mcp/server/serve.rs    # +hold RecordHub next to Page; finish() on teardown/idle/crash
├── cli/daemon.rs          # +same finish() hook on shutdown
├── cli.rs                 # +one-shot: finish() before Browser::shutdown
└── platform.rs            # +find_ffmpeg(), spawn flag helper

tools/record_sidecar/      # THROWAWAY prototype (see Build Order); not shipped in the crate
```

### Structure Rationale

- **`src/record/` as a new module:** every recorder line lives in files that upstream does not touch. The only upstream-owned edits are the hook lines in `state.rs`, the serve/daemon/one-shot teardown calls, and `platform.rs` helpers. This keeps the merge surface to a handful of lines per file (PROJECT.md "additive diff" constraint).
- **Owner is the surface loop, not `Page`:** `Page::switch_tab` does `*self = new_page` (src/page/tabs.rs), which drops the old `Page` and aborts its background tasks. A recorder stored on `Page` would die on every `switch-tab`. The recorder must outlive individual `Page` values and re-bind to the new target.
- **Recordings dir is not `~/.blade/artifacts/`:** `artifacts::write_artifact` calls `rotate_artifacts`, which deletes `blade-*` files beyond 300 (src/artifacts.rs). A video kept "always" (PROJECT.md) would be silently deleted by unrelated tool calls. Use a dedicated directory (`BLADE_RECORD` value, default `~/.blade/recordings/`), reusing `platform::secure_create_dir_all` and `secure_write_file` semantics.
- **Encoder isolated from frame source:** the ffmpeg process can fail (missing binary, codec absent, disk full) without touching the CDP session. Failure must degrade to an honest verdict while browsing continues (PROJECT.md core value).

## Data Flow

### Frame path (steady state)

1. `source` task receives `Page.screencastFrame { data (base64 JPEG), metadata {timestamp, deviceWidth, deviceHeight}, sessionId }` from the CDP broadcast.
2. It immediately sends `Page.screencastFrameAck { sessionId }` on the **same session**. Ack is decoupled from encoding: acking only after ffmpeg consumes a frame would couple Chrome's frame production to encoder speed and stall the page's compositor path.
3. Decode base64 (the `base64` crate is already a dependency), `try_send` raw JPEG bytes into a bounded channel (e.g. 64 frames). On full channel: drop the frame and increment a counter (drop-newest). Never block the CDP reader task; never grow unbounded.
4. `encoder` writer task drains the channel into ffmpeg stdin. Input is `-f image2pipe -c:v mjpeg -use_wallclock_as_timestamps 1`, so the output keeps real elapsed time when frames are sparse (screencast only emits on change).
5. ffmpeg outputs WebM (VP8 or VP9, decided by the prototype against the ffmpeg build on the test machines). Format `mp4` is optional and must report honestly if its encoder is absent.

### Control path

1. Agent calls `state` with a record parameter (see Schema decision below). Handler validates args before touching the browser (existing `validate.rs` pattern).
2. `RecordHub::start(page_target_id, cfg)`: resolve config, spawn ffmpeg, start `Page.startScreencast` on the target session, spawn `source` task. Return `recording: <path>` (or the honest failure) in the tool result.
3. `RecordHub::stop()`: `Page.stopScreencast`, abort/join `source`, drop encoder stdin (EOF), wait up to a bounded timeout for ffmpeg to finalize (WebM cues written on clean EOF; SIGKILL leaves an unseekable/truncated file), read stderr tail, produce report.
4. `switch-tab` / `open-tab` (surface loop replaces `Page`): `RecordHub::rebind(new_target_id)` stops the screencast on the old session and starts it on the new one, **keeping the same ffmpeg process and file**. This yields one video that follows the agent across tabs.
5. Teardown: stdin EOF, signal, idle timeout and crash relaunch all call `RecordHub::finish()` before `Browser::shutdown()`. `Drop` is a last resort only (kill, mark file as possibly truncated in the next report).

### Config resolution (precedence)

`state` per-session arg (`record`/`name`) > env `BLADE_RECORD` (directory; enables default recording for every session) > off. `BLADE_RECORD_FORMAT` (webm default) and optional fps are env-only. No new config file: the codebase has none for this surface and a file would be a second source of truth.

## Schema Decision (ask-first)

Changing MCP tool schemas is "ask first" (AGENTS.md). Two shapes, both within the five-tool rule:

| Option | Shape | Tokens added | Verdict |
|--------|-------|--------------|---------|
| A | Two new `state` ops: `record-start`, `record-stop`, reusing the existing `name` field for the file name | ~2 enum values + one line of description | **Recommended** — mirrors existing `save`/`load`/`open-tab` style, no new property |
| B | New optional `record` object property on `state` | Larger object schema billed on every call | Reject |

Open the upstream issue with Option A before the in-tree PR; the sidecar prototype does not need the schema change.

## Architectural Patterns

### Pattern 1: Recorder owned by the surface loop, bound by target id

**What:** `RecordHub` lives beside `Page` in `serve.rs` / `daemon.rs` / one-shot. It stores a target id and an encoder, not a reference to a `Page`.
**When to use:** any state that must survive `switch-tab`, crash relaunch or idle relaunch.
**Trade-offs:** one extra field and a few hook calls in the loop (small diff); the loop must call `rebind` consistently. Forgetting one call site leaves a stale source, which the integration test must cover.

### Pattern 2: Ack-first, bounded, drop-newest frame pump

**What:** ack on receipt, `try_send` into a bounded channel, count drops, report the drop count in the verdict.
**When to use:** any producer that Chrome paces by acks and a consumer that can be slower (ffmpeg).
**Trade-offs:** dropped frames show as stutter, not failure. The verdict must say "N frames dropped" so the agent knows the video is lossy.

### Pattern 3: External process with EOF finalization

**What:** ffmpeg is a child with a piped stdin; finalization is "close stdin, wait with a bound, then kill".
**When to use:** any media container that needs a clean trailer.
**Trade-offs:** on Windows there is no SIGINT path to ffmpeg, so EOF is the only clean signal (good, it is cross-platform). If bladebro is SIGKILLed, ffmpeg sees a broken pipe and exits with a truncated file; the next launch's reaper (`session_profile/reap.rs`) should at least log it (prototype must confirm the file is still playable).

## Build Order (risk-minimizing)

Each step has an exit gate. Do not start the next step until the gate is met.

**Step 0 — Open issue (no code).** File the upstream issue for the `state` op shape (Option A) and the recordings-dir decision. Ask-first items get feedback before code exists.

**Step 1 — Sidecar prototype (throwaway, `tools/record_sidecar/`, not shipped).** A standalone binary/script that:
- reads the browser WS URL from `/json/version` on a `--remote-debugging-port` launch (WS transport only; pipe transport has no port, so sidecar is impossible there);
- attaches a **second** `CdpClient` to a page target with `Target.attachToTarget { flatten: true }`;
- enables only the `Page` domain (do **not** enable `Runtime` on the sidecar: `Runtime.enable` is a known detection signal);
- runs `Page.startScreencast` (jpeg, quality ~80, optional `everyNthFrame`), acks each frame, pipes to ffmpeg;
- follows tab switches by listening for `Target.targetCreated`/`targetInfoChanged` and re-attaching;
- measures: effective fps, ack latency, frame drops under a slow consumer, file playability after EOF, behavior when the viewport size changes mid-recording (image2pipe may reject size changes, so a scale filter may be needed), and whether bladebro's own session keeps receiving events while the sidecar records.

**Exit gate:** a playable WebM for a 30 s session across a tab switch, with the four measured numbers recorded. Decision output: (a) in-tree frame source = the existing `Page` session (preferred, no second client), or (b) sidecar-style second client (only if (a) cannot coexist with bladebro's own `Page` events). The sidecar answers this question; it is not the shipped design by default.

**Step 2 — Pure in-tree pieces (`record/config.rs`, `encoder.rs::ffmpeg_args`, `report.rs`).** No browser, no process. Unit tests: env/arg precedence, name sanitizing (no path traversal; `validate_write_path` is a denylist with known gaps, so sanitize the name to `[A-Za-z0-9._-]` rather than rely on it), ffmpeg argument construction per format, verdict strings. Lowest risk; merges first.

**Step 3 — Encoder process manager (`record/encoder.rs`).** Spawn/stdin/finalize/kill with a fake encoder in tests (use a small cross-platform test helper, or cfg-gate Unix-only tests and say so). Verify: EOF finalization path, bounded wait, stderr tail capture, missing-binary verdict (ffmpeg absent must return honest error and not panic). Add `platform::find_ffmpeg()` and the Windows no-console spawn flag here.

**Step 4 — Frame source (`record/source.rs`) against a real page.** Integration test with the local fixture server (tools/qol_probe pattern, scratch `BLADE_HOME`): start screencast on an existing `Page` session, assert frames arrive, acks are sent, drop counter works with a deliberately slow consumer. Gate: frames flow and bladebro's own actions (`act`, `see`) still pass `tools/qol_probe/run.py` 122/122.

**Step 5 — Session/hub and surface hooks (`record/session.rs`, `state.rs`, `serve.rs`, `daemon.rs`, `cli.rs`).** Wire start/stop, rebind on tab switch, finish on every teardown path. Only after the Step 0 schema issue has feedback. Gate: tests for switch-tab rebinding into the same file, idle-shutdown finalization, crash-relaunch verdict, and the one-shot CLI path finalizing before exit.

**Step 6 — Verification instruments (mandatory before PR).** `bladebro audit` 111/111; `python3 tools/diff_oracle/oracle.py` divergent: 0 (screencast must not change any probe; confirm `Page.startScreencast` adds no page-visible value); `tools/qol_probe/run.py`; `tools/lane_matrix.py all 5` to confirm the default path is unchanged; `cargo clippy --release -- -D warnings`; `cargo test --release` on all three OSes in CI. Attach the before/after numbers to the PR.

**Parallelism:** Steps 2 and 3 can proceed in parallel with Step 1 (no shared code). Step 4 depends on Step 1's decision. Step 5 depends on Steps 3 and 4 and the Step 0 feedback.

## Anti-Patterns to Avoid

### Anti-Pattern 1: Recorder field on `Page`
**What:** storing the encoder and screencast state inside `Page`.
**Why bad:** `switch_tab` replaces `Page` wholesale and `Drop` aborts its tasks; the recording dies silently on tab switch.
**Instead:** `RecordHub` in the surface loop, bound by target id.

### Anti-Pattern 2: Acking after encode
**What:** sending `screencastFrameAck` only after ffmpeg accepted the frame.
**Why bad:** couples the page's frame production to the encoder; a slow encode stalls the compositor and changes page timing (settle detection uses timing).
**Instead:** ack on receipt, drop newest on backpressure, report drops.

### Anti-Pattern 3: Writing recordings under `artifacts/`
**What:** reusing `artifacts::write_artifact`.
**Why bad:** rotation at 300 files deletes them without notice.
**Instead:** dedicated recordings dir, never rotated by bladebro.

### Anti-Pattern 4: `Runtime.enable` or automation flags on the sidecar/in-tree session
**What:** enabling extra domains or adding `--enable-automation` to get a cleaner screencast.
**Why bad:** page-visible detection signals and the infobar; violates non-negotiable #7.
**Instead:** `Page` domain only; no launch flag changes.

### Anti-Pattern 5: Relying on `Drop` to finalize
**What:** ending the recording only in `Drop`.
**Why bad:** `Drop` cannot await; killing ffmpeg yields a truncated, unseekable WebM.
**Instead:** explicit `finish()` on stdin EOF with a bounded wait; `Drop` kills only as a last resort and marks the result.

## Scalability Considerations

Recording is single-session and local, so "scale" means file count and CPU, not users.

| Concern | 1 session | 10 sequential sessions (QA run) | Long session (30+ min) |
|---------|-----------|----------------------------------|------------------------|
| CPU | one ffmpeg encode, ~1 core at 1280x800 | same; encode serial per file | watch thermal and drop counts |
| Disk | a few MB per minute at low fps; webm is compact | pruning belongs to the harness (PROJECT.md) | consider fps cap (e.g. 5–10) as default |
| Memory | bounded channel (64 JPEG frames) | bounded per recording | bounded; no growth |
| Files | 1 | N named files, no rotation | 1 |

## Pitfalls Flagged for Later Phases

- **Viewport size change mid-recording:** image2pipe may fail or need a scale filter. Prototype it in Step 1; do not assume.
- **Two CDP clients on one target:** whether bladebro's own event subscriptions are affected by a second `Page.startScreencast` is unproven. Measure in Step 1.
- **Pipe transport (`BLADE_TRANSPORT=pipe`):** no WS port, so a sidecar cannot attach. The in-tree source (Step 4) must work on the pipe session too, or the feature must state the limitation in its verdict.
- **Missing ffmpeg or codec:** must produce `recording: unavailable (ffmpeg not found; set PATH or install ffmpeg)` style verdicts, never block browsing.
- **Windows:** ffmpeg child spawn must not open a console window; path lookup must handle `.exe`. Verify on the Windows CI runner.

## Sources

- Codebase (read directly, commit `e307fa0` / `1a8c3ed`): `src/page/tabs.rs` (`switch_tab`, `*self = new_page`), `src/page/mod.rs` (`Page` fields, `Drop`), `src/browser.rs` (`Browser::shutdown`, `Drop`), `src/artifacts.rs` (`write_artifact`, `rotate_artifacts`, `MAX_ARTIFACTS = 300`), `src/cdp/client.rs` (`subscribe`, `from_pipe`, `connect`), `src/cdp/session.rs` (`CdpSession::root/child`), `src/mcp/tools.rs` (`state` schema), `src/mcp/server/state.rs`, `src/mcp/server/boot.rs` and `serve.rs` (idle, teardown, relaunch), `src/cli/daemon.rs`, `Cargo.toml` (`base64 = "0.23"` already present).
- `.planning/PROJECT.md`, `.planning/codebase/ARCHITECTURE.md`, `AGENTS.md` (non-negotiables and instruments).
- CDP `Page.startScreencast` / `Page.screencastFrameAck` semantics (per-session, ack-gated frame stream) are from the Chrome DevTools Protocol reference and were NOT verified against Chrome 151 in this research pass. Treat as hypothesis until the Step 1 prototype measures them.
