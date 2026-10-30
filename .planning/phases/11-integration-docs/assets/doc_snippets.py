#!/usr/bin/env python3
"""Every fenced code block in docs/recording-and-launch-config.md must be backed by
evidence from a real run (Phase 11, DOC-01).

  python3 -I doc_snippets.py list  [DOC]
  python3 -I doc_snippets.py check MAP [DOC]

list   prints one line per fenced block: <id> L<line> <lang> | <first line>
       <id> = first 8 hex of sha1 of the block body (the lines between the fences).
check  MAP is a tab-separated file, one row per block:
         <id> <TAB> <kind> <TAB> <label> <TAB> <evidence file, relative to MAP's directory>
       kind: run | config | output
       Every block id in DOC must have a row and every row id must still be in DOC.
       The evidence file must exist and contain the line `SNIPPET <label>`.
       For run and config rows it must also contain the block body verbatim
       (trailing spaces ignored), i.e. the exact text that was run.
       Prints one PASS/FAIL line per block and exits 1 on any FAIL."""
import hashlib, os, sys

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
DEFAULT_DOC = os.path.join(REPO, "docs", "recording-and-launch-config.md")


def norm(text):
    return "\n".join(l.rstrip() for l in text.strip("\n").splitlines())


def blocks(doc):
    out, body, start, lang = [], None, 0, ""
    with open(doc, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            s = line.rstrip("\n")
            if body is None and s.lstrip().startswith("```"):
                body, start, lang = [], n, s.strip()[3:].strip() or "-"
            elif body is not None and s.strip() == "```":
                text = "\n".join(body)
                bid = hashlib.sha1(norm(text).encode()).hexdigest()[:8]
                out.append((bid, start, lang, text))
                body = None
            elif body is not None:
                body.append(s)
    if body is not None:
        sys.exit(f"unclosed fence opened at line {start}")
    return out


def main():
    if len(sys.argv) < 2 or sys.argv[1] not in ("list", "check"):
        sys.exit(__doc__)
    if sys.argv[1] == "list":
        doc = sys.argv[2] if len(sys.argv) > 2 else DEFAULT_DOC
        for bid, line, lang, text in blocks(doc):
            first = (text.strip().splitlines() or [""])[0]
            print(f"{bid} L{line} {lang} | {first[:90]}")
        return
    mp = sys.argv[2]
    doc = sys.argv[3] if len(sys.argv) > 3 else DEFAULT_DOC
    rows = {}
    with open(mp, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            if not line.strip() or line.startswith("#"):
                continue
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 4 or parts[1] not in ("run", "config", "output"):
                sys.exit(f"{mp}:{n}: want <id>\\t<run|config|output>\\t<label>\\t<file>")
            rows[parts[0]] = parts[1:]
    fails, seen = 0, set()
    for bid, line, lang, text in blocks(doc):
        seen.add(bid)
        row = rows.get(bid)
        if row is None:
            print(f"FAIL {bid} L{line}: no row in the map (run `list`, run the block, add a row)")
            fails += 1
            continue
        kind, label, rel = row
        path = os.path.join(os.path.dirname(os.path.abspath(mp)), rel)
        if not os.path.isfile(path):
            print(f"FAIL {bid} L{line}: evidence file missing: {rel}")
            fails += 1
            continue
        with open(path, encoding="utf-8", errors="replace") as f:
            ev = f.read()
        if f"SNIPPET {label}" not in ev:
            print(f"FAIL {bid} L{line}: `SNIPPET {label}` not in {rel}")
            fails += 1
        elif kind in ("run", "config") and norm(text) not in norm(ev):
            print(f"FAIL {bid} L{line}: block text not found verbatim in {rel}")
            fails += 1
        else:
            print(f"PASS {bid} L{line} {kind} {label} -> {rel}")
    for bid in rows:
        if bid not in seen:
            print(f"FAIL {bid}: map row for a block that is no longer in the doc")
            fails += 1
    print(f"{len(seen)} blocks, {fails} failing")
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()
