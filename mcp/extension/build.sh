#!/bin/sh
# context-capsule — mcp: build the Claude Desktop extension (context-capsule.mcpb): the server plus its
# runtime dependencies in one file. Double-clicking it in Finder/Explorer installs it in Claude Desktop,
# which brings its own Node.js — users need no developer tools.
set -eu
cd "$(dirname "$0")/.."
nice -n 19 bun run build >/dev/null
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp extension/manifest.json "$STAGE/"
[ -f extension/icon.png ] && cp extension/icon.png "$STAGE/"
mkdir -p "$STAGE/server"
cp dist/*.js "$STAGE/server/"
# Runtime dependencies only.
node -e 'const p=require("./package.json"); console.log(JSON.stringify({name:p.name,version:p.version,type:p.type,dependencies:p.dependencies},null,2))' > "$STAGE/package.json"
(cd "$STAGE" && nice -n 19 npm install --omit=dev --no-audit --no-fund --silent)
mkdir -p dist-ext
npx -y @anthropic-ai/mcpb validate "$STAGE/manifest.json"
npx -y @anthropic-ai/mcpb pack "$STAGE" dist-ext/context-capsule.mcpb
echo "built mcp/dist-ext/context-capsule.mcpb"
