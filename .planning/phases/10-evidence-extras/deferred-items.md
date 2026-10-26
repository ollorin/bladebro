# Deferred items (phase 10)

- [DEFERRED] `act click` verdict says `clicked (no observable DOM change)` for a click that did change the page: on `mark.html` the "turn blue" click turned the body background from red to blue (pixels in the recorded webm and in the poster PNG prove it, `evidence/10-01-live.txt` poster/marker/per-tab ids). The verdict readback misses a background-colour change, so the honest-verdict rule (AGENTS.md non-negotiable 6) is not met for this case. Area: `src/action/` (verdict readback), out of scope for 10-01 — found during task 4 (live evidence run).
