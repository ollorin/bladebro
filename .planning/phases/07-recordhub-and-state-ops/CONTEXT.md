# Phase 7 decisions (coordinator, 2026-10-22; user-delegated where research flagged them)
- `enabled` (BLADE_RECORD / config) is an operator permission gate: record-start when not enabled returns an actionable Err naming BLADE_RECORD and the need to restart the daemon/MCP (`bladebro stop`). No auto-start at this phase.
- Per-call overrides: `name` only (schema delta minimal); docs lines claiming per-call format/fps/size must be corrected.
- Schema description must NOT claim "follows tab switches" (Phase 8); say the video is of the active tab at start.
- record-stop: Ok only for Captured+Finalized; other outcomes return Err whose text carries the full honest report (path, bytes, frames...), so isError stays honest.
- Hub: process-global OnceLock<tokio Mutex<Hub>> in src/record/hub.rs; hook in handle_state via try_handle early return (zero I/O for other ops).
- Pipe transport and one-shot CLI and rb lane: honest refusal messages.
- Surface-exit finish is best effort; Phase 9 owns the matrix.
