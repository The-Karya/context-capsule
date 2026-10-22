#!/bin/sh
# context-capsule — windows: build context-capsule.exe on a Mac (cargo-xwin + LLVM), with voice notes,
# the C/C++ runtime built in (runs on any Windows 10/11 PC, no extra installs), the Claude extension
# (context-capsule.mcpb) and the any-AI program (context-capsule-mcp.exe, zstd-compressed) inside it, then copy
# it as the release file Context-Capsule-<version>-windows.exe: one download that installs itself (no admin)
# and updates itself.
# Needs: brew install llvm lld ninja cmake; cargo install cargo-xwin; rustup target add x86_64-pc-windows-msvc.
# Running it accepts Microsoft's licence for the Windows SDK/CRT files cargo-xwin downloads (XWIN_ACCEPT_LICENSE).
set -eu
cd "$(dirname "$0")/.."
TARGET=x86_64-pc-windows-msvc
export PATH="/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$PATH"
export XWIN_ACCEPT_LICENSE=1

# whisper-rs-sys decides some flags from the HOST os (a Mac here), not the target:
# it asks for the Apple-only ggml-blas library, and skips /utf-8 and advapi32. Fix all three from outside.
SHIM="${TMPDIR:-/tmp}/cc-xwin-shim"
mkdir -p "$SHIM"
if [ ! -f "$SHIM/ggml-blas.lib" ]; then
  : > "$SHIM/empty.c"
  clang-cl --target=$TARGET /c "$SHIM/empty.c" "/Fo$SHIM/empty.obj" >/dev/null
  llvm-lib "/out:$SHIM/ggml-blas.lib" "$SHIM/empty.obj"
fi
export CFLAGS_x86_64_pc_windows_msvc="/utf-8" CXXFLAGS_x86_64_pc_windows_msvc="/utf-8"
# Static C/C++ runtime everywhere (Rust and whisper.cpp must agree).
export CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded
export CMAKE_C_FLAGS_RELEASE="/MT /O2 /Ob2 /DNDEBUG" CMAKE_CXX_FLAGS_RELEASE="/MT /O2 /Ob2 /DNDEBUG"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-C target-feature=+crt-static -L native=$SHIM -l advapi32"

# The Claude extension goes inside the exe (build.rs); build it first with mcp/extension/build.sh.
MCPB=../mcp/dist-ext/context-capsule.mcpb
if [ ! -f "$MCPB" ]; then
  echo "warning: $MCPB is missing: run mcp/extension/build.sh first, or Connect Claude won't work" >&2
fi
# The any-AI program goes inside too (build.rs compresses it); build it with mcp/build-binary.sh windows-x64.
MCP=../mcp/dist-bin/context-capsule-mcp-windows-x64.exe
if [ ! -f "$MCP" ]; then
  echo "warning: $MCP is missing: run mcp/build-binary.sh windows-x64 first, or Connect your AI and Copy my day won't work" >&2
fi

nice -n 19 cargo xwin build --release --target $TARGET -p context-capsule-windows --target-dir target-xwin "$@"
EXE=target-xwin/$TARGET/release/context-capsule.exe
if llvm-objdump -p "$EXE" | grep "DLL Name" | grep -qiE "msvcp|vcruntime|api-ms-win-crt"; then
  echo "warning: $EXE still needs the Visual C++ runtime DLLs" >&2
fi
# Also next to the exe, for a plain zip kit (the exe writes it there itself when missing).
if [ -f "$MCPB" ]; then
  cp "$MCPB" "$(dirname "$EXE")/"
fi
if [ -f "$MCP" ]; then
  cp "$MCP" "$(dirname "$EXE")/context-capsule-mcp.exe"
fi
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
RELEASE=target-xwin/Context-Capsule-$VERSION-windows.exe
cp "$EXE" "$RELEASE"
echo "built $EXE"
echo "release file: $RELEASE ($(wc -c < "$RELEASE" | tr -d ' ') bytes, sha256 $(shasum -a 256 "$RELEASE" | cut -d ' ' -f 1))"
