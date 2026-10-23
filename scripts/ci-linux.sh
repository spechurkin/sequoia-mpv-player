#!/usr/bin/env bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update
egl_dev=libegl-dev
gl_dev=libgl-dev
if [[ "$UBUNTU_VERSION" == 18.04 || "$UBUNTU_VERSION" == 20.04 ]]; then
  egl_dev=libegl1-mesa-dev
  gl_dev=libgl1-mesa-dev
fi
apt-get install -y --no-install-recommends \
  ca-certificates curl build-essential pkg-config python3 \
  libx11-dev libxi-dev libxcursor-dev libxrandr-dev libxinerama-dev \
  libxkbcommon-dev libwayland-dev "$egl_dev" "$gl_dev" \
  libdbus-1-dev mpv
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh
sh /tmp/rustup-init.sh -y --profile minimal --default-toolchain "$RUST_VERSION"
source "$HOME/.cargo/env"
cargo test --locked --target "$RUST_TARGET"
cargo test --locked --target "$RUST_TARGET" --test ipc_smoke -- --ignored
cargo build --release --locked --target "$RUST_TARGET"
python3 scripts/package-ci.py --target "$RUST_TARGET" \
  --label "ubuntu-$UBUNTU_VERSION" --minimum "$UBUNTU_VERSION" --mpv /usr/bin/mpv
