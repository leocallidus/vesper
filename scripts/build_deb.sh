#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAME="vesper"

usage() {
  cat <<'EOF'
Usage: scripts/build_deb.sh [options]

Builds a local binary .deb package for Vesper using dpkg-deb.

Options:
  --no-build         Do not run `cargo build --release` (expects target/release/vesper).
  --no-shlibdeps     Do not try to auto-detect Depends via dpkg-shlibdeps.
  --depends STRING   Override Depends (e.g. "libc6 (>= 2.35), libgtk-4-1").
  --arch ARCH        Override Debian architecture (e.g. amd64, arm64).
  --version VERSION  Override package version (defaults to Cargo.toml version).
  --out-dir DIR      Output directory (default: dist/deb).
  -h, --help         Show this help.

Environment:
  DEB_MAINTAINER  Maintainer field for control (default: "Vesper <noreply@local>").
EOF
}

NO_BUILD=0
NO_SHLIBDEPS=0
OUT_DIR="${ROOT_DIR}/dist/deb"
OVERRIDE_ARCH=""
OVERRIDE_VERSION=""
OVERRIDE_DEPENDS=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --no-build) NO_BUILD=1; shift ;;
    --no-shlibdeps) NO_SHLIBDEPS=1; shift ;;
    --depends) OVERRIDE_DEPENDS="${2:-}"; shift 2 ;;
    --arch) OVERRIDE_ARCH="${2:-}"; shift 2 ;;
    --version) OVERRIDE_VERSION="${2:-}"; shift 2 ;;
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

ARCH="${OVERRIDE_ARCH}"
if [[ -z "${ARCH}" ]]; then
  if command -v dpkg >/dev/null 2>&1; then
    ARCH="$(dpkg --print-architecture)"
  else
    case "$(uname -m)" in
      x86_64) ARCH="amd64" ;;
      aarch64) ARCH="arm64" ;;
      armv7l) ARCH="armhf" ;;
      *) echo "Unknown architecture: $(uname -m). Pass --arch." >&2; exit 1 ;;
    esac
  fi
fi

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
  echo "Run without --no-build or build it first: cargo build --release" >&2
  exit 1
fi

if ! command -v dpkg-deb >/dev/null 2>&1; then
  cat >&2 <<'EOF'
dpkg-deb not found.

Install it (Debian/Ubuntu):
  sudo apt install dpkg-dev
EOF
  exit 1
fi

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

STAGE_DIR="${WORK_DIR}/stage"
mkdir -p "${STAGE_DIR}/DEBIAN"
mkdir -p "${STAGE_DIR}/usr/bin"
mkdir -p "${STAGE_DIR}/usr/share/applications"
mkdir -p "${STAGE_DIR}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${STAGE_DIR}/usr/share/doc/${NAME}"

install -m 0755 "${BIN_SRC}" "${STAGE_DIR}/usr/bin/${NAME}"
install -m 0644 "${ROOT_DIR}/packaging/appimage/${NAME}.desktop" "${STAGE_DIR}/usr/share/applications/${NAME}.desktop"
install -m 0644 "${ROOT_DIR}/packaging/appimage/${NAME}.svg" "${STAGE_DIR}/usr/share/icons/hicolor/scalable/apps/${NAME}.svg"
install -m 0644 "${ROOT_DIR}/README.md" "${STAGE_DIR}/usr/share/doc/${NAME}/README.md"
install -m 0644 "${ROOT_DIR}/LICENSE" "${STAGE_DIR}/usr/share/doc/${NAME}/LICENSE"

if [[ -f "${ROOT_DIR}/packaging/deb/copyright" ]]; then
  install -m 0644 "${ROOT_DIR}/packaging/deb/copyright" "${STAGE_DIR}/usr/share/doc/${NAME}/copyright"
fi

if command -v gzip >/dev/null 2>&1; then
  CHANGELOG="${WORK_DIR}/changelog"
  cat > "${CHANGELOG}" <<EOF
${NAME} (${VERSION}) unstable; urgency=medium

  * Local build.

 -- ${DEB_MAINTAINER:-Vesper <noreply@local>}  $(date -R)
EOF
  gzip -n -9 < "${CHANGELOG}" > "${STAGE_DIR}/usr/share/doc/${NAME}/changelog.gz"
fi

MAINTAINER="${DEB_MAINTAINER:-Vesper <noreply@local>}"

DEPENDS="${OVERRIDE_DEPENDS}"
if [[ -z "${DEPENDS}" && "${NO_SHLIBDEPS}" -eq 0 && -x "$(command -v dpkg-shlibdeps 2>/dev/null)" ]]; then
  # dpkg-shlibdeps insists on a debian/control in the current working directory.
  # Create a minimal one in a temp dir and run dpkg-shlibdeps from there.
  SHLIBDEPS_DIR="${WORK_DIR}/shlibdeps"
  mkdir -p "${SHLIBDEPS_DIR}/debian"
  cat > "${SHLIBDEPS_DIR}/debian/control" <<EOF
Source: ${NAME}
Section: utils
Priority: optional
Maintainer: ${MAINTAINER}
Standards-Version: 4.6.2

Package: ${NAME}
Architecture: any
Description: Vesper screensaver for Linux
 Vesper.
EOF
  : > "${SHLIBDEPS_DIR}/debian/substvars"

  # dpkg-shlibdeps output includes a line like:
  #   shlibs:Depends=libc6 (>= ...), ...
  SHLIBS_OUT=""
  if SHLIBS_OUT="$(cd "${SHLIBDEPS_DIR}" && dpkg-shlibdeps -O -Tdebian/substvars -e"${STAGE_DIR}/usr/bin/${NAME}" 2>&1)"; then
    DEPENDS="$(printf '%s\n' "${SHLIBS_OUT}" | sed -n 's/^shlibs:Depends=//p' | head -n 1)"
  else
    echo "Warning: dpkg-shlibdeps failed; falling back to a minimal Depends list." >&2
    echo "${SHLIBS_OUT}" >&2
  fi
fi
if [[ -z "${DEPENDS}" ]]; then
  # Fallback for non-Debian build hosts or missing dpkg-shlibdeps.
  DEPENDS="libc6, libgtk-4-1, libadwaita-1-0"
fi

INSTALLED_SIZE_KB="$(du -sk "${STAGE_DIR}/usr" | awk '{print $1}')"

cat > "${STAGE_DIR}/DEBIAN/control" <<EOF
Package: ${NAME}
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Depends: ${DEPENDS}
Installed-Size: ${INSTALLED_SIZE_KB}
Maintainer: ${MAINTAINER}
Description: Vesper screensaver for Linux
 A Linux desktop screensaver written in Rust with GTK4.
EOF
chmod 0644 "${STAGE_DIR}/DEBIAN/control"

if [[ -f "${ROOT_DIR}/packaging/deb/postinst" ]]; then
  install -m 0755 "${ROOT_DIR}/packaging/deb/postinst" "${STAGE_DIR}/DEBIAN/postinst"
fi
if [[ -f "${ROOT_DIR}/packaging/deb/postrm" ]]; then
  install -m 0755 "${ROOT_DIR}/packaging/deb/postrm" "${STAGE_DIR}/DEBIAN/postrm"
fi

OUT_DEB="${OUT_DIR}/${NAME}_${VERSION}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "${STAGE_DIR}" "${OUT_DEB}"

echo "Done: ${OUT_DEB}"
