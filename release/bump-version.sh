#!/bin/sh
# context-capsule — release: set the one product version everywhere.
#   release/bump-version.sh 0.2.0
# Mac (capture/build.sh), Windows + Linux (capture-rs/Cargo.toml workspace version, and our crates in Cargo.lock),
# the Claude side (mcp/package.json, mcp/extension/manifest.json, mcp/src/server.ts VERSION).
set -eu
cd "$(dirname "$0")/.."

VER="${1:-}"
if ! printf '%s' "$VER" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'; then
  echo "usage: $0 <version like 0.2.0>" >&2; exit 1
fi

# sed -i differs between macOS and Linux: write to a temp file instead.
edit() { # edit <file> <sed expression>
  tmp="$1.bump.$$"
  sed -E "$2" "$1" > "$tmp" && cat "$tmp" > "$1" && rm -f "$tmp"
}

edit capture/build.sh "s#(<key>CFBundleShortVersionString</key><string>)[^<]*(</string>)#\1$VER\2#"
# The build number: the version's numbers (0.2.0 -> 0.2.0), so each release has a higher one.
edit capture/build.sh "s#(<key>CFBundleVersion</key><string>)[^<]*(</string>)#\1${VER%%-*}\2#"
# Only the [workspace.package] version line (dependency versions are inline tables).
edit capture-rs/Cargo.toml "/^\[workspace\.package\]/,/^\[/ s/^version = \"[^\"]*\"/version = \"$VER\"/"
if [ -f capture-rs/Cargo.lock ]; then
  edit capture-rs/Cargo.lock "/^name = \"context-capsule-(core|windows|linux)\"$/{n;s/^version = \"[^\"]*\"/version = \"$VER\"/;}"
fi
edit mcp/package.json "1,/\"version\"/ s/^(  \"version\": )\"[^\"]*\"/\1\"$VER\"/"
edit mcp/extension/manifest.json "s/^(  \"version\": )\"[^\"]*\"/\1\"$VER\"/"
edit mcp/src/server.ts "s/^(export const VERSION = )\"[^\"]*\"/\1\"$VER\"/"

echo "version is now $VER in:"
grep -n "CFBundleShortVersionString\|CFBundleVersion" capture/build.sh
grep -n "^version" capture-rs/Cargo.toml
grep -n -A1 'name = "context-capsule-' capture-rs/Cargo.lock 2>/dev/null | grep version || true
grep -n '"version"' mcp/package.json mcp/extension/manifest.json
grep -n "export const VERSION" mcp/src/server.ts
