use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;
use crate::i18n::{Language, profile_name};

pub const ACTION_START: &str = "start-screensaver";
pub const ACTION_STOP: &str = "stop-screensaver";
pub const DEFAULT_HOTKEY_START: &str = "<Ctrl><Alt>S";
pub const DEFAULT_HOTKEY_STOP: &str = "<Ctrl><Alt>X";
pub const DEFAULT_SLIDESHOW_INTERVAL_SECONDS: u64 = 5;
pub const DEFAULT_MOUSE_WAKE_DELAY_MS: u64 = 1500;
pub const DEFAULT_VIDEO_VOLUME: u8 = 100;
pub const DEFAULT_TOTAL_RUNTIME_SECONDS: u64 = 0;
pub const DEFAULT_POWER_INTEGRATION_ENABLED: bool = false;
pub const DEFAULT_MPRIS_PAUSE_ENABLED: bool = true;
pub const DEFAULT_START_MINIMIZED: bool = false;
pub const DEFAULT_CLOCK_FORMAT: &str = "%H:%M:%S";
pub const DEFAULT_CLOCK_POSITION: ClockPosition = ClockPosition::TopRight;
pub const DEFAULT_CLOCK_MOVE_ENABLED: bool = false;
pub const DEFAULT_CLOCK_MOVE_INTERVAL_SECONDS: u64 = 10;
pub const DEFAULT_CLOCK_SIZE: u32 = 48;
pub const CLOCK_MOVE_POSITIONS: [ClockPosition; 9] = [
    ClockPosition::TopLeft,
    ClockPosition::TopCenter,
    ClockPosition::TopRight,
    ClockPosition::CenterLeft,
    ClockPosition::Center,
    ClockPosition::CenterRight,
    ClockPosition::BottomLeft,
    ClockPosition::BottomCenter,
    ClockPosition::BottomRight,
];
pub const DEFAULT_PROFILE_COUNT: usize = 5;
pub const MAX_PROFILES: usize = 255;
pub const MAX_ACTIVATION_LOG_ENTRIES: usize = 200;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AnimatedPattern {
    Matrix,
    Stars,
    Geometry,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ClockPosition {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScreensaverMode {
    Color(String), // Hex color
    Gradient { start: String, end: String }, // Hex colors
    Pattern(AnimatedPattern),
    Web(String), // URL
    Stream(String), // URL
    Image(String), // File path
    Video(String), // File path
    Slideshow(String), // Folder path
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub width: i32,
    pub height: i32,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PanelCommand {
    pub name: String,
    pub show_command: String,
    pub hide_command: String,
    pub enabled: bool,
}

impl PanelCommand {
    pub fn kde() -> Self {
        Self {
            name: "KDE Plasma".to_string(),
            show_command: "qdbus org.kde.plasmashell /PlasmaShell org.kde.PlasmaShell.evaluateScript \"panels().forEach(p => { p.hiding = 'none' })\"".to_string(),
            hide_command: "qdbus org.kde.plasmashell /PlasmaShell org.kde.PlasmaShell.evaluateScript \"panels().forEach(p => { p.hiding = 'autohide' })\"".to_string(),
            enabled: false,
        }
    }
    
    pub fn xfce() -> Self {
        Self {
            name: "XFCE".to_string(),
            show_command: "xfconf-query -c xfce4-panel -p /panels/panel-0/autohide-behavior -s 0".to_string(),
            hide_command: "xfconf-query -c xfce4-panel -p /panels/panel-0/autohide-behavior -s 2".to_string(),
            enabled: false,
        }
    }
    
    pub fn gnome() -> Self {
        Self {
            name: "GNOME".to_string(),
            show_command: "gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval \"Main.panel.show()\"".to_string(),
            hide_command: "gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval \"Main.panel.hide()\"".to_string(),
            enabled: false,
        }
    }
    
    pub fn presets() -> Vec<Self> {
        vec![Self::kde(), Self::xfce(), Self::gnome()]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsProfile {
    pub name: String,
    pub inactivity_seconds: u64,
    #[serde(default = "default_mouse_wake_delay_ms")]
    pub mouse_wake_delay_ms: u64,
    #[serde(default = "default_video_volume")]
    pub video_volume: u8,
    pub mode: ScreensaverMode,
    pub mute_video: bool,
    #[serde(default)]
    pub show_clock: bool,
    #[serde(default = "default_clock_format")]
    pub clock_format: String,
    #[serde(default = "default_clock_position")]
    pub clock_position: ClockPosition,
    #[serde(default = "default_clock_move_enabled")]
    pub clock_move_enabled: bool,
    #[serde(default = "default_clock_move_interval_seconds")]
    pub clock_move_interval_seconds: u64,
    #[serde(default = "default_clock_size")]
    pub clock_size: u32,
    #[serde(default)]
    pub random_media: bool,
    #[serde(default)]
    pub media_list: Vec<String>,
    #[serde(default)]
    pub inhibit_sleep: bool,
    #[serde(default = "default_power_integration_enabled")]
    pub power_integration_enabled: bool,
    #[serde(default = "default_lock_screen_enabled")]
    pub lock_screen_enabled: bool,
    #[serde(default = "default_mpris_pause_enabled")]
    pub mpris_pause_enabled: bool,
    #[serde(default)]
    pub app_inhibit_list: Vec<String>,
    #[serde(default)]
    pub panel_commands: Vec<PanelCommand>,
    #[serde(default = "default_fade_enabled")]
    pub fade_enabled: bool,
    #[serde(default = "default_slideshow_interval_seconds")]
    pub slideshow_interval_seconds: u64,
}

impl SettingsProfile {
    pub fn new(name: String) -> Self {
        Self {
            name,
            inactivity_seconds: 300,
            mouse_wake_delay_ms: default_mouse_wake_delay_ms(),
            video_volume: default_video_volume(),
            mode: ScreensaverMode::Color("#000000".to_string()),
            mute_video: true,
            show_clock: false,
            clock_format: default_clock_format(),
            clock_position: default_clock_position(),
            clock_move_enabled: default_clock_move_enabled(),
            clock_move_interval_seconds: default_clock_move_interval_seconds(),
            clock_size: default_clock_size(),
            random_media: false,
            media_list: Vec::new(),
            inhibit_sleep: false,
            power_integration_enabled: default_power_integration_enabled(),
            lock_screen_enabled: default_lock_screen_enabled(),
            mpris_pause_enabled: default_mpris_pause_enabled(),
            app_inhibit_list: Vec::new(),
            panel_commands: Vec::new(),
            fade_enabled: default_fade_enabled(),
            slideshow_interval_seconds: default_slideshow_interval_seconds(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationLogEntry {
    pub timestamp: u64,
    pub profile_name: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub profiles: Vec<SettingsProfile>,
    #[serde(default)]
    pub active_profile: u8,
    #[serde(default = "default_language")]
    pub language: Language,
    #[serde(default = "default_start_minimized")]
    pub start_minimized: bool,
    #[serde(default)]
    pub settings_window: Option<WindowGeometry>,
    #[serde(default = "default_hotkey_start")]
    pub hotkey_start: String,
    #[serde(default = "default_hotkey_stop")]
    pub hotkey_stop: String,
    #[serde(default = "default_total_runtime_seconds")]
    pub total_runtime_seconds: u64,
    #[serde(default)]
    pub activation_log: Vec<ActivationLogEntry>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profiles: default_profiles(),
            active_profile: 0,
            language: default_language(),
            start_minimized: default_start_minimized(),
            settings_window: None,
            hotkey_start: default_hotkey_start(),
            hotkey_stop: default_hotkey_stop(),
            total_runtime_seconds: default_total_runtime_seconds(),
            activation_log: Vec::new(),
        }
    }
}

fn default_fade_enabled() -> bool {
    true
}

fn default_hotkey_start() -> String {
    DEFAULT_HOTKEY_START.to_string()
}

fn default_hotkey_stop() -> String {
    DEFAULT_HOTKEY_STOP.to_string()
}

fn default_language() -> Language {
    Language::Auto
}

fn default_start_minimized() -> bool {
    DEFAULT_START_MINIMIZED
}

fn default_total_runtime_seconds() -> u64 {
    DEFAULT_TOTAL_RUNTIME_SECONDS
}

fn default_mouse_wake_delay_ms() -> u64 {
    DEFAULT_MOUSE_WAKE_DELAY_MS
}

fn default_video_volume() -> u8 {
    DEFAULT_VIDEO_VOLUME
}

fn default_power_integration_enabled() -> bool {
    DEFAULT_POWER_INTEGRATION_ENABLED
}

fn default_lock_screen_enabled() -> bool {
    crate::desktop::is_kde_or_gnome()
}

fn default_mpris_pause_enabled() -> bool {
    DEFAULT_MPRIS_PAUSE_ENABLED
}

fn default_clock_format() -> String {
    DEFAULT_CLOCK_FORMAT.to_string()
}

fn default_clock_position() -> ClockPosition {
    DEFAULT_CLOCK_POSITION
}

fn default_clock_move_enabled() -> bool {
    DEFAULT_CLOCK_MOVE_ENABLED
}

fn default_clock_move_interval_seconds() -> u64 {
    DEFAULT_CLOCK_MOVE_INTERVAL_SECONDS
}

fn default_clock_size() -> u32 {
    DEFAULT_CLOCK_SIZE
}

fn default_slideshow_interval_seconds() -> u64 {
    DEFAULT_SLIDESHOW_INTERVAL_SECONDS
}

impl Config {
    pub fn load() -> Self {
        let config_path = Self::config_path();
        
        if let Ok(content) = fs::read_to_string(&config_path) {
            if let Ok(mut config) = serde_json::from_str::<Config>(&content) {
                config.normalize();
                return config;
            }
            if let Ok(legacy) = serde_json::from_str::<LegacyConfig>(&content) {
                let mut config = Config::from_legacy(legacy);
                config.normalize();
                let _ = config.save();
                return config;
            }
        }
        
        let default_config = Self::default();
        let _ = default_config.save();
        default_config
    }
    
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::config_path();
        
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let content = serde_json::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }

    pub fn active_profile_index(&self) -> usize {
        let idx = self.active_profile as usize;
        if idx < self.profiles.len() {
            idx
        } else {
            0
        }
    }

    pub fn active_profile(&self) -> &SettingsProfile {
        let idx = self.active_profile_index();
        &self.profiles[idx]
    }

    pub fn active_profile_mut(&mut self) -> &mut SettingsProfile {
        let idx = self.active_profile_index();
        &mut self.profiles[idx]
    }

    pub fn set_active_profile(&mut self, index: usize) {
        let clamped = index.min(self.profiles.len().saturating_sub(1));
        self.active_profile = clamped as u8;
    }

    pub fn push_activation_log(&mut self, entry: ActivationLogEntry) {
        self.activation_log.push(entry);
        if self.activation_log.len() > MAX_ACTIVATION_LOG_ENTRIES {
            let overflow = self.activation_log.len() - MAX_ACTIVATION_LOG_ENTRIES;
            self.activation_log.drain(0..overflow);
        }
    }

    pub fn clear_activation_log(&mut self) {
        self.activation_log.clear();
    }

    pub fn normalize(&mut self) {
        if self.profiles.is_empty() {
            self.profiles = default_profiles();
        }
        if self.profiles.len() > MAX_PROFILES {
            self.profiles.truncate(MAX_PROFILES);
        }
        if self.active_profile as usize >= self.profiles.len() {
            self.active_profile = 0;
        }
        if self.activation_log.len() > MAX_ACTIVATION_LOG_ENTRIES {
            let overflow = self.activation_log.len() - MAX_ACTIVATION_LOG_ENTRIES;
            self.activation_log.drain(0..overflow);
        }
    }

    fn from_legacy(legacy: LegacyConfig) -> Self {
        let mut profiles = default_profiles();
        if profiles.is_empty() {
            profiles.push(SettingsProfile::new(default_profile_name(1)));
        }
        profiles[0] = SettingsProfile {
            name: default_profile_name(1),
            inactivity_seconds: legacy.inactivity_seconds,
            mouse_wake_delay_ms: default_mouse_wake_delay_ms(),
            video_volume: default_video_volume(),
            mode: legacy.mode,
            mute_video: legacy.mute_video,
            show_clock: false,
            clock_format: default_clock_format(),
            clock_position: default_clock_position(),
            clock_move_enabled: default_clock_move_enabled(),
            clock_move_interval_seconds: default_clock_move_interval_seconds(),
            clock_size: default_clock_size(),
            random_media: false,
            media_list: Vec::new(),
            inhibit_sleep: legacy.inhibit_sleep,
            power_integration_enabled: default_power_integration_enabled(),
            lock_screen_enabled: default_lock_screen_enabled(),
            mpris_pause_enabled: default_mpris_pause_enabled(),
            app_inhibit_list: Vec::new(),
            panel_commands: legacy.panel_commands,
            fade_enabled: legacy.fade_enabled,
            slideshow_interval_seconds: default_slideshow_interval_seconds(),
        };
        Self {
            profiles,
            active_profile: 0,
            language: default_language(),
            start_minimized: default_start_minimized(),
            settings_window: legacy.settings_window,
            hotkey_start: legacy.hotkey_start,
            hotkey_stop: legacy.hotkey_stop,
            total_runtime_seconds: default_total_runtime_seconds(),
            activation_log: Vec::new(),
        }
    }
    
    fn config_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("", "", "rs-screensaver") {
            proj_dirs.config_dir().join("config.json")
        } else {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                .join(".config")
                .join("rs-screensaver")
                .join("config.json")
        }
    }
}

fn default_profile_name(index: usize) -> String {
    profile_name(Language::Auto, index)
}

fn default_profiles() -> Vec<SettingsProfile> {
    (1..=DEFAULT_PROFILE_COUNT)
        .map(|i| SettingsProfile::new(default_profile_name(i)))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyConfig {
    pub inactivity_seconds: u64,
    pub mode: ScreensaverMode,
    pub mute_video: bool,
    #[serde(default)]
    pub inhibit_sleep: bool,
    #[serde(default)]
    pub panel_commands: Vec<PanelCommand>,
    #[serde(default)]
    pub settings_window: Option<WindowGeometry>,
    #[serde(default = "default_fade_enabled")]
    pub fade_enabled: bool,
    #[serde(default = "default_hotkey_start")]
    pub hotkey_start: String,
    #[serde(default = "default_hotkey_stop")]
    pub hotkey_stop: String,
}
