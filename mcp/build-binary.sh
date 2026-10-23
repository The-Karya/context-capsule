#!/bin/sh
# context-capsule — mcp: build context-capsule-mcp as self-contained binaries (no Node needed) with
# `bun build --compile`, into mcp/dist-bin/. With no arguments: every platform, plus one universal Mac file.
# Or name the ones you want: ./build-binary.sh linux-x64 linux-arm64
#   macos-arm64 macos-x64 windows-x64 linux-x64 linux-arm64
set -eu
cd "$(dirname "$0")"
[ -d node_modules ] || bun install --frozen-lockfile
OUT=dist-bin
mkdir -p "$OUT"
TARGETS="${*:-macos-arm64 macos-x64 windows-x64 linux-x64 linux-arm64}"
NICE=""
command -v nice >/dev/null 2>&1 && NICE="nice -n 19"

for t in $TARGETS; do
  case "$t" in
    macos-arm64) bt=bun-darwin-arm64 ;;
    macos-x64) bt=bun-darwin-x64 ;;
    windows-x64) bt=bun-windows-x64 ;;
    linux-x64) bt=bun-linux-x64 ;;
    linux-arm64) bt=bun-linux-arm64 ;;
    *) echo "unknown target: $t" >&2; exit 2 ;;
  esac
  name="context-capsule-mcp-$t"
  [ "$t" = windows-x64 ] && name="$name.exe"
  $NICE bun build src/index.ts --compile --minify --target="$bt" --outfile "$OUT/$name" >/dev/null
  echo "built $OUT/$name ($(du -h "$OUT/$name" | cut -f1))"
done

# One Mac file for both Apple silicon and Intel.
case "$TARGETS" in *macos*) MAC=1 ;; *) MAC=0 ;; esac
if [ "$MAC" = 1 ] && [ -f "$OUT/context-capsule-mcp-macos-arm64" ] && [ -f "$OUT/context-capsule-mcp-macos-x64" ] && command -v lipo >/dev/null 2>&1; then
  lipo -create "$OUT/context-capsule-mcp-macos-arm64" "$OUT/context-capsule-mcp-macos-x64" -output "$OUT/context-capsule-mcp-macos"
  echo "built $OUT/context-capsule-mcp-macos, universal ($(du -h "$OUT/context-capsule-mcp-macos" | cut -f1))"
fi
