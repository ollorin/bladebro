> **Update 2026-10-21:** the fork is the main repo. No upstream issue/PR/approval is needed; references below to asking upstream or to a 'Phase 7 gate' are superseded. The decisions themselves stand; the only upstream constraint is keeping merges from `upstream/main` clean (SYNC-01).

# Phase 1 Decisions - Session Recording

Date: 2026-10-21
Status: Phase 7 BLOCKED (see Phase 7 gate)

## D-01 Schema shape: Option A (recommended, pending upstream)

Two new `state` ops, `record-start` and `record-stop`, are added to the existing `op` enum. `record-start` reuses the existing `name` property as the recording name. No new properties are added.

Option B (a new object property on `state`, e.g. `record: {...}`) is rejected. Every new property and its description is billed to every agent's context on every session.

Proposed strings (final wording subject to upstream feedback):

```
enum fragment:    , "record-start", "record-stop"
description line: RECORDING: record-start <name> (video of the active tab; follows tab switches), record-stop (returns file path, bytes, duration, frames, drops). Needs ffmpeg; dir from BLADE_RECORD.
name description: , or recording name
```

### Measured token cost

Method: source text of `src/mcp/tools.rs`, whitespace collapsed, chars/4 approximation. This is not a tokenizer count.

Measured by the Step 1 script (output copied verbatim):

```
state ToolDef: raw_chars=2038 collapsed_chars=1681 est_tokens(collapsed/4)=420
all_tools(): raw_chars=18963 collapsed_chars=15581 est_tokens(collapsed/4)=3895
enum: chars=31 est_tokens=7.8
desc line: chars=182 est_tokens=45.5
name desc: chars=19 est_tokens=4.8
addition total chars=232 est_tokens=58
```

| item | chars | est_tokens (chars/4) |
|------|-------|----------------------|
| existing `state` ToolDef (collapsed) | 1681 | 420 |
| all five tools `all_tools()` (collapsed) | 15581 | 3895 |
| addition: enum fragment | 31 | 7.8 |
| addition: description line | 182 | 45.5 |
| addition: `name` description suffix | 19 | 4.8 |
| addition total | 232 | 58 |
| addition as % of `state` ToolDef (232 / 1681) | - | 13.8% |
| addition as % of `all_tools()` (232 / 15581) | - | 1.5% |

Caveat: AGENTS.md states the tool definitions cost about 1,900 tokens. The chars/4 figure from Rust source differs from that because it includes Rust syntax and JSON escaping. Compare the RELATIVE increase (13.8% of `state`, 1.5% of all tools), not the absolute numbers.

## D-02 Recordings directory

Dedicated directory. Default is `~/.blade/recordings/` (under BLADE_HOME). It is overridden by `BLADE_RECORD=<dir>`, which also turns recording on by default.

- Never rotated by bladebro.
- Never inside `~/.blade/artifacts/`. Artifacts rotate at 300 files, which would silently delete evidence.
- All recordings are kept. The harness prunes.
- Unix permissions: 0700 for the directory, 0600 for files (detail in Phase 3).

## D-03 Multi-tab policy

Default: one file per recording that follows the active tab. `switch-tab`, `open-tab` and popups rebind capture into the same ffmpeg process and file. Every switch is reported in the `record-stop` result as a segment boundary with its time offset. No silent merge.

Opt-in: per-tab files mode (one file per tab, all listed at stop).

Detail in Phase 8.

## D-04 Codec default - PROVISIONAL

WebM default (VP9 realtime, VP8 fallback). MP4 with libx264 is opt-in.

Final default is set by Phase 2 CPU and size measurements.

## D-05 FPS cap and frame size - PROVISIONAL

Candidate range: 5-15 fps, bounded maxWidth/maxHeight, JPEG frames at q60-80.

Final values come from Phase 2 size-per-minute and fps measurements.

## D-06 Timestamp strategy - PROVISIONAL

Candidates:
- (a) wallclock timestamps with `-fps_mode vfr`
- (b) concat durations from `metadata.timestamp`

Decided by the Phase 2 duration-drift measurement. Acceptance: within 5% of wall clock on an idle page.

## D-07 Control surface

Env/config default (`BLADE_RECORD`) plus per-call `state` ops. No sixth tool (AGENTS.md non-negotiable #1). No LLM in the driver. No new Rust crate. ffmpeg is an external binary.

## Upstream issue status

Drafted in UPSTREAM-ISSUE-DRAFT.md. NOT posted. Awaiting user approval to post.

- Issue URL: (none yet)
- Upstream answer: (none yet)

## Phase 7 gate

Phase 7 (RecordHub and state Ops) is BLOCKED.

It may start only when this file records EITHER:
- (a) the upstream maintainers' answer on the schema shape and recordings dir, with issue URL and date; OR
- (b) a fallback decision explicitly approved by the user (e.g. proceed with Option A on the fork after N days without an answer), with the approval date.

Neither exists yet.

ROADMAP Phase 1 success criterion 3 is NOT met.

Phases 2-6 do not depend on this gate.
