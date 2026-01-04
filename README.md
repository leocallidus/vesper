# RS Screensaver

[English](README.md) | [Русский](README_ru.md)

A Linux desktop screensaver written in Rust with GTK4.

## Features

- System tray app with quick actions, profiles, and status toggles
- Multiple modes: solid color, gradient, pattern, web page, video stream, image, video, slideshow (folder)
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
./target/release/rs-screensaver
```

## AppImage

Requires `appimagetool` on your system.

```bash
bash scripts/build_appimage.sh
```

The AppImage will be created at `dist/rs-screensaver-<arch>.AppImage`.

## CLI control (D-Bus)

```bash
./target/release/rs-screensaver status
./target/release/rs-screensaver start
./target/release/rs-screensaver stop
./target/release/rs-screensaver show-settings
./target/release/rs-screensaver show
./target/release/rs-screensaver enable
./target/release/rs-screensaver disable
./target/release/rs-screensaver inhibit
./target/release/rs-screensaver uninhibit
./target/release/rs-screensaver set-enabled true
./target/release/rs-screensaver set-inhibit false
./target/release/rs-screensaver switch-profile 2
./target/release/rs-screensaver quit
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

- Config path: `~/.config/rs-screensaver/config.json`
- Profiles and mode-specific settings are stored per profile

## Architecture

- `src/main.rs`: app entry point, D-Bus control, tray
- `src/config.rs`: settings model and persistence
- `src/idle.rs`: X11/Wayland/GNOME idle detection
- `src/ui/settings/*`: settings window
- `src/ui/saver.rs`: full-screen screensaver window

## Notes

- Wayland idle detection requires compositor support for `ext-idle-notify-v1`.
- GNOME uses a D-Bus fallback (Mutter IdleMonitor).
- If video playback fails, install the appropriate GStreamer plugins for your distro.

## Known Limitations

- Tray integration depends on your desktop environment.
- Some video formats require additional GStreamer plugins.
- Wayland idle detection is unavailable on compositors without `ext-idle-notify-v1`.
- Web mode requires WebKitGTK to be installed on the system.

## License

MIT License
