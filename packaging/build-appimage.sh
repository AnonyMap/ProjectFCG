#!/usr/bin/env bash
# Build ProjectFCG-x86_64.AppImage.
#
# Requires: cargo, appimagetool (path in $APPIMAGETOOL or ./tools) and the
# static type2 runtime (path in $APPIMAGE_RUNTIME) so the build works offline
# and the result needs no libfuse2 on the target machine.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TOOLS="${TOOLS:-$ROOT/../tools}"
APPIMAGETOOL="${APPIMAGETOOL:-$TOOLS/appimagetool}"
RUNTIME="${APPIMAGE_RUNTIME:-$TOOLS/runtime-x86_64}"
OUT="${OUT:-$ROOT/dist}"

cd "$ROOT"
cargo build --release

APPDIR="$ROOT/target/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$APPDIR/usr/share/icons/hicolor/scalable/apps"
install -m755 target/release/projectfcg "$APPDIR/usr/bin/projectfcg"
install -m755 packaging/AppRun "$APPDIR/AppRun"
install -m644 packaging/projectfcg.desktop "$APPDIR/projectfcg.desktop"
install -m644 packaging/projectfcg.desktop "$APPDIR/usr/share/applications/projectfcg.desktop"
install -m644 packaging/projectfcg.png "$APPDIR/projectfcg.png"
install -m644 packaging/projectfcg.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/projectfcg.png"
install -m644 packaging/projectfcg.svg "$APPDIR/usr/share/icons/hicolor/scalable/apps/projectfcg.svg"
ln -sf projectfcg.png "$APPDIR/.DirIcon"

mkdir -p "$OUT"
# appimagetool is itself an AppImage; extract-and-run avoids needing FUSE here.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$APPIMAGETOOL" --no-appstream \
    --runtime-file "$RUNTIME" "$APPDIR" "$OUT/ProjectFCG-x86_64.AppImage"
chmod +x "$OUT/ProjectFCG-x86_64.AppImage"
ls -la "$OUT/ProjectFCG-x86_64.AppImage"
