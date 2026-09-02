use crate::config::{
    AnimatedPattern, Config, PanelCommand, ScreensaverMode, SettingsProfile, WindowGeometry,
    MAX_PROFILES,
};
use crate::i18n::{
    language_from_index, language_index, profile_name, resolve_language, tr, yes_no, Language,
};
use crate::AppMessage;
use gdk4::{Display, Key, ModifierType};
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ColorDialogButton, ContentFit, Entry, EventControllerKey, Frame,
    IconLookupFlags, IconTheme, Label, ListBox, Picture, SelectionMode, SpinButton, Stack, Switch,
    TextDirection, EntryIconPosition,
};
use image::GenericImageView;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;

pub mod advanced;
pub mod appearance;
pub mod autostart;
pub mod content;
pub mod general;
pub mod power;
pub mod shaderpacks;

use advanced::*;
use appearance::*;
use autostart::*;
use content::*;
use general::*;
use power::*;
use shaderpacks::*;

pub struct SettingsWindow {
    window: adw::Window,
}

struct SettingsUiRefs {
    save_button: gtk4::Button,
    profile_dropdown: gtk4::DropDown,
    profile_model: gtk4::StringList,
    profile_name_row: adw::EntryRow,
    inactivity_spin: SpinButton,
    mouse_wake_spin: SpinButton,
    fade_switch: Switch,
    language_row: adw::ComboRow,
    start_minimized_switch: Switch,
    tray_icon_switch: Switch,
    tray_click_row: adw::ActionRow,
    tray_click_switch: Switch,
    start_hotkey_entry: Entry,
    stop_hotkey_entry: Entry,
    panic_hotkey_entry: Entry,
    autostart_switch: Switch,
    mode_selector: gtk4::FlowBox,
    stack: Stack,
    color_button: ColorDialogButton,
    gradient_start_button: ColorDialogButton,
    gradient_end_button: ColorDialogButton,
    pattern_row: adw::ComboRow,
    pattern_speed_row: adw::ComboRow,
    pattern_density_row: adw::ComboRow,
    pattern_theme_row: adw::ComboRow,
    water_ripples_bg_row: adw::ActionRow,
    _water_ripples_bg_button: gtk4::Button,
    water_ripples_bg_clear_button: gtk4::Button,
    web_url_row: adw::EntryRow,
    web_interaction_switch: Switch,
    stream_url_row: adw::EntryRow,
    shadertoy_source_row: adw::ComboRow,
    _shadertoy_source_model: gtk4::StringList,
    shadertoy_pack_row: adw::ComboRow,
    shadertoy_pack_model: gtk4::StringList,
    shadertoy_shader_row: adw::ComboRow,
    shadertoy_shader_model: gtk4::StringList,
    shadertoy_packs: Rc<RefCell<Vec<crate::shaderpacks::Shaderpack>>>,
    shadertoy_manual_path: Rc<RefCell<Option<String>>>,
    file_row: adw::ActionRow,
    file_info_row: adw::ActionRow,
	shader_check_row: adw::ActionRow,
	shadertoy_interaction_row: adw::ActionRow,
	shadertoy_interaction_switch: Switch,
	shadertoy_hide_cursor_row: adw::ActionRow,
	shadertoy_hide_cursor_switch: Switch,
	shadertoy_sound_row: adw::ActionRow,
	slideshow_interval_row: adw::ActionRow,
	mute_row: adw::ActionRow,
	volume_row: adw::ActionRow,
    random_row: adw::ActionRow,
    slideshow_interval_spin: SpinButton,
    mute_switch: Switch,
    shadertoy_sound_switch: Switch,
    video_volume_spin: SpinButton,
    random_media_switch: Switch,
    media_files: Rc<RefCell<Vec<String>>>,
    selected_media_preview: Rc<RefCell<Option<String>>>,
    media_list_row: adw::ActionRow,
    media_list_box: ListBox,
    remove_media_button: gtk4::Button,
    now_playing_switch: Switch,
    now_playing_position_row: adw::ComboRow,
    now_playing_move_switch: Switch,
    now_playing_move_interval_row: adw::ActionRow,
    now_playing_move_interval_spin: SpinButton,
    now_playing_preview_box: gtk4::Box,
    rss_switch: Switch,
    rss_speed_spin: SpinButton,
    rss_refresh_spin: SpinButton,
    rss_feeds: Rc<RefCell<Vec<String>>>,
    rss_feeds_list: ListBox,
    rss_feed_entry: adw::EntryRow,
    system_stats_switch: Switch,
    system_stats_position_row: adw::ComboRow,
    system_stats_move_switch: Switch,
    system_stats_move_interval_row: adw::ActionRow,
    system_stats_move_interval_spin: SpinButton,
    clock_switch: Switch,
    clock_two_lines_switch: Switch,
    clock_format_row: adw::ComboRow,
    clock_format_entry: adw::EntryRow,
    clock_time_format_entry: adw::EntryRow,
    clock_date_format_entry: adw::EntryRow,
    clock_position_row: adw::ComboRow,
    clock_move_switch: Switch,
    clock_move_interval_row: adw::ActionRow,
    clock_move_interval_spin: SpinButton,
    clock_size_spin: SpinButton,
    clock_preview_label: Label,
    inhibit_switch: Switch,
    ignore_idle_inhibitors_switch: Switch,
    power_integration_switch: Switch,
    integrated_lock_screen_switch: Switch,
    mpris_pause_switch: Switch,
    app_inhibit_list: ListBox,
    app_inhibit_entry: adw::EntryRow,
    app_inhibit_apps: Rc<RefCell<Vec<String>>>,
    delete_button: gtk4::Button,
    panel_commands_list: ListBox,
    monitor_profile_model: gtk4::StringList,
    monitor_profile_rows: Rc<RefCell<Vec<(String, adw::ComboRow)>>>,
}

struct SettingsController {
    config: Rc<RefCell<Config>>,
    modified: Rc<Cell<bool>>,
    profile_update_guard: Rc<Cell<bool>>,
    lang: Language,
    window_weak: glib::WeakRef<adw::Window>,
    ui: SettingsUiRefs,
    update_status: Rc<dyn Fn()>,
    update_preview: Rc<dyn Fn()>,
}

impl SettingsController {
    fn active_profile_index(&self) -> usize {
        self.ui.profile_dropdown.selected() as usize
    }

    fn set_modified(&self, value: bool) {
        if self.modified.get() == value {
            return;
        }
        self.modified.set(value);
        self.ui.save_button.set_sensitive(value);
    }

    fn mark_modified(&self) {
        if self.profile_update_guard.get() {
            return;
        }
        self.set_modified(true);
        (self.update_status)();
    }

    fn apply_profile_to_ui(self: &Rc<Self>, index: usize) {
        self.profile_update_guard.set(true);

        let (
            profile,
            language,
            hotkey_start,
            hotkey_stop,
            hotkey_panic,
            start_minimized,
            tray_icon_enabled,
            tray_click_starts_screensaver,
            autostart_enabled,
            clamped,
            total_profiles,
        ) = {
            let mut config = self.config.borrow_mut();
            if config.profiles.is_empty() {
                self.profile_update_guard.set(false);
                return;
            }
            let clamped = index.min(config.profiles.len().saturating_sub(1));
            config.active_profile = clamped as u8;
            let profile = config.profiles[clamped].clone();
            let language = config.language;
            let hotkey_start = config.hotkey_start.clone();
            let hotkey_stop = config.hotkey_stop.clone();
            let hotkey_panic = config.hotkey_panic.clone();
            let start_minimized = config.start_minimized;
            let tray_icon_enabled = config.tray_icon_enabled;
            let tray_click_starts_screensaver = config.tray_click_starts_screensaver;
            let autostart_enabled = crate::autostart::is_autostart_enabled();
            let total_profiles = config.profiles.len();
            (
                profile,
                language,
                hotkey_start,
                hotkey_stop,
                hotkey_panic,
                start_minimized,
                tray_icon_enabled,
                tray_click_starts_screensaver,
                autostart_enabled,
                clamped,
                total_profiles,
            )
        };

        self.ui.profile_dropdown.set_selected(clamped as u32);
        self.ui.profile_name_row.set_text(&profile.name);
        self.ui
            .inactivity_spin
            .set_value(profile.inactivity_seconds as f64);
        self.ui
            .mouse_wake_spin
            .set_value(profile.mouse_wake_delay_ms as f64);
        self.ui.fade_switch.set_active(profile.fade_enabled);
        self.ui.language_row.set_selected(language_index(language));
        self.ui.start_minimized_switch.set_active(start_minimized);
        self.ui.tray_icon_switch.set_active(tray_icon_enabled);
        self.ui.tray_click_switch.set_active(tray_click_starts_screensaver);
        self.ui.tray_click_row.set_sensitive(tray_icon_enabled);
        self.ui.autostart_switch.set_active(autostart_enabled);
	        set_hotkey_entry(&self.ui.start_hotkey_entry, &hotkey_start);
	        set_hotkey_entry(&self.ui.stop_hotkey_entry, &hotkey_stop);
	        set_hotkey_entry(&self.ui.panic_hotkey_entry, &hotkey_panic);

        let mode_idx = profile_mode_index(&profile.mode);
        if let Some(child) = self.ui.mode_selector.child_at_index(mode_idx as i32) {
            self.ui.mode_selector.select_child(&child);
        }
        set_stack_for_mode(&self.ui.stack, mode_idx);

        match &profile.mode {
            ScreensaverMode::Color(hex) => {
                let rgba = content::hex_to_rgba(hex).unwrap_or(gdk4::RGBA::new(0.0, 0.0, 0.0, 1.0));
                self.ui.color_button.set_rgba(&rgba);
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(
                    &self.ui.file_row,
                    &self.ui.file_info_row,
                    None,
                    tr(self.lang, "Файл не выбран"),
                    self.lang,
                );
            }
            ScreensaverMode::Gradient { start, end } => {
                let start_rgba =
                    content::hex_to_rgba(start).unwrap_or(gdk4::RGBA::new(0.0, 0.0, 0.0, 1.0));
                let end_rgba =
                    content::hex_to_rgba(end).unwrap_or(gdk4::RGBA::new(0.0, 0.0, 0.0, 1.0));
                self.ui.gradient_start_button.set_rgba(&start_rgba);
                self.ui.gradient_end_button.set_rgba(&end_rgba);
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(
                    &self.ui.file_row,
                    &self.ui.file_info_row,
                    None,
                    tr(self.lang, "Файл не выбран"),
                    self.lang,
                );
            }
            ScreensaverMode::Pattern(pattern) => {
                self.ui
                    .pattern_row
                    .set_selected(content::pattern_index(*pattern));

                let speed_idx = match profile.pattern_speed {
                    crate::config::PatternSpeed::Slow => 0,
                    crate::config::PatternSpeed::Normal => 1,
                    crate::config::PatternSpeed::Fast => 2,
                };
                self.ui.pattern_speed_row.set_selected(speed_idx);

                let density_idx = match profile.pattern_density {
                    crate::config::PatternDensity::Low => 0,
                    crate::config::PatternDensity::Medium => 1,
                    crate::config::PatternDensity::High => 2,
                };
                self.ui.pattern_density_row.set_selected(density_idx);

                let theme_idx = match profile.pattern_theme {
                    crate::config::PatternTheme::Default => 0,
                    crate::config::PatternTheme::Mono => 1,
                    crate::config::PatternTheme::Warm => 2,
                    crate::config::PatternTheme::Cool => 3,
                    crate::config::PatternTheme::Random => 4,
                };
                self.ui.pattern_theme_row.set_selected(theme_idx);

                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(
                    &self.ui.file_row,
                    &self.ui.file_info_row,
                    None,
                    tr(self.lang, "Файл не выбран"),
                    self.lang,
                );
            }
            ScreensaverMode::Web(url) => {
                self.ui.web_url_row.set_text(url);
                self.ui.stream_url_row.set_text("");
                set_file_row_path(
                    &self.ui.file_row,
                    &self.ui.file_info_row,
                    None,
                    tr(self.lang, "Файл не выбран"),
                    self.lang,
                );
            }
            ScreensaverMode::Stream(url) => {
                self.ui.stream_url_row.set_text(url);
                self.ui.web_url_row.set_text("");
                set_file_row_path(
                    &self.ui.file_row,
                    &self.ui.file_info_row,
                    None,
                    tr(self.lang, "Файл не выбран"),
                    self.lang,
                );
            }
            ScreensaverMode::Image(path) => {
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(
                    &self.ui.file_row,
                    &self.ui.file_info_row,
                    Some(path),
                    tr(self.lang, "Файл не выбран"),
                    self.lang,
                );
            }
            ScreensaverMode::Video(path) => {
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(
                    &self.ui.file_row,
                    &self.ui.file_info_row,
                    Some(path),
                    tr(self.lang, "Файл не выбран"),
                    self.lang,
                );
            }
            ScreensaverMode::Slideshow(path) => {
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(
                    &self.ui.file_row,
                    &self.ui.file_info_row,
                    Some(path),
                    tr(self.lang, "Папка не выбрана"),
                    self.lang,
                );
            }
            ScreensaverMode::PythonScript(path) => {
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(
                    &self.ui.file_row,
                    &self.ui.file_info_row,
                    Some(path),
                    tr(self.lang, "Файл не выбран"),
                    self.lang,
                );
            }
            ScreensaverMode::Shadertoy(path) => {
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                {
                    let trimmed = path.trim();
                    *self.ui.shadertoy_manual_path.borrow_mut() = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    };
                }

                set_file_row_path(
                    &self.ui.file_row,
                    &self.ui.file_info_row,
                    Some(path),
                    tr(self.lang, "Файл не выбран"),
                    self.lang,
                );

                // Best-effort: if the current path points into an installed shaderpack,
                // switch the selector to Shaderpack and preselect the matching entry.
                reload_shadertoy_packs_into_models(self);
                let path = std::path::Path::new(path.trim());
                if let Some((pack_idx, shader_idx, image_path)) =
                    find_shadertoy_shaderpack_match_in_cache(self, path)
                {
                    self.ui.shadertoy_source_row.set_selected(1);
                    self.ui.shadertoy_pack_row.set_selected(pack_idx as u32);
                    rebuild_shadertoy_shader_model(self, pack_idx);
                    self.ui.shadertoy_shader_row.set_selected(shader_idx as u32);
                    if let Some(img) = image_path.map(|p| p.to_string_lossy().to_string()) {
                        set_file_row_path(
                            &self.ui.file_row,
                            &self.ui.file_info_row,
                            Some(&img),
                            tr(self.lang, "Файл не выбран"),
                            self.lang,
                        );
                    }
                } else {
                    self.ui.shadertoy_source_row.set_selected(0);
                }
            }
        }

        set_content_mode_visibility(
            &self.ui.stream_url_row,
            &self.ui.shadertoy_source_row,
            &self.ui.shadertoy_pack_row,
            &self.ui.shadertoy_shader_row,
            &self.ui.file_row,
            &self.ui.file_info_row,
            &self.ui.shader_check_row,
            &self.ui.shadertoy_interaction_row,
            &self.ui.shadertoy_hide_cursor_row,
            &self.ui.shadertoy_sound_row,
            &self.ui.slideshow_interval_row,
            &self.ui.mute_row,
            &self.ui.volume_row,
            &self.ui.random_row,
            &self.ui.media_list_row,
            &self.ui.media_list_box,
            mode_idx,
        );

        {
            let path = profile.water_ripples_background_image.trim();
            if !path.is_empty() {
                self.ui.water_ripples_bg_row.set_subtitle(path);
                self.ui.water_ripples_bg_row.set_tooltip_text(Some(path));
                self.ui.water_ripples_bg_clear_button.set_sensitive(true);
            } else {
                self.ui
                    .water_ripples_bg_row
                    .set_subtitle(tr(self.lang, "Файл не выбран"));
                self.ui.water_ripples_bg_row.set_tooltip_text(None);
                self.ui.water_ripples_bg_clear_button.set_sensitive(false);
            }
            let visible = matches!(
                profile.mode,
                ScreensaverMode::Pattern(AnimatedPattern::WaterRipples)
            );
            self.ui.water_ripples_bg_row.set_visible(visible);
        }

        self.ui
            .web_interaction_switch
            .set_active(profile.web_interaction_enabled);

        update_file_info_row(
            &self.ui.file_info_row,
            file_row_path(&self.ui.file_row),
            mode_idx,
            self.lang,
        );

        self.ui
            .slideshow_interval_spin
            .set_value(profile.slideshow_interval_seconds as f64);
        self.ui.mute_switch.set_active(profile.mute_video);
	        self.ui
	            .shadertoy_interaction_switch
	            .set_active(profile.shadertoy_interaction_enabled);
	        self.ui
	            .shadertoy_hide_cursor_switch
	            .set_active(profile.shadertoy_hide_cursor);
	        self.ui
	            .shadertoy_sound_switch
	            .set_active(profile.shadertoy_sound_enabled);
        self.ui
            .video_volume_spin
            .set_value(profile.video_volume as f64);
        self.ui.random_media_switch.set_active(profile.random_media);
        self.ui.media_files.borrow_mut().clear();
        self.ui.media_files.borrow_mut().extend(profile.media_list);
        self.ui.selected_media_preview.borrow_mut().take();
        sync_media_list_box(
            &self.ui.media_list_box,
            &self.ui.media_files.borrow(),
            self.lang,
        );
        self.ui.media_list_box.select_row(None::<&gtk4::ListBoxRow>);
        self.ui.remove_media_button.set_sensitive(false);

        self.ui
            .now_playing_switch
            .set_active(profile.show_now_playing);
        self.ui
            .now_playing_position_row
            .set_selected(appearance::clock_position_index(
                profile.now_playing_position,
            ));
        self.ui
            .now_playing_move_switch
            .set_active(profile.now_playing_move_enabled);
        self.ui
            .now_playing_move_interval_row
            .set_visible(profile.now_playing_move_enabled);
        self.ui
            .now_playing_move_interval_spin
            .set_value(profile.now_playing_move_interval_seconds as f64);
        appearance::apply_widget_position(
            &self.ui.now_playing_preview_box,
            profile.now_playing_position,
            16,
        );

        self.ui.rss_switch.set_active(profile.show_rss_ticker);
        self.ui
            .rss_speed_spin
            .set_value(profile.rss_ticker_speed_px_s as f64);
        self.ui
            .rss_refresh_spin
            .set_value(profile.rss_refresh_interval_minutes as f64);
        {
            let mut feeds = self.ui.rss_feeds.borrow_mut();
            feeds.clear();
            feeds.extend(profile.rss_feeds.clone());
        }
        self.ui.rss_feed_entry.set_text("");
        sync_rss_feeds_list(self);

        self.ui
            .system_stats_switch
            .set_active(profile.show_system_stats);
        self.ui
            .system_stats_position_row
            .set_selected(appearance::clock_position_index(
                profile.system_stats_position,
            ));
        self.ui
            .system_stats_move_switch
            .set_active(profile.system_stats_move_enabled);
        self.ui
            .system_stats_move_interval_row
            .set_visible(profile.system_stats_move_enabled);
        self.ui
            .system_stats_move_interval_spin
            .set_value(profile.system_stats_move_interval_seconds as f64);

        self.ui.clock_switch.set_active(profile.show_clock);
        self.ui
            .clock_two_lines_switch
            .set_active(profile.clock_two_lines);

        self.ui
            .clock_position_row
            .set_selected(appearance::clock_position_index(profile.clock_position));
        self.ui
            .clock_move_switch
            .set_active(profile.clock_move_enabled);
        self.ui
            .clock_move_interval_row
            .set_visible(profile.clock_move_enabled);
        self.ui
            .clock_move_interval_spin
            .set_value(profile.clock_move_interval_seconds as f64);
        self.ui.clock_size_spin.set_value(profile.clock_size as f64);

        self.ui
            .clock_time_format_entry
            .set_text(&profile.clock_time_format);
        self.ui
            .clock_date_format_entry
            .set_text(&profile.clock_date_format);

        let preset_index = appearance::clock_format_preset_index(&profile.clock_format);
        let selected = preset_index.unwrap_or(CLOCK_FORMAT_PATTERNS.len() as u32);
        self.ui.clock_format_row.set_selected(selected);
        self.ui.clock_format_entry.set_text(&profile.clock_format);

        self.ui
            .clock_format_row
            .set_sensitive(!profile.clock_two_lines);
        self.ui
            .clock_format_entry
            .set_visible(!profile.clock_two_lines && preset_index.is_none());
        self.ui
            .clock_time_format_entry
            .set_visible(profile.clock_two_lines);
        self.ui
            .clock_date_format_entry
            .set_visible(profile.clock_two_lines);

        let preview_text = if profile.clock_two_lines {
            format!(
                "{}\n{}",
                appearance::format_clock_text(&profile.clock_time_format),
                appearance::format_clock_text(&profile.clock_date_format)
            )
        } else {
            appearance::format_clock_text(&profile.clock_format)
        };
        self.ui.clock_preview_label.set_text(&preview_text);
        appearance::apply_clock_size(&self.ui.clock_preview_label, profile.clock_size);
        appearance::apply_clock_position(&self.ui.clock_preview_label, profile.clock_position, 20);

        self.ui.inhibit_switch.set_active(profile.inhibit_sleep);
        self.ui
            .ignore_idle_inhibitors_switch
            .set_active(profile.ignore_idle_inhibitors);
        self.ui
            .power_integration_switch
            .set_active(profile.power_integration_enabled);
        let lock_enabled = profile
            .integrated_lock_screen_enabled
            .unwrap_or(profile.lock_screen_enabled);
        self.ui
            .integrated_lock_screen_switch
            .set_active(lock_enabled);
        self.ui
            .mpris_pause_switch
            .set_active(profile.mpris_pause_enabled);
        {
            let mut apps = self.ui.app_inhibit_apps.borrow_mut();
            apps.clear();
            for name in &profile.app_inhibit_list {
                if let Some(norm) = crate::apps::normalize_app_name(name) {
                    if !apps.contains(&norm) {
                        apps.push(norm);
                    }
                }
            }
        }
        self.ui.app_inhibit_entry.set_text("");
        sync_app_inhibit_list(self);
        self.ui.delete_button.set_sensitive(total_profiles > 1);
        sync_panel_commands_list(self);

        self.profile_update_guard.set(false);
        self.set_modified(false);
        (self.update_preview)();
        (self.update_status)();
    }

    fn apply_ui_to_profile(&self, index: usize) {
        let mut config = self.config.borrow_mut();
        if config.profiles.is_empty() {
            return;
        }
        let clamped = index.min(config.profiles.len().saturating_sub(1));
        config.active_profile = clamped as u8;
        let profile = &mut config.profiles[clamped];

        profile.name = self.ui.profile_name_row.text().to_string();
        profile.inactivity_seconds = self.ui.inactivity_spin.value() as u64;
        profile.mouse_wake_delay_ms = self.ui.mouse_wake_spin.value() as u64;
        profile.fade_enabled = self.ui.fade_switch.is_active();
        profile.mute_video = self.ui.mute_switch.is_active();
        profile.shadertoy_interaction_enabled = self.ui.shadertoy_interaction_switch.is_active();
        profile.shadertoy_hide_cursor = self.ui.shadertoy_hide_cursor_switch.is_active();
        profile.shadertoy_sound_enabled = self.ui.shadertoy_sound_switch.is_active();
        profile.video_volume = self.ui.video_volume_spin.value() as u8;
        profile.random_media = self.ui.random_media_switch.is_active();
        profile.media_list = self.ui.media_files.borrow().clone();
        profile.slideshow_interval_seconds = self.ui.slideshow_interval_spin.value() as u64;
        profile.inhibit_sleep = self.ui.inhibit_switch.is_active();
        profile.ignore_idle_inhibitors = self.ui.ignore_idle_inhibitors_switch.is_active();
        profile.power_integration_enabled = self.ui.power_integration_switch.is_active();
        let lock_enabled = self.ui.integrated_lock_screen_switch.is_active();
        profile.lock_screen_enabled = lock_enabled;
        profile.integrated_lock_screen_enabled = Some(lock_enabled);
        profile.mpris_pause_enabled = self.ui.mpris_pause_switch.is_active();
        profile.app_inhibit_list = normalized_app_list(&self.ui.app_inhibit_apps.borrow());
        profile.show_now_playing = self.ui.now_playing_switch.is_active();
        profile.now_playing_position =
            appearance::clock_position_from_index(self.ui.now_playing_position_row.selected());
        profile.now_playing_move_enabled = self.ui.now_playing_move_switch.is_active();
        profile.now_playing_move_interval_seconds =
            self.ui.now_playing_move_interval_spin.value() as u64;
        profile.show_rss_ticker = self.ui.rss_switch.is_active();
        profile.rss_ticker_speed_px_s = self.ui.rss_speed_spin.value() as u32;
        profile.rss_refresh_interval_minutes = self.ui.rss_refresh_spin.value() as u64;
        profile.rss_feeds = normalized_rss_feeds(&self.ui.rss_feeds.borrow());
        profile.show_system_stats = self.ui.system_stats_switch.is_active();
        profile.system_stats_position =
            appearance::clock_position_from_index(self.ui.system_stats_position_row.selected());
        profile.system_stats_move_enabled = self.ui.system_stats_move_switch.is_active();
        profile.system_stats_move_interval_seconds =
            self.ui.system_stats_move_interval_spin.value() as u64;
        profile.show_clock = self.ui.clock_switch.is_active();
        profile.clock_two_lines = self.ui.clock_two_lines_switch.is_active();
        profile.web_interaction_enabled = self.ui.web_interaction_switch.is_active();
        profile.water_ripples_background_image =
            path_or_empty(content::file_row_path(&self.ui.water_ripples_bg_row));
        profile.clock_time_format = if self.ui.clock_time_format_entry.text().trim().is_empty() {
            crate::config::DEFAULT_CLOCK_TIME_FORMAT.to_string()
        } else {
            self.ui.clock_time_format_entry.text().to_string()
        };
        profile.clock_date_format = if self.ui.clock_date_format_entry.text().trim().is_empty() {
            crate::config::DEFAULT_CLOCK_DATE_FORMAT.to_string()
        } else {
            self.ui.clock_date_format_entry.text().to_string()
        };
        profile.clock_position =
            appearance::clock_position_from_index(self.ui.clock_position_row.selected());
        profile.clock_move_enabled = self.ui.clock_move_switch.is_active();
        profile.clock_move_interval_seconds = self.ui.clock_move_interval_spin.value() as u64;
        profile.clock_size = self.ui.clock_size_spin.value() as u32;
        profile.clock_format =
            clock_format_from_ui(&self.ui.clock_format_row, &self.ui.clock_format_entry);

        profile.pattern_speed = match self.ui.pattern_speed_row.selected() {
            0 => crate::config::PatternSpeed::Slow,
            2 => crate::config::PatternSpeed::Fast,
            _ => crate::config::PatternSpeed::Normal,
        };

        profile.pattern_density = match self.ui.pattern_density_row.selected() {
            0 => crate::config::PatternDensity::Low,
            2 => crate::config::PatternDensity::High,
            _ => crate::config::PatternDensity::Medium,
        };

        profile.pattern_theme = match self.ui.pattern_theme_row.selected() {
            0 => crate::config::PatternTheme::Default,
            1 => crate::config::PatternTheme::Mono,
            2 => crate::config::PatternTheme::Warm,
            3 => crate::config::PatternTheme::Cool,
            4 => crate::config::PatternTheme::Random,
            _ => crate::config::PatternTheme::Default,
        };

        let mode_idx = selected_mode_index(&self.ui.mode_selector);
        profile.mode = match mode_idx {
            0 => ScreensaverMode::Color(content::rgba_to_hex(self.ui.color_button.rgba())),
            1 => ScreensaverMode::Gradient {
                start: content::rgba_to_hex(self.ui.gradient_start_button.rgba()),
                end: content::rgba_to_hex(self.ui.gradient_end_button.rgba()),
            },
            2 => ScreensaverMode::Pattern(content::pattern_from_index(
                self.ui.pattern_row.selected(),
            )),
            3 => ScreensaverMode::Web(self.ui.web_url_row.text().to_string()),
            4 => ScreensaverMode::Image(path_or_empty(file_row_path(&self.ui.file_row))),
            5 => ScreensaverMode::Video(path_or_empty(file_row_path(&self.ui.file_row))),
            6 => ScreensaverMode::Slideshow(path_or_empty(file_row_path(&self.ui.file_row))),
            7 => ScreensaverMode::Stream(self.ui.stream_url_row.text().to_string()),
            8 => ScreensaverMode::PythonScript(path_or_empty(file_row_path(&self.ui.file_row))),
            9 => ScreensaverMode::Shadertoy(path_or_empty(file_row_path(&self.ui.file_row))),
            _ => ScreensaverMode::Color(content::rgba_to_hex(self.ui.color_button.rgba())),
        };

        config.language = language_from_index(self.ui.language_row.selected());
        config.tray_icon_enabled = self.ui.tray_icon_switch.is_active();
        config.tray_click_starts_screensaver = self.ui.tray_click_switch.is_active();
        config.start_minimized = self.ui.start_minimized_switch.is_active();
        config.hotkey_start = hotkey_entry_accel(&self.ui.start_hotkey_entry);
        config.hotkey_stop = hotkey_entry_accel(&self.ui.stop_hotkey_entry);
        config.hotkey_panic = hotkey_entry_accel(&self.ui.panic_hotkey_entry);
    }
}

impl SettingsWindow {
    pub fn new(
        app: &Application,
        config: Config,
        sender: std::sync::mpsc::Sender<crate::AppMessage>,
    ) -> Self {
        let (default_width, default_height) = config
            .settings_window
            .as_ref()
            .and_then(|geom| {
                if geom.width > 0 && geom.height > 0 {
                    Some((geom.width, geom.height))
                } else {
                    None
                }
            })
            .unwrap_or((800, 600));

        let lang = resolve_language(config.language);

        let app_icon = crate::ui::app_icon_name();
        let window = adw::Window::builder()
            .application(app)
            .title(tr(lang, "Настройки"))
            .default_width(default_width)
            .default_height(default_height)
            .icon_name(app_icon)
            .build();

        let provider = gtk4::CssProvider::new();
        provider.load_from_string("
            .card { padding: 4px; border-radius: 8px; border: 1px solid transparent; }
            .card:selected { background-color: alpha(@accent_bg_color, 0.2); border-color: @accent_bg_color; }
            .preview-screen { border: 6px solid #222; border-radius: 12px; background-color: #000; box-shadow: 0 4px 12px rgba(0,0,0,0.5); }
            .preview-bg { background: linear-gradient(135deg, #111 0%, #000 100%); }
        ");
        gtk4::style_context_add_provider_for_display(
            &Display::default().expect("Could not connect to a display."),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        if let Some(geom) = config.settings_window.as_ref() {
            if let (Some(x), Some(y)) = (geom.x, geom.y) {
                let position = (x, y);
                window.connect_realize(move |window| {
                    if let Some(display) = Display::default() {
                        if display.backend().is_x11() {
                            if let Some(surface) = window.surface() {
                                if let Ok(x11_surface) = surface.downcast::<gdk4_x11::X11Surface>()
                                {
                                    if let Ok((conn, _)) = x11rb::connect(None) {
                                        use x11rb::connection::Connection as _;
                                        use x11rb::protocol::xproto::ConnectionExt as _;
                                        let _ = conn.configure_window(
                                            x11_surface.xid() as u32,
                                            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                                                .x(position.0)
                                                .y(position.1),
                                        );
                                        let _ = conn.flush();
                                    }
                                }
                            }
                        }
                    }
                });
            }
        }

        let split_view = adw::NavigationSplitView::new();
        let view_stack = adw::ViewStack::new();

        let header_bar = adw::HeaderBar::new();
        let profile_model = gtk4::StringList::new(&[]);
        let profile_dropdown = gtk4::DropDown::builder()
            .model(&profile_model)
            .valign(Align::Center)
            .build();
        header_bar.set_title_widget(Some(&profile_dropdown));

        let save_button = gtk4::Button::builder()
            .label(tr(lang, "Сохранить"))
            .css_classes(["suggested-action"])
            .valign(Align::Center)
            .build();
        header_bar.pack_end(&save_button);

        let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content_box.append(&header_bar);
        content_box.append(&view_stack);

        let general_page = adw::PreferencesPage::new();
        view_stack.add_titled(&general_page, Some("general"), tr(lang, "Общие"));
        let autostart_page = adw::PreferencesPage::new();
        view_stack.add_titled(&autostart_page, Some("autostart"), tr(lang, "Автозапуск"));
        let content_page = adw::PreferencesPage::new();
        view_stack.add_titled(&content_page, Some("content"), tr(lang, "Контент"));
        let shaderpacks_widgets = build_shaderpacks_page(lang);
        view_stack.add_titled(
            &shaderpacks_widgets.page,
            Some("shaderpacks"),
            tr(lang, "Шейдерпаки"),
        );
        let appearance_page = adw::PreferencesPage::new();
        view_stack.add_titled(
            &appearance_page,
            Some("appearance"),
            tr(lang, "Внешний вид"),
        );
        let power_page = adw::PreferencesPage::new();
        view_stack.add_titled(&power_page, Some("power"), tr(lang, "Питание"));
        let advanced_page = adw::PreferencesPage::new();
        view_stack.add_titled(&advanced_page, Some("advanced"), tr(lang, "Система"));
        let profiles_page = adw::PreferencesPage::new();
        view_stack.add_titled(&profiles_page, Some("profiles"), tr(lang, "Профили"));

        let general_widgets = build_general_group(&config, lang);
        general_page.add(&general_widgets.general_group);
        general_page.add(&general_widgets.hotkeys_group);
        let GeneralWidgets {
            inactivity_spin,
            mouse_wake_spin,
            fade_switch,
            language_row,
            start_hotkey_entry,
            stop_hotkey_entry,
            panic_hotkey_entry,
            ..
        } = general_widgets;

        let autostart_widgets = build_autostart_group(&config, lang);
        autostart_page.add(&autostart_widgets.group);
        let AutostartWidgets {
            autostart_switch,
            start_minimized_switch,
            tray_icon_switch,
            tray_click_row,
            tray_click_switch,
            ..
        } = autostart_widgets;

        let content_widgets = build_content_group(&config, lang);
        content_page.add(&content_widgets.group);
        content_page.add(&content_widgets.preview_group);
        let ContentWidgets {
            mode_selector,
            stack,
            color_button,
            gradient_start_button,
            gradient_end_button,
            pattern_row,
            pattern_speed_row,
            pattern_density_row,
            pattern_theme_row,
            water_ripples_bg_row,
            water_ripples_bg_button,
            water_ripples_bg_clear_button,
            web_url_row,
            web_interaction_switch,
            stream_url_row,
            shadertoy_source_row,
            shadertoy_source_model,
            shadertoy_pack_row,
            shadertoy_pack_model,
            shadertoy_shader_row,
            shadertoy_shader_model,
            shadertoy_packs,
            shadertoy_manual_path,
            file_row,
            file_button,
            file_info_row,
            shader_check_row,
	            shader_check_button,
	            shadertoy_interaction_row,
	            shadertoy_interaction_switch,
	            shadertoy_hide_cursor_row,
	            shadertoy_hide_cursor_switch,
	            shadertoy_sound_row,
	            shadertoy_sound_switch,
	            slideshow_interval_row,
            mute_row,
            volume_row,
            random_row,
            slideshow_interval_spin,
            mute_switch,
            video_volume_spin,
            random_media_switch,
            media_list_row,
            media_list_box,
            add_media_button,
            remove_media_button,
            preview_frame,
            preview_pause_row,
            preview_pause_switch,
            media_files,
            selected_media_preview,
            ..
        } = content_widgets;

        let appearance_widgets = build_appearance_group(&config, lang);
        appearance_page.add(&appearance_widgets.now_playing_group);
        appearance_page.add(&appearance_widgets.now_playing_preview_group);
        appearance_page.add(&appearance_widgets.rss_group);
        appearance_page.add(&appearance_widgets.system_stats_group);
        appearance_page.add(&appearance_widgets.group);
        appearance_page.add(&appearance_widgets.clock_preview_group);
        let AppearanceWidgets {
            now_playing_switch,
            now_playing_position_row,
            now_playing_move_switch,
            now_playing_move_interval_row,
            now_playing_move_interval_spin,
            now_playing_preview_box,
            rss_switch,
            rss_speed_spin,
            rss_refresh_spin,
            rss_feeds,
            rss_feeds_list,
            rss_feed_entry,
            rss_add_button,
            system_stats_switch,
            system_stats_position_row,
            system_stats_move_switch,
            system_stats_move_interval_row,
            system_stats_move_interval_spin,
            clock_switch,
            clock_two_lines_switch,
            clock_format_row,
            clock_format_entry,
            clock_time_format_entry,
            clock_date_format_entry,
            clock_position_row,
            clock_size_spin,
            clock_move_switch,
            clock_move_interval_row,
            clock_move_interval_spin,
            clock_preview_label,
            ..
        } = appearance_widgets;

        let power_widgets = build_power_group(&config, lang);
        power_page.add(&power_widgets.group);
        power_page.add(&power_widgets.apps_group);
	        let PowerWidgets {
	            inhibit_switch,
	            ignore_idle_inhibitors_switch,
	            power_integration_switch,
	            integrated_lock_screen_switch,
	            mpris_pause_switch,
	            app_inhibit_list,
	            app_inhibit_entry,
	            app_inhibit_add_button,
	            app_inhibit_refresh_button,
            app_inhibit_apps,
            ..
        } = power_widgets;

        let advanced_widgets = build_advanced_groups(&config, lang);
        advanced_page.add(&advanced_widgets.panel_commands_group);
        advanced_page.add(&advanced_widgets.status_group);
        advanced_page.add(&advanced_widgets.export_import_group);
        advanced_page.add(&advanced_widgets.about_group);
        let AdvancedWidgets {
            status_row,
            panel_commands_list,
            add_command_btn,
            runtime_row,
            activation_group,
            activation_list,
            clear_log_btn,
            export_btn,
            import_btn,
            reset_btn: reset_button,
            about_button,
            ..
        } = advanced_widgets;

        let active_profile_index_init = config.active_profile_index();
        let profile_update_guard = Rc::new(Cell::new(false));

        let profile_names: Vec<String> = config.profiles.iter().map(|p| p.name.clone()).collect();
        let profile_name_refs: Vec<&str> = profile_names.iter().map(|s| s.as_str()).collect();
        profile_model.splice(0, 0, &profile_name_refs);
        profile_dropdown.set_selected(active_profile_index_init as u32);

        let profiles_group = adw::PreferencesGroup::builder()
            .title(tr(lang, "Управление профилями"))
            .build();
        let profile_name_row = adw::EntryRow::builder()
            .title(tr(lang, "Название профиля"))
            .text(config.active_profile().name.as_str())
            .build();
        let add_profile_row = adw::ActionRow::builder()
            .title(tr(lang, "Добавить профиль"))
            .build();
        let add_profile_btn_widget = gtk4::Button::with_label(tr(lang, "Добавить"));
        add_profile_btn_widget.add_css_class("suggested-action");
        add_profile_row.add_suffix(&add_profile_btn_widget);
        profiles_group.add(&profile_name_row);
        profiles_group.add(&add_profile_row);
        profiles_page.add(&profiles_group);

        let monitor_profile_model = gtk4::StringList::new(&[]);
        fill_monitor_profile_model(&monitor_profile_model, &config, lang);
        let monitor_profile_rows: Rc<RefCell<Vec<(String, adw::ComboRow)>>> =
            Rc::new(RefCell::new(Vec::new()));

        let monitors_group = adw::PreferencesGroup::builder()
            .title(tr(lang, "Мониторы"))
            .description(tr(
                lang,
                "Назначьте профиль для каждого монитора (по умолчанию — активный профиль).",
            ))
            .build();

        if let Some(display) = Display::default() {
            let monitors = crate::monitors::list_monitors(&display);
            if monitors.is_empty() {
                let row = adw::ActionRow::builder()
                    .title(tr(lang, "Мониторы не найдены"))
                    .build();
                row.set_sensitive(false);
                monitors_group.add(&row);
            } else {
                for (idx, monitor) in monitors.iter().enumerate() {
                    let monitor_id = crate::monitors::monitor_id(monitor);
                    let title = crate::monitors::monitor_title(lang, idx, monitor);
                    let subtitle = crate::monitors::monitor_subtitle(monitor);
                    let row = adw::ComboRow::builder()
                        .title(&title)
                        .subtitle(&subtitle)
                        .model(&monitor_profile_model)
                        .build();
                    let selected = config
                        .monitor_override_profile_index(&monitor_id)
                        .map(|idx| idx + 1)
                        .unwrap_or(0);
                    row.set_selected(selected as u32);
                    monitors_group.add(&row);
                    monitor_profile_rows
                        .borrow_mut()
                        .push((monitor_id, row.clone()));
                }
            }
        } else {
            let row = adw::ActionRow::builder()
                .title(tr(lang, "Не удалось получить список мониторов"))
                .build();
            row.set_sensitive(false);
            monitors_group.add(&row);
        }

        profiles_page.add(&monitors_group);

        let actions_group = adw::PreferencesGroup::new();
        let buttons_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        buttons_box.set_halign(Align::Center);
        buttons_box.set_margin_top(12);
        let reset_profile_button = gtk4::Button::with_label(tr(lang, "Сбросить"));
        reset_profile_button.add_css_class("destructive-action");
        buttons_box.append(&reset_profile_button);
        let delete_button = gtk4::Button::with_label(tr(lang, "Удалить"));
        delete_button.add_css_class("destructive-action");
        buttons_box.append(&delete_button);
        actions_group.add(&buttons_box);
        profiles_page.add(&actions_group);

        let sidebar_page = adw::NavigationPage::builder()
            .title(tr(lang, "Настройки"))
            .tag("sidebar")
            .build();
        let sidebar_list = ListBox::new();
        sidebar_list.add_css_class("navigation-sidebar");
        sidebar_list.set_selection_mode(SelectionMode::Single);
        let categories = [
            ("general", tr(lang, "Общие"), "preferences-system-symbolic"),
            ("autostart", tr(lang, "Автозапуск"), "system-run-symbolic"),
            (
                "content",
                tr(lang, "Контент"),
                "applications-multimedia-symbolic",
            ),
            (
                "shaderpacks",
                tr(lang, "Шейдерпаки"),
                "applications-graphics-symbolic",
            ),
            (
                "appearance",
                tr(lang, "Внешний вид"),
                "preferences-desktop-appearance-symbolic",
            ),
            (
                "power",
                tr(lang, "Питание"),
                "power-profile-balanced-symbolic",
            ),
            (
                "advanced",
                tr(lang, "Система"),
                "utilities-terminal-symbolic",
            ),
            ("profiles", tr(lang, "Профили"), "user-info-symbolic"),
        ];
        for (tag, title, icon) in categories {
            let row = gtk4::ListBoxRow::new();
            let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
            row_box.set_margin_start(12);
            row_box.set_margin_end(12);
            row_box.set_margin_top(10);
            row_box.set_margin_bottom(10);
            row_box.set_baseline_child(1);
            let img = icon_picture(icon, 16);
            let label = gtk4::Label::new(Some(title));
            label.set_halign(Align::Start);
            label.set_valign(Align::Center);
            label.set_hexpand(true);
            row_box.append(&img);
            row_box.append(&label);
            row.set_child(Some(&row_box));
            unsafe {
                row.set_data("page-tag", tag.to_string());
            }
            sidebar_list.append(&row);
        }
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&sidebar_list)
            .propagate_natural_width(true)
            .build();
        sidebar_page.set_child(Some(&scrolled));
        split_view.set_sidebar(Some(&sidebar_page));

        let content_nav_page = adw::NavigationPage::builder()
            .title(tr(lang, "Настройки"))
            .tag("content-nav")
            .child(&content_box)
            .build();
        split_view.set_content(Some(&content_nav_page));
        sidebar_list.select_row(sidebar_list.row_at_index(0).as_ref());

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&split_view));
        window.set_content(Some(&toast_overlay));

        sidebar_list.connect_row_selected({
            let view_stack = view_stack.clone();
            move |_, row| {
                if let Some(row) = row {
                    let tag = unsafe { row.data::<String>("page-tag").unwrap().as_ref().clone() };
                    view_stack.set_visible_child_name(&tag);
                }
            }
        });

        mode_selector.connect_selected_children_changed({
            let stack = stack.clone();
            let stream_url_row = stream_url_row.clone();
            let shadertoy_source_row = shadertoy_source_row.clone();
            let shadertoy_pack_row = shadertoy_pack_row.clone();
            let shadertoy_shader_row = shadertoy_shader_row.clone();
            let file_row = file_row.clone();
	            let file_info_row = file_info_row.clone();
	            let shader_check_row = shader_check_row.clone();
	            let shadertoy_interaction_row = shadertoy_interaction_row.clone();
	            let shadertoy_hide_cursor_row = shadertoy_hide_cursor_row.clone();
	            let shadertoy_sound_row = shadertoy_sound_row.clone();
	            let slideshow_interval_row = slideshow_interval_row.clone();
            let mute_row = mute_row.clone();
            let volume_row = volume_row.clone();
            let random_row = random_row.clone();
            let media_list_row = media_list_row.clone();
            let media_list_box = media_list_box.clone();
            move |flowbox| {
                let Some(child) = flowbox.selected_children().get(0).cloned() else {
                    return;
                };
                let mode_idx = unsafe { child.data::<u32>("mode-index").unwrap().as_ref().clone() };
                set_stack_for_mode(&stack, mode_idx);
	                set_content_mode_visibility(
	                    &stream_url_row,
	                    &shadertoy_source_row,
	                    &shadertoy_pack_row,
	                    &shadertoy_shader_row,
	                    &file_row,
	                    &file_info_row,
	                    &shader_check_row,
	                    &shadertoy_interaction_row,
	                    &shadertoy_hide_cursor_row,
	                    &shadertoy_sound_row,
	                    &slideshow_interval_row,
	                    &mute_row,
	                    &volume_row,
	                    &random_row,
	                    &media_list_row,
                    &media_list_box,
                    mode_idx,
                );
            }
        });

        let original_config = Rc::new(RefCell::new(config));
        let modified = Rc::new(Cell::new(false));
        save_button.set_sensitive(false);

        let update_status = Rc::new({
            let status_row = status_row.clone();
            let profile_name_row = profile_name_row.clone();
            let inactivity_spin = inactivity_spin.clone();
            let mouse_wake_spin = mouse_wake_spin.clone();
            let mode_selector = mode_selector.clone();
            let color_button = color_button.clone();
            let gradient_start_button = gradient_start_button.clone();
            let gradient_end_button = gradient_end_button.clone();
            let pattern_row = pattern_row.clone();
            let web_url_row = web_url_row.clone();
            let stream_url_row = stream_url_row.clone();
            let file_row = file_row.clone();
            let file_info_row = file_info_row.clone();
            let mute_switch = mute_switch.clone();
            let video_volume_spin = video_volume_spin.clone();
	            let fade_switch = fade_switch.clone();
	            let inhibit_switch = inhibit_switch.clone();
	            let power_integration_switch = power_integration_switch.clone();
	            let integrated_lock_screen_switch = integrated_lock_screen_switch.clone();
	            let clock_switch = clock_switch.clone();
	            let clock_two_lines_switch = clock_two_lines_switch.clone();
	            let clock_format_entry = clock_format_entry.clone();
	            let clock_time_format_entry = clock_time_format_entry.clone();
	            let clock_date_format_entry = clock_date_format_entry.clone();
            let clock_position_row = clock_position_row.clone();
            let clock_move_switch = clock_move_switch.clone();
            let clock_move_interval_spin = clock_move_interval_spin.clone();
            let clock_size_spin = clock_size_spin.clone();
            let slideshow_interval_spin = slideshow_interval_spin.clone();
            let random_media_switch = random_media_switch.clone();
            let media_files = media_files.clone();
            let start_hotkey_entry = start_hotkey_entry.clone();
            let stop_hotkey_entry = stop_hotkey_entry.clone();
            let panic_hotkey_entry = panic_hotkey_entry.clone();
            move || {
                let selected_child = mode_selector.selected_children().get(0).cloned();
                let mode_idx = selected_child
                    .map(|c| unsafe { c.data::<u32>("mode-index").unwrap().as_ref().clone() })
                    .unwrap_or(0);
	                let text = build_status_text(
                    &profile_name_row,
                    &inactivity_spin,
                    &mouse_wake_spin,
                    mode_idx,
                    &color_button,
                    &gradient_start_button,
                    &gradient_end_button,
                    &pattern_row,
                    &web_url_row,
                    &stream_url_row,
                    &file_row,
                    &file_info_row,
                    &mute_switch,
                    &video_volume_spin,
	                    &fade_switch,
	                    &inhibit_switch,
	                    &power_integration_switch,
	                    &integrated_lock_screen_switch,
	                    &clock_switch,
	                    &clock_two_lines_switch,
	                    &clock_format_entry,
                    &clock_time_format_entry,
                    &clock_date_format_entry,
                    &clock_position_row,
                    &clock_move_switch,
                    &clock_move_interval_spin,
                    &clock_size_spin,
                    &slideshow_interval_spin,
                    &random_media_switch,
                    &media_files,
                    &start_hotkey_entry,
                    &stop_hotkey_entry,
                    &panic_hotkey_entry,
                    lang,
                );
                status_row.set_subtitle(&text);
            }
        });

        let preview_media = Rc::new(RefCell::new(None));
        let preview_web = Rc::new(RefCell::new(None));
        let last_preview_mode = Rc::new(Cell::new(u32::MAX));
        let water_ripples_bg_row_for_preview = water_ripples_bg_row.clone();
        let update_preview = Rc::new({
            let preview_frame = preview_frame.clone();
            let mode_selector = mode_selector.clone();
            let file_row = file_row.clone();
            let shadertoy_source_row = shadertoy_source_row.clone();
            let shadertoy_pack_row = shadertoy_pack_row.clone();
            let shadertoy_shader_row = shadertoy_shader_row.clone();
            let shadertoy_packs = shadertoy_packs.clone();
            let random_media_switch = random_media_switch.clone();
            let media_files = media_files.clone();
            let selected_media_preview = selected_media_preview.clone();
            let color_button = color_button.clone();
            let gradient_start_button = gradient_start_button.clone();
            let gradient_end_button = gradient_end_button.clone();
            let pattern_row = pattern_row.clone();
            let pattern_speed_row = pattern_speed_row.clone();
            let pattern_density_row = pattern_density_row.clone();
            let pattern_theme_row = pattern_theme_row.clone();
	            let web_url_row = web_url_row.clone();
	            let stream_url_row = stream_url_row.clone();
	            let mute_switch = mute_switch.clone();
	            let shadertoy_interaction_switch = shadertoy_interaction_switch.clone();
	            let video_volume_spin = video_volume_spin.clone();
	            let clock_switch = clock_switch.clone();
	            let clock_format_row = clock_format_row.clone();
	            let clock_format_entry = clock_format_entry.clone();
	            let clock_time_format_entry = clock_time_format_entry.clone();
            let clock_date_format_entry = clock_date_format_entry.clone();
            let clock_two_lines_switch = clock_two_lines_switch.clone();
            let clock_position_row = clock_position_row.clone();
            let clock_size_spin = clock_size_spin.clone();
            let clock_preview_label = clock_preview_label.clone();
            let preview_pause_row = preview_pause_row.clone();
            let preview_pause_switch = preview_pause_switch.clone();
            let preview_media = preview_media.clone();
            let preview_web = preview_web.clone();
            let last_preview_mode = last_preview_mode.clone();
            let water_ripples_bg_row = water_ripples_bg_row_for_preview.clone();
            move || {
                let selected_child = mode_selector.selected_children().get(0).cloned();
                let mode = selected_child
                    .map(|c| unsafe { c.data::<u32>("mode-index").unwrap().as_ref().clone() })
                    .unwrap_or(0);
                let was_video = matches!(last_preview_mode.get(), 5 | 7);
                let is_video = matches!(mode, 5 | 7);
                if is_video && !was_video {
                    preview_pause_switch.set_active(true);
                }
                last_preview_mode.set(mode);
                preview_pause_row.set_visible(mode == 5 || mode == 7);
                let preview_path = preview_media_path(
                    mode,
                    &file_row,
                    random_media_switch.is_active(),
                    &media_files.borrow(),
                    selected_media_preview.borrow().as_deref(),
                );
                let shadertoy_preview_png = if mode == 9 && shadertoy_source_row.selected() == 1 {
                    let pack_idx = shadertoy_pack_row.selected() as usize;
                    let shader_idx = shadertoy_shader_row.selected() as usize;
                    let packs = shadertoy_packs.borrow();
                    packs.get(pack_idx)
                        .and_then(|p| p.shaders.get(shader_idx))
                        .and_then(|s| s.preview_path.clone())
                } else {
                    None
                };
                let water_ripples_bg_path =
                    crate::ui::settings::content::file_row_path(&water_ripples_bg_row);
                let clock_format = clock_format_from_ui(&clock_format_row, &clock_format_entry);
                let (widget, media, web) = build_preview_widget(
                    mode,
                    &color_button,
                    &gradient_start_button,
                    &gradient_end_button,
                    &pattern_row,
                    &pattern_speed_row,
                    &pattern_density_row,
                    &pattern_theme_row,
                    water_ripples_bg_path.as_deref(),
                    &web_url_row,
                    &stream_url_row,
                    &file_row,
	                    &mute_switch,
	                    video_volume_spin.value() as u8,
	                    preview_pause_switch.is_active(),
	                    preview_path.as_deref(),
	                    shadertoy_preview_png.as_deref(),
	                    shadertoy_interaction_switch.is_active(),
	                    clock_switch.is_active(),
	                    clock_two_lines_switch.is_active(),
	                    &clock_format,
	                    &clock_time_format_entry.text(),
	                    &clock_date_format_entry.text(),
                    appearance::clock_position_from_index(clock_position_row.selected()),
                    clock_size_spin.value() as u32,
                    lang,
                );
                preview_frame.set_child(Some(&widget));
                update_preview_tooltip(&preview_frame, mode, preview_path.as_deref(), lang);
                preview_pause_row.set_visible(media.is_some() && (mode == 5 || mode == 7));
                *preview_media.borrow_mut() = media;
                *preview_web.borrow_mut() = web;

                let preview_text = if clock_two_lines_switch.is_active() {
                    format!(
                        "{}\n{}",
                        appearance::format_clock_text(&clock_time_format_entry.text()),
                        appearance::format_clock_text(&clock_date_format_entry.text())
                    )
                } else {
                    appearance::format_clock_text(&clock_format)
                };
                clock_preview_label.set_text(&preview_text);
                appearance::apply_clock_size(&clock_preview_label, clock_size_spin.value() as u32);
                appearance::apply_clock_position(
                    &clock_preview_label,
                    appearance::clock_position_from_index(clock_position_row.selected()),
                    20,
                );
            }
        });

        let controller = Rc::new(SettingsController {
            config: original_config.clone(),
            modified: modified.clone(),
            profile_update_guard: profile_update_guard.clone(),
            lang,
            window_weak: window.downgrade(),
                ui: SettingsUiRefs {
                save_button: save_button.clone(),
                profile_dropdown: profile_dropdown.clone(),
                profile_model: profile_model.clone(),
                profile_name_row: profile_name_row.clone(),
                inactivity_spin: inactivity_spin.clone(),
                mouse_wake_spin: mouse_wake_spin.clone(),
                fade_switch: fade_switch.clone(),
                language_row: language_row.clone(),
                start_minimized_switch: start_minimized_switch.clone(),
                tray_icon_switch: tray_icon_switch.clone(),
                tray_click_row: tray_click_row.clone(),
                tray_click_switch: tray_click_switch.clone(),
                start_hotkey_entry: start_hotkey_entry.clone(),
                stop_hotkey_entry: stop_hotkey_entry.clone(),
                panic_hotkey_entry: panic_hotkey_entry.clone(),
                autostart_switch: autostart_switch.clone(),
                mode_selector: mode_selector.clone(),
                stack: stack.clone(),
                color_button: color_button.clone(),
                gradient_start_button: gradient_start_button.clone(),
                gradient_end_button: gradient_end_button.clone(),
                pattern_row: pattern_row.clone(),
                pattern_speed_row: pattern_speed_row.clone(),
                pattern_density_row: pattern_density_row.clone(),
                pattern_theme_row: pattern_theme_row.clone(),
                water_ripples_bg_row: water_ripples_bg_row.clone(),
                _water_ripples_bg_button: water_ripples_bg_button.clone(),
                water_ripples_bg_clear_button: water_ripples_bg_clear_button.clone(),
                web_url_row: web_url_row.clone(),
                web_interaction_switch: web_interaction_switch.clone(),
                stream_url_row: stream_url_row.clone(),
                shadertoy_source_row: shadertoy_source_row.clone(),
                _shadertoy_source_model: shadertoy_source_model.clone(),
                shadertoy_pack_row: shadertoy_pack_row.clone(),
                shadertoy_pack_model: shadertoy_pack_model.clone(),
                shadertoy_shader_row: shadertoy_shader_row.clone(),
                shadertoy_shader_model: shadertoy_shader_model.clone(),
                shadertoy_packs: shadertoy_packs.clone(),
                shadertoy_manual_path: shadertoy_manual_path.clone(),
	                file_row: file_row.clone(),
	                file_info_row: file_info_row.clone(),
	                shader_check_row: shader_check_row.clone(),
	                shadertoy_interaction_row: shadertoy_interaction_row.clone(),
	                shadertoy_interaction_switch: shadertoy_interaction_switch.clone(),
	                shadertoy_hide_cursor_row: shadertoy_hide_cursor_row.clone(),
	                shadertoy_hide_cursor_switch: shadertoy_hide_cursor_switch.clone(),
	                shadertoy_sound_row: shadertoy_sound_row.clone(),
	                slideshow_interval_row: slideshow_interval_row.clone(),
                mute_row: mute_row.clone(),
                volume_row: volume_row.clone(),
                random_row: random_row.clone(),
                slideshow_interval_spin: slideshow_interval_spin.clone(),
                mute_switch: mute_switch.clone(),
                shadertoy_sound_switch: shadertoy_sound_switch.clone(),
                video_volume_spin: video_volume_spin.clone(),
                random_media_switch: random_media_switch.clone(),
                media_files: media_files.clone(),
                selected_media_preview: selected_media_preview.clone(),
                media_list_row: media_list_row.clone(),
                media_list_box: media_list_box.clone(),
                remove_media_button: remove_media_button.clone(),
                now_playing_switch: now_playing_switch.clone(),
                now_playing_position_row: now_playing_position_row.clone(),
                now_playing_move_switch: now_playing_move_switch.clone(),
                now_playing_move_interval_row: now_playing_move_interval_row.clone(),
                now_playing_move_interval_spin: now_playing_move_interval_spin.clone(),
                now_playing_preview_box: now_playing_preview_box.clone(),
                rss_switch: rss_switch.clone(),
                rss_speed_spin: rss_speed_spin.clone(),
                rss_refresh_spin: rss_refresh_spin.clone(),
                rss_feeds: rss_feeds.clone(),
                rss_feeds_list: rss_feeds_list.clone(),
                rss_feed_entry: rss_feed_entry.clone(),
                system_stats_switch: system_stats_switch.clone(),
                system_stats_position_row: system_stats_position_row.clone(),
                system_stats_move_switch: system_stats_move_switch.clone(),
                system_stats_move_interval_row: system_stats_move_interval_row.clone(),
                system_stats_move_interval_spin: system_stats_move_interval_spin.clone(),
                clock_switch: clock_switch.clone(),
                clock_two_lines_switch: clock_two_lines_switch.clone(),
                clock_format_row: clock_format_row.clone(),
                clock_format_entry: clock_format_entry.clone(),
                clock_time_format_entry: clock_time_format_entry.clone(),
                clock_date_format_entry: clock_date_format_entry.clone(),
                clock_position_row: clock_position_row.clone(),
                clock_move_switch: clock_move_switch.clone(),
                clock_move_interval_row: clock_move_interval_row.clone(),
                clock_move_interval_spin: clock_move_interval_spin.clone(),
                clock_size_spin: clock_size_spin.clone(),
                clock_preview_label: clock_preview_label.clone(),
                    inhibit_switch: inhibit_switch.clone(),
                    ignore_idle_inhibitors_switch: ignore_idle_inhibitors_switch.clone(),
                    power_integration_switch: power_integration_switch.clone(),
                    integrated_lock_screen_switch: integrated_lock_screen_switch.clone(),
                    mpris_pause_switch: mpris_pause_switch.clone(),
	                app_inhibit_list: app_inhibit_list.clone(),
	                app_inhibit_entry: app_inhibit_entry.clone(),
	                app_inhibit_apps: app_inhibit_apps.clone(),
                delete_button: delete_button.clone(),
                panel_commands_list: panel_commands_list.clone(),
                monitor_profile_model: monitor_profile_model.clone(),
                monitor_profile_rows: monitor_profile_rows.clone(),
            },
            update_status: update_status.clone(),
            update_preview: update_preview.clone(),
        });

        connect_shaderpacks(&shaderpacks_widgets, controller.clone(), toast_overlay.clone());

        for (monitor_id, row) in controller.ui.monitor_profile_rows.borrow().clone() {
            row.connect_notify_local(Some("selected"), {
                let controller = controller.clone();
                let monitor_id = monitor_id.clone();
                move |row, _| {
                    if controller.profile_update_guard.get() {
                        return;
                    }
                    let selected = row.selected() as usize;
                    let override_idx = selected.checked_sub(1);
                    controller
                        .config
                        .borrow_mut()
                        .set_monitor_profile_override(monitor_id.clone(), override_idx);
                    controller.mark_modified();
                    (controller.update_status)();
                }
            });
        }

        add_command_btn.connect_clicked({
            let controller = controller.clone();
            move |_| {
                let presets = PanelCommand::presets();
                if presets.is_empty() {
                    return;
                }
                let Some(window) = controller.window_weak.upgrade() else {
                    return;
                };
                let dialog = adw::MessageDialog::builder()
                    .transient_for(&window)
                    .modal(true)
                    .heading(tr(controller.lang, "Добавить из списка"))
                    .build();
                dialog.add_response("cancel", tr(controller.lang, "Отмена"));
                for (idx, preset) in presets.iter().enumerate() {
                    dialog.add_response(&format!("preset-{idx}"), &preset.name);
                }
                dialog.connect_response(None, {
                    let controller = controller.clone();
                    move |dialog, response| {
                        if let Some(idx) = response
                            .strip_prefix("preset-")
                            .and_then(|v| v.parse::<usize>().ok())
                        {
                            let presets = PanelCommand::presets();
                            if let Some(preset) = presets.get(idx).cloned() {
                                let mut config = controller.config.borrow_mut();
                                let profile_idx = config.active_profile_index();
                                if let Some(profile) = config.profiles.get_mut(profile_idx) {
                                    profile.panel_commands.push(preset);
                                }
                                drop(config);
                                controller.mark_modified();
                                sync_panel_commands_list(&controller);
                            }
                        }
                        dialog.close();
                    }
                });
                dialog.present();
            }
        });

        clear_log_btn.connect_clicked({
            let controller = controller.clone();
            let activation_group = activation_group.clone();
            let activation_list = activation_list.clone();
            let clear_log_btn = clear_log_btn.clone();
            move |_| {
                let mut config = controller.config.borrow_mut();
                config.clear_activation_log();
                let _ = config.save();
                populate_activation_log_group(
                    &activation_group,
                    &activation_list,
                    &config.activation_log,
                    controller.lang,
                );
                clear_log_btn.set_sensitive(false);
                controller.mark_modified();
            }
        });

        export_btn.connect_clicked({
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            move |_| {
                let Some(window) = controller.window_weak.upgrade() else {
                    return;
                };
                let dialog = gtk4::FileDialog::new();
                dialog.set_title(tr(controller.lang, "Экспорт настроек"));
                dialog.set_accept_label(Some(tr(controller.lang, "Сохранить")));
                dialog.set_initial_name(Some("vesper-settings.json"));
                dialog.save(Some(&window), None::<&gio::Cancellable>, {
                    let controller = controller.clone();
                    let toast_overlay = toast_overlay.clone();
                    move |res| {
                        let Ok(file) = res else {
                            return;
                        };
                        let Some(path) = file.path() else {
                            return;
                        };
                        let index = controller.active_profile_index();
                        controller.apply_ui_to_profile(index);
                        let config = controller.config.borrow().clone();
                        let toast_lang = config.language;
                        let content = match serde_json::to_string_pretty(&config) {
                            Ok(content) => content,
                            Err(err) => {
                                toast_overlay.add_toast(adw::Toast::new(
                                    &tr(toast_lang, "Ошибка экспорта: {err}")
                                        .replace("{err}", &err.to_string()),
                                ));
                                return;
                            }
                        };
                        if let Err(err) = std::fs::write(&path, content) {
                            toast_overlay.add_toast(adw::Toast::new(
                                &tr(toast_lang, "Не удалось сохранить файл: {err}")
                                    .replace("{err}", &err.to_string()),
                            ));
                            return;
                        }
                        toast_overlay
                            .add_toast(adw::Toast::new(tr(toast_lang, "Настройки экспортированы")));
                    }
                });
            }
        });

        import_btn.connect_clicked({
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            let activation_group = activation_group.clone();
            let clear_log_btn = clear_log_btn.clone();
            let runtime_row = runtime_row.clone();
            move |_| {
                let activation_list = activation_list.clone();
                let Some(window) = controller.window_weak.upgrade() else {
                    return;
                };
                let dialog = gtk4::FileDialog::new();
                dialog.set_title(tr(controller.lang, "Импорт настроек"));
                dialog.set_accept_label(Some(tr(controller.lang, "Импортировать")));
                dialog.open(Some(&window), None::<&gio::Cancellable>, {
                    let controller = controller.clone();
                    let toast_overlay = toast_overlay.clone();
                    let activation_group = activation_group.clone();
                    let clear_log_btn = clear_log_btn.clone();
                    let runtime_row = runtime_row.clone();
                    move |res| {
                        let Ok(file) = res else {
                            return;
                        };
                        let Some(path) = file.path() else {
                            return;
                        };
                        let content = match std::fs::read_to_string(&path) {
                            Ok(content) => content,
                            Err(err) => {
                                toast_overlay.add_toast(adw::Toast::new(
                                    &tr(controller.lang, "Не удалось прочитать файл: {err}")
                                        .replace("{err}", &err.to_string()),
                                ));
                                return;
                            }
                        };
                        let mut config: Config = match serde_json::from_str(&content) {
                            Ok(cfg) => cfg,
                            Err(err) => {
                                toast_overlay.add_toast(adw::Toast::new(
                                    &tr(controller.lang, "Ошибка формата JSON: {err}")
                                        .replace("{err}", &err.to_string()),
                                ));
                                return;
                            }
                        };
                        config.normalize();
                        if let Err(err) = config.save() {
                            toast_overlay.add_toast(adw::Toast::new(
                                &tr(controller.lang, "Не удалось сохранить файл: {err}")
                                    .replace("{err}", &err.to_string()),
                            ));
                            return;
                        }
                        let toast_lang = config.language;
                        *controller.config.borrow_mut() = config;
                        refresh_profile_model(&controller);
                        refresh_monitor_profile_model(&controller);
                        let profile_index = controller.config.borrow().active_profile_index();
                        controller.apply_profile_to_ui(profile_index);
                        let config = controller.config.borrow();
                        populate_activation_log_group(
                            &activation_group,
                            &activation_list,
                            &config.activation_log,
                            controller.lang,
                        );
                        clear_log_btn.set_sensitive(!config.activation_log.is_empty());
                        runtime_row.set_subtitle(&format_runtime(
                            config.total_runtime_seconds,
                            controller.lang,
                        ));
                        controller.set_modified(false);
                        toast_overlay
                            .add_toast(adw::Toast::new(tr(toast_lang, "Настройки импортированы")));
                    }
                });
            }
        });

        about_button.connect_clicked({
            let controller = controller.clone();
            let window_weak = window.downgrade();
            move |_| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let version = env!("CARGO_PKG_VERSION");
                let display_version = version
                    .rsplit_once('.')
                    .map(|(base, patch)| if patch == "0" { base } else { version })
                    .unwrap_or(version);
                let dialog = adw::AboutDialog::builder()
                    .application_name("Vesper")
                    .application_icon(crate::ui::app_icon_name())
                    .developer_name("leocallidus")
                    .version(display_version)
                    .comments(tr(
                        controller.lang,
                        "Простой скринсейвер сделанный на Rust и GTK4",
                    ))
                    .build();
                dialog.add_link("leocallidus", "https://github.com/leocallidus/");
                dialog.present(&window);
            }
        });

        profile_dropdown.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |dropdown, _| {
                if controller.profile_update_guard.get() {
                    return;
                }
                let new_index = dropdown.selected() as usize;
                let current_index = controller.config.borrow().active_profile as usize;
                if new_index == current_index {
                    return;
                }
                controller.apply_ui_to_profile(current_index);
                controller.apply_profile_to_ui(new_index);
                controller.set_modified(true);
            }
        });

        profile_name_row.connect_notify_local(Some("text"), {
            let controller = controller.clone();
            move |row, _| {
                if controller.profile_update_guard.get() {
                    return;
                }
                let index = controller.active_profile_index();
                let name = row.text().to_string();
                if let Some(profile) = controller.config.borrow_mut().profiles.get_mut(index) {
                    profile.name = name.clone();
                }
                controller
                    .ui
                    .profile_model
                    .splice(index as u32, 1, &[name.as_str()]);
                refresh_monitor_profile_model(&controller);
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        add_profile_btn_widget.connect_clicked({
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            move |_| {
                let mut config = controller.config.borrow_mut();
                if config.profiles.len() >= MAX_PROFILES {
                    toast_overlay.add_toast(adw::Toast::new(tr(
                        controller.lang,
                        "Достигнут лимит профилей",
                    )));
                    return;
                }
                let new_index = config.profiles.len();
                let name = profile_name(controller.lang, new_index + 1);
                config.profiles.push(SettingsProfile::new(name.clone()));
                controller.ui.profile_model.append(&name);
                drop(config);
                refresh_monitor_profile_model(&controller);
                controller.apply_profile_to_ui(new_index);
                controller.set_modified(true);
            }
        });

        let reset_handler = Rc::new({
            let controller = controller.clone();
            let window_weak = window.downgrade();
            move || {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let dialog = adw::MessageDialog::builder()
                    .transient_for(&window)
                    .modal(true)
                    .heading(tr(controller.lang, "Сбросить профиль?"))
                    .body(tr(
                        controller.lang,
                        "Настройки текущего профиля будут сброшены к значениям по умолчанию.",
                    ))
                    .build();
                dialog.add_response("cancel", tr(controller.lang, "Отмена"));
                dialog.add_response("reset", tr(controller.lang, "Сбросить"));
                dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
                dialog.connect_response(None, {
                    let controller = controller.clone();
                    move |dialog, response| {
                        if response == "reset" {
                            let index = controller.active_profile_index();
                            let mut config = controller.config.borrow_mut();
                            if index < config.profiles.len() {
                                let name = config.profiles[index].name.clone();
                                let profile = SettingsProfile::new(name);
                                config.profiles[index] = profile;
                            }
                            drop(config);
                            controller.apply_profile_to_ui(index);
                            controller.set_modified(true);
                        }
                        dialog.close();
                    }
                });
                dialog.present();
            }
        });

        reset_button.connect_clicked({
            let reset_handler = reset_handler.clone();
            move |_| (reset_handler)()
        });

        reset_profile_button.connect_clicked({
            let reset_handler = reset_handler.clone();
            move |_| (reset_handler)()
        });

        delete_button.connect_clicked({
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            let window_weak = window.downgrade();
            move |_| {
                if controller.config.borrow().profiles.len() <= 1 {
                    toast_overlay.add_toast(adw::Toast::new(tr(
                        controller.lang,
                        "Нельзя удалить последний профиль",
                    )));
                    return;
                }
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let dialog = adw::MessageDialog::builder()
                    .transient_for(&window)
                    .modal(true)
                    .heading(tr(controller.lang, "Удалить профиль?"))
                    .body(tr(
                        controller.lang,
                        "Профиль будет удалён без возможности восстановления.",
                    ))
                    .build();
                dialog.add_response("cancel", tr(controller.lang, "Отмена"));
                dialog.add_response("delete", tr(controller.lang, "Удалить"));
                dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
                dialog.connect_response(None, {
                    let controller = controller.clone();
                    let toast_overlay = toast_overlay.clone();
                    move |dialog, response| {
                        if response == "delete" {
                            let mut config = controller.config.borrow_mut();
                            if config.profiles.len() <= 1 {
                                toast_overlay.add_toast(adw::Toast::new(tr(
                                    controller.lang,
                                    "Нельзя удалить последний профиль",
                                )));
                                dialog.close();
                                return;
                            }
                            let index = controller.active_profile_index();
                            if index < config.profiles.len() {
                                let mut new_overrides = Vec::new();
                                for mut o in config.monitor_profile_overrides.drain(..) {
                                    let idx = o.profile_index as usize;
                                    if idx == index {
                                        continue;
                                    }
                                    if idx > index {
                                        o.profile_index = o.profile_index.saturating_sub(1);
                                    }
                                    new_overrides.push(o);
                                }
                                config.monitor_profile_overrides = new_overrides;
                                config.profiles.remove(index);
                                controller.ui.profile_model.remove(index as u32);
                                let new_index = if index > 0 { index - 1 } else { 0 };
                                config.active_profile = new_index as u8;
                                drop(config);
                                refresh_monitor_profile_model(&controller);
                                controller.apply_profile_to_ui(new_index);
                                controller.set_modified(true);
                            }
                        }
                        dialog.close();
                    }
                });
                dialog.present();
            }
        });

        inactivity_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        mouse_wake_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        fade_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        start_minimized_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
            }
        });

        tray_icon_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            let sender = sender.clone();
            move |_, _| {
                if controller.profile_update_guard.get() {
                    return;
                }
                let tray_enabled = controller.ui.tray_icon_switch.is_active();
                controller.ui.tray_click_row.set_sensitive(tray_enabled);
                if !tray_enabled && controller.ui.start_minimized_switch.is_active() {
                    toast_overlay.add_toast(adw::Toast::new(tr(
                        controller.lang,
                        "Без трея и с «Запускать в фоне» приложение может быть трудно открыть",
                    )));
                }
                controller.mark_modified();
                let _ = sender.send(AppMessage::SetTrayIconVisible(tray_enabled));
            }
        });

        tray_click_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                if controller.profile_update_guard.get() {
                    return;
                }
                controller.mark_modified();
            }
        });

        autostart_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            move |switch, _| {
                if controller.profile_update_guard.get() {
                    return;
                }
                let enabled = switch.is_active();
                if let Err(err) = crate::autostart::set_autostart_enabled(enabled) {
                    toast_overlay.add_toast(adw::Toast::new(
                        &tr(controller.lang, "Не удалось обновить автозапуск: {err}")
                            .replace("{err}", &err),
                    ));
                    controller.profile_update_guard.set(true);
                    switch.set_active(!enabled);
                    controller.profile_update_guard.set(false);
                }
            }
        });

        language_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            move |_, _| {
                if controller.profile_update_guard.get() {
                    return;
                }
                let selected = language_from_index(controller.ui.language_row.selected());
                if selected != controller.config.borrow().language {
                    toast_overlay.add_toast(adw::Toast::new(tr(
                        controller.lang,
                        "Язык будет применён после перезапуска приложения",
                    )));
                }
                controller.mark_modified();
            }
        });

        setup_hotkey_capture(
            &start_hotkey_entry,
            crate::config::DEFAULT_HOTKEY_START,
            controller.clone(),
            toast_overlay.clone(),
        );
        setup_hotkey_capture(
            &stop_hotkey_entry,
            crate::config::DEFAULT_HOTKEY_STOP,
            controller.clone(),
            toast_overlay.clone(),
        );
        setup_hotkey_capture(
            &panic_hotkey_entry,
            crate::config::DEFAULT_HOTKEY_PANIC,
            controller.clone(),
            toast_overlay.clone(),
        );

        mode_selector.connect_selected_children_changed({
            let controller = controller.clone();
            move |_| {
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                if let Some(path) = file_row_path(&controller.ui.file_row) {
                    let expects_dir = mode_idx == 6;
                    let expects_file = matches!(mode_idx, 4 | 5);
                    let should_clear =
                        (expects_dir && path.is_file()) || (expects_file && path.is_dir());
                    if should_clear {
                        let empty_label = if expects_dir {
                            tr(controller.lang, "Папка не выбрана")
                        } else {
                            tr(controller.lang, "Файл не выбран")
                        };
                        set_file_row_path(
                            &controller.ui.file_row,
                            &controller.ui.file_info_row,
                            None,
                            empty_label,
                            controller.lang,
                        );
                    }
                }
                update_file_info_row(
                    &controller.ui.file_info_row,
                    file_row_path(&controller.ui.file_row),
                    mode_idx,
                    controller.lang,
                );
                if !matches!(mode_idx, 4 | 5) {
                    controller.ui.selected_media_preview.borrow_mut().take();
                    controller.ui.remove_media_button.set_sensitive(false);
                }
                let is_water = mode_idx == 2
                    && matches!(
                        content::pattern_from_index(controller.ui.pattern_row.selected()),
                        AnimatedPattern::WaterRipples
                    );
                controller.ui.water_ripples_bg_row.set_visible(is_water);
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        shadertoy_source_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            move |row, _| {
                if controller.profile_update_guard.get() {
                    return;
                }
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                if mode_idx != 9 {
                    return;
                }

                if row.selected() == 0 {
                    if let Some(saved) = controller.ui.shadertoy_manual_path.borrow().clone() {
                        set_file_row_path(
                            &controller.ui.file_row,
                            &controller.ui.file_info_row,
                            Some(saved.as_str()),
                            tr(controller.lang, "Файл не выбран"),
                            controller.lang,
                        );
                    }
                } else {
                    if let Some(path) = file_row_path(&controller.ui.file_row) {
                        *controller.ui.shadertoy_manual_path.borrow_mut() =
                            Some(path.to_string_lossy().to_string());
                    }

                    reload_shadertoy_packs_into_models(&controller);
                    if controller.ui.shadertoy_pack_model.n_items() == 0 {
                        toast_overlay.add_toast(adw::Toast::new(tr(
                            controller.lang,
                            "Шейдерпаки не найдены",
                        )));
                        set_file_row_path(
                            &controller.ui.file_row,
                            &controller.ui.file_info_row,
                            None,
                            tr(controller.lang, "Файл не выбран"),
                            controller.lang,
                        );
                    } else {
                        let pack_idx = controller.ui.shadertoy_pack_row.selected() as usize;
                        rebuild_shadertoy_shader_model(&controller, pack_idx);
                        if let Some(img) = shadertoy_selected_shader_image_path(&controller) {
                            let img_str = img.to_string_lossy().to_string();
                            set_file_row_path(
                                &controller.ui.file_row,
                                &controller.ui.file_info_row,
                                Some(img_str.as_str()),
                                tr(controller.lang, "Файл не выбран"),
                                controller.lang,
                            );
                        }
                    }
                }

                update_file_info_row(
                    &controller.ui.file_info_row,
                    file_row_path(&controller.ui.file_row),
                    mode_idx,
                    controller.lang,
                );
                set_content_mode_visibility(
                    &controller.ui.stream_url_row,
                    &controller.ui.shadertoy_source_row,
                    &controller.ui.shadertoy_pack_row,
                    &controller.ui.shadertoy_shader_row,
                    &controller.ui.file_row,
                    &controller.ui.file_info_row,
                    &controller.ui.shader_check_row,
                    &controller.ui.shadertoy_interaction_row,
                    &controller.ui.shadertoy_hide_cursor_row,
                    &controller.ui.shadertoy_sound_row,
                    &controller.ui.slideshow_interval_row,
                    &controller.ui.mute_row,
                    &controller.ui.volume_row,
                    &controller.ui.random_row,
                    &controller.ui.media_list_row,
                    &controller.ui.media_list_box,
                    mode_idx,
                );
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        shadertoy_pack_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            move |row, _| {
                if controller.profile_update_guard.get() {
                    return;
                }
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                if mode_idx != 9 || controller.ui.shadertoy_source_row.selected() != 1 {
                    return;
                }

                rebuild_shadertoy_shader_model(&controller, row.selected() as usize);
                if controller.ui.shadertoy_shader_model.n_items() == 0 {
                    toast_overlay.add_toast(adw::Toast::new(tr(
                        controller.lang,
                        "В шейдерпаке нет шейдеров",
                    )));
                } else {
                    // Force selection refresh: after swapping the model, AdwComboRow can keep showing
                    // a stale "selected item" from the previous pack if the index doesn't change.
                    controller
                        .ui
                        .shadertoy_shader_row
                        .set_selected(gtk4::INVALID_LIST_POSITION);
                    controller.ui.shadertoy_shader_row.set_selected(0);

                    if let Some(img) = shadertoy_selected_shader_image_path(&controller) {
                        let img_str = img.to_string_lossy().to_string();
                        set_file_row_path(
                            &controller.ui.file_row,
                            &controller.ui.file_info_row,
                            Some(img_str.as_str()),
                            tr(controller.lang, "Файл не выбран"),
                            controller.lang,
                        );
                    }
                }

                update_file_info_row(
                    &controller.ui.file_info_row,
                    file_row_path(&controller.ui.file_row),
                    mode_idx,
                    controller.lang,
                );
                set_content_mode_visibility(
                    &controller.ui.stream_url_row,
                    &controller.ui.shadertoy_source_row,
                    &controller.ui.shadertoy_pack_row,
                    &controller.ui.shadertoy_shader_row,
                    &controller.ui.file_row,
                    &controller.ui.file_info_row,
                    &controller.ui.shader_check_row,
                    &controller.ui.shadertoy_interaction_row,
                    &controller.ui.shadertoy_hide_cursor_row,
                    &controller.ui.shadertoy_sound_row,
                    &controller.ui.slideshow_interval_row,
                    &controller.ui.mute_row,
                    &controller.ui.volume_row,
                    &controller.ui.random_row,
                    &controller.ui.media_list_row,
                    &controller.ui.media_list_box,
                    mode_idx,
                );
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        shadertoy_shader_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |_, _| {
                if controller.profile_update_guard.get() {
                    return;
                }
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                if mode_idx != 9 || controller.ui.shadertoy_source_row.selected() != 1 {
                    return;
                }
                if let Some(img) = shadertoy_selected_shader_image_path(&controller) {
                    let img_str = img.to_string_lossy().to_string();
                    set_file_row_path(
                        &controller.ui.file_row,
                        &controller.ui.file_info_row,
                        Some(img_str.as_str()),
                        tr(controller.lang, "Файл не выбран"),
                        controller.lang,
                    );
                }
                update_file_info_row(
                    &controller.ui.file_info_row,
                    file_row_path(&controller.ui.file_row),
                    mode_idx,
                    controller.lang,
                );
                set_content_mode_visibility(
                    &controller.ui.stream_url_row,
                    &controller.ui.shadertoy_source_row,
                    &controller.ui.shadertoy_pack_row,
                    &controller.ui.shadertoy_shader_row,
                    &controller.ui.file_row,
                    &controller.ui.file_info_row,
                    &controller.ui.shader_check_row,
                    &controller.ui.shadertoy_interaction_row,
                    &controller.ui.shadertoy_hide_cursor_row,
                    &controller.ui.shadertoy_sound_row,
                    &controller.ui.slideshow_interval_row,
                    &controller.ui.mute_row,
                    &controller.ui.volume_row,
                    &controller.ui.random_row,
                    &controller.ui.media_list_row,
                    &controller.ui.media_list_box,
                    mode_idx,
                );
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        color_button.connect_rgba_notify({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        gradient_start_button.connect_rgba_notify({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        gradient_end_button.connect_rgba_notify({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        pattern_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |_, _| {
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                let is_water = mode_idx == 2
                    && matches!(
                        content::pattern_from_index(controller.ui.pattern_row.selected()),
                        AnimatedPattern::WaterRipples
                    );
                controller.ui.water_ripples_bg_row.set_visible(is_water);
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        pattern_speed_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        pattern_density_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        pattern_theme_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        web_url_row.connect_notify_local(Some("text"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        web_interaction_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        stream_url_row.connect_notify_local(Some("text"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        slideshow_interval_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        mute_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        shadertoy_interaction_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        shadertoy_hide_cursor_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        shadertoy_sound_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        video_volume_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        preview_pause_switch.connect_notify_local(Some("active"), {
            let preview_media = preview_media.clone();
            move |switch, _| {
                if let Some(media) = preview_media.borrow().as_ref() {
                    media.set_playing(!switch.is_active());
                }
            }
        });

        random_media_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        media_list_box.connect_row_selected({
            let controller = controller.clone();
            move |_, row| {
                if let Some(row) = row {
                    if let Some(path) = media_list_row_path(&row) {
                        *controller.ui.selected_media_preview.borrow_mut() = Some(path);
                        controller.ui.remove_media_button.set_sensitive(true);
                    } else {
                        controller.ui.selected_media_preview.borrow_mut().take();
                        controller.ui.remove_media_button.set_sensitive(false);
                    }
                } else {
                    controller.ui.selected_media_preview.borrow_mut().take();
                    controller.ui.remove_media_button.set_sensitive(false);
                }
                (controller.update_preview)();
            }
        });

        file_button.connect_clicked({
            let controller = controller.clone();
            let window_weak = window.downgrade();
            let toast_overlay = toast_overlay.clone();
            move |_| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                if mode_idx == 6 {
                    let dialog = gtk4::FileDialog::new();
                    dialog.set_title(tr(controller.lang, "Выбрать файл"));
                    if let Some(path) = file_row_path(&controller.ui.file_row) {
                        let initial = if path.is_dir() {
                            path.clone()
                        } else {
                            path.parent().unwrap_or(path.as_path()).to_path_buf()
                        };
                        dialog.set_initial_folder(Some(&gio::File::for_path(&initial)));
                    }
                    dialog.select_folder(Some(&window), None::<&gio::Cancellable>, {
                        let controller = controller.clone();
                        let toast_overlay = toast_overlay.clone();
                        move |res| {
                            let Ok(folder) = res else {
                                return;
                            };
                            let Some(path) = folder.path() else {
                                return;
                            };
                            if let Err(msg) = validate_media_path(
                                &path,
                                content::MediaKind::ImageFolder,
                                false,
                                controller.lang,
                            ) {
                                toast_overlay.add_toast(adw::Toast::new(&msg));
                                return;
                            }
                            let path_str = path.to_string_lossy().to_string();
                            set_file_row_path(
                                &controller.ui.file_row,
                                &controller.ui.file_info_row,
                                Some(path_str.as_str()),
                                tr(controller.lang, "Папка не выбрана"),
                                controller.lang,
                            );
                            update_file_info_row(
                                &controller.ui.file_info_row,
                                Some(path),
                                mode_idx,
                                controller.lang,
                            );
                            set_content_mode_visibility(
                                &controller.ui.stream_url_row,
                                &controller.ui.shadertoy_source_row,
                                &controller.ui.shadertoy_pack_row,
                                &controller.ui.shadertoy_shader_row,
                                &controller.ui.file_row,
                                &controller.ui.file_info_row,
                                &controller.ui.shader_check_row,
                                &controller.ui.shadertoy_interaction_row,
                                &controller.ui.shadertoy_hide_cursor_row,
                                &controller.ui.shadertoy_sound_row,
                                &controller.ui.slideshow_interval_row,
                                &controller.ui.mute_row,
                                &controller.ui.volume_row,
                                &controller.ui.random_row,
                                &controller.ui.media_list_row,
                                &controller.ui.media_list_box,
                                mode_idx,
                            );
                            controller.mark_modified();
                            (controller.update_preview)();
                            (controller.update_status)();
                        }
                    });
                    return;
                }
                if mode_idx == 8 {
                    let dialog = gtk4::FileDialog::new();
                    dialog.set_title(tr(controller.lang, "Выбрать файл"));
                    if let Some(path) = file_row_path(&controller.ui.file_row) {
                        dialog.set_initial_file(Some(&gio::File::for_path(path)));
                    }
                    dialog.open(Some(&window), None::<&gio::Cancellable>, {
                        let controller = controller.clone();
                        let toast_overlay = toast_overlay.clone();
                        move |res| {
                            let Ok(file) = res else {
                                return;
                            };
                            let Some(path) = file.path() else {
                                return;
                            };
                            if let Err(msg) =
                                crate::ui::settings::content::validate_python_script_path(
                                    &path,
                                    controller.lang,
                                )
                            {
                                toast_overlay.add_toast(adw::Toast::new(&msg));
                                return;
                            }
                            let path_str = path.to_string_lossy().to_string();
                            set_file_row_path(
                                &controller.ui.file_row,
                                &controller.ui.file_info_row,
                                Some(path_str.as_str()),
                                tr(controller.lang, "Файл не выбран"),
                                controller.lang,
                            );
                            update_file_info_row(
                                &controller.ui.file_info_row,
                                Some(path),
                                mode_idx,
                                controller.lang,
                            );
                            set_content_mode_visibility(
                                &controller.ui.stream_url_row,
                                &controller.ui.shadertoy_source_row,
                                &controller.ui.shadertoy_pack_row,
                                &controller.ui.shadertoy_shader_row,
                                &controller.ui.file_row,
                                &controller.ui.file_info_row,
                                &controller.ui.shader_check_row,
                                &controller.ui.shadertoy_interaction_row,
                                &controller.ui.shadertoy_hide_cursor_row,
                                &controller.ui.shadertoy_sound_row,
                                &controller.ui.slideshow_interval_row,
                                &controller.ui.mute_row,
                                &controller.ui.volume_row,
                                &controller.ui.random_row,
                                &controller.ui.media_list_row,
                                &controller.ui.media_list_box,
                                mode_idx,
                            );
                            controller.mark_modified();
                            (controller.update_preview)();
                            (controller.update_status)();
                        }
                    });
                    return;
                }
                if mode_idx == 9 {
                    let dialog = gtk4::FileDialog::new();
                    dialog.set_title(tr(controller.lang, "Выбрать файл"));
                    if let Some(path) = file_row_path(&controller.ui.file_row) {
                        dialog.set_initial_file(Some(&gio::File::for_path(path)));
                    }
                    dialog.open(Some(&window), None::<&gio::Cancellable>, {
                        let controller = controller.clone();
                        let toast_overlay = toast_overlay.clone();
                        move |res| {
                            let Ok(file) = res else {
                                return;
                            };
                            let Some(path) = file.path() else {
                                return;
                            };
                            if let Err(msg) =
                                crate::ui::settings::content::validate_shadertoy_shader_path(
                                    &path,
                                    controller.lang,
                                )
                            {
                                toast_overlay.add_toast(adw::Toast::new(&msg));
                                return;
                            }
                            let path_str = path.to_string_lossy().to_string();
                            set_file_row_path(
                                &controller.ui.file_row,
                                &controller.ui.file_info_row,
                                Some(path_str.as_str()),
                                tr(controller.lang, "Файл не выбран"),
                                controller.lang,
                            );
                            update_file_info_row(
                                &controller.ui.file_info_row,
                                Some(path),
                                mode_idx,
                                controller.lang,
                            );
                            set_content_mode_visibility(
                                &controller.ui.stream_url_row,
                                &controller.ui.shadertoy_source_row,
                                &controller.ui.shadertoy_pack_row,
                                &controller.ui.shadertoy_shader_row,
                                &controller.ui.file_row,
                                &controller.ui.file_info_row,
                                &controller.ui.shader_check_row,
                                &controller.ui.shadertoy_interaction_row,
                                &controller.ui.shadertoy_hide_cursor_row,
                                &controller.ui.shadertoy_sound_row,
                                &controller.ui.slideshow_interval_row,
                                &controller.ui.mute_row,
                                &controller.ui.volume_row,
                                &controller.ui.random_row,
                                &controller.ui.media_list_row,
                                &controller.ui.media_list_box,
                                mode_idx,
                            );
                            controller.mark_modified();
                            (controller.update_preview)();
                            (controller.update_status)();
                        }
                    });
                    return;
                }
                if !matches!(mode_idx, 4 | 5) {
                    toast_overlay.add_toast(adw::Toast::new(tr(
                        controller.lang,
                        "Сначала выберите режим",
                    )));
                    return;
                }
                let kind = if mode_idx == 4 {
                    content::MediaKind::Image
                } else {
                    content::MediaKind::Video
                };
                let dialog = gtk4::FileDialog::new();
                dialog.set_title(tr(controller.lang, "Выбрать файл"));
                if let Some(path) = file_row_path(&controller.ui.file_row) {
                    dialog.set_initial_file(Some(&gio::File::for_path(path)));
                }
                dialog.open(Some(&window), None::<&gio::Cancellable>, {
                    let controller = controller.clone();
                    let toast_overlay = toast_overlay.clone();
                    move |res| {
                        let Ok(file) = res else {
                            return;
                        };
                        let Some(path) = file.path() else {
                            return;
                        };
                        if let Err(msg) = validate_media_path(&path, kind, false, controller.lang) {
                            toast_overlay.add_toast(adw::Toast::new(&msg));
                            return;
                        }
                        let path_str = path.to_string_lossy().to_string();
                        set_file_row_path(
                            &controller.ui.file_row,
                            &controller.ui.file_info_row,
                            Some(path_str.as_str()),
                            tr(controller.lang, "Файл не выбран"),
                            controller.lang,
                        );
                        update_file_info_row(
                            &controller.ui.file_info_row,
                            Some(path),
                            mode_idx,
                            controller.lang,
                        );
                        set_content_mode_visibility(
                            &controller.ui.stream_url_row,
                            &controller.ui.shadertoy_source_row,
                            &controller.ui.shadertoy_pack_row,
                            &controller.ui.shadertoy_shader_row,
                            &controller.ui.file_row,
                            &controller.ui.file_info_row,
                            &controller.ui.shader_check_row,
                            &controller.ui.shadertoy_interaction_row,
                            &controller.ui.shadertoy_hide_cursor_row,
                            &controller.ui.shadertoy_sound_row,
                            &controller.ui.slideshow_interval_row,
                            &controller.ui.mute_row,
                            &controller.ui.volume_row,
                            &controller.ui.random_row,
                            &controller.ui.media_list_row,
                            &controller.ui.media_list_box,
                            mode_idx,
                        );
                        controller.mark_modified();
                        (controller.update_preview)();
                        (controller.update_status)();
                    }
                });
            }
        });

        shader_check_button.connect_clicked({
            let controller = controller.clone();
            let window_weak = window.downgrade();
            let toast_overlay = toast_overlay.clone();
            move |_| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                if mode_idx != 9 {
                    toast_overlay.add_toast(adw::Toast::new(tr(
                        controller.lang,
                        "Сначала выберите режим",
                    )));
                    return;
                }

                let source_idx = controller.ui.shadertoy_source_row.selected();
                let selected_pack_shader: Option<(String, String)> = if source_idx == 1 {
                    let packs = controller.ui.shadertoy_packs.borrow();
                    let pack = packs.get(controller.ui.shadertoy_pack_row.selected() as usize);
                    let shader = pack.and_then(|p| p.shaders.get(controller.ui.shadertoy_shader_row.selected() as usize));
                    match (pack, shader) {
                        (Some(pack), Some(shader)) => Some((pack.name.clone(), shader.name.clone())),
                        _ => None,
                    }
                } else {
                    None
                };

                let Some(path) = file_row_path(&controller.ui.file_row) else {
                    toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Файл не выбран")));
                    return;
                };
                if let Err(msg) =
                    crate::ui::settings::content::validate_shadertoy_shader_path(&path, controller.lang)
                {
                    toast_overlay.add_toast(adw::Toast::new(&msg));
                    return;
                }

                let detected = crate::ui::settings::content::detect_shadertoy_files(&path);
                let mut body = String::new();
                if let Some((pack_name, shader_name)) = selected_pack_shader {
                    body.push_str(&format!(
                        "{}: {}\n{}: {}\n\n",
                        tr(controller.lang, "Шейдерпак"),
                        pack_name,
                        tr(controller.lang, "Шейдер"),
                        shader_name
                    ));
                }
                body.push_str(&format!(
                    "{}: {}\n\n",
                    tr(controller.lang, "Папка"),
                    detected.base_dir.to_string_lossy()
                ));
                body.push_str(&format!(
                    "Image: {}\n",
                    detected
                        .image
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .unwrap_or(tr(controller.lang, "Нет данных"))
                ));
                body.push_str(&format!(
                    "Common: {}\n",
                    detected
                        .common
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .unwrap_or("-")
                ));
                for (idx, label) in ["BufferA", "BufferB", "BufferC", "BufferD"].iter().enumerate()
                {
                    let name = detected.buffers[idx]
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .unwrap_or("-");
                    body.push_str(&format!("{label}: {name}\n"));
                }
                let sound_name = detected
                    .sound
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("-");
                body.push_str(&format!("Sound: {sound_name}\n"));

                let dialog = adw::MessageDialog::builder()
                    .transient_for(&window)
                    .modal(true)
                    .heading(tr(controller.lang, "Проверка шейдеров"))
                    .body(&body)
                    .build();
                dialog.add_response("ok", tr(controller.lang, "ОК"));
                dialog.present();
            }
        });

        water_ripples_bg_button.connect_clicked({
            let controller = controller.clone();
            let window_weak = window.downgrade();
            let toast_overlay = toast_overlay.clone();
            move |_| {
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                let is_water = mode_idx == 2
                    && matches!(
                        content::pattern_from_index(controller.ui.pattern_row.selected()),
                        AnimatedPattern::WaterRipples
                    );
                if !is_water {
                    toast_overlay.add_toast(adw::Toast::new(tr(
                        controller.lang,
                        "Сначала выберите паттерн: Водная рябь",
                    )));
                    return;
                }
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let dialog = gtk4::FileDialog::new();
                dialog.set_title(tr(controller.lang, "Выбрать файл"));
                if let Some(path) = content::file_row_path(&controller.ui.water_ripples_bg_row) {
                    dialog.set_initial_file(Some(&gio::File::for_path(path)));
                }
                dialog.open(Some(&window), None::<&gio::Cancellable>, {
                    let controller = controller.clone();
                    let toast_overlay = toast_overlay.clone();
                    move |res| {
                        let Ok(file) = res else {
                            return;
                        };
                        let Some(path) = file.path() else {
                            return;
                        };
                        if let Err(msg) = validate_media_path(
                            &path,
                            content::MediaKind::Image,
                            false,
                            controller.lang,
                        ) {
                            toast_overlay.add_toast(adw::Toast::new(&msg));
                            return;
                        }
                        let path_str = path.to_string_lossy().to_string();
                        controller.ui.water_ripples_bg_row.set_subtitle(&path_str);
                        controller
                            .ui
                            .water_ripples_bg_row
                            .set_tooltip_text(Some(&path_str));
                        controller
                            .ui
                            .water_ripples_bg_clear_button
                            .set_sensitive(true);
                        controller.mark_modified();
                        (controller.update_preview)();
                        (controller.update_status)();
                    }
                });
            }
        });

        water_ripples_bg_clear_button.connect_clicked({
            let controller = controller.clone();
            move |_| {
                controller
                    .ui
                    .water_ripples_bg_row
                    .set_subtitle(tr(controller.lang, "Файл не выбран"));
                controller.ui.water_ripples_bg_row.set_tooltip_text(None);
                controller
                    .ui
                    .water_ripples_bg_clear_button
                    .set_sensitive(false);
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        add_media_button.connect_clicked({
            let controller = controller.clone();
            let window_weak = window.downgrade();
            let toast_overlay = toast_overlay.clone();
            move |_| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                if !matches!(mode_idx, 4 | 5) {
                    toast_overlay.add_toast(adw::Toast::new(tr(
                        controller.lang,
                        "Список доступен только для изображения или видео",
                    )));
                    return;
                }
                let kind = if mode_idx == 4 {
                    content::MediaKind::Image
                } else {
                    content::MediaKind::Video
                };
                let dialog = gtk4::FileDialog::new();
                dialog.set_title(tr(controller.lang, "Добавить медиафайлы"));
                dialog.open_multiple(Some(&window), None::<&gio::Cancellable>, {
                    let controller = controller.clone();
                    let toast_overlay = toast_overlay.clone();
                    move |res| {
                        let Ok(list_model) = res else {
                            return;
                        };
                        let mut skipped = false;
                        let mut added = 0usize;
                        let mut files = controller.ui.media_files.borrow_mut();
                        for i in 0..list_model.n_items() {
                            let Some(item) = list_model.item(i) else {
                                continue;
                            };
                            let file = item.downcast::<gio::File>().ok();
                            let Some(file) = file else {
                                continue;
                            };
                            let Some(path) = file.path() else {
                                continue;
                            };
                            if let Err(_) = validate_media_path(&path, kind, false, controller.lang)
                            {
                                skipped = true;
                                continue;
                            }
                            let path_str = path.to_string_lossy().to_string();
                            if files.iter().any(|p| p == &path_str) {
                                continue;
                            }
                            files.push(path_str);
                            added += 1;
                        }
                        drop(files);
                        if added > 0 {
                            sync_media_list_box(
                                &controller.ui.media_list_box,
                                &controller.ui.media_files.borrow(),
                                controller.lang,
                            );
                            controller.mark_modified();
                            (controller.update_status)();
                            (controller.update_preview)();
                        }
                        if skipped {
                            toast_overlay.add_toast(adw::Toast::new(tr(
                                controller.lang,
                                "Некоторые файлы пропущены",
                            )));
                        }
                    }
                });
            }
        });

        remove_media_button.connect_clicked({
            let controller = controller.clone();
            move |_| {
                let Some(row) = controller.ui.media_list_box.selected_row() else {
                    return;
                };
                let Some(path) = media_list_row_path(&row) else {
                    return;
                };
                let mut files = controller.ui.media_files.borrow_mut();
                if let Some(pos) = files.iter().position(|p| p == &path) {
                    files.remove(pos);
                }
                drop(files);
                *controller.ui.selected_media_preview.borrow_mut() = None;
                sync_media_list_box(
                    &controller.ui.media_list_box,
                    &controller.ui.media_files.borrow(),
                    controller.lang,
                );
                controller.ui.remove_media_button.set_sensitive(false);
                controller.mark_modified();
                (controller.update_status)();
                (controller.update_preview)();
            }
        });

        clock_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        now_playing_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        now_playing_position_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |_, _| {
                appearance::apply_widget_position(
                    &controller.ui.now_playing_preview_box,
                    appearance::clock_position_from_index(
                        controller.ui.now_playing_position_row.selected(),
                    ),
                    16,
                );
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        now_playing_move_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller
                    .ui
                    .now_playing_move_interval_row
                    .set_visible(controller.ui.now_playing_move_switch.is_active());
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        now_playing_move_interval_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        rss_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        rss_speed_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        rss_refresh_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        rss_add_button.connect_clicked({
            let controller = controller.clone();
            move |_| add_rss_feed_from_entry(&controller)
        });

        system_stats_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        system_stats_position_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        system_stats_move_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller
                    .ui
                    .system_stats_move_interval_row
                    .set_visible(controller.ui.system_stats_move_switch.is_active());
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        system_stats_move_interval_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        clock_format_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |row, _| {
                let selected = row.selected() as usize;
                if selected < CLOCK_FORMAT_PATTERNS.len() {
                    controller
                        .ui
                        .clock_format_entry
                        .set_text(CLOCK_FORMAT_PATTERNS[selected]);
                    controller.ui.clock_format_entry.set_visible(false);
                } else {
                    controller
                        .ui
                        .clock_format_entry
                        .set_visible(!controller.ui.clock_two_lines_switch.is_active());
                }
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        clock_format_entry.connect_notify_local(Some("text"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        clock_two_lines_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                let enabled = controller.ui.clock_two_lines_switch.is_active();
                controller.ui.clock_format_row.set_sensitive(!enabled);

                let preset_index = appearance::clock_format_preset_index(&clock_format_from_ui(
                    &controller.ui.clock_format_row,
                    &controller.ui.clock_format_entry,
                ));
                controller
                    .ui
                    .clock_format_entry
                    .set_visible(!enabled && preset_index.is_none());

                controller.ui.clock_time_format_entry.set_visible(enabled);
                controller.ui.clock_date_format_entry.set_visible(enabled);

                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        clock_time_format_entry.connect_notify_local(Some("text"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        clock_date_format_entry.connect_notify_local(Some("text"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        clock_position_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        clock_move_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller
                    .ui
                    .clock_move_interval_row
                    .set_visible(controller.ui.clock_move_switch.is_active());
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        clock_move_interval_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        clock_size_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
            }
        });

        inhibit_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            let sender = sender.clone();
            move |_, _| {
                let active = controller.ui.inhibit_switch.is_active();
                controller.mark_modified();
                (controller.update_status)();
                let _ = sender.send(AppMessage::ToggleInhibitSleep(active));
            }
        });

        ignore_idle_inhibitors_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        power_integration_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        integrated_lock_screen_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                if controller.profile_update_guard.get() {
                    return;
                }
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        mpris_pause_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        app_inhibit_add_button.connect_clicked({
            let controller = controller.clone();
            move |_| {
                add_inhibit_app_from_entry(&controller);
            }
        });

        app_inhibit_refresh_button.connect_clicked({
            let controller = controller.clone();
            move |_| {
                sync_app_inhibit_list(&controller);
            }
        });

        save_button.connect_clicked({
            let window_weak = window.downgrade();
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            let sender = sender.clone();
            move |_| {
                let Some(win) = window_weak.upgrade() else {
                    return;
                };
                save_settings(&controller, &win, &toast_overlay, &sender);
            }
        });

        window.connect_close_request({
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            let sender = sender.clone();
            move |win| {
                if !controller.modified.get() {
                    persist_window_geometry(win, &controller.config, controller.modified.as_ref());
                    return glib::Propagation::Proceed;
                }
                let dialog = adw::MessageDialog::builder()
                    .transient_for(win)
                    .modal(true)
                    .heading(tr(controller.lang, "Сохранить изменения?"))
                    .body(tr(controller.lang, "Есть несохранённые изменения."))
                    .build();
                dialog.add_response("cancel", tr(controller.lang, "Отмена"));
                dialog.add_response("discard", tr(controller.lang, "Не сохранять"));
                dialog.add_response("save", tr(controller.lang, "Сохранить"));
                dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
                dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
                dialog.connect_response(None, {
                    let controller = controller.clone();
                    let toast_overlay = toast_overlay.clone();
                    let sender = sender.clone();
                    let win_weak = win.downgrade();
                    move |dialog, response| {
                        if response == "save" {
                            if let Some(win) = win_weak.upgrade() {
                                save_settings(&controller, &win, &toast_overlay, &sender);
                                win.close();
                            }
                        } else if response == "discard" {
                            *controller.config.borrow_mut() = Config::load();
                            controller.set_modified(false);
                            if let Some(win) = win_weak.upgrade() {
                                win.close();
                            }
                        }
                        dialog.close();
                    }
                });
                dialog.present();
                glib::Propagation::Stop
            }
        });

        controller.apply_profile_to_ui(active_profile_index_init);

        Self { window }
    }

    pub fn show(&self) {
        self.window.present();
    }
}

fn update_preview_tooltip(
    preview_frame: &Frame,
    mode: u32,
    preview_path: Option<&std::path::Path>,
    lang: Language,
) {
    if matches!(mode, 4 | 5 | 6) {
        if let Some(path) = preview_path {
            preview_frame.set_tooltip_text(Some(
                &tr(lang, "{path}\nНажмите, чтобы скопировать")
                    .replace("{path}", &path.to_string_lossy()),
            ));
            return;
        }
    }
    preview_frame.set_tooltip_text(None);
}

fn capture_window_geometry(window: &adw::Window) -> Option<WindowGeometry> {
    let (w, h) = (window.width(), window.height());
    if w <= 0 || h <= 0 {
        return None;
    }
    let mut geom = WindowGeometry {
        width: w,
        height: h,
        x: None,
        y: None,
    };
    if let Some(display) = Display::default() {
        if display.backend().is_x11() {
            if let Some(surface) = window.surface() {
                if let Ok(x11) = surface.downcast::<gdk4_x11::X11Surface>() {
                    if let Ok((conn, screen)) = x11rb::connect(None) {
                        use x11rb::connection::Connection as _;
                        use x11rb::protocol::xproto::ConnectionExt as _;
                        let root = conn.setup().roots[screen].root;
                        if let Ok(cookie) = conn.translate_coordinates(x11.xid() as u32, root, 0, 0)
                        {
                            if let Ok(reply) = cookie.reply() {
                                geom.x = Some(reply.dst_x as i32);
                                geom.y = Some(reply.dst_y as i32);
                            }
                        }
                    }
                }
            }
        }
    }
    Some(geom)
}

fn persist_window_geometry(
    window: &adw::Window,
    original_config: &Rc<RefCell<Config>>,
    modified: &Cell<bool>,
) {
    if let Some(geometry) = capture_window_geometry(window) {
        let mut config = original_config.borrow().clone();
        config.settings_window = Some(geometry);
        *original_config.borrow_mut() = config.clone();
        if !modified.get() {
            let _ = config.save();
        }
    }
}

fn build_status_text(
    profile_name_row: &adw::EntryRow,
    inactivity_spin: &SpinButton,
    mouse_wake_spin: &SpinButton,
    mode_index: u32,
    color_button: &ColorDialogButton,
    gradient_start_button: &ColorDialogButton,
    gradient_end_button: &ColorDialogButton,
    pattern_row: &adw::ComboRow,
    web_url_row: &adw::EntryRow,
    stream_url_row: &adw::EntryRow,
    file_row: &adw::ActionRow,
    file_info_row: &adw::ActionRow,
    mute_switch: &Switch,
    video_volume_spin: &SpinButton,
    fade_switch: &Switch,
    inhibit_switch: &Switch,
    power_integration_switch: &Switch,
    system_lock_screen_switch: &Switch,
    clock_switch: &Switch,
    clock_two_lines_switch: &Switch,
    clock_format_entry: &adw::EntryRow,
    clock_time_format_entry: &adw::EntryRow,
    clock_date_format_entry: &adw::EntryRow,
    clock_position_row: &adw::ComboRow,
    clock_move_switch: &Switch,
    clock_move_interval_spin: &SpinButton,
    clock_size_spin: &SpinButton,
    slideshow_interval_spin: &SpinButton,
    random_media_switch: &Switch,
    media_files: &Rc<RefCell<Vec<String>>>,
    start_hotkey_entry: &Entry,
    stop_hotkey_entry: &Entry,
    panic_hotkey_entry: &Entry,
    lang: Language,
) -> String {
    let name = profile_name_row.text().to_string();
    let mode_idx = mode_index;
    let mode_lbl = content::mode_label(mode_idx, lang);
    let mut detail = String::new();
    match mode_idx {
        0 => detail = content::rgba_to_hex(color_button.rgba()),
        1 => {
            detail = format!(
                "{} -> {}",
                content::rgba_to_hex(gradient_start_button.rgba()),
                content::rgba_to_hex(gradient_end_button.rgba())
            )
        }
        2 => detail = content::pattern_label(pattern_row.selected(), lang).to_string(),
        3 => detail = web_url_row.text().to_string(),
        7 => detail = stream_url_row.text().to_string(),
        4 | 5 => {
            detail = file_row
                .subtitle()
                .map(|s| s.to_string())
                .unwrap_or_default()
        }
        _ => {}
    }
    let list_count = media_files.borrow().len();
    if random_media_switch.is_active() && list_count > 0 {
        detail = tr(lang, "Список: {list_count}").replace("{list_count}", &list_count.to_string());
    }
    let info = file_info_row
        .subtitle()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let mode_txt = if detail.is_empty() {
        mode_lbl.to_string()
    } else {
        format!("{mode_lbl}: {detail}")
    };
    let mode_txt = if info.is_empty() || info == tr(lang, "Нет данных") {
        mode_txt
    } else {
        format!("{mode_txt} • {info}")
    };
    let ss_suffix = if mode_idx == 6 {
        format!(
            " • {}",
            tr(lang, "Интервал: {slideshow_interval}с").replace(
                "{slideshow_interval}",
                &(slideshow_interval_spin.value() as u64).to_string()
            )
        )
    } else {
        String::new()
    };
    let vol_suffix = if matches!(mode_idx, 5 | 7) {
        format!(
            " • {}",
            tr(lang, "Громкость: {volume}%")
                .replace("{volume}", &(video_volume_spin.value() as u32).to_string())
        )
    } else {
        String::new()
    };
    let clock_txt = if clock_switch.is_active() {
        let pos = appearance::clock_position_label(
            appearance::clock_position_from_index(clock_position_row.selected()),
            lang,
        );
        if clock_move_switch.is_active() {
            tr(
                lang,
                "Да ({clock_format}, {clock_position}, {clock_size}пт, {interval}с)",
            )
            .replace(
                "{clock_format}",
                &if clock_two_lines_switch.is_active() {
                    format!(
                        "{} / {}",
                        clock_time_format_entry.text(),
                        clock_date_format_entry.text()
                    )
                } else {
                    clock_format_entry.text().to_string()
                },
            )
            .replace("{clock_position}", pos)
            .replace(
                "{clock_size}",
                &(clock_size_spin.value() as u32).to_string(),
            )
            .replace(
                "{interval}",
                &(clock_move_interval_spin.value() as u64).to_string(),
            )
        } else {
            tr(
                lang,
                "Да ({clock_format}, {clock_position}, {clock_size}пт)",
            )
            .replace(
                "{clock_format}",
                &if clock_two_lines_switch.is_active() {
                    format!(
                        "{} / {}",
                        clock_time_format_entry.text(),
                        clock_date_format_entry.text()
                    )
                } else {
                    clock_format_entry.text().to_string()
                },
            )
            .replace("{clock_position}", pos)
            .replace(
                "{clock_size}",
                &(clock_size_spin.value() as u32).to_string(),
            )
        }
    } else {
        tr(lang, "Нет").to_string()
    };
    let start_hotkey_text = {
        let v = start_hotkey_entry.text().to_string();
        if v.is_empty() { "—".to_string() } else { v }
    };
    let stop_hotkey_text = {
        let v = stop_hotkey_entry.text().to_string();
        if v.is_empty() { "—".to_string() } else { v }
    };
    let panic_hotkey_text = {
        let v = panic_hotkey_entry.text().to_string();
        if v.is_empty() { "—".to_string() } else { v }
    };
    tr(lang, "Профиль: {profile_name} • Режим: {mode_text}{slideshow_suffix} • Таймер: {inactivity}с • Задержка мыши: {mouse_delay_ms}мс • Без звука: {mute}{volume_suffix} • Сон: {inhibit} • Интеграция питания: {power_integration} • Блокировка: {lock_screen} • Часы: {clock} • Fade: {fade} • ГК: {start_hotkey}/{stop_hotkey}/{panic_hotkey}")
        .replace("{profile_name}", &name)
        .replace("{mode_text}", &mode_txt)
        .replace("{slideshow_suffix}", &ss_suffix)
        .replace("{inactivity}", &(inactivity_spin.value() as u64).to_string())
        .replace("{mouse_delay_ms}", &(mouse_wake_spin.value() as u64).to_string())
        .replace("{mute}", yes_no(lang, mute_switch.is_active()))
        .replace("{volume_suffix}", &vol_suffix)
        .replace("{inhibit}", yes_no(lang, inhibit_switch.is_active()))
        .replace("{power_integration}", yes_no(lang, power_integration_switch.is_active()))
        .replace("{lock_screen}", yes_no(lang, system_lock_screen_switch.is_active()))
        .replace("{clock}", &clock_txt)
        .replace("{fade}", yes_no(lang, fade_switch.is_active()))
        .replace("{start_hotkey}", &start_hotkey_text)
        .replace("{stop_hotkey}", &stop_hotkey_text)
        .replace("{panic_hotkey}", &panic_hotkey_text)
}

fn selected_mode_index(mode_selector: &gtk4::FlowBox) -> u32 {
    mode_selector
        .selected_children()
        .get(0)
        .cloned()
        .map(|child| unsafe { child.data::<u32>("mode-index").unwrap().as_ref().clone() })
        .unwrap_or(0)
}

fn profile_mode_index(mode: &ScreensaverMode) -> u32 {
    match mode {
        ScreensaverMode::Color(_) => 0,
        ScreensaverMode::Gradient { .. } => 1,
        ScreensaverMode::Pattern(_) => 2,
        ScreensaverMode::Web(_) => 3,
        ScreensaverMode::Image(_) => 4,
        ScreensaverMode::Video(_) => 5,
        ScreensaverMode::Slideshow(_) => 6,
        ScreensaverMode::Stream(_) => 7,
        ScreensaverMode::PythonScript(_) => 8,
        ScreensaverMode::Shadertoy(_) => 9,
    }
}

fn set_stack_for_mode(stack: &Stack, mode_idx: u32) {
    match mode_idx {
        0 => stack.set_visible_child_name("color_page"),
        1 => stack.set_visible_child_name("gradient_page"),
        2 => stack.set_visible_child_name("pattern_page"),
        3 => stack.set_visible_child_name("web_page"),
        4 | 5 | 6 | 7 | 8 | 9 => stack.set_visible_child_name("file_page"),
        _ => {}
    }
}

fn set_content_mode_visibility(
    stream_url_row: &adw::EntryRow,
    shadertoy_source_row: &adw::ComboRow,
    shadertoy_pack_row: &adw::ComboRow,
    shadertoy_shader_row: &adw::ComboRow,
    file_row: &adw::ActionRow,
    file_info_row: &adw::ActionRow,
    shader_check_row: &adw::ActionRow,
    shadertoy_interaction_row: &adw::ActionRow,
    shadertoy_hide_cursor_row: &adw::ActionRow,
    shadertoy_sound_row: &adw::ActionRow,
    slideshow_interval_row: &adw::ActionRow,
    mute_row: &adw::ActionRow,
    volume_row: &adw::ActionRow,
    random_row: &adw::ActionRow,
    media_list_row: &adw::ActionRow,
    media_list_box: &ListBox,
    mode_idx: u32,
) {
    let is_image = mode_idx == 4;
    let is_video = mode_idx == 5;
    let is_slideshow = mode_idx == 6;
    let is_stream = mode_idx == 7;
    let is_python = mode_idx == 8;
    let is_shadertoy = mode_idx == 9;
    let is_shaderpack_source = is_shadertoy && shadertoy_source_row.selected() == 1;
    let is_file_path = is_image || is_video || is_slideshow || is_python || is_shadertoy;
    let list_visible = is_image || is_video;
    let has_shadertoy_sound = is_shadertoy
        && crate::ui::settings::content::file_row_path(file_row)
            .as_deref()
            .map(shadertoy_dir_has_sound)
            .unwrap_or(false);
    stream_url_row.set_visible(is_stream);
    shadertoy_source_row.set_visible(is_shadertoy);
    shadertoy_pack_row.set_visible(is_shaderpack_source);
    shadertoy_shader_row.set_visible(is_shaderpack_source);
    file_row.set_visible(is_file_path && !(is_shadertoy && is_shaderpack_source));
    file_info_row.set_visible(is_file_path);
    shader_check_row.set_visible(is_shadertoy);
    shadertoy_interaction_row.set_visible(is_shadertoy);
    shadertoy_hide_cursor_row.set_visible(is_shadertoy);
    shadertoy_sound_row.set_visible(has_shadertoy_sound);
    slideshow_interval_row.set_visible(is_slideshow);
    mute_row.set_visible(is_video || is_stream || has_shadertoy_sound);
    volume_row.set_visible(is_video || is_stream || has_shadertoy_sound);
    random_row.set_visible(list_visible);
    media_list_row.set_visible(list_visible);
    media_list_box.set_visible(list_visible);
}

fn shadertoy_dir_has_sound(path: &std::path::Path) -> bool {
    let dir = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or_else(|| std::path::Path::new("."))
    };
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "glsl" | "frag" | "fs") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let key: String = stem
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect();
        if key == "sound" {
            return true;
        }
    }
    false
}

fn reload_shadertoy_packs_into_models(controller: &SettingsController) {
    let packs = match crate::shaderpacks::discover_installed_shaderpacks() {
        Ok(v) => v,
        Err(err) => {
            eprintln!("Shaderpack discovery failed: {err}");
            Vec::new()
        }
    };

    {
        let mut dst = controller.ui.shadertoy_packs.borrow_mut();
        *dst = packs;
    }

    let pack_names: Vec<String> = controller
        .ui
        .shadertoy_packs
        .borrow()
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let pack_name_refs: Vec<&str> = pack_names.iter().map(|s| s.as_str()).collect();
    controller.ui.shadertoy_pack_model.splice(
        0,
        controller.ui.shadertoy_pack_model.n_items(),
        &pack_name_refs,
    );

    if controller.ui.shadertoy_pack_model.n_items() == 0 {
        controller
            .ui
            .shadertoy_shader_model
            .splice(0, controller.ui.shadertoy_shader_model.n_items(), &[]);
        return;
    }

    let pack_sel = controller
        .ui
        .shadertoy_pack_row
        .selected()
        .min(controller.ui.shadertoy_pack_model.n_items().saturating_sub(1));
    controller.ui.shadertoy_pack_row.set_selected(pack_sel);
    rebuild_shadertoy_shader_model(controller, pack_sel as usize);
}

fn rebuild_shadertoy_shader_model(controller: &SettingsController, pack_idx: usize) {
    let packs = controller.ui.shadertoy_packs.borrow();
    let Some(pack) = packs.get(pack_idx) else {
        controller
            .ui
            .shadertoy_shader_model
            .splice(0, controller.ui.shadertoy_shader_model.n_items(), &[]);
        return;
    };
    let shader_names: Vec<String> = pack.shaders.iter().map(|s| s.name.clone()).collect();
    let shader_name_refs: Vec<&str> = shader_names.iter().map(|s| s.as_str()).collect();
    controller.ui.shadertoy_shader_model.splice(
        0,
        controller.ui.shadertoy_shader_model.n_items(),
        &shader_name_refs,
    );
    if controller.ui.shadertoy_shader_model.n_items() > 0 {
        let shader_sel = controller
            .ui
            .shadertoy_shader_row
            .selected()
            .min(controller.ui.shadertoy_shader_model.n_items().saturating_sub(1));
        controller.ui.shadertoy_shader_row.set_selected(shader_sel);
    }
}

fn shadertoy_selected_shader_image_path(controller: &SettingsController) -> Option<std::path::PathBuf> {
    let pack_idx = controller.ui.shadertoy_pack_row.selected() as usize;
    let shader_idx = controller.ui.shadertoy_shader_row.selected() as usize;
    let packs = controller.ui.shadertoy_packs.borrow();
    let pack = packs.get(pack_idx)?;
    let shader = pack.shaders.get(shader_idx)?;
    shader.detected.image.clone()
}

fn find_shadertoy_shaderpack_match_in_cache(
    controller: &SettingsController,
    path: &Path,
) -> Option<(usize, usize, Option<std::path::PathBuf>)> {
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let packs = controller.ui.shadertoy_packs.borrow();
    for (pack_idx, pack) in packs.iter().enumerate() {
        for (shader_idx, shader) in pack.shaders.iter().enumerate() {
            let shader_dir = shader.dir.canonicalize().unwrap_or_else(|_| shader.dir.clone());
            if target.starts_with(&shader_dir) {
                return Some((pack_idx, shader_idx, shader.detected.image.clone()));
            }
            if let Some(img) = shader.detected.image.as_ref() {
                let img_canon = img.canonicalize().unwrap_or_else(|_| img.clone());
                if img_canon == target {
                    return Some((pack_idx, shader_idx, shader.detected.image.clone()));
                }
            }
        }
    }
    None
}

fn set_file_row_path(
    file_row: &adw::ActionRow,
    file_info_row: &adw::ActionRow,
    path: Option<&str>,
    empty_label: &str,
    lang: Language,
) {
    let path = path.map(|p| p.trim()).filter(|p| !p.is_empty());
    if let Some(path) = path {
        file_row.set_subtitle(path);
        file_row.set_tooltip_text(Some(path));
    } else {
        file_row.set_subtitle(empty_label);
        file_row.set_tooltip_text(None);
    }
    file_info_row.set_subtitle(tr(lang, "Нет данных"));
}

fn update_file_info_row(
    file_info_row: &adw::ActionRow,
    path: Option<std::path::PathBuf>,
    mode_idx: u32,
    lang: Language,
) {
    let Some(path) = path else {
        file_info_row.set_subtitle(tr(lang, "Нет данных"));
        return;
    };
    let kind = match mode_idx {
        4 => content::MediaKind::Image,
        5 => content::MediaKind::Video,
        6 => content::MediaKind::ImageFolder,
        8 => {
            if let Err(msg) = crate::ui::settings::content::validate_python_script_path(&path, lang) {
                file_info_row.set_subtitle(&msg);
                return;
            }
            let size_txt = std::fs::metadata(&path)
                .ok()
                .map(|m| format_file_size(m.len(), lang))
                .unwrap_or_else(|| tr(lang, "Нет данных").to_string());
            file_info_row.set_subtitle(&size_txt);
            return;
        }
        9 => {
            if let Err(msg) =
                crate::ui::settings::content::validate_shadertoy_shader_path(&path, lang)
            {
                file_info_row.set_subtitle(&msg);
                return;
            }
            let size_txt = std::fs::metadata(&path)
                .ok()
                .map(|m| format_file_size(m.len(), lang))
                .unwrap_or_else(|| tr(lang, "Нет данных").to_string());
            file_info_row.set_subtitle(&size_txt);
            return;
        }
        _ => {
            file_info_row.set_subtitle(tr(lang, "Нет данных"));
            return;
        }
    };
    if let Err(msg) = validate_media_path(&path, kind, false, lang) {
        file_info_row.set_subtitle(&msg);
        return;
    }
    let info = match kind {
        content::MediaKind::ImageFolder => {
            let count = collect_image_paths(&path).len();
            tr(lang, "Список: {list_count}").replace("{list_count}", &count.to_string())
        }
        content::MediaKind::Image => {
            let size_txt = std::fs::metadata(&path)
                .ok()
                .map(|m| format_file_size(m.len(), lang))
                .unwrap_or_else(|| tr(lang, "Нет данных").to_string());
            let dims = image::open(&path).ok().map(|img| {
                let (w, h) = img.dimensions();
                format!("{w}x{h}")
            });
            if let Some(dims) = dims {
                format!("{dims} - {size_txt}")
            } else {
                size_txt
            }
        }
        content::MediaKind::Video => std::fs::metadata(&path)
            .ok()
            .map(|m| format_file_size(m.len(), lang))
            .unwrap_or_else(|| tr(lang, "Нет данных").to_string()),
    };
    file_info_row.set_subtitle(&info);
}

fn sync_media_list_box(media_list_box: &ListBox, media_files: &[String], lang: Language) {
    let mut child = media_list_box.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        media_list_box.remove(&widget);
        child = next;
    }
    for path in media_files {
        let filename = Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(path);
        let action_row = adw::ActionRow::builder()
            .title(filename)
            .subtitle(path)
            .build();
        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&action_row));
        unsafe {
            row.set_data("media-path", path.to_string());
        }
        media_list_box.append(&row);
    }
    if media_files.is_empty() {
        let row = gtk4::ListBoxRow::new();
        let label = gtk4::Label::new(Some(tr(lang, "Нет данных")));
        label.add_css_class("dim-label");
        row.set_child(Some(&label));
        row.set_selectable(false);
        media_list_box.append(&row);
    }
}

fn media_list_row_path(row: &gtk4::ListBoxRow) -> Option<String> {
    unsafe { row.data::<String>("media-path").map(|s| s.as_ref().clone()) }
}

fn normalized_rss_feeds(list: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for url in list {
        let url = url.trim();
        if url.is_empty() {
            continue;
        }
        if !out.iter().any(|existing: &String| existing == url) {
            out.push(url.to_string());
        }
    }
    out
}

fn sync_rss_feeds_list(controller: &Rc<SettingsController>) {
    let list = &controller.ui.rss_feeds_list;
    let mut child = list.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        list.remove(&widget);
        child = next;
    }

    let feeds = normalized_rss_feeds(&controller.ui.rss_feeds.borrow());
    if feeds.is_empty() {
        let row = gtk4::ListBoxRow::new();
        let label = gtk4::Label::new(Some(tr(controller.lang, "Нет RSS лент")));
        label.add_css_class("dim-label");
        row.set_child(Some(&label));
        row.set_selectable(false);
        list.append(&row);
        return;
    }

    for url in feeds {
        let action_row = adw::ActionRow::builder().title(&url).build();
        let remove_button = gtk4::Button::with_label(tr(controller.lang, "Удалить"));
        remove_button.add_css_class("destructive-action");
        remove_button.set_valign(Align::Center);
        action_row.add_suffix(&remove_button);
        list.append(&action_row);

        remove_button.connect_clicked({
            let controller = controller.clone();
            let url = url.clone();
            move |_| {
                controller
                    .ui
                    .rss_feeds
                    .borrow_mut()
                    .retain(|v| v.trim() != url);
                controller.mark_modified();
                (controller.update_status)();
                sync_rss_feeds_list(&controller);
            }
        });
    }
}

fn add_rss_feed_from_entry(controller: &Rc<SettingsController>) {
    let url = controller.ui.rss_feed_entry.text().to_string();
    let url = url.trim();
    if url.is_empty() {
        return;
    }

    let mut feeds = controller.ui.rss_feeds.borrow_mut();
    if !feeds.iter().any(|v| v.trim() == url) {
        feeds.push(url.to_string());
    }
    controller.ui.rss_feed_entry.set_text("");
    drop(feeds);

    controller.mark_modified();
    (controller.update_status)();
    sync_rss_feeds_list(controller);
}

fn save_settings(
    controller: &Rc<SettingsController>,
    window: &adw::Window,
    toast_overlay: &adw::ToastOverlay,
    sender: &mpsc::Sender<AppMessage>,
) {
    let index = controller.active_profile_index();
    controller.apply_ui_to_profile(index);
    let mut config = controller.config.borrow().clone();
    config.settings_window = capture_window_geometry(window);
    if let Err(err) = config.save() {
        toast_overlay.add_toast(adw::Toast::new(
            &tr(controller.lang, "Не удалось сохранить файл: {err}")
                .replace("{err}", &err.to_string()),
        ));
        return;
    }
    let toast_lang = config.language;
    *controller.config.borrow_mut() = config.clone();
    controller.set_modified(false);
    let _ = sender.send(AppMessage::UpdateConfig(config));
    toast_overlay.add_toast(adw::Toast::new(tr(toast_lang, "Настройки сохранены")));
}

fn refresh_profile_model(controller: &SettingsController) {
    let names: Vec<String> = controller
        .config
        .borrow()
        .profiles
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let total = controller.ui.profile_model.n_items();
    controller.ui.profile_model.splice(0, total, &name_refs);
    controller
        .ui
        .profile_dropdown
        .set_selected(controller.config.borrow().active_profile_index() as u32);
}

fn fill_monitor_profile_model(model: &gtk4::StringList, config: &Config, lang: Language) {
    let mut names: Vec<String> = Vec::with_capacity(config.profiles.len() + 1);
    names.push(tr(lang, "По умолчанию (активный профиль)").to_string());
    names.extend(config.profiles.iter().map(|p| p.name.clone()));
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let total = model.n_items();
    model.splice(0, total, &refs);
}

fn refresh_monitor_profile_model(controller: &SettingsController) {
    let config = controller.config.borrow();
    fill_monitor_profile_model(
        &controller.ui.monitor_profile_model,
        &config,
        controller.lang,
    );
    drop(config);

    controller.profile_update_guard.set(true);
    for (monitor_id, row) in controller.ui.monitor_profile_rows.borrow().iter() {
        let selected = controller
            .config
            .borrow()
            .monitor_override_profile_index(monitor_id)
            .map(|idx| idx + 1)
            .unwrap_or(0);
        row.set_selected(selected as u32);
    }
    controller.profile_update_guard.set(false);
}

fn sync_panel_commands_list(controller: &Rc<SettingsController>) {
    let commands = controller
        .config
        .borrow()
        .active_profile()
        .panel_commands
        .clone();
    let list = &controller.ui.panel_commands_list;
    let mut child = list.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        list.remove(&widget);
        child = next;
    }
    if commands.is_empty() {
        let row = gtk4::ListBoxRow::new();
        let label = gtk4::Label::new(Some(tr(controller.lang, "Нет данных")));
        label.add_css_class("dim-label");
        row.set_child(Some(&label));
        row.set_selectable(false);
        list.append(&row);
        return;
    }
    for (idx, command) in commands.iter().enumerate() {
        let action_row = adw::ActionRow::builder()
            .title(&command.name)
            .subtitle(&command.show_command)
            .build();
        let tooltip = format!(
            "{}: {}\n{}: {}",
            tr(controller.lang, "Команда показа"),
            command.show_command,
            tr(controller.lang, "Команда скрытия"),
            command.hide_command
        );
        action_row.set_tooltip_text(Some(&tooltip));

        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let edit_button = gtk4::Button::with_label(tr(controller.lang, "Редактировать"));
        let delete_button = gtk4::Button::with_label(tr(controller.lang, "Удалить"));
        delete_button.add_css_class("destructive-action");
        let enabled_switch = Switch::builder()
            .valign(Align::Center)
            .active(command.enabled)
            .build();
        unsafe {
            edit_button.set_data("command-index", idx as u32);
            delete_button.set_data("command-index", idx as u32);
            enabled_switch.set_data("command-index", idx as u32);
        }
        controls.append(&edit_button);
        controls.append(&delete_button);
        controls.append(&enabled_switch);
        action_row.add_suffix(&controls);

        enabled_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |switch, _| {
                let Some(index) = command_index(switch) else {
                    return;
                };
                let mut config = controller.config.borrow_mut();
                let profile_idx = config.active_profile_index();
                if let Some(profile) = config.profiles.get_mut(profile_idx) {
                    if let Some(cmd) = profile.panel_commands.get_mut(index) {
                        cmd.enabled = switch.is_active();
                    }
                }
                controller.mark_modified();
            }
        });

        edit_button.connect_clicked({
            let controller = controller.clone();
            move |button| {
                let Some(index) = command_index(button) else {
                    return;
                };
                open_panel_command_editor(&controller, index);
            }
        });

        delete_button.connect_clicked({
            let controller = controller.clone();
            move |button| {
                let Some(index) = command_index(button) else {
                    return;
                };
                let mut config = controller.config.borrow_mut();
                let profile_idx = config.active_profile_index();
                if let Some(profile) = config.profiles.get_mut(profile_idx) {
                    if index < profile.panel_commands.len() {
                        profile.panel_commands.remove(index);
                    }
                }
                drop(config);
                controller.mark_modified();
                sync_panel_commands_list(&controller);
            }
        });

        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&action_row));
        row.set_selectable(false);
        list.append(&row);
    }
}

fn normalized_app_list(list: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for name in list {
        if let Some(norm) = crate::apps::normalize_app_name(name) {
            if !out.contains(&norm) {
                out.push(norm);
            }
        }
    }
    out.sort();
    out
}

fn sync_app_inhibit_list(controller: &Rc<SettingsController>) {
    let mut candidates = crate::apps::list_running_gui_apps();
    let selected = controller.ui.app_inhibit_apps.borrow().clone();
    for app_id in &selected {
        if !candidates.iter().any(|app| app.id == *app_id) {
            if let Some(entry) = crate::apps::resolve_app_info(app_id) {
                candidates.push(entry);
            } else {
                candidates.push(crate::apps::RunningGuiApp {
                    id: app_id.clone(),
                    display_name: app_id.clone(),
                    icon: None,
                });
            }
        }
    }
    candidates.sort_by(|a, b| {
        a.display_name
            .to_ascii_lowercase()
            .cmp(&b.display_name.to_ascii_lowercase())
    });
    candidates.dedup_by(|a, b| a.id == b.id);

    let list = &controller.ui.app_inhibit_list;
    let mut child = list.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        list.remove(&widget);
        child = next;
    }

    if candidates.is_empty() {
        let row = gtk4::ListBoxRow::new();
        let label = gtk4::Label::new(Some(tr(controller.lang, "Нет данных")));
        label.add_css_class("dim-label");
        row.set_child(Some(&label));
        row.set_selectable(false);
        list.append(&row);
        return;
    }

    for app in candidates {
        let action_row = adw::ActionRow::builder().title(&app.display_name).build();
        if app.display_name != app.id {
            action_row.set_subtitle(&app.id);
        }
        if let Some(icon) = &app.icon {
            if let Some(picture) = icon_picture_from_gicon(icon, 18) {
                action_row.add_prefix(&picture);
            }
        }
        let active = selected.contains(&app.id);
        let switch = Switch::builder()
            .valign(Align::Center)
            .active(active)
            .build();
        action_row.add_suffix(&switch);
        list.append(&action_row);

        switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            let app_id = app.id.clone();
            move |switch, _| {
                let mut apps = controller.ui.app_inhibit_apps.borrow_mut();
                if switch.is_active() {
                    if !apps.contains(&app_id) {
                        apps.push(app_id.clone());
                    }
                } else {
                    apps.retain(|name| name != &app_id);
                }
                drop(apps);
                controller.mark_modified();
                (controller.update_status)();
                sync_app_inhibit_list(&controller);
            }
        });
    }
}

fn add_inhibit_app_from_entry(controller: &Rc<SettingsController>) {
    let text = controller.ui.app_inhibit_entry.text().to_string();
    let Some(name) = crate::apps::normalize_app_name(&text) else {
        return;
    };
    let mut apps = controller.ui.app_inhibit_apps.borrow_mut();
    if !apps.contains(&name) {
        apps.push(name);
    }
    controller.ui.app_inhibit_entry.set_text("");
    drop(apps);
    controller.mark_modified();
    (controller.update_status)();
    sync_app_inhibit_list(controller);
}

fn command_index(widget: &impl IsA<gtk4::Widget>) -> Option<usize> {
    unsafe {
        widget
            .as_ref()
            .data::<u32>("command-index")
            .map(|v| *v.as_ref() as usize)
    }
}

fn open_panel_command_editor(controller: &Rc<SettingsController>, index: usize) {
    let Some(window) = controller.window_weak.upgrade() else {
        return;
    };
    let command = {
        let config = controller.config.borrow();
        let profile_idx = config.active_profile_index();
        config
            .profiles
            .get(profile_idx)
            .and_then(|p| p.panel_commands.get(index))
            .cloned()
    };
    let Some(command) = command else {
        return;
    };

    let dialog = adw::MessageDialog::builder()
        .transient_for(&window)
        .modal(true)
        .heading(tr(controller.lang, "Редактировать"))
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    let name_row = adw::EntryRow::builder()
        .title(tr(controller.lang, "Название"))
        .build();
    name_row.set_text(&command.name);
    let show_row = adw::EntryRow::builder()
        .title(tr(controller.lang, "Команда показа"))
        .build();
    show_row.set_text(&command.show_command);
    let hide_row = adw::EntryRow::builder()
        .title(tr(controller.lang, "Команда скрытия"))
        .build();
    hide_row.set_text(&command.hide_command);
    content.append(&name_row);
    content.append(&show_row);
    content.append(&hide_row);
    dialog.set_extra_child(Some(&content));

    dialog.add_response("cancel", tr(controller.lang, "Отмена"));
    dialog.add_response("save", tr(controller.lang, "Сохранить"));
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.connect_response(None, {
        let controller = controller.clone();
        move |dialog, response| {
            if response == "save" {
                let mut config = controller.config.borrow_mut();
                let profile_idx = config.active_profile_index();
                if let Some(profile) = config.profiles.get_mut(profile_idx) {
                    if let Some(cmd) = profile.panel_commands.get_mut(index) {
                        cmd.name = name_row.text().to_string();
                        cmd.show_command = show_row.text().to_string();
                        cmd.hide_command = hide_row.text().to_string();
                    }
                }
                drop(config);
                controller.mark_modified();
                sync_panel_commands_list(&controller);
            }
            dialog.close();
        }
    });
    dialog.present();
}

fn path_or_empty(path: Option<std::path::PathBuf>) -> String {
    path.map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn clock_format_from_ui(
    clock_format_row: &adw::ComboRow,
    clock_format_entry: &adw::EntryRow,
) -> String {
    let selected = clock_format_row.selected() as usize;
    if selected < CLOCK_FORMAT_PATTERNS.len() {
        CLOCK_FORMAT_PATTERNS[selected].to_string()
    } else {
        clock_format_entry.text().to_string()
    }
}

fn setup_hotkey_capture(
    entry: &Entry,
    default_accel: &str,
    controller: Rc<SettingsController>,
    toast_overlay: adw::ToastOverlay,
) {
    let entry = entry.clone();
    let entry_clone = entry.clone();
    let default_accel = default_accel.to_string();

    // Ensure we have a stored accelerator even if the visible text is a friendly label.
    set_hotkey_entry(&entry, &hotkey_entry_stored_accel_or_text(&entry));

    entry.connect_icon_release({
        let controller = controller.clone();
        let toast_overlay = toast_overlay.clone();
        move |entry, pos| {
            if controller.profile_update_guard.get() {
                return;
            }
            match pos {
                EntryIconPosition::Primary => {
                    set_hotkey_entry(entry, &default_accel);
                    controller.mark_modified();
                    (controller.update_status)();
                    warn_hotkey_conflicts(&controller, &toast_overlay);
                }
                EntryIconPosition::Secondary => {
                    set_hotkey_entry(entry, "");
                    controller.mark_modified();
                    (controller.update_status)();
                    warn_hotkey_conflicts(&controller, &toast_overlay);
                }
                _ => {}
            }
        }
    });

    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed(move |_, keyval, _keycode, state| {
        if controller.profile_update_guard.get() {
            return glib::Propagation::Stop;
        }
        if keyval == Key::BackSpace {
            set_hotkey_entry(&entry_clone, "");
            controller.mark_modified();
            (controller.update_status)();
            warn_hotkey_conflicts(&controller, &toast_overlay);
            return glib::Propagation::Stop;
        }
        if is_modifier_key(keyval) {
            return glib::Propagation::Stop;
        }
        let mods = filtered_modifiers(state);
        // Allow Esc only when used with modifiers (default panic hotkey uses Escape).
        if keyval == Key::Escape && mods.is_empty() {
            return glib::Propagation::Stop;
        }
        let valid = is_function_key(keyval) || has_primary_modifier(mods);
        if !valid {
            toast_overlay.add_toast(adw::Toast::new(tr(
                controller.lang,
                "Используйте Ctrl/Alt/Super или F-клавиши",
            )));
            return glib::Propagation::Stop;
        }
        let accel = gtk4::accelerator_name(keyval, mods).to_string();
        if accel.trim().is_empty() {
            toast_overlay.add_toast(adw::Toast::new(tr(
                controller.lang,
                "Не удалось распознать комбинацию",
            )));
            return glib::Propagation::Stop;
        }
        set_hotkey_entry(&entry_clone, &accel);
        controller.mark_modified();
        (controller.update_status)();
        warn_hotkey_conflicts(&controller, &toast_overlay);
        glib::Propagation::Stop
    });
    entry.add_controller(key_controller);
}

const HOTKEY_ACCEL_DATA_KEY: &str = "vesper-hotkey-accel";

fn hotkey_entry_stored_accel_or_text(entry: &Entry) -> String {
    let stored = unsafe {
        entry
            .data::<String>(HOTKEY_ACCEL_DATA_KEY)
            .map(|v| v.as_ref().clone())
            .unwrap_or_default()
    };
    if !stored.trim().is_empty() {
        return stored;
    }
    // Fallback (should be replaced by `set_hotkey_entry` on init).
    entry.text().to_string()
}

fn hotkey_entry_accel(entry: &Entry) -> String {
    let accel = hotkey_entry_stored_accel_or_text(entry);
    // Try to normalize to a GTK accelerator string if we only have a label.
    if gtk4::accelerator_parse(&accel).is_some() {
        accel
    } else {
        String::new()
    }
}

fn accel_to_label(accel: &str) -> String {
    let accel = accel.trim();
    if accel.is_empty() {
        return String::new();
    }
    if let Some((key, mods)) = gtk4::accelerator_parse(accel) {
        gtk4::accelerator_get_label(key, mods).to_string()
    } else {
        accel.to_string()
    }
}

fn set_hotkey_entry(entry: &Entry, accel: &str) {
    let accel = accel.trim().to_string();
    unsafe {
        entry.set_data(HOTKEY_ACCEL_DATA_KEY, accel.clone());
    }
    entry.set_text(&accel_to_label(&accel));
    entry.set_icon_sensitive(EntryIconPosition::Secondary, !accel.is_empty());
    entry.set_icon_activatable(EntryIconPosition::Secondary, !accel.is_empty());
}

fn warn_hotkey_conflicts(controller: &SettingsController, toast_overlay: &adw::ToastOverlay) {
    let start = hotkey_entry_accel(&controller.ui.start_hotkey_entry);
    let stop = hotkey_entry_accel(&controller.ui.stop_hotkey_entry);
    let panic = hotkey_entry_accel(&controller.ui.panic_hotkey_entry);
    if !start.is_empty() && start == stop {
        toast_overlay.add_toast(adw::Toast::new(
            &tr(controller.lang, "Конфликт: запуск и остановка используют {hotkey}")
                .replace("{hotkey}", &accel_to_label(&start)),
        ));
        return;
    }
    if !start.is_empty() && start == panic {
        toast_overlay.add_toast(adw::Toast::new(
            &tr(controller.lang, "Конфликт: запуск и принудительное закрытие используют {hotkey}")
                .replace("{hotkey}", &accel_to_label(&start)),
        ));
        return;
    }
    if !stop.is_empty() && stop == panic {
        toast_overlay.add_toast(adw::Toast::new(
            &tr(controller.lang, "Конфликт: остановка и принудительное закрытие используют {hotkey}")
                .replace("{hotkey}", &accel_to_label(&stop)),
        ));
    }
}

fn filtered_modifiers(state: ModifierType) -> ModifierType {
    let allowed = ModifierType::SHIFT_MASK
        | ModifierType::CONTROL_MASK
        | ModifierType::ALT_MASK
        | ModifierType::SUPER_MASK
        | ModifierType::META_MASK
        | ModifierType::HYPER_MASK;
    state & allowed
}

fn has_primary_modifier(mods: ModifierType) -> bool {
    mods.intersects(
        ModifierType::CONTROL_MASK
            | ModifierType::ALT_MASK
            | ModifierType::SUPER_MASK
            | ModifierType::META_MASK
            | ModifierType::HYPER_MASK,
    )
}

fn is_modifier_key(keyval: Key) -> bool {
    matches!(
        keyval,
        Key::Shift_L
            | Key::Shift_R
            | Key::Control_L
            | Key::Control_R
            | Key::Alt_L
            | Key::Alt_R
            | Key::Super_L
            | Key::Super_R
            | Key::Meta_L
            | Key::Meta_R
            | Key::Hyper_L
            | Key::Hyper_R
    )
}

fn is_function_key(keyval: Key) -> bool {
    matches!(
        keyval,
        Key::F1
            | Key::F2
            | Key::F3
            | Key::F4
            | Key::F5
            | Key::F6
            | Key::F7
            | Key::F8
            | Key::F9
            | Key::F10
            | Key::F11
            | Key::F12
    )
}

fn icon_picture(icon_name: &str, size: i32) -> Picture {
    let Some(display) = gdk4::Display::default() else {
        let picture = Picture::new();
        picture.set_size_request(size, size);
        return picture;
    };
    let icon_theme = IconTheme::for_display(&display);
    let paintable = icon_theme.lookup_icon(
        icon_name,
        &[],
        size,
        1,
        TextDirection::Ltr,
        IconLookupFlags::empty(),
    );
    let picture = Picture::for_paintable(&paintable);
    picture.set_content_fit(ContentFit::Contain);
    picture.set_can_shrink(false);
    picture.set_halign(Align::Center);
    picture.set_valign(Align::Center);
    picture.set_size_request(size, size);
    picture
}

fn icon_picture_from_gicon(icon: &gio::Icon, size: i32) -> Option<Picture> {
    let display = gdk4::Display::default()?;
    let icon_theme = IconTheme::for_display(&display);
    if !icon_theme.has_gicon(icon) {
        return None;
    }
    let paintable =
        icon_theme.lookup_by_gicon(icon, size, 1, TextDirection::Ltr, IconLookupFlags::empty());
    let picture = Picture::for_paintable(&paintable);
    picture.set_content_fit(ContentFit::Contain);
    picture.set_can_shrink(false);
    picture.set_halign(Align::Center);
    picture.set_valign(Align::Center);
    picture.set_size_request(size, size);
    Some(picture)
}
