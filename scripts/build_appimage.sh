#!/usr/bin/env bash
set -euo pipefail

APP_NAME="vesper"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APPDIR="${ROOT_DIR}/dist/AppDir"
OUT_DIR="${ROOT_DIR}/dist"
ARCH="$(uname -m)"
export ARCH

mkdir -p "${OUT_DIR}"
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/128x128/apps"

NO_BUILD=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build) NO_BUILD=1; shift ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ "${NO_BUILD}" -eq 0 ]]; then
  echo "Building release..."
  cargo build --release
fi

cp "${ROOT_DIR}/target/release/${APP_NAME}" "${APPDIR}/usr/bin/${APP_NAME}"
cp "${ROOT_DIR}/packaging/appimage/${APP_NAME}.desktop" "${APPDIR}/usr/share/applications/${APP_NAME}.desktop"
cp "${ROOT_DIR}/packaging/appimage/${APP_NAME}.svg" "${APPDIR}/usr/share/icons/hicolor/scalable/apps/${APP_NAME}.svg"
cp "${ROOT_DIR}/packaging/appimage/${APP_NAME}.desktop" "${APPDIR}/${APP_NAME}.desktop"
cp "${ROOT_DIR}/packaging/appimage/${APP_NAME}.svg" "${APPDIR}/${APP_NAME}.svg"

# Ensure icon theme is discoverable (needed for some trays)
cat > "${APPDIR}/usr/share/icons/hicolor/index.theme" << 'EOF'
[Icon Theme]
Name=Hicolor
Comment=Default Theme
Directories=128x128/apps,scalable/apps

[128x128/apps]
Size=128
Type=Fixed
Context=Applications

[scalable/apps]
Size=128
Type=Scalable
MinSize=1
MaxSize=256
Context=Applications
EOF

# Optional: rasterize SVG to PNG for trays that don't support SVG icons
if command -v rsvg-convert >/dev/null 2>&1; then
  rsvg-convert -w 128 -h 128 -o "${APPDIR}/usr/share/icons/hicolor/128x128/apps/${APP_NAME}.png" \
    "${ROOT_DIR}/packaging/appimage/${APP_NAME}.svg" || true
fi

cat > "${APPDIR}/AppRun" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail
HERE="$(dirname "$(readlink -f "$0")")"
exec "${HERE}/usr/bin/vesper" "$@"
EOF
chmod +x "${APPDIR}/AppRun"

echo "Building AppImage..."
appimagetool "${APPDIR}" "${OUT_DIR}/${APP_NAME}-${ARCH}.AppImage"
echo "Done: ${OUT_DIR}/${APP_NAME}-${ARCH}.AppImage"
