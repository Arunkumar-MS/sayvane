#!/usr/bin/env bash
# Builds the bundled local speech worker (sayvane-speech) for macOS and places
# it where Tauri expects sidecars: src-tauri/binaries/sayvane-speech-<triple>.
#
# Usage: scripts/build-worker.sh [aarch64-apple-darwin|x86_64-apple-darwin]
#   (default: the host triple)
#
# Environment:
#   CMAKE                 path to cmake (else PATH, else $HOME/.local/cmake-bin/cmake)
#   SAYVANE_X86_LEVEL     x86_64 only: avx2 (default) | sse42 | native (never ship native)
#   SAYVANE_DEPLOYMENT_TARGET  macOS deployment target (default 13.0)
#   SAYVANE_ENGINE_TARBALL     optional local copy of the pinned engine tarball (hash still checked)
#   SAYVANE_EXTRA_CMAKE_ARGS   extra arguments passed to the configure step
#   JOBS                  parallel build jobs (default: CPU count)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/speech-worker"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: build-worker.sh builds macOS targets; use scripts/build-worker.ps1 on Windows" >&2
  exit 1
fi

host_arch="$(uname -m)"
case "$host_arch" in
  arm64|aarch64) host_triple="aarch64-apple-darwin" ;;
  x86_64)        host_triple="x86_64-apple-darwin" ;;
  *) echo "error: unsupported host architecture $host_arch" >&2; exit 1 ;;
esac

TRIPLE="${1:-$host_triple}"
case "$TRIPLE" in
  aarch64-apple-darwin) OSX_ARCH="arm64";  LIPO_ARCH="arm64" ;;
  x86_64-apple-darwin)  OSX_ARCH="x86_64"; LIPO_ARCH="x86_64" ;;
  *) echo "error: unsupported target triple '$TRIPLE' (expected aarch64-apple-darwin or x86_64-apple-darwin)" >&2; exit 1 ;;
esac

# Locate cmake: $CMAKE, then PATH, then ~/.local/cmake-bin/cmake.
if [[ -n "${CMAKE:-}" ]]; then
  CMAKE_BIN="$CMAKE"
elif command -v cmake >/dev/null 2>&1; then
  CMAKE_BIN="$(command -v cmake)"
elif [[ -x "$HOME/.local/cmake-bin/cmake" ]]; then
  CMAKE_BIN="$HOME/.local/cmake-bin/cmake"
else
  echo "error: cmake not found (set CMAKE, add cmake to PATH, or install to ~/.local/cmake-bin)" >&2
  exit 1
fi
echo "Using cmake: $CMAKE_BIN ($("$CMAKE_BIN" --version | head -1))"

BUILD_DIR="$SRC/build/$TRIPLE"
OUT_DIR="$ROOT/src-tauri/binaries"
OUT="$OUT_DIR/sayvane-speech-$TRIPLE"
JOBS="${JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || echo 4)}"

cfg_args=(
  -S "$SRC" -B "$BUILD_DIR"
  -DCMAKE_BUILD_TYPE=Release
  -DCMAKE_OSX_ARCHITECTURES="$OSX_ARCH"
  -DCMAKE_OSX_DEPLOYMENT_TARGET="${SAYVANE_DEPLOYMENT_TARGET:-13.0}"
)
if [[ "$OSX_ARCH" != "$host_arch" ]]; then
  # Cross-compiling: tell CMake so ggml does not probe the host CPU.
  cfg_args+=(-DCMAKE_SYSTEM_NAME=Darwin -DCMAKE_SYSTEM_PROCESSOR="$OSX_ARCH")
fi
if [[ "$OSX_ARCH" == "x86_64" ]]; then
  cfg_args+=(-DSAYVANE_X86_LEVEL="${SAYVANE_X86_LEVEL:-avx2}")
fi
if [[ -n "${SAYVANE_ENGINE_TARBALL:-}" ]]; then
  cfg_args+=(-DSAYVANE_ENGINE_TARBALL="$SAYVANE_ENGINE_TARBALL")
fi
if [[ -n "${SAYVANE_EXTRA_CMAKE_ARGS:-}" ]]; then
  # shellcheck disable=SC2206
  cfg_args+=($SAYVANE_EXTRA_CMAKE_ARGS)
fi

"$CMAKE_BIN" "${cfg_args[@]}"
"$CMAKE_BIN" --build "$BUILD_DIR" --config Release --target sayvane-speech -j "$JOBS"

mkdir -p "$OUT_DIR"
cp "$BUILD_DIR/sayvane-speech" "$OUT"
strip -x "$OUT" 2>/dev/null || true
chmod 755 "$OUT"

# Verify architecture and dynamic dependencies.
archs="$(lipo -archs "$OUT")"
echo "Built: $OUT"
echo "  lipo -archs: $archs"
echo "  file: $(file -b "$OUT")"
if [[ "$archs" != "$LIPO_ARCH" ]]; then
  echo "error: expected a single $LIPO_ARCH slice, got '$archs'" >&2
  exit 1
fi
echo "  linked libraries:"
otool -L "$OUT" | tail -n +2 | sed 's/^/    /'
if otool -L "$OUT" | tail -n +2 | grep -Ev '^\s*(/usr/lib/|/System/Library/)' | grep -q .; then
  echo "error: the worker links a non-system library; it must be self-contained" >&2
  exit 1
fi
if strings "$OUT" | grep -q "$ROOT"; then
  echo "warning: the binary contains the build machine path $ROOT (assert/file macros)" >&2
fi
