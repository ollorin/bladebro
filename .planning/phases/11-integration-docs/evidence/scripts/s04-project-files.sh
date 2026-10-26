#!/usr/bin/env bash
# 11-01: the two .bladebro.json examples (project file with record.dir + fps; suite file), each
# placed in a scratch checkout (a dir with .git) and used from that dir. BLADE_HEADLESS is unset
# here on purpose: both files say "headless": true, and that must be what hides the browser.
set -u
source "$(dirname "$0")/common.sh"
unset BLADE_HEADLESS
F="http://127.0.0.1:8765"

snip project-file project.bladebro.json
PROJ=$(mktemp -d); mkdir "$PROJ/.git"; cp "$SN/project.bladebro.json" "$PROJ/.bladebro.json"; chmod 644 "$PROJ/.bladebro.json"
cd "$PROJ" || exit 1
echo "\$ cd $PROJ && bladebro headless status"; bladebro headless status
bladebro nav "$F/mark.html" 2>&1 | head -1
bladebro state record-start proj; sleep 1
bladebro act click "turn blue" | head -1; sleep 1
bladebro state record-stop
echo "recordings in the project: $(ls "$PROJ/recordings" | tr '\n' ' ')"
for v in "$PROJ"/recordings/*.webm; do probe "$v"; done
echo "dir mode: $(stat -f %Lp "$PROJ/recordings")"
bladebro stop

snip suite-file suite.bladebro.json
SUITE=$(mktemp -d); mkdir "$SUITE/.git"; cp "$SN/suite.bladebro.json" "$SUITE/.bladebro.json"; chmod 644 "$SUITE/.bladebro.json"
cd "$SUITE" || exit 1
echo "\$ cd $SUITE && bladebro headless status"; bladebro headless status
echo '$ BLADE_RECORD unset: the suite file does not enable recording'
bladebro nav "$F/mark.html" 2>&1 | head -1
bladebro state record-start suite 2>&1 | head -1
bladebro stop
echo '$ BLADE_RECORD=1 (env enables it; format/fps come from the file)'
export BLADE_RECORD=1
bladebro nav "$F/mark.html" 2>&1 | head -1
bladebro state record-start suite; sleep 1.5
S=$(bladebro state record-stop); echo "$S"
V=$(saved_path "$S"); probe "$V"
bladebro stop
cd "$REPO"; rm -rf "$PROJ" "$SUITE"
