#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAME="vesper"

usage() {
  cat <<'EOF'
Usage: scripts/build_pacman.sh [options]

Builds a binary pacman package (.pkg.tar.zst) for Arch Linux.

Options:
  --no-build         Do not run `cargo build --release` (expects target/release/vesper).
  --version VERSION  Override package version (defaults to Cargo.toml version).
  --pkgrel REL       Override package release number (default: 1).
  --arch ARCH        Override package architecture (default: x86_64).
  --out-dir DIR      Output directory (default: dist/pacman).
  -h, --help         Show this help.
EOF
}

NO_BUILD=0
OUT_DIR="${ROOT_DIR}/dist/pacman"
OVERRIDE_ARCH=""
OVERRIDE_VERSION=""
PKGREL="1"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --no-build) NO_BUILD=1; shift ;;
    --version) OVERRIDE_VERSION="${2:-}"; shift 2 ;;
    --pkgrel) PKGREL="${2:-}"; shift 2 ;;
    --arch) OVERRIDE_ARCH="${2:-}"; shift 2 ;;
    --out-dir) OUT_DIR="${2:-}"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

VERSION="${OVERRIDE_VERSION}"
if [[ -z "${VERSION}" ]]; then
  VERSION="$(awk -F'"' '/^version[[:space:]]*=[[:space:]]*"/ {print $2; exit}' "${ROOT_DIR}/Cargo.toml")"
fi
if [[ -z "${VERSION}" ]]; then
  echo "Failed to detect version from Cargo.toml" >&2
  exit 1
fi

ARCH="${OVERRIDE_ARCH:-x86_64}"

mkdir -p "${OUT_DIR}"

if [[ "${NO_BUILD}" -eq 0 ]]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found. Install Rust (cargo) or run with --no-build." >&2
    exit 1
  fi
  echo "Building release..."
  cargo build --release
fi

BIN_SRC="${ROOT_DIR}/target/release/${NAME}"
if [[ ! -f "${BIN_SRC}" ]]; then
  echo "Binary not found: ${BIN_SRC}" >&2
  exit 1
fi

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

STAGE_DIR="${WORK_DIR}/pkg"
mkdir -p "${STAGE_DIR}/usr/bin"
mkdir -p "${STAGE_DIR}/usr/share/applications"
mkdir -p "${STAGE_DIR}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${STAGE_DIR}/usr/share/licenses/${NAME}"
mkdir -p "${STAGE_DIR}/usr/share/doc/${NAME}"

install -m 0755 "${BIN_SRC}" "${STAGE_DIR}/usr/bin/${NAME}"
install -m 0644 "${ROOT_DIR}/packaging/appimage/${NAME}.desktop" "${STAGE_DIR}/usr/share/applications/${NAME}.desktop"
install -m 0644 "${ROOT_DIR}/packaging/appimage/${NAME}.svg" "${STAGE_DIR}/usr/share/icons/hicolor/scalable/apps/${NAME}.svg"
install -m 0644 "${ROOT_DIR}/LICENSE" "${STAGE_DIR}/usr/share/licenses/${NAME}/LICENSE"
install -m 0644 "${ROOT_DIR}/README.md" "${STAGE_DIR}/usr/share/doc/${NAME}/README.md"

BUILD_DATE="$(date +%s)"
INSTALLED_SIZE="$(du -sb "${STAGE_DIR}" | awk '{print $1}')"

cat > "${STAGE_DIR}/.PKGINFO" <<EOF
pkgname = ${NAME}
pkgbase = ${NAME}
pkgver = ${VERSION}-${PKGREL}
pkgdesc = Linux desktop screensaver written in Rust with GTK4
url = https://github.com/leocallidus/vesper
builddate = ${BUILD_DATE}
packager = leocallidus <leoalekseev27@gmail.com>
size = ${INSTALLED_SIZE}
arch = ${ARCH}
license = MIT
depend = gtk4
depend = libadwaita
depend = webkitgtk-6.0
EOF

OUT_PKG="${OUT_DIR}/${NAME}-${VERSION}-${PKGREL}-${ARCH}.pkg.tar.zst"

if command -v bsdtar >/dev/null 2>&1; then
  (
    cd "${STAGE_DIR}"
    bsdtar -czf .MTREE --format=mtree \
      --options='!all,use-set,type,uid,gid,mode,time,size,md5,sha256,link' \
      .PKGINFO usr
    bsdtar -cf - .PKGINFO .MTREE usr | zstd -T0 -19 > "${OUT_PKG}"
  )
elif command -v zstd >/dev/null 2>&1; then
  (
    cd "${STAGE_DIR}"
    tar --sort=name --owner=root:0 --group=root:0 --numeric-owner -cf - .PKGINFO usr | zstd -T0 -19 > "${OUT_PKG}"
  )
else
  echo "Neither bsdtar nor zstd found. Install zstd." >&2
  exit 1
fi

echo "Done: ${OUT_PKG}"
