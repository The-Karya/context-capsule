#!/bin/sh
# context-capsule — linux: build the release AppImage, Context-Capsule-<version>-<arch>.AppImage (the release
# is x86_64, built by the "appimage" job in .github/workflows/ci.yml): one file people download, make runnable
# and open. It offers to add itself to the apps, and updates itself. The arch is this machine's (uname -m).
#   ./appimage.sh   on Linux: builds here (needs what ../ci/linux.Dockerfile installs, and curl)
#                   anywhere else (a Mac): builds in that Docker image, natively (an arm Mac makes aarch64)
# Output: ../target-appimage/Context-Capsule-<version>-<arch>.AppImage
# The AppImage holds the program, its .desktop entry and icon. Like the source build it uses the system's
# PipeWire, D-Bus, ALSA and Tesseract (see README.md, "Install the dependencies").
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
WS=$(dirname "$HERE") # the capture-rs workspace

if [ "$(uname -s)" != Linux ]; then
  IMAGE=context-capsule-linux
  docker build -t "$IMAGE" -f "$WS/ci/linux.Dockerfile" "$WS/ci"
  exec docker run --rm -v "$WS:/src" -v cc-cargo-registry-$(uname -m):/usr/local/cargo/registry \
    -w /src "$IMAGE" sh linux/appimage.sh
fi

ARCH=$(uname -m)
cd "$WS"
OUT=target-appimage
nice -n 19 cargo build --release -p context-capsule-linux --target-dir "$OUT"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
ID=com.thekarya.contextcapsule

APPDIR=$OUT/AppDir
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps"
cp "$OUT/release/context-capsule" "$APPDIR/usr/bin/"
strip "$APPDIR/usr/bin/context-capsule"
sed "s/^Icon=.*/Icon=$ID/" linux/$ID.desktop > "$APPDIR/$ID.desktop"
cp "$APPDIR/$ID.desktop" "$APPDIR/usr/share/applications/"
cp linux/$ID.png "$APPDIR/$ID.png"
cp linux/$ID.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/"
ln -s "$ID.png" "$APPDIR/.DirIcon"
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE=$(dirname "$(readlink -f "$0")")
exec "$HERE/usr/bin/context-capsule" "$@"
EOF
chmod +x "$APPDIR/AppRun"

# appimagetool (itself an AppImage: run it extracted, so no FUSE is needed in a container).
TOOL=$OUT/tools/appimagetool-$ARCH.AppImage
if [ ! -x "$TOOL" ]; then
  mkdir -p "$OUT/tools"
  curl -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
  chmod +x "$TOOL"
fi
RELEASE=$OUT/Context-Capsule-$VERSION-$ARCH.AppImage
rm -f "$RELEASE"
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$ARCH nice -n 19 "$TOOL" --no-appstream "$APPDIR" "$RELEASE"

echo "libraries it needs from the system:"
ldd "$APPDIR/usr/bin/context-capsule" | sed 's/ (0x.*//; s/^/  /'
echo "release file: $RELEASE ($(wc -c < "$RELEASE" | tr -d ' ') bytes, sha256 $(sha256sum "$RELEASE" | cut -d ' ' -f 1))"
