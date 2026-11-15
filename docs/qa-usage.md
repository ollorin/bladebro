# Using bladebro as a UI QA driver

How to run an exploratory or scripted QA pass over a local site with the CLI (the MCP tools take the same parameters). Every option is documented in full in [recording-and-launch-config.md](recording-and-launch-config.md); this page is the short path.

## Recommended environment

Set these in the shell that starts the first `bladebro` command. The daemon reads its environment once at start, so run `bladebro stop` first if one is already running.

```bash
export BLADE_HEADLESS=1          # hidden browser
export BLADE_NO_WARMING=1        # no first-run visits to google/github/wikipedia
export BLADE_PACE=off            # no human-like gaps between actions
export BLADE_NO_UPDATE_CHECK=1   # no update check
export BLADE_HOME=$(mktemp -d)/home   # isolated daemon, profile, artifacts
export BLADE_RECORD=1            # recording on (optional, see below)
```

Recording prerequisites (only when you want video):

- `BLADE_RECORD=1` (or an absolute directory). Without it `state record-start` is refused with `recording is off: the operator has not enabled it. Set BLADE_RECORD=1`. That gate is deliberate and does not change.
- ffmpeg 4.3 or newer on `PATH`, or `BLADE_FFMPEG=/abs/path/to/ffmpeg`. `bladebro doctor` shows what was found.
- Optional: `BLADE_RECORD_FORMAT=webm|mp4`, `BLADE_RECORD_FPS=5-15`, `BLADE_RECORD_SIZE=1280x720`, `BLADE_RECORD_TABS=follow|per-tab`.
- Recording needs the daemon (CLI) or MCP server; a one-shot `--no-daemon` run has no session to record.

`bladebro help state` lists the same set.

## Viewport

```bash
bladebro act viewport --width 375 --height 800            # CSS px, 100-10000
bladebro act viewport --width 390 --height 844 --dpr 3 --mobile
bladebro act viewport --reset
```

The override is CDP device-metrics emulation. It persists across navigations and tab switches in the daemon session until `--reset`, `vision` screenshots reflect it (the output names the emulated size), and the command reports the page's own `innerWidth`/`innerHeight` as the verdict. Without a daemon (`--no-daemon`) each command is a fresh browser, so use the daemon, or put the viewport step in the same `run`.

## Audit

```bash
bladebro see audit                    # counts + top 5 findings per kind
bladebro see audit --limit 10
bladebro see audit --full             # every finding
bladebro see audit --format json      # same report as JSON (also trimmed unless --full)
bladebro see audit --min-target 44    # stricter touch targets (default 24 CSS px, WCAG 2.2 SC 2.5.8)
bladebro see audit --links off        # skip the link check (default: same-origin only)
bladebro see audit --links all --max-links 50   # also cross-origin, checked from the driver (no cookies)
```

Read-only. It reports, each with a selector and a reason:

| Kind | What it flags |
|---|---|
| `contrast` | text below 4.5:1 (3:1 for large text) against its resolved background; text over images/gradients is skipped, not guessed |
| `names` | buttons, links, form controls and images without an accessible name or `alt`; missing `lang` and `<title>`; placeholder-only inputs |
| `duplicate-ids` | ids used more than once |
| `overflow` | document `scrollWidth` wider than the viewport, and the outermost elements extending past the edge (clipped scroll containers and fixed elements excluded) |
| `small-targets` | interactive elements under `--min-target` that also have another target within the WCAG 2.5.8 spacing circle; inline links inside text are exempt |
| `broken-links` | same-origin `<a href>` answering 4xx/5xx or failing (HEAD, then GET on 405/501), capped by `--max-links` (default 20); needs an http(s) page, not `file://` |

It is a self-implemented subset, not axe-core: axe-core is MPL-2.0 (file-level copyleft against this repo's Apache-2.0), about 550 KB, and would need a deny/licence review for the release binary. The checks above cover the layout and form issues a QA pass hits; they do not replace a full WCAG audit (no ARIA-validity, focus-order or landmark rules).

## Screenshots

```bash
bladebro vision                 # visible viewport (default)
bladebro vision --full-page     # the whole scrollable page (capped at 16384 CSS px tall)
```

The output says which: `screenshot (viewport 375x800 CSS px ...)` or `screenshot (full page 375x2400 CSS px)`, plus `(emulated viewport ...)` when `act viewport` is active. The PNG path is printed (`--json`: `image_path`).

## `act eval` results

`bladebro act eval "..."` prints `result: <compact JSON>`. With `--json` the object also carries `data`: when the script returns an object or array, `data` is that value parsed, so no second `JSON.parse`; strings and scalars get no `data` field (read `text`). `text` is unchanged.

```bash
bladebro --json act eval "({w: innerWidth, links: document.links.length})"
# {"ok":true,"is_error":false,"text":"result: {\"w\":375,\"links\":4}","data":{"w":375,"links":4}}
```

MCP clients get the same value as `structuredContent` next to the text content.

## One page at three widths

One `run` call; the audit step is read inline and its output lands in the result:

```bash
bladebro run '[
  {"action":"navigate","url":"http://localhost:3000/pricing"},
  {"action":"viewport","width":375,"height":800},
  {"action":"see","mode":"audit"},
  {"action":"viewport","width":768,"height":1024},
  {"action":"see","mode":"audit"},
  {"action":"viewport","width":1280,"height":800},
  {"action":"see","mode":"audit"},
  {"action":"viewport","reset":true}
]'
```

Add `{"action":"state","op":"record-start","name":"pricing-widths"}` at the start and `{"action":"state","op":"record-stop"}` at the end for a video (see the prerequisites above). A screenshot per width is a separate call, because `vision` is its own tool: run `bladebro act viewport --width 375 --height 800` then `bladebro vision --full-page`.

Finish with `bladebro stop`.
