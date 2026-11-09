#!/bin/bash
# context-capsule — ui: render-all — renders every page × light/dark with snap into /tmp/shared-ui (+ full/, narrow/, large/),
# on throwaway data seeded for the page's clock (capture-rs/ci/windows-smoke-seed.py), the clock of the Mac's
# capture/tests/render-web.sh, so the seed and the page agree on any day.
# Usage: [CONTEXT_CAPSULE_NOW=2026-10-23T19:00:00] ui/dev/render-all.sh   (PORT=5174 by default; it must be free)
set -euo pipefail
DEV="$(cd "$(dirname "$0")" && pwd)"
PORT="${PORT:-5174}"
OUT=/tmp/shared-ui
TOOLS=/tmp/shared-ui-tools
NOW="${CONTEXT_CAPSULE_NOW:-2026-10-23T19:00:00}"
DATA="$(mktemp -d /tmp/cc-shared-ui-data.XXXXXX)"
python3 "$DEV/../../capture-rs/ci/windows-smoke-seed.py" "$DATA" "$NOW" >/dev/null

mkdir -p "$TOOLS" "$OUT/full" "$OUT/narrow" "$OUT/large"
if [ ! -x "$TOOLS/snap" ] || [ "$DEV/snap.swift" -nt "$TOOLS/snap" ]; then
  echo "building snap…"; swiftc -O "$DEV/snap.swift" -o "$TOOLS/snap"
fi

# Its own server on the seeded data (one already on the port would serve other data, for another clock).
if curl -sf "http://localhost:$PORT/dev/shim.js" >/dev/null; then
  echo "port $PORT is in use: stop that server or set PORT" >&2; rm -rf "$DATA"; exit 1
fi
bun "$DEV/server.js" "$DATA" --port "$PORT" >"$OUT/server.log" 2>&1 &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null; rm -rf "$DATA"' EXIT
for _ in $(seq 50); do curl -sf "http://localhost:$PORT/dev/shim.js" >/dev/null && break; sleep 0.1; done

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
