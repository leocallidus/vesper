# Vesper

[English](README.md) | [Русский](README_ru.md)

A Linux desktop screensaver written in Rust with GTK4.

## Features

- System tray app with quick actions, profiles, and status toggles
- Multiple modes: solid color, gradient, pattern, web page, video stream, image, video, slideshow (folder), GLSL shader (Shadertoy), Python script
- User GLSL shaders (Shadertoy-style): single-file or multi-pass folders (Common/Image/BufferA-D), optional `Sound.glsl`
- Shaderpacks library: import folders/`.zip`, previews, and selection inside GLSL mode
- Python script mode (`.py`) for procedural/community patterns without recompiling
- Random media list for image/video modes
- Clock overlay with format, position, size, and movement options
- Profiles with fast switching (including tray menu)
- Hotkeys for start/stop
- Autostart toggle
- Power integration (sleep inhibit, lock screen on KDE/GNOME)
- Import/export settings (JSON)
- Activation history log
- Panel commands (hide/show panels on activation)
- X11 + Wayland idle detection (ext-idle-notify-v1 with GNOME D-Bus fallback)
- Russian and English UI (auto-detect)

## Requirements

- Rust toolchain (stable)
- GTK4 and libadwaita development packages
- Optional: WebKitGTK 6 development package for Web mode (package name varies by distro)
- Runtime: GStreamer plugins for video playback if your system is missing codecs
- Optional: OpenGL 3.3 + `libGL.so.1` (GLSL shaders / Python script mode)
- Optional: `python3` (or `python`) in `PATH` (Python script mode)
- Optional: `pw-cat` (PipeWire) or `paplay` (PulseAudio) for `Sound.glsl`
- Optional: `bsdtar` for importing shaderpack `.zip` archives

### Linux packages (examples)

**Ubuntu/Debian:**
```bash
sudo apt install libgtk-4-dev libadwaita-1-dev
```

**Fedora:**
```bash
sudo dnf install gtk4-devel libadwaita-devel
```

**Arch Linux:**
```bash
sudo pacman -S gtk4 libadwaita
```

## Build

```bash
cargo build --release
```

## Run

```bash
# From source
cargo run --release

# Or direct binary
./target/release/vesper
```

## GLSL shaders (Shadertoy-style)

- Enable: **Settings → Content → “GLSL shader”**
- Select a single shader file (`.glsl` / `.frag` / `.fs`) or a folder containing `Image.glsl` (auto-detects `Common.glsl`, `BufferA..D.glsl`, `Sound.glsl` in the same folder)
- Shaderpacks: import in **Settings → Shaderpacks**, then select **Source: Shaderpack** in GLSL mode
- Docs: `docs/SHADERTOY_SHADERS.md` (format/uniforms) and `docs/SHADERPACKS.md` (shaderpacks)

## Python scripts

- Enable: **Settings → Content → “Python script”**
- Select a `.py` file (see `example.py`)
- Docs: `docs/PYTHON_PLUGINS.md` (API)

## AppImage

Requires `appimagetool` on your system.

```bash
bash scripts/build_appimage.sh
```

The AppImage will be created at `dist/vesper-<arch>.AppImage`.

## Debian/Ubuntu (.deb)

Requires `dpkg-deb` (package: `dpkg-dev`) on your system.

```bash
bash scripts/build_deb.sh
```

The `.deb` will be created at `dist/deb/vesper_<version>_<arch>.deb`.

## CLI control (D-Bus)

```bash
./target/release/vesper status
./target/release/vesper start
./target/release/vesper stop
./target/release/vesper show-settings
./target/release/vesper show
./target/release/vesper enable
./target/release/vesper disable
./target/release/vesper inhibit
./target/release/vesper uninhibit
./target/release/vesper set-enabled true
./target/release/vesper set-inhibit false
./target/release/vesper switch-profile 2
./target/release/vesper quit
```

## System tray menu

- Enabled / Block sleep toggles
- Profiles submenu
- Settings, Start, Exit

## Screenshots

![Main window](screenshots/main.png)
![Settings](screenshots/settings.png)
![Screensaver](screenshots/screensaver.png)

## Configuration

- Config path: `~/.config/vesper/config.json`
- Profiles and mode-specific settings are stored per profile
- Shaderpacks (installed): `~/.local/share/vesper/shaderpacks` (or `XDG_DATA_HOME`)
- Python host helper script: `~/.cache/vesper/python_plugin_host.py` (or `XDG_CACHE_HOME`)

## Architecture

- `src/main.rs`: app entry point, D-Bus control, tray
- `src/config.rs`: settings model and persistence
- `src/idle.rs`: X11/Wayland/GNOME idle detection
- `src/ui/shadertoy.rs`: user GLSL (Shadertoy-style) runtime (multi-pass + optional sound)
- `src/shaderpacks.rs`: shaderpack loader/import and storage
- `src/ui/python_plugins.rs`: Python script mode (process + RGBA frames)
- `src/ui/settings/*`: settings window
- `src/ui/saver.rs`: full-screen screensaver window

## Notes

- Wayland idle detection requires compositor support for `ext-idle-notify-v1`.
- GNOME uses a D-Bus fallback (Mutter IdleMonitor).
- If video playback fails, install the appropriate GStreamer plugins for your distro.
- User GLSL shaders and Python scripts are not sandboxed (they can crash/hang GPU/process). Configure the “Force close” hotkey for emergency exit.

## Known Limitations

- Tray integration depends on your desktop environment.
- Some video formats require additional GStreamer plugins.
- Wayland idle detection is unavailable on compositors without `ext-idle-notify-v1`.
- Web mode requires WebKitGTK to be installed on the system.
- Shader texture channels are limited (see `docs/SHADERTOY_SHADERS.md`).

## License

MIT License
