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
  <!-- The name people see is "Karya Context Capsule" (CFBundleName: menus, About; en.lproj below: Finder, Dock,
       Spotlight). The file stays "Context Capsule.app" (see AppName in Core.swift), and Finder only uses the
       localized name when the file name matches CFBundleDisplayName here, so this one stays "Context Capsule". -->
  <key>CFBundleName</key><string>Karya Context Capsule</string>
  <key>CFBundleDisplayName</key><string>Context Capsule</string>
  <key>LSHasLocalizedDisplayName</key><true/>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.2.0</string>
  <key>CFBundleVersion</key><string>0.2.0</string>
  <key>LSMinimumSystemVersion</key><string>$MIN_MACOS</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <!-- Offered for .md files in "Open With" (a capsule someone sent), never the default app for them. -->
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Capsule</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>net.daringfireball.markdown</string></array>
      <key>CFBundleTypeExtensions</key><array><string>md</string></array>
    </dict>
  </array>
  <key>UTImportedTypeDeclarations</key>
  <array>
    <dict>
      <key>UTTypeIdentifier</key><string>net.daringfireball.markdown</string>
      <key>UTTypeDescription</key><string>Markdown</string>
      <key>UTTypeConformsTo</key><array><string>public.plain-text</string></array>
      <key>UTTypeTagSpecification</key>
      <dict><key>public.filename-extension</key><array><string>md</string><string>markdown</string></array></dict>
    </dict>
  </array>
  <key>NSMicrophoneUsageDescription</key>
  <string>Context Capsule listens only while you hold a voice note (⌥V, then ⌥V again), to turn what you say about your work into text. Audio is never saved and never leaves this Mac.</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>Context Capsule turns your voice notes into text on this Mac (on-device recognition only; nothing is sent to Apple). Only the text is kept, in your session.</string>
  <key>NSScreenCaptureUsageDescription</key>
  <string>Context Capsule reads the text of the window or screen you pick, so your AI can tell what you worked on. Frames are never saved; only text is kept, on this Mac.</string>
</dict>
</plist>
PLIST

# The name Finder, the Dock and Spotlight show (any language falls back to en).
mkdir -p "$APP/Contents/Resources/en.lproj"
cat > "$APP/Contents/Resources/en.lproj/InfoPlist.strings" <<STRINGS
"CFBundleDisplayName" = "Karya Context Capsule";
"CFBundleName" = "Karya Context Capsule";
STRINGS

# The app icon: the Karya panther. Resources/AppIcon.png is the 1024 px tile (white brush panther on black),
# AppIcon-small.png the same tile with the heavier mark, for 64 px and under (the brush detail turns to dust there).
# macOS icons sit on a grid: an 824 px tile with 100 px of clear space around it in 1024, so each size is padded.
ICONSET="build/AppIcon.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
icon() { # icon <canvas px> <file name>
  src=Resources/AppIcon.png
  if [ "$1" -le 64 ]; then src=Resources/AppIcon-small.png; fi
  tile=$(( ($1 * 824 + 512) / 1024 ))
  sips -z "$tile" "$tile" "$src" --out "$ICONSET/tile.png" >/dev/null
  sips -p "$1" "$1" "$ICONSET/tile.png" --out "$ICONSET/$2" >/dev/null
}
icon 16 icon_16x16.png
icon 32 icon_16x16@2x.png
icon 32 icon_32x32.png
icon 64 icon_32x32@2x.png
icon 128 icon_128x128.png
icon 256 icon_128x128@2x.png
icon 256 icon_256x256.png
icon 512 icon_256x256@2x.png
icon 512 icon_512x512.png
icon 1024 icon_512x512@2x.png
rm -f "$ICONSET/tile.png"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf "$ICONSET"
# The menu-bar mark (a template image: macOS draws it black or white), 18 pt tall.
sips -z 18 13 Resources/MenuBarMark.png --out "$APP/Contents/Resources/MenuBarMark.png" >/dev/null
sips -z 36 27 Resources/MenuBarMark.png --out "$APP/Contents/Resources/MenuBarMark@2x.png" >/dev/null

# The Claude Desktop extension ("Connect to Claude"), when it has been built (mcp/extension/build.sh).
EXT="../mcp/dist-ext/context-capsule.mcpb"
if [ -f "$EXT" ]; then cp "$EXT" "$APP/Contents/Resources/"; else echo "note: $EXT not built; Connect to Claude will be missing"; fi

# context-capsule-mcp, the one program for any AI (SPEC "Any AI"): Connect your AI, Copy my day, Save from clipboard.
# Built in mcp/dist-bin/ as one universal binary, or one per arch (-arm64, -x64): pick what ARCHS needs.
MCPBIN="../mcp/dist-bin/context-capsule-mcp-macos"
HELPER="$APP/Contents/Resources/context-capsule-mcp"
mcp_for() { # mcp_for <arch>: the binary for one arch, if there is one
  case "$1" in arm64) f="$MCPBIN-arm64" ;; x86_64) f="$MCPBIN-x64" ;; *) f="" ;; esac
  if [ -n "$f" ] && [ -f "$f" ]; then echo "$f"; fi
}
HAVE=""
MISSING=""
for arch in $ARCHS; do
  f="$(mcp_for "$arch")"
  if [ -n "$f" ]; then HAVE="$HAVE $f"; else MISSING="$MISSING $arch"; fi
done
if [ -f "$MCPBIN" ]; then
  cp "$MCPBIN" "$HELPER"
elif [ -z "$MISSING" ] && [ -n "$HAVE" ]; then
  # shellcheck disable=SC2086
  set -- $HAVE
  if [ $# -eq 1 ]; then cp "$1" "$HELPER"; else lipo -create "$@" -output "$HELPER"; fi
else
  echo "warning: $MCPBIN not built (for:$MISSING); Connect your AI and Copy my day will be missing"
fi
[ -f "$HELPER" ] && chmod 755 "$HELPER"

# Sign with a fixed local identity when there is one, so macOS keeps the Accessibility / Microphone
# permissions across rebuilds (it ties them to the signature). Else ad-hoc (permissions reset on rebuild).
# Make one with Keychain Access → Certificate Assistant → Create a Certificate (type: Code Signing),
# named "Context Capsule Dev", or set CODESIGN_IDENTITY.
IDENTITY="${CODESIGN_IDENTITY:-Context Capsule Dev}"
# The helper program is signed first, with the same identity, so the app's signature covers a signed helper.
if security find-certificate -c "$IDENTITY" >/dev/null 2>&1 \
   && { [ ! -f "$HELPER" ] || codesign --force -s "$IDENTITY" --identifier com.thekarya.contextcapsule.mcp "$HELPER" 2>/dev/null; } \
   && codesign --force -s "$IDENTITY" --identifier com.thekarya.contextcapsule "$APP" 2>/dev/null; then
  echo "signed with \"$IDENTITY\""
else
  if [ -f "$HELPER" ]; then codesign --force -s - --identifier com.thekarya.contextcapsule.mcp "$HELPER"; fi
  codesign --force -s - --identifier com.thekarya.contextcapsule "$APP"
  echo "signed ad-hoc (permissions reset after each rebuild)"
fi
echo "built $APP"
