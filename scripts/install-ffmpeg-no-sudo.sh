#!/usr/bin/env bash
# Builds ffmpeg (with libvpx VP9 and libx264, plus ffprobe) into ~/opt/ffmpeg. No sudo.
# Needs: Xcode command line tools (clang, make), git, curl, python3. Tested on Apple Silicon macOS (arm64:
# libvpx and x264 need no nasm there; on x86_64 install nasm first).
# KEEP_SRC=1 keeps the build directory (~540 MB); by default it is deleted at the end.
set -euo pipefail

PREFIX="$HOME/opt/ffmpeg"
SRC="$HOME/src-build"
JOBS="$(sysctl -n hw.ncpu)"
FFMPEG_VER="7.1.1"
LIBVPX_VER="1.15.0"

mkdir -p "$PREFIX" "$SRC"
export PATH="$PREFIX/bin:$PATH"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig"

cd "$SRC"

echo "== libvpx $LIBVPX_VER"
if [ -f "$PREFIX/lib/libvpx.a" ]; then echo "already built, skipping"; else
[ -f "libvpx-$LIBVPX_VER.tar.gz" ] || curl -fsSL "https://github.com/webmproject/libvpx/archive/refs/tags/v$LIBVPX_VER.tar.gz" -o "libvpx-$LIBVPX_VER.tar.gz"
rm -rf "libvpx-$LIBVPX_VER" && tar -xzf "libvpx-$LIBVPX_VER.tar.gz"
(cd "libvpx-$LIBVPX_VER" && ./configure --prefix="$PREFIX" --disable-examples --disable-unit-tests --disable-docs --enable-vp9-highbitdepth --enable-pic && make -j"$JOBS" && make install)
fi

echo "== x264 (stable)"
if [ -f "$PREFIX/lib/libx264.a" ]; then echo "already built, skipping"; else
[ -d x264 ] || git clone --depth 1 --branch stable https://code.videolan.org/videolan/x264.git
(cd x264 && ./configure --prefix="$PREFIX" --enable-static --enable-pic --disable-cli && make -j"$JOBS" && make install)
fi

echo "== pkg-config shim (macOS ships none)"
mkdir -p "$PREFIX/bin"
cat > "$PREFIX/bin/pkg-config" <<'PYEOF'
#!/usr/bin/env python3
# Minimal pkg-config: --exists --cflags --libs --modversion --static, reads PKG_CONFIG_PATH. Ignores version constraints.
import os, re, sys
args = [a for a in sys.argv[1:]]
flags = {a for a in args if a.startswith("--")}
names = []
i = 0
rest = [a for a in args if not a.startswith("--")]
while i < len(rest):
    names.append(rest[i])
    if i + 1 < len(rest) and rest[i + 1] in (">=", "<=", "=", ">", "<"):
        i += 3
    else:
        i += 1
dirs = [d for d in os.environ.get("PKG_CONFIG_PATH", "").split(":") if d]
def load(name):
    for d in dirs:
        p = os.path.join(d, name + ".pc")
        if os.path.exists(p):
            vars_, fields = {}, {}
            for line in open(p):
                line = line.rstrip("\n")
                m = re.match(r"^([A-Za-z0-9_.]+)=(.*)$", line)
                if m: vars_[m.group(1)] = m.group(2); continue
                m = re.match(r"^([A-Za-z.]+):\s*(.*)$", line)
                if m: fields[m.group(1)] = m.group(2)
            def sub(s):
                for _ in range(10):
                    s = re.sub(r"\$\{([^}]+)\}", lambda mm: vars_.get(mm.group(1), ""), s)
                return s
            return {k: sub(v) for k, v in fields.items()}
    return None
pcs = []
for n in names:
    pc = load(n)
    if pc is None:
        if "--print-errors" in flags:
            sys.stderr.write("Package %s was not found in the pkg-config search path.\n" % n)
        sys.exit(1)
    pcs.append(pc)
if "--exists" in flags: sys.exit(0)
if "--modversion" in flags:
    for pc in pcs: print(pc.get("Version", ""))
    sys.exit(0)
out = []
for pc in pcs:
    if "--cflags" in flags: out.append(pc.get("Cflags", ""))
    if "--libs" in flags:
        out.append(pc.get("Libs", ""))
        if "--static" in flags: out.append(pc.get("Libs.private", ""))
print(" ".join(x for x in out if x).strip())
PYEOF
chmod +x "$PREFIX/bin/pkg-config"

echo "== ffmpeg $FFMPEG_VER"
rm -f ffmpeg.tar.gz ffmpeg.tar.xz
curl -fsSL "https://github.com/FFmpeg/FFmpeg/archive/refs/tags/n$FFMPEG_VER.tar.gz" -o ffmpeg.tar.gz
rm -rf "FFmpeg-n$FFMPEG_VER" && tar -xzf ffmpeg.tar.gz
(cd "FFmpeg-n$FFMPEG_VER" && ./configure --prefix="$PREFIX" --enable-gpl --enable-libvpx --enable-libx264 \
  --extra-cflags="-I$PREFIX/include" --extra-ldflags="-L$PREFIX/lib" --disable-doc --disable-ffplay \
  && make -j"$JOBS" && make install)

echo "== check"
"$PREFIX/bin/ffmpeg" -hide_banner -version | head -1
"$PREFIX/bin/ffmpeg" -hide_banner -encoders 2>/dev/null | grep -E "libvpx-vp9|libx264" || { echo "MISSING encoders"; exit 1; }
echo "OK: $PREFIX/bin/ffmpeg"

if [ "${KEEP_SRC:-0}" != "1" ]; then rm -rf "$SRC"; echo "removed $SRC"; fi
echo "Add to PATH: export PATH=\"$PREFIX/bin:\$PATH\"   (and BLADE_FFMPEG=$PREFIX/bin/ffmpeg)"
