# Phase 10 plan notes (design decisions, binding on executors)

PHASE_BASE = d18dae1 (HEAD when planned). No research step: decisions below come from reading
src/record/** (hub/machine.rs, hub/report.rs, hub/msg.rs, hub/follow/{sink,run,merge,files}.rs,
hub/follow_report.rs, encoder/*, dir.rs, names.rs), src/mcp/server/{proto,state,act,run}.rs,
src/cli.rs `dispatch`, src/cli/args/state.rs, src/mcp/tools.rs, src/record/hub/schema_tests.rs and
docs/recording-and-launch-config.md.

## D10-1 Poster frame: from the FINISHED video, last frame, via ffmpeg (not from a captured JPEG)
- How: after the encoder finalized (exit 0, non-empty file), one short ffmpeg run on the file:
  `-nostdin -hide_banner -loglevel error -sseof -1 -i <video> -map 0:v:0 -an -update 1 -f image2 -c:v png -y <poster>`
  (decodes at most the last second, keeps overwriting one PNG, so the file holds the LAST frame). If that run fails
  (e.g. a container that cannot seek from the end) one retry without `-sseof -1`, inside the same budget.
- Why not keep the last screencast JPEG in memory: it still needs a JPEG->PNG conversion (no image crate allowed, so
  ffmpeg anyway), and in follow mode the last JPEG the source saw is not necessarily what the video ends on (stale
  generations are dropped in SegmentSink). Decoding the finished file proves the video itself decodes and shows exactly
  what a reviewer will see. Zero CDP calls (the recorder's closed set of 5 methods is unchanged, SAFE-01).
- Why the last frame: it is the page state at record-stop, i.e. where a marker/failure note was given; it always has
  content when the video has content (the encoder holds the last frame to the end).
- Budget: `POSTER_TIMEOUT = 3 s` total (typical ~0.1-0.3 s, measured in 10-01 Task 3). Process: argv only, own process
  group on unix, CREATE_NO_WINDOW on Windows, `kill_on_drop(true)`, stdin null. Only when `ffmpeg -encoders` listed `png`
  (EncoderSet only parses video encoders; png is one). Success = exit 0 AND the file starts with the 8-byte PNG magic;
  otherwise the poster file is removed and the result says `Poster: not made (<reason>)`. A poster failure NEVER makes
  the recording fail or changes ok/err.
- File: `<video file stem>.poster.png` next to the video, created with `dir::create_unique` (create_new, 0600, single
  path component, numbered `<stem>.<n>.poster.png` on collision; a planted symlink is never followed); ffmpeg then
  opens the existing file with O_TRUNC, which keeps mode 0600 (checked by a test). The stem is the already sanitized
  video name (names.rs); nothing caller-supplied is added.

## D10-2 Marker: `value` on `record-stop`, no new op, no new property
- MCP: `{"op":"record-stop","value":"FAIL: login button did nothing"}`. CLI: `bladebro state record-stop --value "..."`
  (the existing `--value` flag already parses for every op; no CLI parser change). run/batch: `{"action":"state","op":"record-stop","value":"..."}`.
- Offset: the instant record-stop was received (before the finalize), on the video clock (D10-4), clamped to
  [0, duration]. Result gets `. Marker at <T.T> s` (the note itself is NOT echoed: the agent wrote it; tokens).
  10-02 also writes it into the timeline as `{"kind":"marker","at_s":..,"note":..}`.
- Sanitizing (`record::marker::clean_text`, also used for verdict lines): every control char (incl. \n \r \t) becomes a
  space; zero-width/bidi format chars U+200B-U+200F, U+202A-U+202E, U+2060-U+2069, U+FEFF are dropped (they can
  disguise text for a reviewer); whitespace runs collapse; trimmed; at most `MAX_MARKER_CHARS = 200` chars (char
  boundary; a cut ends with "..."). It reaches disk only through serde_json (escaped) and never reaches stdout.
- A bad marker never costs the recording: not a string -> `. Marker not placed: value must be a string`; empty after
  cleaning -> `. Marker not placed: empty note`; no active recording -> the existing no-recording/orphan text plus
  `. Marker not placed: no active recording`. `record-start` ignores `value` (unchanged behaviour).
- Rejected alternative: a `record-mark` op for mid-recording marks (+1 enum value and description text on every
  agent's context). The timeline's per-action entries (with `ok:false` on failed actions) already give the
  mid-recording jump points; the marker labels the outcome at stop.

## D10-3 Timeline sidecar: from the tool results already produced, two tiny upstream hooks
- Source of entries: the verdict text every `act` / `run` / tab-changing `state` call already returns. Hooked at the
  two tool dispatch points that every lane goes through: `src/mcp/server/proto.rs handle_tools_call` (MCP) and
  `src/cli.rs dispatch` (daemon; one-shot cannot record). Each hook = 1 comment line + `let tl = ...begin(..)` before
  the handler match + `...end(tl, &result)` after it. No change to any handler, no CDP call, no DOM work.
- Recording off: `begin` is one relaxed atomic load (`hub::recording_active()`) returning None; `end(None, _)` returns.
  No allocation, no lock, no output change (byte-identical; proven by unit test, source pins and a live diff vs the
  d18dae1 binary).
- Included calls: every `act` (action = args.action), every `run` (action "run", `steps` = top-level step count),
  `state` only for open-tab / close-tab / switch-tab. Not included: `see` (reads; a `see url=` navigation is not
  listed), `vision`, other state ops, record-start/record-stop. One entry per tool call (a run is one entry; its
  verdict is the result's first line, e.g. `run stopped at step 3 of 5 ...`).
- Entry: `at_s` (call start), `end_s` (result produced), `tool`, `action`, `steps` (run only), `ok` (Ok vs Err),
  `verdict` (first non-empty line of the result or error text, `clean_text(.., 200)`).
- Storage: `record::timeline::Log` (std Mutex, at most `MAX_ENTRIES = 1000`, then counted as `dropped`) owned by the
  active recording (`Active.log: Arc<Log>`); the global hub publishes the current Arc in `publish_active` (same place as
  the RECORDING flag). A `Begin` holds the Arc of the recording it started in, so an entry that ends after its
  recording stopped lands in a closed log and is discarded (never in the next recording). Hub instances in unit tests
  get their own Log and never touch the global.
- Written ONLY by the agent's `record-stop` (finish paths: stop). Exit/interrupt/limit finalizes (finish_orphan,
  interrupt_if, stop_on_limit) write the video only, as before: no ffmpeg spawn while a process is exiting and no new
  files in the exit matrix (Phase 9 tests count files). The log is discarded there.
- File `<primary video stem>.timeline.json`, 0600, `create_unique`, pretty JSON, version 1:
  `{"version":1,"clock":"seconds from the first video frame","video":"<basename>","duration_s":D,
    "files":[{"file":"<basename>","start_s":S|null,"duration_s":D}],
    "entries":[{"kind":"action","at_s":..,"end_s":..,"tool":"act","action":"click","ok":true,"verdict":".."},
               {"kind":"action",..,"tool":"run","action":"run","steps":4,..},
               {"kind":"marker","at_s":..,"note":".."}],
    "dropped":0}`
  Basenames only (no absolute paths, no home dir in the file). Entries sorted by at_s (stable). 2-decimal offsets.
  Result gets `. Timeline: <abs path> (<N> actions)` (`, <K> not kept` when dropped > 0), or
  `. Timeline: not written (<reason>)`; never changes ok/err.

## D10-4 Video clock and alignment
- 0 s = the arrival instant of the first frame the video accepted (`FollowShared::t0_at`), i.e. the same clock as the
  existing notes (`tab X gone at T s`, `Policy::clock`). Segment offsets in the result use the frames' capture
  timestamps relative to the first one; the two differ by the capture-to-arrival latency (tens of ms) plus at most one
  frame slot (1/fps). Documented as the alignment bound; 10-02 Task 3 measures it live (colour change of a clicked
  button vs the click entry's offsets).
- No frame at all: fallback clock = record-start instant (`Active.started`), stated in the JSON `clock` text
  (`"seconds from record-start (no video frame)"`).
- Offsets: `saturating` (an action that started before the first frame -> 0.0) and clamped to the video duration.
- Plumbing (fork-owned): `Merged.t0_at: Option<Instant>` (set in run.rs right after `merge`, 1 line),
  `FileOutcome.t0_at: Option<Instant>` (set in files.rs from `f.shared.t0_at()`).

## D10-5 Per-tab mode (several files): one poster, one timeline, one clock
- Recording clock = the earliest file's first frame (min t0_at = `Policy::clock`, the clock per-tab notes already use).
- Poster: from the file of the tab the video followed at stop (the `last` tab finish_active already computes); if that
  file is not finalized, the first finalized file in tab order; none -> `Poster: not made (no finalized file)`. Named
  after that file (`...-tab2-....poster.png`).
- Timeline: ONE, named after file 1 (`...-tab1-....timeline.json`); entries and marker on the recording clock; `files[]`
  lists every file with `start_s` (its first frame on the recording clock; null when it got no frame) so file-local
  time = at_s - start_s. Marker offset in the result is on the recording clock.

## D10-6 Output budget (tokens)
- Recording on, record-stop result grows by `. Poster: <abs path>. Timeline: <abs path> (N actions)[. Marker at T s]`
  (~45-60 tokens once per recording). Absolute paths because agents pass them on; no new field in any other tool output.
- MCP schema (10-02 Task 4): RECORDING line + `value` description only; no new property, no new op. Ceiling asserted in
  schema_tests.rs (old 240 chars -> new <= 330 chars total added since Phase 7; report chars and ~tokens).

## D10-7 Upstream hooks (all listed in HOOKS.md with commit sha, checked with `git cat-file -e upstream/main:<path>`)
- 10-01: none (marker parsing, poster and stop wiring are all in src/record/**; `--value` already parses). machine.rs (491 lines) is kept <= 500 by moving stop/finish code into the child module hub/finish.rs (10-01 Task 3); 10-02 hub additions go there or into evidence.rs.
- 10-02: src/mcp/server/proto.rs (+3 lines), src/cli.rs (+3 lines), src/mcp/tools.rs (2 description strings),
  src/cli/help.rs (RECORDING block: 1 changed + 1 added line). rustfmt must not re-indent any upstream line:
  `git diff -U0 <file>` shows only the hook lines.

## D10-8 Test-helper rule (sidecars appear in the recordings dir after record-stop)
Every helper that lists the recordings dir keeps only `.webm`/`.mp4` names: `files_in` in
src/record/hub/{machine_tests,per_tab_tests,per_tab_popup_tests}.rs (exit_tests.rs uses machine_tests' helper),
`Mcp::recordings()` in tests/record_support/mod.rs (fork-owned), and any `ls`/glob of the recordings dir in
tools/record_sidecar/** (grep `recordings`). Add `sidecars()` helpers where a test needs them.
