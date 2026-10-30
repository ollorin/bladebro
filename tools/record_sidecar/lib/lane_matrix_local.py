#!/usr/bin/env python3
"""Run tools/lane_matrix.py unchanged except for two environment adaptations (run with `python3 -I`).

    REC_HUB_FIX=http://127.0.0.1:8765 python3 -I lane_matrix_local.py <lane> <rounds>

1. tools/lane_matrix.py sweeps leftover processes through /proc, which does not exist on macOS (its
   cleanup raises FileNotFoundError after the first lane). `_pids_under` is replaced by a `ps`-based
   equivalent with the same filter.
2. The matrix navigates to https://example.com/ before probing; the probes read GL identity and
   display geometry, which do not depend on the page. For the local-fixtures-only rule the URL is
   replaced by $REC_HUB_FIX/idle.html.

Nothing else is touched: lanes, probes, classification and exit codes are lane_matrix.py's own. The
original file is read, patched in memory and executed; it is never rewritten.
"""
import os
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
SRC = os.path.join(REPO, "tools", "lane_matrix.py")
FIX = os.environ.get("REC_HUB_FIX", "").rstrip("/")
if not FIX.startswith("http://127.0.0.1"):
    sys.exit("REC_HUB_FIX must be a http://127.0.0.1:PORT fixture base URL")

PS_SWEEP = '''
def _pids_under(home):
    import subprocess as _sp
    out = []
    for line in _sp.run(["ps", "-ax", "-o", "pid=,command="], capture_output=True, text=True).stdout.splitlines():
        pid, _, cmd = line.strip().partition(" ")
        if home in cmd and "chrome_crashpad" not in cmd and "--type=" not in cmd and pid.isdigit():
            out.append(int(pid))
    return out
'''

code = open(SRC).read().replace("https://example.com/", FIX + "/idle.html")
start = code.index("def _pids_under(home):")
end = code.index("\ndef lane_daemon", start)
code = code[:start] + PS_SWEEP.lstrip("\n") + "\n" + code[end:]
sys.argv = [SRC] + sys.argv[1:]
exec(compile(code, SRC, "exec"), {"__name__": "__main__", "__file__": SRC})
