use crate::i18n::{profile_name, Language};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const ACTION_START: &str = "start-screensaver";
pub const ACTION_STOP: &str = "stop-screensaver";
pub const ACTION_PANIC: &str = "panic-stop-screensaver";
pub const DEFAULT_HOTKEY_START: &str = "<Ctrl><Alt>S";
pub const DEFAULT_HOTKEY_STOP: &str = "<Ctrl><Alt>X";
pub const DEFAULT_HOTKEY_PANIC: &str = "<Ctrl><Alt><Shift>Escape";
pub const DEFAULT_SLIDESHOW_INTERVAL_SECONDS: u64 = 5;
pub const DEFAULT_MOUSE_WAKE_DELAY_MS: u64 = 1500;
pub const DEFAULT_VIDEO_VOLUME: u8 = 100;
pub const DEFAULT_TOTAL_RUNTIME_SECONDS: u64 = 0;
pub const DEFAULT_POWER_INTEGRATION_ENABLED: bool = false;
pub const DEFAULT_MPRIS_PAUSE_ENABLED: bool = true;
pub const DEFAULT_START_MINIMIZED: bool = false;
pub const DEFAULT_CLOCK_FORMAT: &str = "%H:%M:%S";
pub const DEFAULT_CLOCK_TIME_FORMAT: &str = "%H:%M";
pub const DEFAULT_CLOCK_DATE_FORMAT: &str = "%d.%m.%Y";
pub const DEFAULT_CLOCK_POSITION: ClockPosition = ClockPosition::TopRight;
pub const DEFAULT_CLOCK_MOVE_ENABLED: bool = false;
pub const DEFAULT_CLOCK_MOVE_INTERVAL_SECONDS: u64 = 10;
pub const DEFAULT_CLOCK_SIZE: u32 = 48;
pub const DEFAULT_NOW_PLAYING_POSITION: ClockPosition = ClockPosition::BottomLeft;
pub const DEFAULT_NOW_PLAYING_MOVE_ENABLED: bool = false;
pub const DEFAULT_NOW_PLAYING_MOVE_INTERVAL_SECONDS: u64 = 10;
pub const DEFAULT_RSS_TICKER_ENABLED: bool = false;
pub const DEFAULT_RSS_TICKER_SPEED_PX_S: u32 = 80;
pub const DEFAULT_RSS_REFRESH_INTERVAL_MINUTES: u64 = 10;
pub const DEFAULT_SYSTEM_STATS_ENABLED: bool = false;
pub const DEFAULT_SYSTEM_STATS_POSITION: ClockPosition = ClockPosition::TopLeft;
pub const DEFAULT_SYSTEM_STATS_MOVE_ENABLED: bool = false;
pub const DEFAULT_SYSTEM_STATS_MOVE_INTERVAL_SECONDS: u64 = 10;
pub const DEFAULT_WEB_INTERACTION_ENABLED: bool = false;
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
    Flowfield,
    Aurora,
    Plasma,
    Bokeh,
    Constellation,
    Lissajous,
    Waves,
    Voronoi,
    Scanline,
    Fireflies,
    SmokeInk,
    WaterRipples,
    MatrixRain3D,
    Lcars,
    Terminal,
    Fractals,
    ReactionDiffusion,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PatternSpeed {
    Slow,
    Normal,
    Fast,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PatternDensity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PatternTheme {
    Default,
    Mono,
    Warm,
    Cool,
    Random,
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
    Color(String),                           // Hex color
    Gradient { start: String, end: String }, // Hex colors
    Pattern(AnimatedPattern),
    Web(String),       // URL
    Stream(String),    // URL
    Image(String),     // File path
    Video(String),     // File path
    Slideshow(String), // Folder path
    PythonScript(String), // File path (.py)
    Shadertoy(String), // File path (.glsl/.frag) in Shadertoy format
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
            show_command: "xfconf-query -c xfce4-panel -p /panels/panel-0/autohide-behavior -s 0"
                .to_string(),
            hide_command: "xfconf-query -c xfce4-panel -p /panels/panel-0/autohide-behavior -s 2"
                .to_string(),
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
    #[serde(default)]
    pub show_now_playing: bool,
    #[serde(default = "default_now_playing_position")]
    pub now_playing_position: ClockPosition,
    #[serde(default = "default_now_playing_move_enabled")]
    pub now_playing_move_enabled: bool,
    #[serde(default = "default_now_playing_move_interval_seconds")]
    pub now_playing_move_interval_seconds: u64,
    #[serde(default = "default_rss_ticker_enabled")]
    pub show_rss_ticker: bool,
    #[serde(default)]
    pub rss_feeds: Vec<String>,
    #[serde(default = "default_rss_ticker_speed_px_s")]
    pub rss_ticker_speed_px_s: u32,
    #[serde(default = "default_rss_refresh_interval_minutes")]
    pub rss_refresh_interval_minutes: u64,
    #[serde(default = "default_system_stats_enabled")]
    pub show_system_stats: bool,
    #[serde(default = "default_system_stats_position")]
    pub system_stats_position: ClockPosition,
    #[serde(default = "default_system_stats_move_enabled")]
    pub system_stats_move_enabled: bool,
    #[serde(default = "default_system_stats_move_interval_seconds")]
    pub system_stats_move_interval_seconds: u64,
    #[serde(default = "default_clock_format")]
    pub clock_format: String,
    #[serde(default)]
    pub clock_two_lines: bool,
    #[serde(default = "default_web_interaction_enabled")]
    pub web_interaction_enabled: bool,
    #[serde(default = "default_clock_time_format")]
    pub clock_time_format: String,
    #[serde(default = "default_clock_date_format")]
    pub clock_date_format: String,
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
    #[serde(default = "default_pattern_speed")]
    pub pattern_speed: PatternSpeed,
    #[serde(default = "default_pattern_density")]
    pub pattern_density: PatternDensity,
    #[serde(default = "default_pattern_theme")]
    pub pattern_theme: PatternTheme,
    #[serde(default)]
    pub water_ripples_background_image: String,
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
            show_now_playing: false,
            now_playing_position: default_now_playing_position(),
            now_playing_move_enabled: default_now_playing_move_enabled(),
            now_playing_move_interval_seconds: default_now_playing_move_interval_seconds(),
            show_rss_ticker: default_rss_ticker_enabled(),
            rss_feeds: Vec::new(),
            rss_ticker_speed_px_s: default_rss_ticker_speed_px_s(),
            rss_refresh_interval_minutes: default_rss_refresh_interval_minutes(),
            show_system_stats: default_system_stats_enabled(),
            system_stats_position: default_system_stats_position(),
            system_stats_move_enabled: default_system_stats_move_enabled(),
            system_stats_move_interval_seconds: default_system_stats_move_interval_seconds(),
            clock_format: default_clock_format(),
            clock_two_lines: false,
            web_interaction_enabled: default_web_interaction_enabled(),
            clock_time_format: default_clock_time_format(),
            clock_date_format: default_clock_date_format(),
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
            pattern_speed: default_pattern_speed(),
            pattern_density: default_pattern_density(),
            pattern_theme: default_pattern_theme(),
            water_ripples_background_image: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationLogEntry {
    pub timestamp: u64,
    pub profile_name: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MonitorProfileOverride {
    pub monitor_id: String,
    pub profile_index: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub profiles: Vec<SettingsProfile>,
    #[serde(default)]
    pub active_profile: u8,
    #[serde(default)]
    pub monitor_profile_overrides: Vec<MonitorProfileOverride>,
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
    #[serde(default = "default_hotkey_panic")]
    pub hotkey_panic: String,
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
            monitor_profile_overrides: Vec::new(),
            language: default_language(),
            start_minimized: default_start_minimized(),
            settings_window: None,
            hotkey_start: default_hotkey_start(),
            hotkey_stop: default_hotkey_stop(),
            hotkey_panic: default_hotkey_panic(),
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

fn default_hotkey_panic() -> String {
    DEFAULT_HOTKEY_PANIC.to_string()
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

fn default_web_interaction_enabled() -> bool {
    DEFAULT_WEB_INTERACTION_ENABLED
}

fn default_clock_time_format() -> String {
    DEFAULT_CLOCK_TIME_FORMAT.to_string()
}

fn default_clock_date_format() -> String {
    DEFAULT_CLOCK_DATE_FORMAT.to_string()
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

fn default_now_playing_position() -> ClockPosition {
    DEFAULT_NOW_PLAYING_POSITION
}

fn default_now_playing_move_enabled() -> bool {
    DEFAULT_NOW_PLAYING_MOVE_ENABLED
}

fn default_now_playing_move_interval_seconds() -> u64 {
    DEFAULT_NOW_PLAYING_MOVE_INTERVAL_SECONDS
}

fn default_rss_ticker_enabled() -> bool {
    DEFAULT_RSS_TICKER_ENABLED
}

fn default_rss_ticker_speed_px_s() -> u32 {
    DEFAULT_RSS_TICKER_SPEED_PX_S
}

fn default_rss_refresh_interval_minutes() -> u64 {
    DEFAULT_RSS_REFRESH_INTERVAL_MINUTES
}

fn default_system_stats_enabled() -> bool {
    DEFAULT_SYSTEM_STATS_ENABLED
}

fn default_system_stats_position() -> ClockPosition {
    DEFAULT_SYSTEM_STATS_POSITION
}

fn default_system_stats_move_enabled() -> bool {
    DEFAULT_SYSTEM_STATS_MOVE_ENABLED
}

fn default_system_stats_move_interval_seconds() -> u64 {
    DEFAULT_SYSTEM_STATS_MOVE_INTERVAL_SECONDS
}

fn default_slideshow_interval_seconds() -> u64 {
    DEFAULT_SLIDESHOW_INTERVAL_SECONDS
}

fn default_pattern_speed() -> PatternSpeed {
    PatternSpeed::Normal
}

fn default_pattern_density() -> PatternDensity {
    PatternDensity::Medium
}

fn default_pattern_theme() -> PatternTheme {
    PatternTheme::Default
}

impl Config {
    pub fn load() -> Self {
        Self::migrate_legacy_config();
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

    pub fn monitor_override_profile_index(&self, monitor_id: &str) -> Option<usize> {
        self.monitor_profile_overrides
            .iter()
            .find(|o| o.monitor_id == monitor_id)
            .map(|o| o.profile_index as usize)
            .filter(|idx| *idx < self.profiles.len())
    }

    pub fn resolved_profile_index_for_monitor(&self, monitor_id: &str) -> usize {
        self.monitor_override_profile_index(monitor_id)
            .unwrap_or_else(|| self.active_profile_index())
    }

    pub fn set_monitor_profile_override(
        &mut self,
        monitor_id: String,
        profile_index: Option<usize>,
    ) {
        let monitor_id = monitor_id.trim().to_string();
        if monitor_id.is_empty() {
            return;
        }

        let profile_index = profile_index
            .and_then(|idx| u8::try_from(idx).ok())
            .filter(|idx| (*idx as usize) < self.profiles.len());

        if let Some(profile_index) = profile_index {
            if let Some(existing) = self
                .monitor_profile_overrides
                .iter_mut()
                .find(|o| o.monitor_id == monitor_id)
            {
                existing.profile_index = profile_index;
            } else {
                self.monitor_profile_overrides.push(MonitorProfileOverride {
                    monitor_id,
                    profile_index,
                });
            }
        } else {
            self.monitor_profile_overrides
                .retain(|o| o.monitor_id != monitor_id);
        }
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

        self.monitor_profile_overrides.retain(|o| {
            !o.monitor_id.trim().is_empty() && (o.profile_index as usize) < self.profiles.len()
        });
        self.monitor_profile_overrides
            .sort_by(|a, b| a.monitor_id.cmp(&b.monitor_id));
        self.monitor_profile_overrides
            .dedup_by(|a, b| a.monitor_id == b.monitor_id);
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
            show_now_playing: false,
            now_playing_position: default_now_playing_position(),
            now_playing_move_enabled: default_now_playing_move_enabled(),
            now_playing_move_interval_seconds: default_now_playing_move_interval_seconds(),
            show_rss_ticker: default_rss_ticker_enabled(),
            rss_feeds: Vec::new(),
            rss_ticker_speed_px_s: default_rss_ticker_speed_px_s(),
            rss_refresh_interval_minutes: default_rss_refresh_interval_minutes(),
            show_system_stats: default_system_stats_enabled(),
            system_stats_position: default_system_stats_position(),
            system_stats_move_enabled: default_system_stats_move_enabled(),
            system_stats_move_interval_seconds: default_system_stats_move_interval_seconds(),
            clock_format: default_clock_format(),
            clock_two_lines: false,
            web_interaction_enabled: default_web_interaction_enabled(),
            clock_time_format: default_clock_time_format(),
            clock_date_format: default_clock_date_format(),
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
            pattern_speed: default_pattern_speed(),
            pattern_density: default_pattern_density(),
            pattern_theme: default_pattern_theme(),
            water_ripples_background_image: String::new(),
        };
        Self {
            profiles,
            active_profile: 0,
            monitor_profile_overrides: Vec::new(),
            language: default_language(),
            start_minimized: default_start_minimized(),
            settings_window: legacy.settings_window,
            hotkey_start: legacy.hotkey_start,
            hotkey_stop: legacy.hotkey_stop,
            hotkey_panic: default_hotkey_panic(),
            total_runtime_seconds: default_total_runtime_seconds(),
            activation_log: Vec::new(),
        }
    }

    fn migrate_legacy_config() {
        let Some(new_dirs) = ProjectDirs::from("", "", "vesper") else {
            return;
        };
        let Some(old_dirs) = ProjectDirs::from("", "", "rs-screensaver") else {
            return;
        };

        let new_config_dir = new_dirs.config_dir();
        let old_config_dir = old_dirs.config_dir();

        if new_config_dir.exists() {
            return;
        }

        if !old_config_dir.exists() {
            return;
        }

        if let Some(parent) = new_config_dir.parent() {
            let _ = fs::create_dir_all(parent);
        }

        match fs::rename(old_config_dir, new_config_dir) {
            Ok(_) => eprintln!(
                "Migrated config from {:?} to {:?}",
                old_config_dir, new_config_dir
            ),
            Err(e) => eprintln!("Failed to migrate config: {}", e),
        }
    }

    fn config_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("", "", "vesper") {
            proj_dirs.config_dir().join("config.json")
        } else {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                .join(".config")
                .join("vesper")
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
