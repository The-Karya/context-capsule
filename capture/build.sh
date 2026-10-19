#!/bin/sh
# context-capsule — capture: build "build/Context Capsule.app" with swiftc (no Xcode project) and ad-hoc sign it.
# ARCHS="arm64" ./build.sh builds one arch only; default is a universal arm64 + x86_64 app.
set -eu
cd "$(dirname "$0")"

ARCHS="${ARCHS:-arm64 x86_64}"
MIN_MACOS=14.0 # SCContentSharingPicker (the system picker) needs macOS 14
APP="build/Context Capsule.app"
SDK="$(xcrun --sdk macosx --show-sdk-path)"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

BINS=""
for arch in $ARCHS; do
  echo "compiling ${arch}..."
  nice -n 19 swiftc -O -swift-version 5 -sdk "$SDK" -target "$arch-apple-macos$MIN_MACOS" \
    Sources/*.swift -o "build/ContextCapsule-$arch"
  BINS="$BINS build/ContextCapsule-$arch"
done
# shellcheck disable=SC2086
lipo -create $BINS -output "$APP/Contents/MacOS/ContextCapsule"
# shellcheck disable=SC2086
rm -f $BINS

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>ContextCapsule</string>
  <key>CFBundleIdentifier</key><string>com.thekarya.contextcapsule</string>
  <key>CFBundleName</key><string>Context Capsule</string>
  <key>CFBundleDisplayName</key><string>Context Capsule</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>$MIN_MACOS</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSScreenCaptureUsageDescription</key>
  <string>Context Capsule reads the text of the window or screen you pick, so your AI can tell what you worked on. Frames are never saved; only text is kept, on this Mac.</string>
</dict>
</plist>
PLIST

codesign --force -s - --identifier com.thekarya.contextcapsule "$APP"
echo "built $APP"
