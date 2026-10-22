#!/bin/sh
# context-capsule — capture: build the Mac app and package it for people.
#   dist/Context-Capsule-<ver>-mac.dmg   a plain drag-to-Applications disk image (for the download page)
#   dist/Context-Capsule-<ver>-mac.zip   the same app, zipped (for the built-in updater)
#
# Signing: by default build.sh signs with the local "Context Capsule Dev" certificate (or ad-hoc).
# For a release anyone can open, set both:
#   DEVELOPER_ID="Developer ID Application: Your Name (TEAMID)"   a Developer ID certificate in your keychain
#   NOTARY_PROFILE=context-capsule                                  xcrun notarytool store-credentials context-capsule …
# Then the app is signed with the hardened runtime and a timestamp, notarized and stapled, and so is the disk image.
# The updater only installs a new version signed by the same signer as the running app, so keep one identity.
set -eu
cd "$(dirname "$0")"

VER="$(sed -nE 's#.*<key>CFBundleShortVersionString</key><string>([^<]*)</string>.*#\1#p' build.sh)"
[ -n "$VER" ] || { echo "cannot read the version from build.sh" >&2; exit 1; }
APP="build/Context Capsule.app"
OUT="../dist"
ZIP="$OUT/Context-Capsule-$VER-mac.zip"
DMG="$OUT/Context-Capsule-$VER-mac.dmg"
DEVELOPER_ID="${DEVELOPER_ID:-}"
NOTARY_PROFILE="${NOTARY_PROFILE:-}"

if [ ! -f ../mcp/dist-ext/context-capsule.mcpb ] && [ -z "${SKIP_EXTENSION:-}" ]; then
  echo "The Claude extension is not built (mcp/extension/build.sh). Build it first, or SKIP_EXTENSION=1 for a test." >&2
  exit 1
fi
if { [ -n "$DEVELOPER_ID" ] && [ -z "$NOTARY_PROFILE" ]; } || { [ -z "$DEVELOPER_ID" ] && [ -n "$NOTARY_PROFILE" ]; }; then
  echo "Set both DEVELOPER_ID and NOTARY_PROFILE, or neither." >&2
  exit 1
fi

./build.sh

if [ -n "$DEVELOPER_ID" ]; then
  # Hardened runtime: the microphone (voice notes) must be allowed by an entitlement.
  ENT="build/entitlements.plist"
  cat > "$ENT" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.device.audio-input</key><true/>
</dict>
</plist>
PLIST
  # The context-capsule-mcp helper (a Bun-compiled program): its JavaScript engine needs these under the hardened runtime.
  HELPER="$APP/Contents/Resources/context-capsule-mcp"
  if [ -f "$HELPER" ]; then
    HENT="build/helper-entitlements.plist"
    cat > "$HENT" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.cs.allow-jit</key><true/>
  <key>com.apple.security.cs.allow-unsigned-executable-memory</key><true/>
  <key>com.apple.security.cs.disable-executable-page-protection</key><true/>
  <key>com.apple.security.cs.allow-dyld-environment-variables</key><true/>
  <key>com.apple.security.cs.disable-library-validation</key><true/>
</dict>
</plist>
PLIST
    codesign --force --options runtime --timestamp --entitlements "$HENT" \
      -s "$DEVELOPER_ID" --identifier com.thekarya.contextcapsule.mcp "$HELPER"
  fi
  codesign --force --options runtime --timestamp --entitlements "$ENT" \
    -s "$DEVELOPER_ID" --identifier com.thekarya.contextcapsule "$APP"
  echo "signed with \"$DEVELOPER_ID\" (hardened runtime)"
fi
codesign --verify --strict --deep "$APP"

mkdir -p "$OUT"
rm -f "$ZIP" "$DMG"

notarize() { # notarize <file>: send to Apple and wait for the answer
  xcrun notarytool submit "$1" --keychain-profile "$NOTARY_PROFILE" --wait
}

if [ -n "$DEVELOPER_ID" ]; then
  # Notarize the app (sent as a zip), then staple the ticket to the app so it opens offline too.
  ditto -c -k --keepParent "$APP" "build/notarize.zip"
  notarize "build/notarize.zip"
  rm -f "build/notarize.zip"
  xcrun stapler staple "$APP"
fi

# The updater's zip (holds the stapled app when notarized; a zip itself can't be stapled).
ditto -c -k --keepParent "$APP" "$ZIP"

# The disk image: the app and a link to Applications. Nothing else.
STAGE="$(mktemp -d)"
ditto "$APP" "$STAGE/Context Capsule.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -quiet -volname "Context Capsule" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG"
rm -rf "$STAGE"

if [ -n "$DEVELOPER_ID" ]; then
  codesign --force --timestamp -s "$DEVELOPER_ID" "$DMG"
  notarize "$DMG"
  xcrun stapler staple "$DMG"
fi

echo "made:"
ls -l "$ZIP" "$DMG"
echo "next: release/make-release.sh $VER"
