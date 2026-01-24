pub mod gl_patterns;
pub mod python_plugins;
pub mod shadertoy;
pub mod saver;
pub mod settings;

pub use saver::ScreensaverWindow;
pub use settings::SettingsWindow;

use std::path::PathBuf;
use std::fs;

use gdk4::Display;
use gtk4::IconTheme;

const ICON_CANDIDATES: [&str; 3] = [
    "vesper",
    "preferences-desktop-screensaver",
    "kscreensaver",
];

pub fn get_local_icon_theme_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let target_icons = cwd.join("target/icons");
    if target_icons.exists() {
        Some(target_icons)
    } else {
        None
    }
}

pub fn init_app_icon_theme() {
    let Ok(cwd) = std::env::current_dir() else { return };
    
    // Setup local icon theme for development/running from source
    let source_icon = cwd.join("packaging/appimage/vesper.svg");
    if source_icon.exists() {
        let target_base = cwd.join("target/icons");
        let hicolor_dir = target_base.join("hicolor");
        
        // 1. Create directory structure
        let scaled_dir = hicolor_dir.join("128x128/apps");
        let scalable_dir = hicolor_dir.join("scalable/apps");
        
        if fs::create_dir_all(&scaled_dir).is_ok() && fs::create_dir_all(&scalable_dir).is_ok() {
            // 2. Copy SVG
            let dest_svg = scalable_dir.join("vesper.svg");
            let _ = fs::copy(&source_icon, &dest_svg);

            // 3. Generate PNG using rsvg-convert if available
            let dest_png = scaled_dir.join("vesper.png");
            if !dest_png.exists() {
                let _ = std::process::Command::new("rsvg-convert")
                    .arg("-w").arg("128")
                    .arg("-h").arg("128")
                    .arg("-o").arg(&dest_png)
                    .arg(&source_icon)
                    .output();
            }

            // 4. Create index.theme
            let index_theme_content = r#"[Icon Theme]
Name=Hicolor
Comment=Default Theme
Directories=128x128/apps,scalable/apps

[128x128/apps]
Size=128
Type=Fixed
Context=Applications

[scalable/apps]
Size=128
Type=Scalable
MinSize=1
MaxSize=256
Context=Applications
"#;
            let _ = fs::write(hicolor_dir.join("index.theme"), index_theme_content);
            
            // Register with GTK
            if let Some(display) = Display::default() {
                let icon_theme = IconTheme::for_display(&display);
                icon_theme.add_search_path(&target_base);
            }
        }
    }

    // AppImage support
    if let Ok(appdir) = std::env::var("APPDIR") {
        let Some(display) = Display::default() else {
            return;
        };
        let icon_dir = PathBuf::from(appdir).join("usr/share/icons");
        if icon_dir.is_dir() {
            let icon_theme = IconTheme::for_display(&display);
            icon_theme.add_search_path(&icon_dir);
        }
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
