#!/usr/bin/env bash
# Builds the Linux bundles (.deb, .rpm, .AppImage) and the `tcar` CLI inside a
# container, so you can produce them from a Mac. Requires Docker to be running.
#
#   ./scripts/build-linux.sh
#
# Output lands in target/linux/release/bundle/.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! docker info >/dev/null 2>&1; then
  echo "Docker is not running. Start Docker Desktop and try again." >&2
  exit 1
fi

docker build -f scripts/linux-builder.Dockerfile -t tcar-linux-builder .
docker run --rm \
  -v "$PWD":/work -w /work \
  -e CARGO_TARGET_DIR=/work/target/linux \
  tcar-linux-builder \
  bash -lc 'cargo tauri build --config apps/desktop/tauri.conf.json && cargo build --release -p tcar-cli'

echo
echo "Bundles:"
find target/linux/release/bundle -maxdepth 2 -type f \
  \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' \) -print
