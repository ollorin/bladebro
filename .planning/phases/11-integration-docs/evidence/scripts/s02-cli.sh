#!/usr/bin/env bash
# 11-01: CLI daemon lane. Example 5 (per-test recording), the CLI forms of the two ops, --json,
# the run-wrapped recording (known issue: empty timeline), the click verdict miss (known issue),
# and the recording-name rules.
set -u
source "$(dirname "$0")/common.sh"
F="http://127.0.0.1:8765"
snip per-test-cli per-test-cli.sh
( bash -x "$SN/per-test-cli.sh" ) 2>&1
for v in $(ls "$BLADE_HOME"/recordings/*.webm 2>/dev/null); do probe "$v"; done
echo "files: $(ls "$BLADE_HOME"/recordings | tr '\n' ' ')"

echo; echo "SNIPPET cli-forms"
export BLADE_RECORD=1
bladebro nav "$F/mark.html" >/dev/null 2>&1
echo '$ bladebro state record-start login-flow';                         bladebro state record-start login-flow; echo "exit=$?"
sleep 1
echo '$ bladebro state record-stop';                                     bladebro state record-stop; echo "exit=$?"
echo '$ bladebro state record-start login-flow --json';                  bladebro state record-start login-flow --json; echo; echo "exit=$?"
sleep 1
echo '$ bladebro state record-stop --value "FAIL: login button did nothing" --json'
bladebro state record-stop --value "FAIL: login button did nothing" --json; echo; echo "exit=$?"
echo '$ bladebro state record-stop   (none active)';                     bladebro state record-stop; echo "exit=$?"
echo '$ bladebro state record-stop --value x   (none active, with a note)'; bladebro state record-stop --value x; echo "exit=$?"

echo; echo "SNIPPET known-in-run-timeline"
echo '$ bladebro run [state record-start, click, state record-stop]'
R=$(bladebro run '[{"action":"state","op":"record-start","name":"inrun"},{"action":"click","text":"turn green"},{"action":"state","op":"record-stop","value":"in-run"}]' 2>&1)
printf '%s\n' "$R" | grep -E '^step|recording'
V=$(saved_path "$R"); TL="${V%.*}.timeline.json"
echo "timeline entries: $(python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); print([e["kind"] for e in d["entries"]])' "$TL")"
echo '$ the same three calls as separate commands'
bladebro state record-start sep >/dev/null; sleep 1
bladebro act click "turn blue" | head -1
sleep 1; S=$(bladebro state record-stop --value sep); echo "$S"
TL="$(saved_path "$S")"; TL="${TL%.*}.timeline.json"
echo "timeline entries: $(python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); print([e["kind"] for e in d["entries"]])' "$TL")"

echo; echo "SNIPPET known-click-verdict"
bladebro nav "$F/mark.html" >/dev/null 2>&1
bladebro state record-start verdict >/dev/null; sleep 1
echo '$ bladebro act click "turn blue"'; bladebro act click "turn blue" | head -1
sleep 1; S=$(bladebro state record-stop); V=$(saved_path "$S"); PO="${V%.*}.poster.png"
echo "poster 1x1 rgb (blue page = low red, high blue): $("$HOME/.local/ffmpeg/bin/ffmpeg" -nostdin -v error -i "$PO" -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | tr -s ' ')"

echo; echo "SNIPPET name-rules"
for n in '../../evil name%d' "$(printf 'a%.0s' $(seq 1 100))" '...' 'ok.name_1-2'; do
  echo "\$ bladebro state record-start '$n'"
  bladebro state record-start "$n" 2>&1 | sed -E 's/ \(tab [^)]*\).*//'; echo "exit=${PIPESTATUS[0]}"
  sleep 0.5; bladebro state record-stop >/dev/null 2>&1
done
echo '$ bladebro state record-start ""'; bladebro state record-start "" 2>&1 | head -2; echo "exit=${PIPESTATUS[0]}"
bladebro state record-stop >/dev/null 2>&1
