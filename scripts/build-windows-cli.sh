#!/usr/bin/env bash
# Cross-builds ONLY the `tcar.exe` command line tool for Windows from macOS or
# Linux, using the mingw-w64 toolchain.
#
# The desktop app cannot be cross-built this way: it links against WebView2 and
# needs the MSVC toolchain, so build it on Windows or in CI
# (.github/workflows/release.yml).
#
#   brew install mingw-w64        # macOS
#   ./scripts/build-windows-cli.sh
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v x86_64-w64-mingw32-gcc >/dev/null; then
  echo "mingw-w64 is missing. On macOS: brew install mingw-w64" >&2
  exit 1
fi

rustup target add x86_64-pc-windows-gnu
cargo build --release -p tcar-cli --target x86_64-pc-windows-gnu
ls -lh target/x86_64-pc-windows-gnu/release/tcar.exe
