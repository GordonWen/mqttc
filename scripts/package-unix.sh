#!/usr/bin/env bash
# Package a release binary for Linux or macOS.
#
#   scripts/package-unix.sh <binary> linux-x64
#   scripts/package-unix.sh <binary> macos-universal --app
#
# Output: dist/release/mqttc-v<version>-<platform>.tar.gz|zip + .sha256
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${1:?usage: package-unix.sh <binary> <platform> [--app]}"
PLATFORM="${2:?usage: package-unix.sh <binary> <platform> [--app]}"
MAKE_APP=0
[[ "${3:-}" == "--app" ]] && MAKE_APP=1

VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
[[ -n "$VERSION" ]] || { echo "cannot determine version" >&2; exit 1; }
[[ -f "$BIN" ]] || { echo "missing binary: $BIN" >&2; exit 1; }

OUT="$(pwd)/dist/release"
STAGE="$OUT/stage-$PLATFORM"
rm -rf "$STAGE"
mkdir -p "$STAGE"

CONFIG="tools/bench/fixtures/mqttc.json"
README="READ-ME.txt"
cat > "$STAGE/$README" <<EOF
mqttc $VERSION - portable $PLATFORM build
$(printf '=%.0s' $(seq 1 40))

1. Extract this archive anywhere (a path with spaces is fine).
2. Run the mqttc binary (macOS: open mqttc.app).
3. Configuration and exported logs are written next to the executable:
   mqttc.json and logs/.
4. No installer, no registry entries, no files in your home directory.

Linux runtime packages (Debian/Ubuntu): libxkbcommon0, libwayland-client0,
libx11-6, libxcb1, libfontconfig1, libfreetype6. The binary is built on
Ubuntu 22.04 and needs glibc 2.35+.

macOS: the build is unsigned. On first launch, right-click mqttc.app ->
Open, or run: xattr -dr com.apple.quarantine mqttc.app

Source and issues: https://github.com/GordonWen/mqttc
License: MIT.
EOF

ASSET=""
if [[ "$MAKE_APP" == "1" ]]; then
    APP="$STAGE/mqttc.app"
    mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
    cp "$BIN" "$APP/Contents/MacOS/mqttc"
    chmod +x "$APP/Contents/MacOS/mqttc"
    cp "$CONFIG" "$APP/Contents/MacOS/mqttc.json"
    cp "$STAGE/$README" "$APP/Contents/Resources/READ-ME.txt"
    cp assets/icon.png "$APP/Contents/Resources/icon.png"
    cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>mqttc</string>
  <key>CFBundleDisplayName</key><string>mqttc</string>
  <key>CFBundleIdentifier</key><string>dev.mqttc.app</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>mqttc</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>icon.png</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
    ASSET="$OUT/mqttc-v$VERSION-$PLATFORM.zip"
    rm -f "$ASSET"
    if command -v ditto >/dev/null 2>&1; then
        ditto -c -k --sequesterRsrc --keepParent "$APP" "$ASSET"
    else
        (cd "$STAGE" && zip -qry "$(basename "$ASSET")" "mqttc.app")
    fi
else
    mkdir -p "$STAGE/mqttc"
    cp "$BIN" "$STAGE/mqttc/mqttc"
    chmod +x "$STAGE/mqttc/mqttc"
    cp "$CONFIG" "$STAGE/mqttc/mqttc.json"
    cp "$STAGE/$README" "$STAGE/mqttc/READ-ME.txt"
    ASSET="$OUT/mqttc-v$VERSION-$PLATFORM.tar.gz"
    rm -f "$ASSET"
    tar -czf "$ASSET" -C "$STAGE" mqttc
fi

if command -v sha256sum >/dev/null 2>&1; then
    HASH=$(sha256sum "$ASSET" | awk '{print $1}')
else
    HASH=$(shasum -a 256 "$ASSET" | awk '{print $1}')
fi
printf '%s  %s\n' "$HASH" "$(basename "$ASSET")" > "$ASSET.sha256"

echo "asset : $ASSET"
echo "sha256: $ASSET.sha256"
ls -lh "$ASSET"
