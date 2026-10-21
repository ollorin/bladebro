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
- Launch note: each managed launch process prints exactly one stderr line `[bladebro] browser mode: <mode> (source: <source>)`. Modes: `headless`, `headed`, `headed (virtual display)`, `headless (no Xvfb; fallback)`. Source: `env BLADE_HEADLESS`, `project file <path>`, `user config <path>` or `default`. stdout is untouched, so `--json` and MCP output stay machine-clean.
- Stealth: a forced-headless launch sets the launched-headless flag, so the headless-only masks apply. The "no Xvfb display was available" advisory is not emitted when headless was requested, since it would be false.
- The real-browser lane ignores `BLADE_HEADLESS`; use `bladebro rb visible on|off` there.
- Older workaround: `BLADE_CHROME_FLAGS="--headless=new"`. Replace it with `BLADE_HEADLESS=1`: with raw flags, stealth believed the launch was headed.

## Recording settings

> **Not functional yet.** The config and output-path layer exists; the recorder itself is not wired in. Setting these variables records nothing today.

**Recordings contain whatever was on screen**: passwords as typed into visible fields, session tokens, personal data, private pages. Treat every recording as sensitive. Do not enable recording for sessions that use real credentials. bladebro never prunes recordings; delete them in your harness.

Config file: `<data root>/record.json` (data root: `~/.blade`, or `BLADE_HOME` when set; on Windows `%USERPROFILE%\.blade`). Fields: `enabled` (bool), `dir` (absolute path), `format`, `fps`, `size`, `quality` (JPEG 1-100, default `70`, file only). Unknown fields are an error. Precedence: per-call > env > project file > `record.json` > default (see [Configuration layers and precedence](#configuration-layers-and-precedence)). Per-call input covers `format`, `fps`, `size` and `name` only; `enabled`, `dir` and `quality` have no per-call layer. A bad value in any layer is an error, even when a higher layer overrides it. Invalid values are rejected, never clamped. Errors name their source (`BLADE_RECORD_FPS:`, `record.json fps:`, `per-call fps:`).

Where files go:
- Default dir: `<data root>/recordings/`. A dir inside `<data root>/artifacts/` is refused (artifacts rotate at 300 files). Nothing in bladebro rotates, prunes or deletes recordings.
- File names: `<name>-<UTC timestamp>-<id>.webm` (or `.mp4`). The name keeps only `A-Z a-z 0-9 . _ -`, is capped at 80 characters, and falls back to `recording`. Existing files are never overwritten.
- Unix: a new or empty dir is set to `0700`; files are `0600`. A non-empty dir that is group- or other-accessible is refused (run `chmod 700` on it, or use a dedicated dir). A symlinked dir and a dir owned by another user are refused.
- Windows: files inherit the directory's permissions. The default under your profile is private to your account; keep `BLADE_RECORD` inside your profile. bladebro sets no Windows ACLs.

## Configuration layers and precedence

Highest layer first. A layer that does not set a value falls through to the next one.

| Setting | per-call | env | project `.bladebro.json` | user file | default |
|---|---|---|---|---|---|
| headless | none (suite-level by design) | `BLADE_HEADLESS` | `headless` | `<data root>/launch.json` | platform default |
| recording on/off | none | `BLADE_RECORD` | `record.enabled` | `record.json` `enabled` | off |
| recording dir | none | `BLADE_RECORD` (absolute path) | `record.dir` (vetted, see below) | `record.json` `dir` | `<data root>/recordings` |
| format, fps, size | yes | `BLADE_RECORD_FORMAT`, `BLADE_RECORD_FPS`, `BLADE_RECORD_SIZE` | `record.format`, `record.fps`, `record.size` | `record.json` | `webm`, 10, 1280x720 |
| quality | none | none | `record.quality` | `record.json` `quality` | 70 |

Per-call input exists only for recording `format`, `fps`, `size` and `name`. Headless has no per-call layer: it is a property of the whole browser launch, not of a call.

## Project file (.bladebro.json)

A JSON file that travels with a project (commit it, or keep it local):

```json
{
  "headless": true,
  "record": { "enabled": true, "dir": "recordings", "fps": 8 }
}
```

Keys: `headless` (bool) and `record` with `enabled`, `dir`, `format`, `fps`, `size`, `quality`. Both levels are strict: an unknown key, a bad value, invalid JSON or a file over 64 KiB is an error that names the file.

Discovery (from the current directory upward):
- The first `.bladebro.json` found wins.
- The walk stops after a directory that contains `.git`: a file in a parent folder cannot steer a checkout.
- The home directory is never read as a project (user settings belong in `launch.json`), and the walk stops at the filesystem root or after 64 levels.

Trust rules (Unix): the file must be a regular file (not a symlink), owned by the current user and not group/other-writable. Otherwise it is ignored with a stderr warning naming the path and the reason. Windows has no owner or mode check.

`record.dir`: a relative path is joined to the project directory. The resolved path (symlinks followed) must be inside the project directory or `<data root>/recordings`. Anything else (`..`, an absolute path elsewhere, a symlinked subfolder pointing out) is dropped with a warning and the next layer or default applies. Recordings may contain secrets: add a project-local recordings dir to `.gitignore`. The artifacts-folder guard still applies afterwards.

## User config files

- `<data root>/launch.json`: `{ "headless": true|false }`, written by `bladebro headless on|off`. Atomic write, mode `0600` on Unix. Unknown keys are an error.
- `<data root>/record.json`: recording settings, see Recording settings.

## bladebro headless on|off|status

- `bladebro headless status` prints `browser mode: <headless|headed|platform default> (source: <where>)`. When `launch.json` is overridden by the environment or a project file, a second line says `user config <path> says on|off but <source> takes precedence`.
- `bladebro headless on|off` writes `launch.json`, prints `headless: on (saved to <path>)`, the fresh status line, and `applies at the next browser launch; run bladebro stop first if a daemon is running`.
- `--json` prints one object `{"ok":true,"is_error":false,"text":"..."}`. An unknown subcommand is a usage error (exit 2).
- `bladebro doctor` has one `Browser launch mode` line with the mode and its source; an invalid setting fails the check with the error.

## Daemon and MCP behavior

The browser mode is pinned per process. A CLI command whose mode (headless or not) differs from the running daemon's is refused with exit 1:

```
browser mode mismatch: the running daemon's browser is headless (source: ...) but this command resolves to headed (source: ...). Run `bladebro stop`, then retry; the next command starts the browser in the requested mode.
```

Run `bladebro stop` and retry. There is no automatic restart: it would kill another session's browser. `bladebro stop` always works. Different sources with the same resulting behavior are not a mismatch. The real-browser lane ignores headless, so it is never refused. The MCP server reads its mode from its own environment and directory at first launch; restart it to change the mode. Windows has no daemon (commands run one-shot), so no mismatch arises there. A daemon started by a previous build ignores the new field: run `bladebro stop` once after upgrading.

## Threat model: project files

A cloned repository controls its `.bladebro.json`, so the file is treated as untrusted input.
- It can set `headless` and recording `enabled`, `format`, `fps`, `size`, `quality`.
- It cannot redirect recordings outside the project directory or `<data root>/recordings`.
- It cannot run commands: no key in the schema executes anything.
- A symlinked, foreign-owned or group/other-writable file is ignored with a warning (Unix).
- A file in a parent folder cannot cross a `.git` boundary, and a file in your home directory is never read.
- Size is capped at 64 KiB and unknown keys are rejected.
