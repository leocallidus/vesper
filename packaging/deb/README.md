# Debian/Ubuntu packaging (local)

This folder contains helper files for building a local **binary** `.deb` package for Vesper.

## Build prerequisites (Ubuntu/Debian example)

```bash
sudo apt install dpkg-dev gzip
```

Build toolchain (same as building from source):

```bash
sudo apt install build-essential pkg-config
sudo apt install libgtk-4-dev libadwaita-1-dev
```

> Note: the app links to system GTK/libadwaita (and optionally WebKitGTK); the `.deb` does not bundle them.

## Build a .deb

From the repo root:

```bash
bash scripts/build_deb.sh
```

Output:
- `dist/deb/vesper_<version>_<arch>.deb`

### Installing locally

```bash
sudo apt install ./dist/deb/vesper_<version>_<arch>.deb
```

## Notes

- If `dpkg-shlibdeps` is available, `scripts/build_deb.sh` will auto-detect `Depends:` from the built binary.
- If it is not available (or you are building on a non-Debian host), the script falls back to:
  `libc6, libgtk-4-1, libadwaita-1-0`
- Override dependencies manually with:

```bash
bash scripts/build_deb.sh --depends "libc6 (>= 2.35), libgtk-4-1, libadwaita-1-0"
```
