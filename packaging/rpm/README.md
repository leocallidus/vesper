# RPM packaging (local)

This folder contains a simple `rpmbuild` spec for packaging Vesper.

## Build prerequisites (Fedora example)

```bash
sudo dnf install rpm-build rust cargo gcc pkgconf-pkg-config \
  gtk4-devel libadwaita-devel webkitgtk6.0-devel
```

## Build an RPM (recommended: vendored crates)

From the repo root:

```bash
bash scripts/build_rpm.sh
```

Outputs:
- `dist/rpm/RPMS/*/*.rpm`
- `dist/rpm/SRPMS/*.src.rpm`

## Notes

- The build is offline/reproducible when the source tarball includes `vendor/` and `.cargo/config.toml`.
- `scripts/build_rpm.sh` generates that source tarball via `cargo vendor`.
