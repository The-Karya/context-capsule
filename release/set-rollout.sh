#!/bin/sh
# context-capsule — release: change a published release's staged rollout (the % of people whose app updates by
# itself; 0 stops it), re-sign latest.json and print the upload command. Pushes nothing.
#
#   release/set-rollout.sh <version> <0-100> [folder]     (folder: where make-release.sh wrote latest.json; default dist/)
#
# People who click Check for updates get the new version at any rollout above 0. New downloads from the download
# page always get the newest version.
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
VER="${1:-}"
PCT="${2:-}"
DIR="${3:-$ROOT/dist}"
KEY="${UPDATE_KEY:-$HOME/.context-capsule-release/update-key.private}"
PUBLIC_KEY="WHTBrAjhTOQIG3S15PKYqHHdj5nbon4J/eEam7LrXd4="
REPO="The-Karya/context-capsule"

case "$PCT" in ''|*[!0-9]*) echo "usage: $0 <version> <0-100> [folder]" >&2; exit 1 ;; esac
[ "$PCT" -le 100 ] || { echo "usage: $0 <version> <0-100> [folder]" >&2; exit 1; }
FEED="$DIR/latest.json"
[ -f "$FEED" ] || { echo "no $FEED (run make-release.sh first)" >&2; exit 1; }
grep -q "\"version\": \"$VER\"" "$FEED" || { echo "$FEED is not for $VER" >&2; exit 1; }
[ -f "$KEY" ] || { echo "no signing key at $KEY" >&2; exit 1; }

sed -i '' -E "s/\"rollout\": [0-9]+/\"rollout\": $PCT/" "$FEED"
grep -q "\"rollout\": $PCT," "$FEED" || { echo "no rollout line in $FEED" >&2; exit 1; }
swift "$HERE/sign.swift" sign "$KEY" "$FEED" "$FEED.sig"
swift "$HERE/sign.swift" verify "$PUBLIC_KEY" "$FEED" "$FEED.sig"

echo "rollout for $VER: $PCT%"
echo
echo "Run (replaces latest.json on the release):"
echo
echo "  cd \"$DIR\" && gh release upload v$VER --repo $REPO --clobber latest.json latest.json.sig"
