#!/usr/bin/env bash
# SYNC-01: trial merge of upstream into the fork in a throwaway worktree + branch. Never touches the
# working branch, never fetches, never pushes, never commits to a real branch.
#   bash tools/record_sidecar/lib/trial_merge.sh [--upstream REF] [--gate fmt,clippy,test,audit] [--out FILE]
#   bash tools/record_sidecar/lib/trial_merge.sh --self-test   (synthetic upstream commits: clean, hook-only, unlisted)
#   bash tools/record_sidecar/lib/trial_merge.sh --cleanup     (remove leftovers of a killed run)
# REF defaults to upstream/main (run `git fetch upstream` first). Steps:
#   1. additive report: every path changed between merge-base(REF, HEAD) and HEAD is NEW (absent upstream)
#      or TOUCHED (exists upstream); every TOUCHED path must be listed in .planning/HOOKS.md.
#   2. `git merge --no-commit --no-ff REF` in a worktree under a mktemp dir, on branch trial-merge/<stamp>;
#      conflicted files are checked against HOOKS.md and their conflict hunks printed.
#   3. optional gate inside the worktree (CARGO_TARGET_DIR=<repo>/target/trial-merge, scratch BLADE_HOME).
#   4. worktree, branch and temp dir removed; the working branch's commit and tracked state are checked unchanged.
# Exit: 0 clean (and gate green if asked); 1 conflicts only in HOOKS.md-listed files (resolve them per
# HOOKS.md, then rerun the gate on the result); 2 a touched or conflicted path is NOT in HOOKS.md;
# 3 gate failed; 4 usage or setup error.
set -u
SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
REPO="$(git rev-parse --show-toplevel)" || exit 4
cd "$REPO" || exit 4
UP="upstream/main"; GATE=""; OUT=""; MODE=run
while [ $# -gt 0 ]; do
  case "$1" in
    --upstream) UP="${2:?--upstream needs a ref}"; shift 2 ;;
    --gate) GATE="${2:?--gate needs a list}"; shift 2 ;;
    --out) OUT="${2:?--out needs a file}"; shift 2 ;;
    --self-test) MODE=selftest; shift ;;
    --cleanup) MODE=cleanup; shift ;;
    *) echo "unknown argument: $1" >&2; exit 4 ;;
  esac
done

say() {
  echo "$*"
  if [ -n "$OUT" ]; then echo "$*" >> "$OUT"; fi
}

# File column of every HOOKS.md table row ("| <phase> | <file> | ...").
hooked_files() {
  awk -F'|' '/^\| *[0-9]/ { f = $3; gsub(/^[ \t]+|[ \t]+$/, "", f); print f }' "$REPO/.planning/HOOKS.md" | sort -u
}
is_listed() { hooked_files | grep -qxF "$1"; }

cleanup_leftovers() {
  git worktree list --porcelain | sed -n 's/^worktree //p' | grep '/bbmerge\.' | while read -r w; do
    git worktree remove --force "$w" >/dev/null 2>&1 && echo "removed worktree $w"
  done
  git worktree prune
  git for-each-ref --format='%(refname:short)' refs/heads/trial-merge/ | while read -r b; do
    git branch -D "$b" >/dev/null 2>&1 && echo "deleted branch $b"
  done
  return 0
}

# 1. Additive report. Sets UNLISTED_TOUCHED.
UNLISTED_TOUCHED=0
additive() {
  local base st p new=0 touched=0 newlist=""
  base=$(git merge-base "$UP" HEAD) || { say "ERROR: no merge-base between $UP and HEAD"; return 4; }
  say "## additive diff: merge-base $(git rev-parse --short "$base") .. HEAD $(git rev-parse --short HEAD) ($UP = $(git rev-parse --short "$UP"))"
  while IFS=$'\t' read -r st p; do
    [ -n "$p" ] || continue
    if git cat-file -e "$UP:$p" 2>/dev/null || git cat-file -e "$base:$p" 2>/dev/null; then
      touched=$((touched + 1))
      if is_listed "$p"; then say "TOUCHED $st $p listed"
      else say "TOUCHED $st $p UNLISTED"; UNLISTED_TOUCHED=$((UNLISTED_TOUCHED + 1)); fi
    else
      new=$((new + 1)); newlist="$newlist$p"$'\n'
    fi
  done < <(git diff --name-status --no-renames "$base" HEAD)
  say "NEW files by folder:"
  printf '%s' "$newlist" | grep -v '^$' | cut -d/ -f1-2 | sort | uniq -c | sort -rn | while read -r n d; do say "  $n $d"; done
  say "# additive: new=$new touched=$touched unlisted_touched=$UNLISTED_TOUCHED"
  return 0
}

gate() {
  local tgt="$REPO/target/trial-merge" step r rc=0 h
  for step in $(echo "$GATE" | tr ',' ' '); do
    case "$step" in
      fmt) (cd "$WT" && cargo fmt --check) > "$SCR/fmt.txt" 2>&1; r=$? ;;
      clippy) (cd "$WT" && CARGO_TARGET_DIR="$tgt" cargo clippy --release --all-targets -- -D warnings) > "$SCR/clippy.txt" 2>&1; r=$?
              tail -2 "$SCR/clippy.txt" | while read -r l; do say "  clippy: $l"; done ;;
      test) (cd "$WT" && CARGO_TARGET_DIR="$tgt" cargo test --release --no-fail-fast) > "$SCR/test.txt" 2>&1; r=$?
            say "  test: $(grep -E '^test result' "$SCR/test.txt" | awk '{p+=$4; f+=$6; i+=$8} END {print "passed="p" failed="f" ignored="i}')"
            grep -E ' FAILED$' "$SCR/test.txt" | while read -r l; do say "  test: $l"; done ;;
      audit) (cd "$WT" && CARGO_TARGET_DIR="$tgt" cargo build --release) > "$SCR/build.txt" 2>&1; r=$?
             if [ "$r" = 0 ]; then
               h=$(mktemp -d "${TMPDIR:-/tmp}/bbmerge-home.XXXXXX")
               BLADE_HOME="$h" BLADE_HEADLESS=1 BLADE_NO_UPDATE_CHECK=1 "$tgt/release/bladebro" audit > "$SCR/audit.txt" 2>&1
               BLADE_HOME="$h" "$tgt/release/bladebro" stop > /dev/null 2>&1; rm -rf "$h"
               say "  audit: $(grep -oE 'SCORE:? *[0-9]+/[0-9]+' "$SCR/audit.txt" | head -1) $(grep -oE 'VERDICT: [A-Z]+' "$SCR/audit.txt" | head -1)"
               grep -q 'VERDICT: PASS' "$SCR/audit.txt" || r=1
             fi ;;
      *) say "unknown gate step: $step (use fmt,clippy,test,audit)"; return 4 ;;
    esac
    say "gate $step exit=$r"
    [ "$r" = 0 ] || rc=3
  done
  return $rc
}

selftest() {
  local base sha fails=0 r
  base=$(git merge-base "$UP" HEAD) || { echo "no merge-base with $UP"; return 4; }
  SCR0=$(mktemp -d "${TMPDIR:-/tmp}/bbmerge.XXXXXX"); FX="$SCR0/fx"; FXB="trial-merge/fixture-$$"
  trap 'git worktree remove --force "$FX" >/dev/null 2>&1; git worktree prune; git branch -D "$FXB" >/dev/null 2>&1; rm -rf "$SCR0"' EXIT
  git worktree add -q -b "$FXB" "$FX" "$base" || return 4
  G() { git -C "$FX" -c user.name=trial-merge -c user.email=trial-merge@invalid "$@"; }
  # Case clean: a new upstream-only file + an edit to an upstream file the fork never touched.
  echo "trial" > "$FX/UPSTREAM_TRIAL_NEW.txt"; echo "trial line" >> "$FX/CHANGELOG.md"
  G add -A >/dev/null; G commit -q -m "fixture: clean"; local c_clean; c_clean=$(G rev-parse HEAD)
  # Case hook: upstream inserts a line where the fork inserted `pub mod record;` (listed in HOOKS.md).
  G checkout -q --detach "$base"
  perl -0pi -e 's/^(pub mod realbrowser;\n)/$1pub mod recap;\n/m' "$FX/src/lib.rs"
  G add -A >/dev/null; G commit -q -m "fixture: hook"; local c_hook; c_hook=$(G rev-parse HEAD)
  # Case unlisted: the hook edit + upstream adds a file at a fork-owned path (add/add conflict).
  mkdir -p "$FX/src/record"; echo "// upstream file at a fork-owned path" > "$FX/src/record/names.rs"
  G add -A >/dev/null; G commit -q -m "fixture: unlisted"; local c_unl; c_unl=$(G rev-parse HEAD)
  for case in "clean:$c_clean:0:^RESULT clean" "hook:$c_hook:1:^CONFLICT src/lib.rs listed" "unlisted:$c_unl:2:^CONFLICT src/record/names.rs UNLISTED"; do
    IFS=: read -r name sha want pat <<< "$case"
    bash "$SELF" --upstream "$sha" > "$SCR0/$name.txt" 2>&1; r=$?
    if [ "$r" = "$want" ] && grep -qE "$pat" "$SCR0/$name.txt" && grep -q '^# invariants ok' "$SCR0/$name.txt"; then
      echo "SELFTEST PASS $name (exit $r)"
    else
      echo "SELFTEST FAIL $name (exit $r, want $want, pattern '$pat')"; sed 's/^/    /' "$SCR0/$name.txt"; fails=$((fails + 1))
    fi
  done
  echo "# selftest: $fails FAIL"
  return $fails
}

case "$MODE" in
  cleanup) cleanup_leftovers; exit 0 ;;
  selftest) selftest; exit $? ;;
esac

git rev-parse -q --verify "$UP^{commit}" > /dev/null || { say "ERROR: unknown ref $UP (git fetch upstream?)"; exit 4; }
BR_NAME=$(git symbolic-ref -q --short HEAD || echo DETACHED)
HEAD_BEFORE=$(git rev-parse HEAD)
TRACKED_BEFORE=$(git diff HEAD | shasum | cut -c1-12)
say "# trial_merge  $(date -u +%FT%TZ)  branch $BR_NAME $(git rev-parse --short HEAD)  upstream $UP $(git rev-parse --short "$UP")"
additive || exit 4

SCR=$(mktemp -d "${TMPDIR:-/tmp}/bbmerge.XXXXXX"); WT="$SCR/wt"; TB="trial-merge/$(date +%Y%m%d%H%M%S)-$$"
cleanup() {
  git worktree remove --force "$WT" > /dev/null 2>&1; git worktree prune
  git branch -D "$TB" > /dev/null 2>&1; rm -rf "$SCR"
}
trap cleanup EXIT
git worktree add -q -b "$TB" "$WT" HEAD || { say "ERROR: git worktree add failed"; exit 4; }
MOUT=$(git -C "$WT" -c user.name=trial-merge -c user.email=trial-merge@invalid merge --no-commit --no-ff "$UP" 2>&1); MRC=$?
say "## trial merge of $UP into $TB (worktree under $SCR)"
say "$MOUT"
CONF=$(git -C "$WT" diff --name-only --diff-filter=U)
RESULT=0
if [ -n "$CONF" ]; then
  for f in $CONF; do
    if is_listed "$f"; then say "CONFLICT $f listed"; [ "$RESULT" -ge 1 ] || RESULT=1
    else say "CONFLICT $f UNLISTED"; RESULT=2; fi
    if grep -q '^<<<<<<< ' "$WT/$f" 2>/dev/null; then
      awk '/^<<<<<<< /{p=1} p{print "  " FILENAME ":" FNR ": " $0} /^>>>>>>> /{p=0}' "$WT/$f" | sed "s#$WT/##" | while IFS= read -r l; do say "$l"; done
    else
      say "  (no conflict markers: modify/delete or binary conflict)"
    fi
  done
elif [ "$MRC" != 0 ]; then
  say "ERROR: merge failed without conflicts (exit $MRC)"; RESULT=4
fi
[ "$UNLISTED_TOUCHED" = 0 ] || RESULT=2
if [ "$RESULT" = 0 ] && [ -n "$GATE" ]; then
  say "## gate in the merged worktree: $GATE"
  gate; GR=$?; [ "$GR" = 0 ] || RESULT=$GR
fi
git -C "$WT" merge --abort > /dev/null 2>&1
cleanup; trap - EXIT
if [ "$(git rev-parse HEAD)" = "$HEAD_BEFORE" ] && [ "$(git symbolic-ref -q --short HEAD || echo DETACHED)" = "$BR_NAME" ] \
   && [ "$(git diff HEAD | shasum | cut -c1-12)" = "$TRACKED_BEFORE" ] && ! git rev-parse -q --verify "refs/heads/$TB" > /dev/null \
   && ! git worktree list | grep -qF "$WT"; then
  say "# invariants ok: $BR_NAME still at $(git rev-parse --short HEAD), tracked files unchanged, trial branch and worktree removed"
else
  say "# INVARIANT BROKEN: check git status, git worktree list, git branch --list 'trial-merge/*'"; RESULT=4
fi
case "$RESULT" in
  0) say "RESULT clean" ;;
  1) say "RESULT conflicts only in HOOKS.md-listed files: resolve per HOOKS.md, then rerun the gate" ;;
  2) say "RESULT a touched or conflicted path is not in HOOKS.md: add the hook row or move the change into fork-owned files" ;;
  3) say "RESULT gate failed" ;;
  *) say "RESULT error" ;;
esac
exit "$RESULT"
