# 12-02: what this machine did not run, and how to run it

## diff_oracle: NOT RUN
`python3 tools/diff_oracle/oracle.py` imports the python `websockets` module, which is not installed;
installs are forbidden in this milestone. Nothing about `divergent: 0` is claimed. To run it where installs
are allowed: `python3 -m pip install websockets`, then `python3 tools/diff_oracle/oracle.py` (stock Chrome vs
bladebro, same display) with recording off, and again while a recording runs in the bladebro instance.
Substitute evidence here: audit 100/100 with the product recorder attached (12-02-audit-recording.txt),
live sites off == on (12-02-live-sites.txt).

## ubuntu / windows CI: NOT RUN (push is the user's decision)
`.github/workflows/ci.yml` runs on push to `main`, pull_request to `main`, and `workflow_dispatch`, on
ubuntu-latest, macos-latest, windows-latest: `cargo fmt --check`, `cargo clippy --release -- -D warnings`,
`cargo test --release`, `cargo build --release` (plus Linux-only check jobs). The branch has never been pushed.
Commands for the user (not run by the agent):
```bash
git push origin gsd/browser-recording
gh workflow run ci.yml --repo ollorin/bladebro --ref gsd/browser-recording
gh run list --repo ollorin/bladebro --workflow ci.yml --branch gsd/browser-recording --limit 1
gh run watch --repo ollorin/bladebro <run-id>
```
(or open a PR from gsd/browser-recording to main on ollorin/bladebro). What only CI will prove:
`src/record/encoder/windows_tests.rs` (cfg(windows), 4 tests, never compiled), every `cfg(unix)`-gated
path compiling out cleanly on Windows, and the Linux build.
Workflow notes: a plain push of `gsd/browser-recording` does NOT start CI, because the push trigger only
covers `main`. Use `workflow_dispatch` (the `gh workflow run` line above) or open a PR into `main`.
`workflow_dispatch` needs `ci.yml` to exist on the default branch of ollorin/bladebro, so confirm that before
dispatching. The `paths-ignore` list skips docs-only pushes, so a push that changes only `.md` files will not
trigger CI either.

## Linux-only instruments: NOT RUN
- `bladebro audit` 111/111 is the Linux (headed, Xvfb) total; macOS headless reports 100/100.
- `python3 tools/lane_matrix.py all 5` needs `/proc` (Linux); here `tools/record_sidecar/lib/lane_matrix_local.py`
  ran the same lanes (daemon, oneshot, mcp, mcp-pipe) with a `ps` sweep. The real-browser lane was not run.
On a Linux host: `cargo build --release && ./target/release/bladebro audit` (expect 111/111),
`python3 tools/lane_matrix.py all 5`, `python3 tools/qol_probe/run.py`, each once with `BLADE_RECORD` unset and
once with `BLADE_RECORD=1`.

## Live sites headed: NOT RUN
AGENTS.md's live-site targets are for a headed browser. Here both modes ran headless (12-02-live-sites.txt).
Headed: `python3 -I tools/record_sidecar/lib/live_sites.py --mode off --headed --out <file>` and `--mode on`.

## Caveats found while writing this record (added by the executor)
- The audit "recorder attached" run (12-02-audit-recording.txt, step [B]) attached the recorder for about 1 s
  before `ConnectionLost { after_ms: 1001 }` (5 frames received). It is not a multi-minute attached audit.
- The evidence_live `off` per-method count check fails strictly in 3 runs (12-02-scen-evidence-a.txt,
  12-02-scen-evidence-b.txt, 12-02-scen-evidence-off-rerun.txt). The cause is `Input.dispatchMouseEvent`
  (random mouse path): the off-rerun probe measured 29/31 on base and 31/27 on new. The tolerant check, which
  excludes that method, passes in all runs. This is recorded as known noise, not a clean pass.
- `ci.yml` only starts on `main` pushes, pull requests into `main`, or `workflow_dispatch` (see above).
