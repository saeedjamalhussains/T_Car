# Build environment for T-Car's Linux bundles.
# Ubuntu 22.04 keeps the glibc requirement low enough for most distros.
FROM rust:1-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
      libwebkit2gtk-4.1-dev \
      libappindicator3-dev \
      librsvg2-dev \
      libssl-dev \
      patchelf \
      file \
      desktop-file-utils \
      fuse3 \
      build-essential \
      curl \
      ca-certificates \
 && rm -rf /var/lib/apt/lists/*

RUN cargo install tauri-cli --version "^2" --locked

WORKDIR /work
