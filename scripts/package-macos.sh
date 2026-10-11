#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Requires Node.js, Rust, Xcode command line tools, and an arm64 FFmpeg installation at build time only.
python3 scripts/bundle-macos-tools.py
if [ "${STR_REUSE_MODULES:-0}" = "1" ]; then
  for module in contact_sheet_v031 filter_short_v030 notify_discord_v031 notify_telegram_v031; do
    test -x "desktop/src-tauri/resources/modules/$module"
  done
else
  node -e 'require("./scripts/common.js").buildModules("release", "desktop/src-tauri/resources/modules")'
fi
export CARGO_TARGET_DIR="$PWD/build_tmp/desktop/target"
(cd desktop && npm run tauri -- build --bundles app)
mkdir -p build/desktop-macos
APP="$CARGO_TARGET_DIR/release/bundle/macos/STR Recorder.app"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"
STAGE="$PWD/build_tmp/dmg-stage"
mkdir -p "$STAGE"
if [ -d "$STAGE/STR Recorder.app" ]; then rm -rf "$STAGE/STR Recorder.app"; fi
ditto "$APP" "$STAGE/STR Recorder.app"
ln -sfn /Applications "$STAGE/Applications"
cp docs/MACOS.md "$STAGE/安装说明.txt"
DMG="$PWD/build/desktop-macos/STR-Recorder-0.3.5-Chaturbate-merged-logo-arm64.dmg"
hdiutil create -volname 'STR Recorder' -srcfolder "$STAGE" -ov -format UDZO "$DMG"
hdiutil verify "$DMG"
shasum -a 256 "$DMG" > "$DMG.sha256"
printf '%s\n' "$DMG"
