#!/usr/bin/env bash
# 11-01: the env-var table's values and the troubleshooting texts, from real runs: one bad value per
# recording variable (each in a fresh daemon), recording off, an invalid BLADE_HEADLESS, a
# group-writable .bladebro.json and a record.dir outside the project.
set -u
source "$(dirname "$0")/common.sh"
F="http://127.0.0.1:8765"
echo; echo "SNIPPET env-errors"
try() {  # try VAR=VALUE: start a fresh daemon with that one setting and record-start
  bladebro stop >/dev/null 2>&1
  echo "\$ $1 bladebro state record-start x   (BLADE_RECORD=1)"
  env BLADE_RECORD=1 "$1" bash -c "bladebro nav '$F/a.html' >/dev/null 2>&1; bladebro state record-start x 2>&1 | head -2; bladebro state record-stop >/dev/null 2>&1"
}
try BLADE_RECORD=relative/dir
try BLADE_RECORD_FORMAT=avi
try BLADE_RECORD_FPS=20
try BLADE_RECORD_SIZE=161x120
try BLADE_RECORD_TABS=both
try BLADE_FFMPEG=/nonexistent/ffmpeg
try BLADE_RECORD_FPS=15
bladebro stop >/dev/null 2>&1

echo; echo "SNIPPET troubleshooting"
echo '$ (BLADE_RECORD unset) bladebro state record-start x'
bladebro nav "$F/a.html" >/dev/null 2>&1; bladebro state record-start x 2>&1 | head -1
echo '$ export BLADE_RECORD=1 without restarting the daemon, then record-start'
BLADE_RECORD=1 bladebro state record-start x 2>&1 | head -1
bladebro stop >/dev/null 2>&1
echo '$ BLADE_HEADLESS=maybe bladebro nav ...'
BLADE_HEADLESS=maybe bladebro nav "$F/a.html" 2>&1 | head -2; echo "exit=${PIPESTATUS[0]}"
PROJ=$(mktemp -d); mkdir "$PROJ/.git"; echo '{"headless": true}' > "$PROJ/.bladebro.json"; chmod 664 "$PROJ/.bladebro.json"
echo '$ group-writable .bladebro.json: bladebro headless status'
( cd "$PROJ" && unset BLADE_HEADLESS && bladebro headless status 2>&1 )
echo '{"headless": true, "record": {"dir": "../outside"}}' > "$PROJ/.bladebro.json"; chmod 644 "$PROJ/.bladebro.json"
echo '$ record.dir outside the project: record-start from the project dir'
( cd "$PROJ" && BLADE_RECORD=1 bladebro nav "$F/a.html" 2>&1 | grep -iE 'record|warn' | head -3; BLADE_RECORD=1 bladebro state record-start x 2>&1 | grep -E '^(recording|✗)' | head -1; bladebro state record-stop >/dev/null 2>&1; bladebro stop >/dev/null 2>&1 )
rm -rf "$PROJ"
