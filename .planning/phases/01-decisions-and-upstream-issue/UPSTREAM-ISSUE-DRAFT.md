<!-- DRAFT - NOT POSTED. Do not post without explicit user approval. -->
# DRAFT - NOT POSTED
Target repo: dondai44423/bladebro (upstream). Template: .github/ISSUE_TEMPLATE/feature_request.yml (label: enhancement)
Status: awaiting user approval to post.

## Proposed title

Opt-in session video recording via state record-start / record-stop (schema ask-first)

---

### What problem does this solve?

Agents doing autonomous UI/UX QA need watchable evidence of what happened in a run, pass or fail. README lists "Session video recording" as "Not supported" (line 597) and tells users to use `vision` screenshots. `vision` gives single PNGs only, so a reviewer cannot replay a flow, see a transient state, or check the order of steps across a tab switch.

### Proposed solution

Two new ops on `state`, reusing the existing `name` property:

- `record-start` (with `name`): starts an opt-in recording of the active tab.
- `record-stop`: finalizes it and returns absolute file path, bytes, duration, frame count, dropped-frame count, and segment boundaries (tab switches with time offsets).

Capture uses the stock CDP `Page.startScreencast` (ack per frame), piped to an external `ffmpeg` binary. No new crate, no bundled binary, no launch flags, no injected JS. The existing gates stay as they are: `bladebro audit` 111/111 and `diff_oracle` divergent 0.

- Opt-in: off unless `BLADE_RECORD=<dir>` is set or `record-start` is called.
- Recordings go to a dedicated directory that bladebro never rotates, default `~/.blade/recordings/`. They do not go to `~/.blade/artifacts/`, which rotates at 300 files.
- Multi-tab: one file follows the active tab. `switch-tab`, `open-tab` and popups rebind capture into the same file, and each switch is a reported segment boundary. Per-tab files are opt-in.
- If ffmpeg is missing, the op returns an honest error. Browsing is unaffected.

Exact proposed strings:

```
enum fragment:    , "record-start", "record-stop"
description line: RECORDING: record-start <name> (video of the active tab; follows tab switches), record-stop (returns file path, bytes, duration, frames, drops). Needs ffmpeg; dir from BLADE_RECORD.
name description: , or recording name
```

#### Token cost

Method: source text of `src/mcp/tools.rs`, whitespace collapsed, chars/4 approximation. This is not a tokenizer count.

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

### Which tool should this extend?

state

### Alternatives considered

- Option B: a new object property on `state` (e.g. `record: {...}`). Each property and description is billed to every agent on every session, so it costs more tokens than two enum values plus one description line.
- A sixth tool. Rejected by the project's five-tools rule, so it is not proposed here.
- Polling `vision` / `Page.captureScreenshot`. Heavy per frame and low fps; not a video.
- OS screen capture (x11grab, avfoundation, gdigrab). Captures the whole screen rather than the browser tab, and differs per platform.
- `HeadlessExperimental.beginFrame`. Needs automation-related launch flags, which the project rejects.
- An external sidecar CDP client. Works only on the WebSocket transport, not `BLADE_TRANSPORT=pipe`, and duplicates the session.

### Questions for maintainers

1. Is Option A acceptable, or do you prefer a different op naming or shape?
2. Is `~/.blade/recordings/` with `BLADE_RECORD` an acceptable location and override?
3. Would you accept this as an opt-in PR after a sidecar prototype in `tools/record_sidecar/` shows capture and stealth neutrality (audit and diff_oracle unchanged)?
4. Is there any objection to ffmpeg as an optional external runtime dependency, reported by `doctor`?
