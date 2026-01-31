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

NO_VENDOR=0
RPMBUILD_FLAG="-ba" # -ba (binary+source), -bs (source only)
TARBALL_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
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
