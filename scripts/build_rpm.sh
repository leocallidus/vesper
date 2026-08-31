#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAME="vesper"

usage() {
  cat <<'EOF'
Usage: scripts/build_rpm.sh [--no-vendor] [--tarball-only|--srpm-only]

Creates a source tarball and builds RPMs via rpmbuild.

Options:
  --no-vendor   Do not run cargo vendor; the RPM build may require network access.
  --tarball-only  Only write dist/rpm/vesper-<version>.tar.gz and exit.
  --srpm-only     Build only the source RPM (.src.rpm).
EOF
}

NO_BUILD=0
NO_VENDOR=0
RPMBUILD_FLAG="-ba" # -ba (binary+source), -bs (source only)
TARBALL_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --no-build)
      NO_BUILD=1
      shift
      ;;
    --no-vendor)
      NO_VENDOR=1
      shift
      ;;
    --tarball-only)
      TARBALL_ONLY=1
      shift
      ;;
    --srpm-only)
      RPMBUILD_FLAG="-bs"
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

VERSION="$(awk -F'"' '/^version[[:space:]]*=[[:space:]]*"/ {print $2; exit}' "${ROOT_DIR}/Cargo.toml")"
if [[ -z "${VERSION}" ]]; then
  echo "Failed to detect version from Cargo.toml" >&2
  exit 1
fi

OUT_DIR="${ROOT_DIR}/dist/rpm"
mkdir -p "${OUT_DIR}"

BIN_SRC="${ROOT_DIR}/target/release/${NAME}"

if [[ "${NO_BUILD}" -eq 1 ]]; then
  if [[ ! -f "${BIN_SRC}" ]]; then
    echo "Binary not found: ${BIN_SRC}" >&2
    exit 1
  fi
  if ! command -v rpmbuild >/dev/null 2>&1; then
    echo "rpmbuild not found. Install rpm-build." >&2
    exit 1
  fi

  TOPDIR="${OUT_DIR}/_rpmbuild"
  rm -rf "${TOPDIR}"
  mkdir -p "${TOPDIR}"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}

  cp -f "${BIN_SRC}" "${TOPDIR}/SOURCES/${NAME}"
  cp -f "${ROOT_DIR}/packaging/appimage/${NAME}.desktop" "${TOPDIR}/SOURCES/${NAME}.desktop"
  cp -f "${ROOT_DIR}/packaging/appimage/${NAME}.svg" "${TOPDIR}/SOURCES/${NAME}.svg"
  cp -f "${ROOT_DIR}/LICENSE" "${TOPDIR}/SOURCES/LICENSE"
  cp -f "${ROOT_DIR}/README.md" "${TOPDIR}/SOURCES/README.md"

  cat > "${TOPDIR}/SPECS/${NAME}.spec" <<EOF
Name:           ${NAME}
Version:        ${VERSION}
Release:        1
Summary:        Linux desktop screensaver written in Rust with GTK4
License:        MIT
%global debug_package %{nil}
%description
Linux desktop screensaver written in Rust with GTK4
%install
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_datadir}/applications
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/scalable/apps
mkdir -p %{buildroot}%{_datadir}/doc/${NAME}
mkdir -p %{buildroot}%{_datadir}/licenses/${NAME}
install -Dpm0755 %{_sourcedir}/${NAME} %{buildroot}%{_bindir}/${NAME}
install -Dpm0644 %{_sourcedir}/${NAME}.desktop %{buildroot}%{_datadir}/applications/${NAME}.desktop
install -Dpm0644 %{_sourcedir}/${NAME}.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/${NAME}.svg
install -Dpm0644 %{_sourcedir}/LICENSE %{buildroot}%{_datadir}/licenses/${NAME}/LICENSE
install -Dpm0644 %{_sourcedir}/README.md %{buildroot}%{_datadir}/doc/${NAME}/README.md
%files
%{_bindir}/${NAME}
%{_datadir}/applications/${NAME}.desktop
%{_datadir}/icons/hicolor/scalable/apps/${NAME}.svg
%{_datadir}/licenses/${NAME}/LICENSE
%{_datadir}/doc/${NAME}/README.md
EOF

  rpmbuild -bb "${TOPDIR}/SPECS/${NAME}.spec" --define "_topdir ${TOPDIR}" --define "_sourcedir ${TOPDIR}/SOURCES"
  find "${TOPDIR}/RPMS" -type f -name "*.rpm" -exec cp -f {} "${OUT_DIR}/" \;
  echo "Built RPMs in ${OUT_DIR}"
  exit 0
fi

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

SRC_DIR="${WORK_DIR}/${NAME}-${VERSION}"
mkdir -p "${SRC_DIR}"

# Copy the project sources into a clean staging dir (exclude build artifacts and VCS).
echo "Staging sources..."
tar -C "${ROOT_DIR}" \
  --exclude-vcs \
  --exclude='./target' \
  --exclude='./dist' \
  --exclude='./.idea' \
  --exclude='./__pycache__' \
  -cf - . | tar -C "${SRC_DIR}" -xf -

if [[ "${NO_VENDOR}" -eq 0 ]]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found. Install Rust (cargo) or run with --no-vendor." >&2
    exit 1
  fi
  echo "Vendoring crates (cargo vendor)..."
  (
    cd "${SRC_DIR}"
    mkdir -p .cargo
    # Vendor crates for an offline rpmbuild. This may download crates if they're not cached.
    if ! cargo vendor vendor --locked > .cargo/config.toml; then
      cat >&2 <<'EOF'
cargo vendor failed.

If you're in a restricted/no-network environment, run this script somewhere with network
access (or after running `cargo fetch --locked`), then re-run to produce the vendored
source tarball used for offline rpmbuilds.
EOF
      exit 1
    fi
  )
fi

TARBALL="${OUT_DIR}/${NAME}-${VERSION}.tar.gz"
tar -C "${WORK_DIR}" -czf "${TARBALL}" "${NAME}-${VERSION}"

if [[ "${TARBALL_ONLY}" -eq 1 ]]; then
  echo "Wrote tarball: ${TARBALL}"
  exit 0
fi

if ! command -v rpmbuild >/dev/null 2>&1; then
  echo "rpmbuild not found. Install rpm-build (Fedora: sudo dnf install rpm-build)." >&2
  exit 1
fi

TOPDIR="${OUT_DIR}/_rpmbuild"
rm -rf "${TOPDIR}"
mkdir -p "${TOPDIR}"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}

cp -f "${TARBALL}" "${TOPDIR}/SOURCES/"
sed -E "s/^(Version:[[:space:]]+).*/\\1${VERSION}/" \
  "${ROOT_DIR}/packaging/rpm/${NAME}.spec" > "${TOPDIR}/SPECS/${NAME}.spec"

rpmbuild "${RPMBUILD_FLAG}" "${TOPDIR}/SPECS/${NAME}.spec" --define "_topdir ${TOPDIR}"

echo "Built RPMs:"
echo "  ${TOPDIR}/RPMS/*/*.rpm"
echo "  ${TOPDIR}/SRPMS/*.src.rpm"
