use std::fs;
use std::path::PathBuf;

const AUTOSTART_FILE_NAME: &str = "com.example.vesper.desktop";

pub fn migrate_legacy_autostart() {
    let Some(config_dir) = config_dir() else {
        return;
    };
    let autostart_dir = config_dir.join("autostart");
    if !autostart_dir.is_dir() {
        return;
    }

    let legacy_names = ["rs-screensaver.desktop", "com.example.RSScreensaver.desktop"];
    let mut migrated = false;

    for name in legacy_names {
        let legacy_path = autostart_dir.join(name);
        if legacy_path.exists() {
            eprintln!("Removing legacy autostart file: {:?}", legacy_path);
            if let Err(e) = fs::remove_file(&legacy_path) {
                eprintln!("Failed to remove legacy autostart file: {}", e);
            } else {
                migrated = true;
            }
        }
    }

    if migrated {
        eprintln!("Migrating autostart to Vesper...");
        if let Err(e) = set_autostart_enabled(true) {
            eprintln!("Failed to enable Vesper autostart during migration: {}", e);
        }
    }
}

pub fn is_autostart_enabled() -> bool {
    autostart_file_path()
        .map(|path| path.exists())
        .unwrap_or(false)
}

pub fn set_autostart_enabled(enable: bool) -> Result<(), String> {
    let path = autostart_file_path().ok_or_else(|| "Autostart path not available".to_string())?;
    if enable {
        let dir = path
            .parent()
            .ok_or_else(|| "Autostart dir not available".to_string())?;
        fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        let exec = current_exec().map_err(|err| err.to_string())?;
        let content = desktop_entry_content(&exec);
        fs::write(&path, content).map_err(|err| err.to_string())?;
    } else if path.exists() {
        fs::remove_file(&path).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn autostart_file_path() -> Option<PathBuf> {
    let mut dir = config_dir()?;
    dir.push("autostart");
    dir.push(AUTOSTART_FILE_NAME);
    Some(dir)
}

fn config_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        if !dir.trim().is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    let home = std::env::var("HOME").ok()?;
    if home.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(home).join(".config"))
}

struct ExecPaths {
    exec: String,
    try_exec: String,
}

fn current_exec() -> Result<ExecPaths, std::io::Error> {
    if let Ok(appimage) = std::env::var("APPIMAGE") {
        let appimage = appimage.trim().to_string();
        if !appimage.is_empty() {
            return Ok(ExecPaths {
                exec: format_exec(appimage.clone()),
                try_exec: appimage,
            });
        }
    }
    let path = std::env::current_exe()?;
    let text = path.to_string_lossy().to_string();
    Ok(ExecPaths {
        exec: format_exec(text.clone()),
        try_exec: text,
    })
}

fn format_exec(exec: String) -> String {
    if exec.contains(' ') {
        format!("\"{}\"", exec.replace('"', "\\\""))
    } else {
        exec
    }
}

fn desktop_entry_content(exec: &ExecPaths) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Vesper\nComment=Rust Screensaver Application\nExec={}\nTryExec={}\nIcon=vesper\nTerminal=false\nStartupWMClass=vesper\nCategories=Utility;\nStartupNotify=false\nX-GNOME-Autostart-enabled=true\nX-AppImage-Integrate=true\n",
        exec.exec, exec.try_exec
    )
}
