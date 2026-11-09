#!/bin/bash
# context-capsule — capture: renders the shared window (ui/ in the app's real web view) for review: every page in light
# and dark into /tmp/mac-web/<page>-<mode>.png (flat), and glass into /tmp/mac-web/glass-<page>-<mode>.png, on
# throwaway data seeded for the page's clock (capture-rs/ci/windows-smoke-seed.py; the clock of ui/dev/render-all.sh).
# A render that stays blank (one colour) after a few tries fails, and so does this script.
# Usage: [CONTEXT_CAPSULE_NOW=2026-10-23T19:00:00] capture/tests/render-web.sh   (build the app first: capture/build.sh)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
APP="$HERE/../build/Context Capsule.app/Contents/MacOS/ContextCapsule"
NOW="${CONTEXT_CAPSULE_NOW:-2026-10-23T19:00:00}"
OUT=/tmp/mac-web
DATA="$(mktemp -d /tmp/cc-web-render.XXXXXX)"
trap 'rm -rf "$DATA"' EXIT
python3 "$HERE/../../capture-rs/ci/windows-smoke-seed.py" "$DATA" "$NOW" >/dev/null
mkdir -p "$OUT"
render() { # page out [args…]: the app's own messages are dropped, except why a render failed
  if ! CONTEXT_CAPSULE_HOME="$DATA" CONTEXT_CAPSULE_NOW="$NOW" "$APP" --render-web-page "$@" 2>"$DATA/render.err"; then
    grep '^render\|CONTEXT_CAPSULE_NOW' "$DATA/render.err" >&2 || true
    echo "render failed: $*" >&2
    exit 1
  fi
  grep '^render' "$DATA/render.err" >&2 || true
}
for page in home timeline timeline-open capsules projects settings help welcome; do
  for mode in light dark; do
    render "$page" "$OUT/$page-$mode.png" "$mode"
    render "$page" "$OUT/glass-$page-$mode.png" "$mode" 860 620 glass
  done
done
echo "rendered into $OUT (clock $NOW)"
