#!/usr/bin/env bash
# 11-01: tab following (follow result, tab closed), state tabs while a window-feature popup is
# followed (Phase 8.1 semantics: input on the opener does not bring the video back; switch-tab does),
# and per-tab files.
set -u
source "$(dirname "$0")/common.sh"
F="http://127.0.0.1:8765"
export BLADE_RECORD=1
tab_id() { printf '%s\n' "$1" | grep -oE '[0-9A-F]{32}' | head -1; }   # first full target id in a text
star_id() { bladebro state tabs | awk '/^\*/{print $2; exit}'; }         # the agent's tab

echo; echo "SNIPPET follow-result"
bladebro nav "$F/a.html" >/dev/null 2>&1
A=$(star_id); echo "opener tab: $A"
bladebro state record-start f | sed 's/^/start: /'; sleep 1.5
O=$(bladebro state open-tab "$F/b.html"); echo "open-tab: $(printf '%s' "$O" | head -1)"; sleep 1.5
bladebro state switch-tab "$A" | head -1; sleep 1.5
bladebro state record-stop
echo '-- tab closed while recording:'
bladebro state record-start c >/dev/null; sleep 1.5
O=$(bladebro state open-tab "$F/b.html"); B=$(star_id); echo "second tab: $B"; sleep 1.5
bladebro state close-tab "$B" | head -1; sleep 1.5
bladebro state record-stop

echo; echo "SNIPPET tabs-popup"
echo "SNIPPET window-popup"
bladebro nav "$F/popper.html" >/dev/null 2>&1
P=$(star_id); echo "opener: $P"
bladebro state record-start pop >/dev/null; sleep 1.5
echo '$ bladebro act click "open window"   (window.open with width/height features)'; bladebro act click "open window" | head -1
sleep 1.5
echo '$ bladebro state tabs'; bladebro state tabs
echo '$ bladebro act press Tab   (input on the opener)'; bladebro act press Tab | head -1; sleep 1.5
echo '$ bladebro state tabs'; bladebro state tabs
echo "\$ bladebro state switch-tab $P   (the opener, where the agent already is)"; bladebro state switch-tab "$P" | head -1; sleep 1.5
echo '$ bladebro state tabs'; bladebro state tabs
W=$(bladebro state tabs | awk '/^ R/{print $2; exit}')
echo "\$ bladebro state close-tab $W   (the popup)"; bladebro state close-tab "$W" | head -1; sleep 1.5
echo '$ bladebro state tabs'; bladebro state tabs
echo '-- a second window-feature popup, then switch-tab to another tab and back to the opener:'
bladebro act click "open window" | head -1; sleep 1.5
echo '$ bladebro state tabs'; bladebro state tabs
OTH=$(bladebro state tabs | awk '/^   /{print $1; exit}')
echo "\$ bladebro state switch-tab $OTH   (another tab)"; bladebro state switch-tab "$OTH" | head -1; sleep 1.5
echo "\$ bladebro state switch-tab $P   (back to the opener)"; bladebro state switch-tab "$P" | head -1; sleep 1.5
echo '$ bladebro state tabs'; bladebro state tabs
bladebro state record-stop

echo; echo "SNIPPET per-tab-result"
bladebro stop >/dev/null 2>&1
export BLADE_RECORD_TABS=per-tab
bladebro nav "$F/a.html" >/dev/null 2>&1
bladebro state record-start pt >/dev/null; sleep 1.5
bladebro state open-tab "$F/b.html" | head -1; sleep 1.5
bladebro state record-stop
