#!/bin/bash
# context-capsule — capture: renders the shared window (ui/ in the app's real web view) for review: every page in light
# and dark into /tmp/mac-web/<page>-<mode>.png (flat), and glass into /tmp/mac-web/glass-<page>-<mode>.png.
# Usage: capture/tests/render-web.sh [dataFolder=/tmp/mac-ref-data]   (build the app first: capture/build.sh)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
APP="$HERE/../build/Context Capsule.app/Contents/MacOS/ContextCapsule"
DATA="${1:-/tmp/mac-ref-data}"
OUT=/tmp/mac-web
mkdir -p "$OUT"
for page in home timeline timeline-open capsules projects settings help welcome; do
  for mode in light dark; do
    CONTEXT_CAPSULE_HOME="$DATA" "$APP" --render-web-page "$page" "$OUT/$page-$mode.png" "$mode" 2>/dev/null
    CONTEXT_CAPSULE_HOME="$DATA" "$APP" --render-web-page "$page" "$OUT/glass-$page-$mode.png" "$mode" 860 620 glass 2>/dev/null
  done
done
echo "rendered into $OUT"
