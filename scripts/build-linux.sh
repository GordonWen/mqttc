#!/usr/bin/env bash
# Build the portable Linux clients. Tested command line only on Ubuntu 22.04;
# this script is provided for completeness and has NOT been run in this repo's
# Windows-only measurement session.
set -euo pipefail
cd "$(dirname "$0")/.."

# Build prerequisites (Ubuntu 22.04):
#   sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
#        libxcb1-dev libx11-dev libxrandr-dev libxi-dev libxcursor-dev \
#        libfontconfig1-dev libfreetype6-dev
#
# egui uses glow/OpenGL (libGL), iced uses tiny-skia (CPU), GPUI needs Vulkan
# headers (libvulkan-dev) and a working GPU driver at runtime.

TARGET_DIR="${TARGET_DIR:-target}"

cargo build --release -p mqttc-egui -p mqttc-iced
if [[ "${SKIP_GPUI:-0}" != "1" ]]; then
    cargo build --profile release-unwind -p mqttc-gpui || \
        echo "warning: GPUI build failed; see docs/ARCHITECTURE.md" >&2
fi

mkdir -p dist
cp "$TARGET_DIR/release/mqttc-egui" dist/mqttc-egui
cp "$TARGET_DIR/release/mqttc-iced" dist/mqttc-iced
[[ -f "$TARGET_DIR/release-unwind/mqttc-gpui" ]] && cp "$TARGET_DIR/release-unwind/mqttc-gpui" dist/mqttc-gpui || true
cp tools/bench/fixtures/mqttc.json dist/mqttc.json

ls -l dist
