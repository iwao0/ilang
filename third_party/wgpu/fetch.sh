#!/usr/bin/env bash
# Fetch the prebuilt wgpu-native library + headers used by the
# examples/wgpu_triangle PoC. The binaries are NOT committed (see
# .gitignore); run this once after checkout.
#
# Pinned version — the bindings/wgpu declarations were written against
# this exact webgpu.h / wgpu.h.
#
#   Usage:   third_party/wgpu/fetch.sh
#   Runtime: DYLD_LIBRARY_PATH=third_party/wgpu/<os-arch>/lib \
#              cargo run -p ilang -- run examples/wgpu_triangle/main.il
#
# Requires the GitHub CLI (`gh`). Downloads only the release-build
# dynamic library + headers; the 33 MB static archive and the source
# zip are removed to keep the working tree small.
set -euo pipefail

TAG="v29.0.0.0"
REPO="gfx-rs/wgpu-native"
HERE="$(cd "$(dirname "$0")" && pwd)"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)        ASSET="wgpu-macos-aarch64-release.zip";       OUT="macos-aarch64" ;;
  Darwin-x86_64)       ASSET="wgpu-macos-x86_64-release.zip";        OUT="macos-x86_64"  ;;
  Linux-x86_64)        ASSET="wgpu-linux-x86_64-release.zip";        OUT="linux-x86_64"  ;;
  Linux-aarch64)       ASSET="wgpu-linux-aarch64-release.zip";       OUT="linux-aarch64" ;;
  # git-bash / MSYS report e.g. MINGW64_NT-10.0 / MSYS_NT-10.0.
  MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64)
    ASSET="wgpu-windows-x86_64-msvc-release.zip"; OUT="windows-x86_64" ;;
  *) echo "unsupported host: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

cd "$HERE"
echo "Downloading $REPO $TAG :: $ASSET"
gh release download "$TAG" -R "$REPO" -p "$ASSET" --clobber
rm -rf "$OUT"
unzip -q "$ASSET" -d "$OUT"
rm -f "$ASSET"
# Drop the static archive + debug symbols — the PoC loads the dynamic
# library at runtime, so the import lib / static lib / pdb aren't needed.
rm -f "$OUT/lib/"*.a "$OUT/lib/wgpu_native.lib" "$OUT/lib/"*.pdb
echo "Ready: $HERE/$OUT/lib (set DYLD_LIBRARY_PATH / LD_LIBRARY_PATH to it,"
echo "       or put $OUT/lib/wgpu_native.dll on PATH on Windows)"
