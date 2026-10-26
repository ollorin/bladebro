# Deferred items (phase 11)

- [DEFERRED] `state switch-tab <id>` to the tab the agent is already on does not re-front it. With a window-feature popup in front during a recording, the video stays on the popup (11-01 evidence `11-01-s03-tabs.txt`, section window-popup). Workarounds (close the popup, or switch away and back) are documented. Area: switch-tab in src/state.rs (upstream) or the follower in src/record/hub/follow; a behavior change, out of scope for the docs phase.
- [CARRIED from 10] `act click` verdict misses a background-colour change (src/action, upstream). Documented under Known issues.
- [CARRIED from 10, documented] A `run` containing record-start ... record-stop gets an empty timeline (src/record/timeline.rs records whole calls). Documented under Known issues; no code change.
- [FIXED in 11-01] `(1 actions)` and the double period before an idle marker note.
