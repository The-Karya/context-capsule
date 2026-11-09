#!/bin/sh
# context-capsule — capture: drives the shared window through its real bridge (--test-web-window) on throwaway data
# seeded for one clock (capture-rs/ci/windows-smoke-seed.py): every page, search, a recording opened, a read deleted,
# settings changed, events sent. The seed and the window get the same clock, so it passes on any day.
# Usage: [CONTEXT_CAPSULE_NOW=2026-10-23T19:00:00] capture/tests/web-window.sh   (default: today at 19:00 on this computer)
# Build the app first: capture/build.sh. Never touches ~/.context-capsule.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
APP="$HERE/../build/Context Capsule.app/Contents/MacOS/ContextCapsule"
NOW="${CONTEXT_CAPSULE_NOW:-$(date +%Y-%m-%d)T19:00:00}"
DATA="$(mktemp -d /tmp/cc-web-check.XXXXXX)"
trap 'rm -rf "$DATA"' EXIT
echo "clock $NOW, seeded day $(python3 "$HERE/../../capture-rs/ci/windows-smoke-seed.py" "$DATA" "$NOW" | head -1)"
CONTEXT_CAPSULE_HOME="$DATA" CONTEXT_CAPSULE_NOW="$NOW" "$APP" --test-web-window 2>/dev/null
