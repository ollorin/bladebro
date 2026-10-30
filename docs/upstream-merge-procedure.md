# Merging upstream into this fork

This fork keeps its recording and launch-mode work additive so `upstream/main` keeps merging cleanly:
new code lives in fork-owned paths (`src/record/**`, `src/launch_mode/**`, `tools/record_sidecar/**`,
`tests/record_*`, `docs/recording-and-launch-config.md`, `.planning/**`), and every edited line in a file
that also exists upstream is listed in `.planning/HOOKS.md`.

## Procedure

1. `git fetch upstream`
2. Trial run (throwaway worktree and branch, nothing pushed, the working branch untouched):
   ```bash
   bash tools/record_sidecar/lib/trial_merge.sh --upstream upstream/main --out /tmp/trial.txt
   ```
   Exit 0: clean. Exit 1: conflicts only in files listed in `.planning/HOOKS.md` (the printed hunks
   show where; re-apply the listed hook lines on top of upstream's version). Exit 2: a touched or
   conflicted file is not in `HOOKS.md`: add the row, or move the change into a fork-owned file.
3. Gate on the merged tree (one build dir, `target/trial-merge`, reused between runs):
   ```bash
   CARGO_TARGET_DIR=target/trial-merge cargo test --release --no-run   # warm the dependencies once
   bash tools/record_sidecar/lib/trial_merge.sh --gate fmt,clippy
   bash tools/record_sidecar/lib/trial_merge.sh --gate test,audit
   ```
4. Real merge, only after steps 2-3 are green: on a new branch,
   `git merge upstream/main`, resolve hook conflicts as in step 2, then the full instrument gate
   (fmt, clippy, tests, `bladebro audit`, `tools/qol_probe/run.py`, lane matrix, recording scenarios
   in `tools/record_sidecar/README.md`, live stealth sites).
5. Update `.planning/HOOKS.md` if a hook line moved.

`--self-test` proves the script on three synthetic upstream commits (clean, hook-only conflict,
unlisted add/add conflict); `--cleanup` removes a killed run's worktree and `trial-merge/*` branch.
The self-test leaves its fixture commits unreferenced; `git gc` drops them.
