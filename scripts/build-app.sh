#!/usr/bin/env bash
# Build a locally signed macOS application from source.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG="${1:-debug}"
case "$CONFIG" in debug|release) ;; *) echo "Usage: $0 [debug|release]" >&2; exit 2;; esac
APP_NAME="Agent HUD Open"
APP_DIR="$ROOT/build/$APP_NAME.app"
# Swift 6.4 made XCBuild the default backend, and it stamps LC_BUILD_VERSION's sdk field with the
# deployment target instead of the SDK actually compiled against. AppKit reads that field to decide
# whether an app gets the macOS 26 control design, so a default build silently ships legacy controls.
# Drop this flag once XCBuild stamps the SDK correctly; it is deprecated and will be removed.
BUILD_FLAGS=(--package-path "$ROOT" --disable-sandbox --build-system native -c "$CONFIG")
if [[ -n "${SWIFT_SCRATCH_PATH:-}" ]]; then BUILD_FLAGS+=(--scratch-path "$SWIFT_SCRATCH_PATH"); fi
swift build "${BUILD_FLAGS[@]}" --product AgentHUDOpen >&2
BIN_DIR="$(swift build "${BUILD_FLAGS[@]}" --show-bin-path)"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$BIN_DIR/AgentHUDOpen" "$APP_DIR/Contents/MacOS/$APP_NAME"
cp -R "$BIN_DIR/AgentHUDOpen_AgentHUDDesktop.bundle" "$APP_DIR/Contents/Resources/"
cp "$ROOT/assets/AppIcon.icns" "$APP_DIR/Contents/Resources/"
cp "$ROOT/LICENSE" "$APP_DIR/Contents/Resources/LICENSE.txt"
cat > "$APP_DIR/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>Agent HUD Open</string>
  <key>CFBundleExecutable</key><string>Agent HUD Open</string>
  <key>CFBundleIdentifier</key><string>app.agenthud.open</string>
  <key>CFBundleIconFile</key><string>AppIcon.icns</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>Agent HUD Open</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.4.19</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>
PLIST
printf 'APPL????' > "$APP_DIR/Contents/PkgInfo"
codesign --force --sign - --identifier app.agenthud.open "$APP_DIR" >&2
codesign --verify --deep --strict "$APP_DIR" >&2
printf '%s\n' "$APP_DIR"
