#!/bin/sh
# context-capsule — linux: release build of the Linux recorder (target/release/context-capsule).
#   ./build.sh            with voice notes (needs cmake, clang and a C++ compiler for whisper.cpp)
#   ./build.sh --no-voice without voice notes
set -eu
cd "$(dirname "$0")/.." # the capture-rs workspace
if [ "${1:-}" = "--no-voice" ]; then
  exec nice -n 19 cargo build --release -p context-capsule-linux --no-default-features
fi
for tool in cmake clang pkg-config; do
  command -v "$tool" >/dev/null 2>&1 || echo "warning: $tool not found; see linux/README.md (or ./build.sh --no-voice)" >&2
done
exec nice -n 19 cargo build --release -p context-capsule-linux
