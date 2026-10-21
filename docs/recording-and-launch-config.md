# Recording and launch configuration

Single reference for every bladebro-fork option: session recording, the ffmpeg override and the managed browser's launch mode. Each option documented here is also reachable from the README.

## Environment variables

| Var | Default | Values | What it does |
|---|---|---|---|
| `BLADE_HEADLESS` | unset (platform default) | `1`/`on`/`true`/`yes`: hidden browser on every OS. `0`/`off`/`false`/`no`: visible browser. Anything else is an error naming the value | Forces the managed browser hidden or visible. Managed lane only |
| `BLADE_RECORD` | off | `1`/`on`/`true`/`yes`: on, default dir. `0`/`off`/`false`/`no`: off. Absolute path: on, record into that dir. Relative paths are rejected | Session recording switch |
| `BLADE_RECORD_FORMAT` | `webm` | `webm` (VP9) or `mp4` (H.264) | Recording container |
| `BLADE_RECORD_FPS` | `10` | Whole frames per second, 5-15 | Recording frame rate |
| `BLADE_RECORD_SIZE` | `1280x720` | `WIDTHxHEIGHT`, both sides even, 160x120 to 3840x2160 | Recording frame size |
| `BLADE_FFMPEG` | auto | Absolute path to the ffmpeg binary used for recording. Set but unusable is reported, never silently ignored. Otherwise PATH, then known install locations. `bladebro doctor` shows what was found | ffmpeg override |

## Browser launch mode (BLADE_HEADLESS)

What each setting does to the managed browser, per OS:

| OS | unset or `0` | `1` |
|---|---|---|
| macOS | native window (visible) | `--headless=new`, no window |
| Windows | native window (visible) | `--headless=new`, no window |
| Linux | headful on bladebro's private Xvfb display (invisible to you by design, best stealth); if Xvfb is missing, the existing honest headless fallback with its warning | `--headless=new`, Xvfb is never started and no Xvfb warning is printed |

Notes:

- Unset and `0` behave identically on every OS. `0` exists to override a lower layer (a later config file) that says headless.
- On Linux, "headed" means headful on bladebro's own Xvfb display, not a window on your desktop. Showing a window on your own Linux desktop is not offered: the managed lane strips Wayland and pins X11 to Xvfb.
- The mode is decided once per process. The daemon and the MCP server keep one mode for their lifetime, so restart them (`bladebro stop`) to change it. A GL-ladder or crash relaunch never flips the mode.
- Launch note: each managed launch process prints exactly one stderr line `[bladebro] browser mode: <mode> (source: <source>)`. Modes: `headless`, `headed`, `headed (virtual display)`, `headless (no Xvfb; fallback)`. Source: `env BLADE_HEADLESS` or `default`. stdout is untouched, so `--json` and MCP output stay machine-clean.
- Stealth: a forced-headless launch sets the launched-headless flag, so the headless-only masks apply. The "no Xvfb display was available" advisory is not emitted when headless was requested, since it would be false.
- The real-browser lane ignores `BLADE_HEADLESS`; use `bladebro rb visible on|off` there.
- Older workaround: `BLADE_CHROME_FLAGS="--headless=new"`. Replace it with `BLADE_HEADLESS=1`: with raw flags, stealth believed the launch was headed.

## Recording settings

> **Not functional yet.** The config and output-path layer exists; the recorder itself is not wired in. Setting these variables records nothing today.

**Recordings contain whatever was on screen**: passwords as typed into visible fields, session tokens, personal data, private pages. Treat every recording as sensitive. Do not enable recording for sessions that use real credentials. bladebro never prunes recordings; delete them in your harness.

Config file: `<data root>/record.json` (data root: `~/.blade`, or `BLADE_HOME` when set; on Windows `%USERPROFILE%\.blade`). Fields: `enabled` (bool), `dir` (absolute path), `format`, `fps`, `size`, `quality` (JPEG 1-100, default `70`, file only). Unknown fields are an error. Precedence: per-call > env > `record.json` > default. Per-call input covers `format`, `fps`, `size` and `name` only; `enabled`, `dir` and `quality` have no per-call layer. A bad value in any layer is an error, even when a higher layer overrides it. Invalid values are rejected, never clamped. Errors name their source (`BLADE_RECORD_FPS:`, `record.json fps:`, `per-call fps:`).

Where files go:
- Default dir: `<data root>/recordings/`. A dir inside `<data root>/artifacts/` is refused (artifacts rotate at 300 files). Nothing in bladebro rotates, prunes or deletes recordings.
- File names: `<name>-<UTC timestamp>-<id>.webm` (or `.mp4`). The name keeps only `A-Z a-z 0-9 . _ -`, is capped at 80 characters, and falls back to `recording`. Existing files are never overwritten.
- Unix: a new or empty dir is set to `0700`; files are `0600`. A non-empty dir that is group- or other-accessible is refused (run `chmod 700` on it, or use a dedicated dir). A symlinked dir and a dir owned by another user are refused.
- Windows: files inherit the directory's permissions. The default under your profile is private to your account; keep `BLADE_RECORD` inside your profile. bladebro sets no Windows ACLs.
