#!/bin/bash
# context-capsule — capture: renders the shared window (ui/ in the app's real web view) for review: every page in light
# and dark into /tmp/mac-web/<page>-<mode>.png (flat), glass into /tmp/mac-web/glass-<page>-<mode>.png, and each next to
# the old SwiftUI window's render (/tmp/mac-ref, from --render-page) in /tmp/mac-web/compare/.
# Usage: capture/tests/render-web.sh [dataFolder=/tmp/mac-ref-data]   (build the app first: capture/build.sh)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
APP="$HERE/../build/Context Capsule.app/Contents/MacOS/ContextCapsule"
DATA="${1:-/tmp/mac-ref-data}"
OUT=/tmp/mac-web
mkdir -p "$OUT/compare"
for page in home timeline timeline-open capsules projects settings help welcome; do
  for mode in light dark; do
    CONTEXT_CAPSULE_HOME="$DATA" "$APP" --render-web-page "$page" "$OUT/$page-$mode.png" "$mode" 2>/dev/null
    CONTEXT_CAPSULE_HOME="$DATA" "$APP" --render-web-page "$page" "$OUT/glass-$page-$mode.png" "$mode" 860 620 glass 2>/dev/null
  done
done
if [ -d /tmp/mac-ref ]; then
  TOOLS=/tmp/shared-ui-tools
  mkdir -p "$TOOLS"
  [ -x "$TOOLS/compare" ] || swiftc -O "$HERE/../../ui/dev/compare.swift" -o "$TOOLS/compare"
  "$TOOLS/compare" /tmp/mac-ref "$OUT" "$OUT/compare" >/dev/null
fi
echo "rendered into $OUT (compare/: the old window left, the shared one right)"
