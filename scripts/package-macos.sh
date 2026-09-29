#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="mqttc"
APP_DIR="${ROOT}/dist/${APP_NAME}.app"
CONTENTS="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS}/MacOS"
RESOURCES="${CONTENTS}/Resources"
BINARY="${ROOT}/target/release/${APP_NAME}"
ICON_PNG="${ROOT}/assets/icon.png"
INFO_PLIST="${ROOT}/scripts/macos/Info.plist"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "This script must be run on macOS."
    echo "Install Rust, then in this repo run:"
    echo "  cargo test"
    echo "  bash scripts/package-macos.sh"
    exit 1
fi

cd "${ROOT}"
cargo build --release

if [[ ! -x "${BINARY}" ]]; then
    echo "missing release binary: ${BINARY}"
    exit 1
fi

rm -rf "${APP_DIR}"
mkdir -p "${MACOS_DIR}" "${RESOURCES}"
cp "${BINARY}" "${MACOS_DIR}/${APP_NAME}"
chmod +x "${MACOS_DIR}/${APP_NAME}"
cp "${INFO_PLIST}" "${CONTENTS}/Info.plist"

ICONSET="$(mktemp -d "${TMPDIR:-/tmp}/mqttc-iconset.XXXXXX")"
trap 'rm -rf "${ICONSET}"' EXIT
mkdir -p "${ICONSET}/mqttc.iconset"

sips -z 16 16 "${ICON_PNG}" --out "${ICONSET}/mqttc.iconset/icon_16x16.png" >/dev/null
sips -z 32 32 "${ICON_PNG}" --out "${ICONSET}/mqttc.iconset/icon_16x16@2x.png" >/dev/null
sips -z 32 32 "${ICON_PNG}" --out "${ICONSET}/mqttc.iconset/icon_32x32.png" >/dev/null
sips -z 64 64 "${ICON_PNG}" --out "${ICONSET}/mqttc.iconset/icon_32x32@2x.png" >/dev/null
sips -z 128 128 "${ICON_PNG}" --out "${ICONSET}/mqttc.iconset/icon_128x128.png" >/dev/null
sips -z 256 256 "${ICON_PNG}" --out "${ICONSET}/mqttc.iconset/icon_128x128@2x.png" >/dev/null
sips -z 256 256 "${ICON_PNG}" --out "${ICONSET}/mqttc.iconset/icon_256x256.png" >/dev/null
sips -z 512 512 "${ICON_PNG}" --out "${ICONSET}/mqttc.iconset/icon_256x256@2x.png" >/dev/null
sips -z 512 512 "${ICON_PNG}" --out "${ICONSET}/mqttc.iconset/icon_512x512.png" >/dev/null
sips -z 1024 1024 "${ICON_PNG}" --out "${ICONSET}/mqttc.iconset/icon_512x512@2x.png" >/dev/null
iconutil -c icns "${ICONSET}/mqttc.iconset" -o "${RESOURCES}/mqttc.icns"

ARCH="$(uname -m)"
echo "Built ${APP_DIR} (${ARCH})"
echo "Open with:"
echo "  open ${APP_DIR}"
