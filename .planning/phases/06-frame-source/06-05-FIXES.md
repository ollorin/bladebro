# 06-05 Fixes (verification gaps 1-3)

1. Flaky lagged-bus test: sink now stalls inside `tokio::task::block_in_place` on a 4-worker runtime (other workers keep the CDP reader and fake server running, so the bus overflow is guaranteed); the probe counts `Lagged(n)` as seen events and asserts >= 18000 on the bus. Still asserts lost>0, lag_acks>=2, wire acks == acks_sent + lag_acks, cached id, allowlist. Stability: isolated release x30 = 30/30; full `cargo test --release --lib` x5 = 5/5 (585 passed each).
2. mark_end completeness: writer hold-flush tick now clamps its hold to `end_at` once marked. New test `hold_tick_after_mark_end_does_not_write_past_it` (mark at 0.3 s, sleep 1.5 s across the 1 s tick; fails with 10 frames without the fix, 3 with). Docs drift claim corrected to the measured range. Live dur-tick8 x8 (4 whole-second, 4 off-second via test-only `REC_LIVE_EXTRA_MS`): -0.02 -0.01 -0.01 +0.00 +0.16 -0.27 -0.74 +0.04 %; zero leftovers (evidence/06-05-live.txt). Pre-fix was +5.00 %.
3. Ack-order test `ack_reaches_the_wire_before_the_sink_runs`: the sink waits (bounded 2 s) for its frame's ack on the fake wire. Mutation (forward before ack spawn in capture.rs) fails it; mutation reverted, capture.rs unchanged.

Gates: fmt ok, clippy --all-targets -D warnings clean, `cargo test --release` 628 passed (baseline 626 + 2 new).
