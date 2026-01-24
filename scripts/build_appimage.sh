#!/usr/bin/env bash
set -euo pipefail

APP_NAME="vesper"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APPDIR="${ROOT_DIR}/dist/AppDir"
OUT_DIR="${ROOT_DIR}/dist"
ARCH="$(uname -m)"

mkdir -p "${OUT_DIR}"
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/scalable/apps"

echo "Building release..."
cargo build --release

cp "${ROOT_DIR}/target/release/${APP_NAME}" "${APPDIR}/usr/bin/${APP_NAME}"
cp "${ROOT_DIR}/packaging/appimage/${APP_NAME}.desktop" "${APPDIR}/usr/share/applications/${APP_NAME}.desktop"
cp "${ROOT_DIR}/packaging/appimage/${APP_NAME}.svg" "${APPDIR}/usr/share/icons/hicolor/scalable/apps/${APP_NAME}.svg"
cp "${ROOT_DIR}/packaging/appimage/${APP_NAME}.desktop" "${APPDIR}/${APP_NAME}.desktop"
cp "${ROOT_DIR}/packaging/appimage/${APP_NAME}.svg" "${APPDIR}/${APP_NAME}.svg"

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
