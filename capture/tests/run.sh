#!/bin/sh
# karya-capsule — capture: compile and run the unit tests (pure logic only, no screen access).
set -eu
cd "$(dirname "$0")/.."
mkdir -p build
nice -n 19 swiftc -swift-version 5 -target "$(uname -m)-apple-macos13.0" \
  Sources/Core.swift tests/main.swift -o build/capsule-tests
./build/capsule-tests
