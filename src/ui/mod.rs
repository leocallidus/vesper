pub mod settings;
pub mod saver;

pub use settings::SettingsWindow;
pub use saver::ScreensaverWindow;

use std::path::PathBuf;

use gdk4::Display;
use gtk4::IconTheme;

const ICON_CANDIDATES: [&str; 3] = [
    "rs-screensaver",
    "preferences-desktop-screensaver",
    "kscreensaver",
];

pub fn init_app_icon_theme() {
    let Ok(appdir) = std::env::var("APPDIR") else {
        return;
    };
    let Some(display) = Display::default() else {
        return;
    };
    let icon_dir = PathBuf::from(appdir).join("usr/share/icons");
    if icon_dir.is_dir() {
        let icon_theme = IconTheme::for_display(&display);
        icon_theme.add_search_path(&icon_dir);
    }
}

pub fn app_icon_name() -> &'static str {
    if let Some(display) = Display::default() {
        let icon_theme = IconTheme::for_display(&display);
        for name in ICON_CANDIDATES {
            if icon_theme.has_icon(name) {
                return name;
            }
        }
    }
    ICON_CANDIDATES[0]
}
