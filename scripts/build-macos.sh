#!/usr/bin/env bash
# Build the portable macOS clients (Apple Silicon or Intel).
# Requires Xcode Command Line Tools. NOT exercised in this repo's Windows-only
# measurement session; provided for completeness.
set -euo pipefail
cd "$(dirname "$0")/.."

# Common prerequisites:
#   xcode-select --install
# egui uses glow/Metal via eframe; iced uses tiny-skia; GPUI uses Metal.
# Universal binary example:
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin

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

# Optional .app wrapper (portable ad-hoc bundle, no signing):
if [[ "${MAKE_APP:-0}" = "1" && -f dist/mqttc-egui ]]; then
    APP="dist/mqttc.app"
    rm -rf "$APP"
    mkdir -p "$APP/Contents/MacOS"
    cp dist/mqttc-egui "$APP/Contents/MacOS/mqttc"
    cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>mqttc</string>
  <key>CFBundleExecutable</key><string>mqttc</string>
  <key>CFBundleIdentifier</key><string>dev.mqttc.egui</string>
  <key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
PLIST
    echo "built $APP"
fi

ls -l dist
