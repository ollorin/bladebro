# Fix round 08-03

Closes the three gaps in 08-VERIFICATION.md.

## 1. Flaky `a_stop_is_bounded_when_one_encoder_ignores_eof`

- `src/record/hub/per_tab_tests.rs`: the fixed 300 ms sleep before `pgrep -f "sleep 4242.7"` is replaced by `until(...)` (polls up to 15 s) for the hung encoder to exist, then the assertion.
- Sibling tests were checked: no other test asserts on a pgrep count after a bare sleep (they wait with `until(started)` first, or assert after a stop); no change needed.
- Full `cargo test --release`: runs 1 and 2 had the lib binary green (675 passed) but stopped at the known flake `concurrent_cold_starts_have_one_owner` (daemon_lifecycle), under machine load averages of 120 to 180. A leaked scratch daemon (pid 32026, BLADE_HOME `bladebro-daemon-race-32012`, confirmed by name) from the daemon_race test was killed. Runs 3 and 4 (`--no-fail-fast`) were fully green, every target `ok`, the flake passing. No daemon left afterwards.

## 2. `hub_live.sh` stale check

- Line 126 now expects the Phase 8 segments listing (`segments [0.0 s tab X, N s tab Y`) instead of "agent is now on tab". The edit was committed together with fix 1 (242bbe0).
- Headless run `hub_live.sh --only daemon-rec` (scratch BLADE_HOME, local fixtures): 0 failures, segments `[0.0 s ..., 1.8 s ...]`, nothing left running. Evidence: `evidence/08-03-hub-live-daemon-rec.txt`.

## 3. Window-feature popup, measured

- New `popup-measure` scenario in `follow_live.sh`; evidence `evidence/08-02-popup-measure.txt`.
- Without `switch-tab`: the recording followed the popup (segment at 3.7 s for a click at about 1.5 s); `state tabs` kept the opener as current. After `press Tab` at about 7 s it did not return to the opener; the return segment is at 10.7 s, at `record-stop`. Only `press` was measured; the earlier claim of a return at the next input action is not supported.
- `docs/recording-and-launch-config.md` and ROADMAP Phase 8 criterion 1 now state: click popups become a segment after `switch-tab`; window-feature popups are followed per measurement.
