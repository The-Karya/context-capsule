#!/bin/sh
# context-capsule — release: write and sign latest.json for already-built files, add stable-named copies for the
# download page, and print the `gh release create` command. Creates no tag or release and pushes nothing.
#
#   release/make-release.sh <version> [folder]        (folder: where the built files are; default dist/)
#
# Looks in the folder for any of:
#   Context-Capsule-<version>-mac.zip  (+ Context-Capsule-<version>-mac.dmg)   capture/package.sh
#   Context-Capsule-<version>-windows.exe                                       capture-rs/windows
#   Context-Capsule-<version>-x86_64.AppImage                                   capture-rs/linux
# Env: NOTES="One or two plain sentences." (shown to people), ROLLOUT=10 (staged: the % of people who get it by
# themselves; default 100; raise it later with release/set-rollout.sh), UPDATE_KEY (default ~/.context-capsule-release/update-key.private)
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
VER="${1:-}"
DIR="${2:-$ROOT/dist}"
KEY="${UPDATE_KEY:-$HOME/.context-capsule-release/update-key.private}"
NOTES="${NOTES:-Small fixes and improvements.}"
ROLLOUT="${ROLLOUT:-100}"
PUBLIC_KEY="WHTBrAjhTOQIG3S15PKYqHHdj5nbon4J/eEam7LrXd4="
REPO="The-Karya/context-capsule"
BASE="https://github.com/$REPO/releases/download/v$VER"

if ! printf '%s' "$VER" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'; then
  echo "usage: $0 <version like 0.2.0> [folder]" >&2; exit 1
fi
[ -d "$DIR" ] || { echo "no folder $DIR" >&2; exit 1; }
DIR="$(cd "$DIR" && pwd)"
[ -f "$KEY" ] || { echo "no signing key at $KEY" >&2; exit 1; }
case "$ROLLOUT" in ''|*[!0-9]*) echo "ROLLOUT must be a number 0-100" >&2; exit 1 ;; esac
[ "$ROLLOUT" -le 100 ] || { echo "ROLLOUT must be a number 0-100" >&2; exit 1; }
case "$NOTES" in *'
'*) echo "NOTES must be one line" >&2; exit 1 ;; esac

# JSON string escaping for the notes (backslash and double quote; tabs become spaces).
NOTES_JSON="$(printf '%s' "$NOTES" | tr '\t' ' ' | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')"

sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
size() { wc -c < "$1" | tr -d ' '; }

ENTRIES=""
UPLOAD=""
add() { # add <json key> <versioned file> [<stable copy name>]
  key="$1"; file="$2"; stable="${3:-}"
  [ -f "$DIR/$file" ] || return 0
  entry="  \"$key\": { \"url\": \"$BASE/$file\", \"sha256\": \"$(sha "$DIR/$file")\", \"size\": $(size "$DIR/$file") }"
  ENTRIES="${ENTRIES:+$ENTRIES,
}$entry"
  UPLOAD="$UPLOAD \"$file\""
  if [ -n "$stable" ]; then cp "$DIR/$file" "$DIR/$stable"; UPLOAD="$UPLOAD \"$stable\""; fi
  echo "  $key: $file"
}
stable_only() { # a file people download but the updater doesn't use (the Mac disk image)
  [ -f "$DIR/$1" ] || return 0
  cp "$DIR/$1" "$DIR/$2"
  UPLOAD="$UPLOAD \"$1\" \"$2\""
  echo "  download page: $1"
}

MAC_ZIP="Context-Capsule-$VER-mac.zip"
if [ -f "$DIR/$MAC_ZIP" ]; then
  # The app inside must say the same version, or every Mac would refuse the update.
  T="$(mktemp -d)"
  ditto -x -k "$DIR/$MAC_ZIP" "$T"
  INNER="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$T/Context Capsule.app/Contents/Info.plist" 2>/dev/null || true)"
  rm -rf "$T"
  [ "$INNER" = "$VER" ] || { echo "$MAC_ZIP holds version '$INNER', not $VER" >&2; exit 1; }
fi

echo "files for v$VER in $DIR:"
add mac "$MAC_ZIP"
stable_only "Context-Capsule-$VER-mac.dmg" "Context-Capsule-mac.dmg"
add windows "Context-Capsule-$VER-windows.exe" "Context-Capsule-windows.exe"
add linux "Context-Capsule-$VER-x86_64.AppImage" "Context-Capsule-x86_64.AppImage"
[ -n "$ENTRIES" ] || { echo "no release files for $VER in $DIR" >&2; exit 1; }

cat > "$DIR/latest.json" <<JSON
{
  "version": "$VER",
  "date": "$(date -u +%Y-%m-%d)",
  "notes": "$NOTES_JSON",
  "rollout": $ROLLOUT,
$ENTRIES
}
JSON

swift "$HERE/sign.swift" sign "$KEY" "$DIR/latest.json" "$DIR/latest.json.sig"
swift "$HERE/sign.swift" verify "$PUBLIC_KEY" "$DIR/latest.json" "$DIR/latest.json.sig"
UPLOAD="$UPLOAD \"latest.json\" \"latest.json.sig\""

echo
echo "latest.json:"
cat "$DIR/latest.json"
echo
echo "Check it, then run (this creates the tag and the public release):"
echo
echo "  cd \"$DIR\" && gh release create v$VER --repo $REPO --title \"$VER\" --notes \"$NOTES_JSON\"$UPLOAD"
