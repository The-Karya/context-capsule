#!/bin/bash
# context-capsule — ui: render-all — renders every page × light/dark with snap into /tmp/shared-ui (+ full/, narrow/, large/).
# Usage: ui/dev/render-all.sh [dataFolder=/tmp/mac-ref-data]   (PORT=5174 by default; reuses a server already on it)
set -euo pipefail
DEV="$(cd "$(dirname "$0")" && pwd)"
DATA="${1:-/tmp/mac-ref-data}"
PORT="${PORT:-5174}"
OUT=/tmp/shared-ui
TOOLS=/tmp/shared-ui-tools
NOW="2026-10-23T19:00:00"

mkdir -p "$TOOLS" "$OUT/full" "$OUT/narrow" "$OUT/large"
if [ ! -x "$TOOLS/snap" ] || [ "$DEV/snap.swift" -nt "$TOOLS/snap" ]; then
  echo "building snap…"; swiftc -O "$DEV/snap.swift" -o "$TOOLS/snap"
fi

SERVER_PID=""
if ! curl -sf "http://localhost:$PORT/dev/shim.js" >/dev/null; then
  bun "$DEV/server.js" "$DATA" --port "$PORT" >"$OUT/server.log" 2>&1 &
  SERVER_PID=$!
  trap 'kill $SERVER_PID 2>/dev/null' EXIT
  for _ in $(seq 50); do curl -sf "http://localhost:$PORT/dev/shim.js" >/dev/null && break; sleep 0.1; done
fi

# page  query  hash
PAGES="home||home
timeline||timeline
timeline-open|open=first&|timeline
capsules||capsules
projects||projects
settings||settings
help||help
welcome|firstRun=1&|settings"

jobs_file=$(mktemp)
while IFS='|' read -r page query hash; do
  for mode in light dark; do
    dark=0; [ "$mode" = dark ] && dark=1
    url="http://localhost:$PORT/?${query}now=$NOW&dark=$dark#$hash"
    printf '%s\n' "$url $OUT/$page-$mode.png 860 620 $mode" \
      "$url $OUT/full/$page-$mode.png 860 1700 $mode" \
      "$url $OUT/narrow/$page-$mode.png 640 620 $mode" \
      "$url $OUT/large/$page-$mode.png 1280 900 $mode" >>"$jobs_file"
  done
done <<<"$PAGES"

xargs -P 6 -L 1 "$TOOLS/snap" <"$jobs_file"
rm -f "$jobs_file"
echo "rendered $(ls "$OUT"/*.png | wc -l | tr -d ' ') pages × modes into $OUT (+ full/ narrow/ large/)"
