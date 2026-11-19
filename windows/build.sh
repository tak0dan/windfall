#!/usr/bin/env bash
# Builds the Windows binaries and the installer inside Docker; nothing is installed on the host.
#   windows/build.sh            ->  dist/windfalld.exe, dist/windfallctl.exe, dist/Windfall-Setup.exe
set -euo pipefail
cd "$(dirname "$0")/.."
IMG=windfall-win-builder
docker image inspect "$IMG" >/dev/null 2>&1 || docker build -t "$IMG" windows
mkdir -p dist .cache/cargo-win
docker run --rm -v "$PWD":/src -v "$PWD/.cache/cargo-win":/usr/local/cargo/registry -e CARGO_TARGET_DIR=/src/target-win \
  -u "$(id -u):$(id -g)" -e HOME=/tmp -e CARGO_HOME=/tmp/cargo "$IMG" bash -c '
    set -e
    mkdir -p /tmp/cargo && cp -rn /usr/local/cargo/registry /tmp/cargo/ 2>/dev/null || true
    cargo build --release --target x86_64-pc-windows-gnu -p windfalld -p windfallctl
    cp target-win/x86_64-pc-windows-gnu/release/windfalld.exe target-win/x86_64-pc-windows-gnu/release/windfallctl.exe dist/
    VERSION=$(grep -m1 "^version" Cargo.toml | cut -d\" -f2)
    makensis -V2 -DVERSION="$VERSION" windows/windfall.nsi
  '
ls -la dist
