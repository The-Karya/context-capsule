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
  <key>CFBundleShortVersionString</key><string>0.1.1</string>
  <key>CFBundleVersion</key><string>0.1.1</string>
  <key>LSMinimumSystemVersion</key><string>$MIN_MACOS</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key>
  <string>Context Capsule listens only while you hold a voice note (⌥V, then ⌥V again), to turn what you say about your work into text. Audio is never saved and never leaves this Mac.</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>Context Capsule turns your voice notes into text on this Mac (on-device recognition only; nothing is sent to Apple). Only the text is kept, in your session.</string>
  <key>NSScreenCaptureUsageDescription</key>
  <string>Context Capsule reads the text of the window or screen you pick, so your AI can tell what you worked on. Frames are never saved; only text is kept, on this Mac.</string>
</dict>
</plist>
PLIST

# The Claude Desktop extension ("Connect to Claude"), when it has been built (mcp/extension/build.sh).
EXT="../mcp/dist-ext/context-capsule.mcpb"
if [ -f "$EXT" ]; then cp "$EXT" "$APP/Contents/Resources/"; else echo "note: $EXT not built; Connect to Claude will be missing"; fi

# Sign with a fixed local identity when there is one, so macOS keeps the Accessibility / Microphone
# permissions across rebuilds (it ties them to the signature). Else ad-hoc (permissions reset on rebuild).
# Make one with Keychain Access → Certificate Assistant → Create a Certificate (type: Code Signing),
# named "Context Capsule Dev", or set CODESIGN_IDENTITY.
IDENTITY="${CODESIGN_IDENTITY:-Context Capsule Dev}"
if security find-certificate -c "$IDENTITY" >/dev/null 2>&1 \
   && codesign --force -s "$IDENTITY" --identifier com.thekarya.contextcapsule "$APP" 2>/dev/null; then
  echo "signed with \"$IDENTITY\""
else
  codesign --force -s - --identifier com.thekarya.contextcapsule "$APP"
  echo "signed ad-hoc (permissions reset after each rebuild)"
fi
echo "built $APP"
