#!/bin/sh
# context-capsule — capture: drives the shared window through its real bridge (--test-web-window) on a COPY of some data
# (default /tmp/mac-ref-data): every page, search, a recording opened, a read deleted, settings changed, events sent.
# Usage: capture/tests/web-window.sh [dataFolder]   (build the app first: capture/build.sh). Never touches ~/.context-capsule.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
APP="$HERE/../build/Context Capsule.app/Contents/MacOS/ContextCapsule"
DATA="${1:-/tmp/mac-ref-data}"
COPY="$(mktemp -d /tmp/cc-web-check.XXXXXX)"
trap 'rm -rf "$COPY"' EXIT
cp -R "$DATA"/. "$COPY"/
CONTEXT_CAPSULE_HOME="$COPY" "$APP" --test-web-window 2>/dev/null
