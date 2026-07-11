#!/usr/bin/env bash
# run-app.sh — launch voxel-world wrapped in a macOS .app bundle.
#
# Why this exists: a raw binary launched from a terminal is not promoted to the
# frontmost/key macOS application, so keyboard and mouse events route to the
# terminal instead of the game window and pointer lock never engages. Under
# tmux (or any terminal multiplexer) it fails every time, because tmux's
# setsid() severs the "responsible process" chain macOS uses to decide app
# activation. Wrapping the binary in a .app bundle and launching it via `open`
# gives the process a real application identity, so focus and input work even
# when invoked from inside tmux.
#
# Usage:
#   scripts/run-app.sh [--no-launch] [<game args...>]
#     --no-launch   build + assemble + validate the bundle but do not open it
#                   (used by `make` verification; no window appears)
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="voxel-world"
BIN="$ROOT_DIR/target/release/$APP_NAME"
APP_DIR="$ROOT_DIR/.dev/$APP_NAME.app"
MACOS_DIR="$APP_DIR/Contents/MacOS"
SHIM="$MACOS_DIR/$APP_NAME"
INFO_PLIST="$APP_DIR/Contents/Info.plist"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -1)"

# MoltenVK environment (mirrors the Makefile exports). LaunchServices starts the
# app with a clean environment, so the shim below re-establishes these before
# handing off to the real binary — otherwise Vulkan init cannot find MoltenVK.
DYLD_PATHS="/opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib"
VK_ICD="/opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json"

NO_LAUNCH=0
if [[ "${1:-}" == "--no-launch" ]]; then
  NO_LAUNCH=1
  shift
fi

cd "$ROOT_DIR"
cargo build --release

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR"

cat > "$INFO_PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>com.paulrobello.$APP_NAME.dev</string>
  <key>CFBundleExecutable</key><string>$APP_NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSUIElement</key><false/>
  <key>LSMinimumSystemVersion</key><string>10.13</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# The bundle executable is a shim: LaunchServices runs it with a clean env and
# cwd=/, so it restores the MoltenVK env and the repo working directory (the
# game loads shaders/textures and writes saves relative to cwd), then execs the
# real binary. exec preserves the pid so the .app stays the process identity
# macOS activates.
cat > "$SHIM" <<SHIM
#!/bin/bash
export DYLD_LIBRARY_PATH="$DYLD_PATHS"
export DYLD_FALLBACK_LIBRARY_PATH="$DYLD_PATHS"
export VK_ICD_FILENAMES="$VK_ICD"
cd "$ROOT_DIR"
exec "$BIN" "\$@"
SHIM
chmod +x "$SHIM"

if command -v plutil >/dev/null 2>&1; then
  plutil -lint "$INFO_PLIST" >/dev/null
fi
[[ -x "$SHIM" && -x "$BIN" ]]

if [[ "$NO_LAUNCH" == "1" ]]; then
  echo "run-app: bundle assembled at $APP_DIR (not launched)"
  exit 0
fi

echo "run-app: launching $APP_DIR"
open -n "$APP_DIR" --args "$@"
