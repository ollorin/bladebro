# Developer setup without sudo (macOS, Apple Silicon)

How to get this fork building and testing on a Mac where you cannot use `sudo` and Homebrew is not available.
Everything goes under your home directory. Written from a real restore on 2026-10-30; versions are the ones that worked.

| Tool | Version | Where | Needed for |
|---|---|---|---|
| Xcode command line tools | any current | system (`xcode-select --install` needs admin once; often preinstalled) | C compiler, `make`, `git` |
| Rust (rustup, minimal profile) | 1.86+ (tested 1.99.1) | `~/.cargo`, `~/.rustup` | building bladebro |
| Node.js | 24.18.0 | `~/opt/node-v24.18.0-darwin-arm64` | page/stealth tests run JavaScript through `node` |
| ffmpeg + ffprobe | 7.1.1, with `libvpx-vp9` and `libx264` | `~/opt/ffmpeg` | recording and the live record tests |
| Chrome or Chromium | any current | `/Applications` | live-browser tests and real use |

pnpm is not needed for bladebro itself (it is a pure Rust project). The sibling `gsd-dsh` repo uses pnpm 11.7.0 via corepack.

## 1. Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
~/.cargo/bin/rustup component add rustfmt clippy   # the minimal profile has neither; AGENTS.md needs both
```

## 2. Node 24.18.0 (tarball, no installer)

```bash
mkdir -p ~/opt && curl -fsSL https://nodejs.org/dist/v24.18.0/node-v24.18.0-darwin-arm64.tar.gz | tar -xz -C ~/opt
# optional checksum: compare shasum -a 256 of the tarball with https://nodejs.org/dist/v24.18.0/SHASUMS256.txt
```

## 3. ffmpeg with VP9 and H.264, compiled from source

```bash
bash scripts/install-ffmpeg-no-sudo.sh
```

The script builds `libvpx` 1.15.0, `x264` (stable) and `ffmpeg` 7.1.1 into `~/opt/ffmpeg`, and deletes its build directory
(`~/src-build`, about 540 MB) when it finishes; set `KEEP_SRC=1` to keep it. It takes roughly 15 minutes. Two things it works around:

- macOS ships no `pkg-config`, and ffmpeg's configure needs one to find x264. The script writes a small Python shim to
  `~/opt/ffmpeg/bin/pkg-config`; it reads the `.pc` files that the two libraries install and ignores version constraints.
- `ffmpeg.org` sometimes resets the connection, so the source tarball is fetched from the GitHub `FFmpeg/FFmpeg` mirror
  (tag `n7.1.1`).

It ends by checking that both encoders are present. Re-running skips libraries that are already built.

## 4. PATH

Add to `~/.zshrc` (or run per shell):

```bash
export PATH="$HOME/.cargo/bin:$HOME/opt/node-v24.18.0-darwin-arm64/bin:$HOME/opt/ffmpeg/bin:$PATH"
export BLADE_FFMPEG="$HOME/opt/ffmpeg/bin/ffmpeg"
export BLADE_FFPROBE="$HOME/opt/ffmpeg/bin/ffprobe"
```

`tools/record_sidecar/env.sh` defaults ffmpeg to `~/.local/ffmpeg/bin`; the two variables above override that.

## 5. Build and test

```bash
cargo build
cargo test -- --test-threads=4
```

Notes from the restore run:

- **Use `--test-threads=4`.** At the default thread count (one per core, 12 on the test machine) the five
  `record::hub::first_frame_tests` fail with "no frames reached the encoder": they use 120 ms frame intervals and
  starve under load. They pass alone and at 4 threads.
- **Node must be on `PATH`**, otherwise `tests/page_unit.rs` and `tests/stealth_behavior.rs` fail instantly.
- `tests/daemon_lifecycle.rs::concurrent_cold_starts_have_one_owner` failed on the test machine on every run, including on a
  clean checkout of upstream v4.6.6, so it is not caused by this fork. It starts 8 daemons and waits 500 ms for the pidfile.
- The `#[ignore]` live record tests need Chrome and are run through `tools/record_sidecar/scenarios/*.sh`, which expect a
  release binary: `cargo build --release`.

## Cleaning up

Everything installed above lives in `~/.cargo`, `~/.rustup`, `~/opt/node-*`, `~/opt/ffmpeg` and the repo's `target/`
(about 1.5 GB for debug plus release). Delete those to remove it; nothing is installed system-wide.
