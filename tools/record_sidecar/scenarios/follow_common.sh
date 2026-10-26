#!/usr/bin/env bash
# Shared live helpers for the record_sidecar scenarios (follow_live.sh, popup_live.sh): check/say
# bookkeeping, scratch-home lifecycle, state-tabs parsing, ffmpeg/ffprobe frame analysis.
# Sourced, never run directly. The caller sources ../env.sh first (BB, FFMPEG, FFPROBE, EVIDENCE)
# and sets ONLY itself (want reads it at call time).
#
# Recording markers: `state tabs` rows start with a 1-2 character marker. `*` is the agent's current
# tab (`* ID`, or `*R ID` / `*  ID` when a recording owns it); anything else (` `, ` R`) is not current.
# Only `*`-prefixed rows count as current; every row counts as a tab id.

FAILS=0
SCN_HOME=""
CUR_BIN="$BB"
TAP_PID=""

say() { echo "$*" | tee -a "$EVIDENCE"; }
check() { # check ID "description" <command that follows>
  local id="$1" desc="$2"; shift 2
  if "$@"; then say "PASS [$id] $desc"; else say "FAIL [$id] $desc"; FAILS=$((FAILS + 1)); fi
}
want() { [ -z "$ONLY" ] || [ "$ONLY" = "$1" ]; }

new_home() { SCN_HOME="$(mktemp -d -t bbfol)"; export BLADE_HOME="$SCN_HOME"; }

# Stop our daemon, then show nothing of ours remains (home basename survives the /private symlink).
end_scn() {
  local id="$1" tag left
  [ -n "$TAP_PID" ] && kill "$TAP_PID" 2>/dev/null && TAP_PID=""
  [ -n "$SCN_HOME" ] || return 0
  tag="$(basename "$SCN_HOME")"
  BLADE_HOME="$SCN_HOME" "$CUR_BIN" stop >/dev/null 2>&1
  # Chrome helpers can outlive the browser by a few seconds: poll, then judge.
  local n=0
  while :; do
    left=$( { pgrep -fl "$tag"; pgrep -fl ffmpeg; pgrep -fl cdp_tap; } 2>/dev/null | grep -v pgrep)
    { [ -z "$left" ] || [ $n -ge 10 ]; } && break
    n=$((n + 1)); sleep 1
  done
  if [ -z "$left" ]; then say "PASS [$id] cleanup: no daemon/Chrome/ffmpeg/cdp_tap left"
  else say "FAIL [$id] cleanup: leftovers:"; echo "$left" | tee -a "$EVIDENCE"; FAILS=$((FAILS + 1))
       pkill -f "$tag" 2>/dev/null; fi
  rm -rf "$SCN_HOME"; SCN_HOME=""; CUR_BIN="$BB"
}

now_ms() { python3 -I -c 'import time;print(int(time.time()*1000))'; }
saved_path() { sed -nE 's/.*recording saved: (.*) \(bytes .*/\1/p' <<<"$1" | head -1; }
num_after() { sed -nE "s/.*$2 ([0-9.]+).*/\1/p" <<<"$1" | head -1; }
cur_id() { "$BB" state tabs 2>&1 | sed -nE 's/^\*[R ]? *([0-9A-F]{32}) .*/\1/p' | head -1; }
opened_id() { sed -nE 's/.*tab ([0-9A-F]{32}).*/\1/p' <<<"$1" | head -1; }
# seg_ids TEXT / seg_offs TEXT: one entry per segment of `segments [0.0 s tab A, 4.2 s tab B]`
seg_inner() { sed -nE 's/.*segments \[([^]]*)\].*/\1/p' <<<"$1" | head -1; }
seg_ids() { seg_inner "$1" | tr ',' '\n' | sed -E 's/^ *[0-9.]+ s tab //; s/ \(no frames\)//'; }
seg_offs() { seg_inner "$1" | tr ',' '\n' | sed -E 's/^ *([0-9.]+) s tab .*/\1/'; }
rgb_at() { "$FFMPEG" -nostdin -v error -ss "$2" -i "$1" -frames:v 1 -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - 2>/dev/null | od -An -tu1 | tr -s ' ' | sed 's/^ //'; }
tone() { awk -v r="$1" -v g="$2" -v b="$3" 'BEGIN{ if(r>200&&g<60&&b<60)print "red"; else if(b>200&&r<60&&g<60)print "blue"; else if(g>120&&r<60&&b<60)print "green"; else if(r>200&&g>160&&b<60)print "yellow"; else print "other(" r "," g "," b ")"}'; }
tone_at() { local c; c=$(rgb_at "$1" "$2"); tone $c; }
# mid_of TEXT K TOTAL: midpoint (s) of segment K (0-based) on the video clock
mid_of() { python3 -I -c "
import sys
offs=[float(x) for x in '''$(seg_offs "$1")'''.split()]
k=int('$2'); tot=float('$3')
end=offs[k+1] if k+1<len(offs) else tot
print(round((offs[k]+end)/2,2))"; }
# analyse ID FILE TEXT: constant 1280x720 vp9, full clean decode, decoded frames == reported frames
analyse() {
  local id="$1" f="$2" text="$3" pj dec rc nb
  [ -s "$f" ] || { say "FAIL [$id] output file missing or empty: $f"; FAILS=$((FAILS + 1)); return; }
  pj=$("$FFPROBE" -v error -show_entries stream=codec_name,width,height:format=duration -of json "$f" | tr -d '\n ')
  say "[$id] ffprobe: $pj"
  dec=$("$FFMPEG" -nostdin -v error -i "$f" -f null - 2>&1 >/dev/null); rc=$?
  check "$id" "full decode exit=$rc stderr_lines=$(printf '%s' "$dec" | grep -c .)" test "$rc" = 0 -a -z "$dec"
  check "$id" "codec vp9 1280x720 (constant output size)" bash -c "echo '$pj' | grep -q '\"codec_name\":\"vp9\"' && echo '$pj' | grep -q '\"width\":1280' && echo '$pj' | grep -q '\"height\":720'"
  nb=$("$FFPROBE" -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 "$f")
  check "$id" "decoded frames ($nb) == reported frames ($(num_after "$text" frames))" test "$nb" = "$(num_after "$text" frames)"
}
ffmpeg_pids() { pgrep -f "ffmpeg.*$(basename "$SCN_HOME")" 2>/dev/null | tr '\n' ' '; }
# tab_ids: every tab id in `state tabs` output (any marker), one per line
tab_ids() { sed -nE 's/^[*R ]{1,2} *([0-9A-F]{32}) .*/\1/p'; }
