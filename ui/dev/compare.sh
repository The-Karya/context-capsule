#!/bin/bash
# context-capsule — ui: compare — Mac render (left) beside ours (right) into /tmp/shared-ui/compare (and compare/full).
# Usage: ui/dev/compare.sh   (run ui/dev/render-all.sh first)
set -euo pipefail
DEV="$(cd "$(dirname "$0")" && pwd)"
TOOLS=/tmp/shared-ui-tools
mkdir -p "$TOOLS"
if [ ! -x "$TOOLS/compare" ] || [ "$DEV/compare.swift" -nt "$TOOLS/compare" ]; then
  echo "building compare…"; swiftc -O "$DEV/compare.swift" -o "$TOOLS/compare"
fi
"$TOOLS/compare" /tmp/mac-ref /tmp/shared-ui /tmp/shared-ui/compare
"$TOOLS/compare" /tmp/mac-ref/full /tmp/shared-ui/full /tmp/shared-ui/compare/full
