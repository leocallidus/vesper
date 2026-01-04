use gtk4::prelude::*;
use gtk4::{Application, ColorDialogButton, ContentFit, IconLookupFlags, IconTheme, Picture, Stack, SpinButton, Switch, Align, Frame, Label, Entry, EventControllerKey, ListBox, SelectionMode, TextDirection};
use libadwaita as adw;
use libadwaita::prelude::*;
use gdk4::{Display, Key, ModifierType};
use crate::config::{Config, PanelCommand, ScreensaverMode, SettingsProfile, WindowGeometry, MAX_PROFILES};
use crate::AppMessage;
use crate::i18n::{Language, resolve_language, tr, yes_no, language_from_index, language_index, profile_name};
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;
use image::GenericImageView;

pub mod general;
pub mod appearance;
pub mod content;
pub mod power;
pub mod advanced;
pub mod autostart;

use general::*;
use appearance::*;
use content::*;
use power::*;
use advanced::*;
use autostart::*;

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
    start_hotkey_entry: Entry,
    stop_hotkey_entry: Entry,
    autostart_switch: Switch,
    mode_selector: gtk4::FlowBox,
    stack: Stack,
    color_button: ColorDialogButton,
    gradient_start_button: ColorDialogButton,
    gradient_end_button: ColorDialogButton,
    pattern_row: adw::ComboRow,
    web_url_row: adw::EntryRow,
    stream_url_row: adw::EntryRow,
    file_row: adw::ActionRow,
    file_info_row: adw::ActionRow,
    slideshow_interval_row: adw::ActionRow,
    mute_row: adw::ActionRow,
    volume_row: adw::ActionRow,
    random_row: adw::ActionRow,
    slideshow_interval_spin: SpinButton,
    mute_switch: Switch,
    video_volume_spin: SpinButton,
    random_media_switch: Switch,
    media_files: Rc<RefCell<Vec<String>>>,
    selected_media_preview: Rc<RefCell<Option<String>>>,
    media_list_row: adw::ActionRow,
    media_list_box: ListBox,
    remove_media_button: gtk4::Button,
    clock_switch: Switch,
    clock_format_row: adw::ComboRow,
    clock_format_entry: adw::EntryRow,
    clock_position_row: adw::ComboRow,
    clock_move_switch: Switch,
    clock_move_interval_row: adw::ActionRow,
    clock_move_interval_spin: SpinButton,
    clock_size_spin: SpinButton,
    clock_preview_label: Label,
    inhibit_switch: Switch,
    power_integration_switch: Switch,
    lock_screen_switch: Switch,
    mpris_pause_switch: Switch,
    app_inhibit_list: ListBox,
    app_inhibit_entry: adw::EntryRow,
    app_inhibit_apps: Rc<RefCell<Vec<String>>>,
    delete_button: gtk4::Button,
    panel_commands_list: ListBox,
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

        let (profile, language, hotkey_start, hotkey_stop, start_minimized, autostart_enabled, clamped, total_profiles) = {
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
            let start_minimized = config.start_minimized;
            let autostart_enabled = crate::autostart::is_autostart_enabled();
            let total_profiles = config.profiles.len();
            (profile, language, hotkey_start, hotkey_stop, start_minimized, autostart_enabled, clamped, total_profiles)
        };

        self.ui.profile_dropdown.set_selected(clamped as u32);
        self.ui.profile_name_row.set_text(&profile.name);
        self.ui.inactivity_spin.set_value(profile.inactivity_seconds as f64);
        self.ui.mouse_wake_spin.set_value(profile.mouse_wake_delay_ms as f64);
        self.ui.fade_switch.set_active(profile.fade_enabled);
        self.ui.language_row.set_selected(language_index(language));
        self.ui.start_minimized_switch.set_active(start_minimized);
        self.ui.autostart_switch.set_active(autostart_enabled);
        self.ui.start_hotkey_entry.set_text(&hotkey_start);
        self.ui.stop_hotkey_entry.set_text(&hotkey_stop);

        let mode_idx = profile_mode_index(&profile.mode);
        if let Some(child) = self.ui.mode_selector.child_at_index(mode_idx as i32) {
            self.ui.mode_selector.select_child(&child);
        }
        set_stack_for_mode(&self.ui.stack, mode_idx);
        set_content_mode_visibility(
            &self.ui.stream_url_row,
            &self.ui.file_row,
            &self.ui.file_info_row,
            &self.ui.slideshow_interval_row,
            &self.ui.mute_row,
            &self.ui.volume_row,
            &self.ui.random_row,
            &self.ui.media_list_row,
            &self.ui.media_list_box,
            mode_idx,
        );

        match &profile.mode {
            ScreensaverMode::Color(hex) => {
                let rgba = content::hex_to_rgba(hex).unwrap_or(gdk4::RGBA::new(0.0, 0.0, 0.0, 1.0));
                self.ui.color_button.set_rgba(&rgba);
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(&self.ui.file_row, &self.ui.file_info_row, None, tr(self.lang, "Файл не выбран"), self.lang);
            }
            ScreensaverMode::Gradient { start, end } => {
                let start_rgba = content::hex_to_rgba(start).unwrap_or(gdk4::RGBA::new(0.0, 0.0, 0.0, 1.0));
                let end_rgba = content::hex_to_rgba(end).unwrap_or(gdk4::RGBA::new(0.0, 0.0, 0.0, 1.0));
                self.ui.gradient_start_button.set_rgba(&start_rgba);
                self.ui.gradient_end_button.set_rgba(&end_rgba);
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(&self.ui.file_row, &self.ui.file_info_row, None, tr(self.lang, "Файл не выбран"), self.lang);
            }
            ScreensaverMode::Pattern(pattern) => {
                self.ui.pattern_row.set_selected(content::pattern_index(*pattern));
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(&self.ui.file_row, &self.ui.file_info_row, None, tr(self.lang, "Файл не выбран"), self.lang);
            }
            ScreensaverMode::Web(url) => {
                self.ui.web_url_row.set_text(url);
                self.ui.stream_url_row.set_text("");
                set_file_row_path(&self.ui.file_row, &self.ui.file_info_row, None, tr(self.lang, "Файл не выбран"), self.lang);
            }
            ScreensaverMode::Stream(url) => {
                self.ui.stream_url_row.set_text(url);
                self.ui.web_url_row.set_text("");
                set_file_row_path(&self.ui.file_row, &self.ui.file_info_row, None, tr(self.lang, "Файл не выбран"), self.lang);
            }
            ScreensaverMode::Image(path) => {
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(&self.ui.file_row, &self.ui.file_info_row, Some(path), tr(self.lang, "Файл не выбран"), self.lang);
            }
            ScreensaverMode::Video(path) => {
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(&self.ui.file_row, &self.ui.file_info_row, Some(path), tr(self.lang, "Файл не выбран"), self.lang);
            }
            ScreensaverMode::Slideshow(path) => {
                self.ui.web_url_row.set_text("");
                self.ui.stream_url_row.set_text("");
                set_file_row_path(&self.ui.file_row, &self.ui.file_info_row, Some(path), tr(self.lang, "Папка не выбрана"), self.lang);
            }
        }

        update_file_info_row(&self.ui.file_info_row, file_row_path(&self.ui.file_row), mode_idx, self.lang);

        self.ui.slideshow_interval_spin.set_value(profile.slideshow_interval_seconds as f64);
        self.ui.mute_switch.set_active(profile.mute_video);
        self.ui.video_volume_spin.set_value(profile.video_volume as f64);
        self.ui.random_media_switch.set_active(profile.random_media);
        self.ui.media_files.borrow_mut().clear();
        self.ui.media_files.borrow_mut().extend(profile.media_list);
        self.ui.selected_media_preview.borrow_mut().take();
        sync_media_list_box(&self.ui.media_list_box, &self.ui.media_files.borrow(), self.lang);
        self.ui.media_list_box.select_row(None::<&gtk4::ListBoxRow>);
        self.ui.remove_media_button.set_sensitive(false);

        self.ui.clock_switch.set_active(profile.show_clock);
        self.ui.clock_position_row.set_selected(appearance::clock_position_index(profile.clock_position));
        self.ui.clock_move_switch.set_active(profile.clock_move_enabled);
        self.ui.clock_move_interval_row.set_visible(profile.clock_move_enabled);
        self.ui.clock_move_interval_spin.set_value(profile.clock_move_interval_seconds as f64);
        self.ui.clock_size_spin.set_value(profile.clock_size as f64);
        self.ui.clock_preview_label.set_text(&appearance::format_clock_text(&profile.clock_format));
        appearance::apply_clock_size(&self.ui.clock_preview_label, profile.clock_size);
        appearance::apply_clock_position(&self.ui.clock_preview_label, profile.clock_position, 20);
        let preset_index = appearance::clock_format_preset_index(&profile.clock_format);
        let selected = preset_index.unwrap_or(CLOCK_FORMAT_PATTERNS.len() as u32);
        self.ui.clock_format_row.set_selected(selected);
        self.ui.clock_format_entry.set_text(&profile.clock_format);
        self.ui.clock_format_entry.set_visible(preset_index.is_none());

        self.ui.inhibit_switch.set_active(profile.inhibit_sleep);
        self.ui.power_integration_switch.set_active(profile.power_integration_enabled);
        self.ui.lock_screen_switch.set_active(profile.lock_screen_enabled);
        self.ui.mpris_pause_switch.set_active(profile.mpris_pause_enabled);
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
        profile.video_volume = self.ui.video_volume_spin.value() as u8;
        profile.random_media = self.ui.random_media_switch.is_active();
        profile.media_list = self.ui.media_files.borrow().clone();
        profile.slideshow_interval_seconds = self.ui.slideshow_interval_spin.value() as u64;
        profile.inhibit_sleep = self.ui.inhibit_switch.is_active();
        profile.power_integration_enabled = self.ui.power_integration_switch.is_active();
        profile.lock_screen_enabled = self.ui.lock_screen_switch.is_active();
        profile.mpris_pause_enabled = self.ui.mpris_pause_switch.is_active();
        profile.app_inhibit_list = normalized_app_list(&self.ui.app_inhibit_apps.borrow());
        profile.show_clock = self.ui.clock_switch.is_active();
        profile.clock_position = appearance::clock_position_from_index(self.ui.clock_position_row.selected());
        profile.clock_move_enabled = self.ui.clock_move_switch.is_active();
        profile.clock_move_interval_seconds = self.ui.clock_move_interval_spin.value() as u64;
        profile.clock_size = self.ui.clock_size_spin.value() as u32;
        profile.clock_format = clock_format_from_ui(&self.ui.clock_format_row, &self.ui.clock_format_entry);

        let mode_idx = selected_mode_index(&self.ui.mode_selector);
        profile.mode = match mode_idx {
            0 => ScreensaverMode::Color(content::rgba_to_hex(self.ui.color_button.rgba())),
            1 => ScreensaverMode::Gradient {
                start: content::rgba_to_hex(self.ui.gradient_start_button.rgba()),
                end: content::rgba_to_hex(self.ui.gradient_end_button.rgba()),
            },
            2 => ScreensaverMode::Pattern(content::pattern_from_index(self.ui.pattern_row.selected())),
            3 => ScreensaverMode::Web(self.ui.web_url_row.text().to_string()),
            4 => ScreensaverMode::Image(path_or_empty(file_row_path(&self.ui.file_row))),
            5 => ScreensaverMode::Video(path_or_empty(file_row_path(&self.ui.file_row))),
            6 => ScreensaverMode::Slideshow(path_or_empty(file_row_path(&self.ui.file_row))),
            7 => ScreensaverMode::Stream(self.ui.stream_url_row.text().to_string()),
            _ => ScreensaverMode::Color(content::rgba_to_hex(self.ui.color_button.rgba())),
        };

        config.language = language_from_index(self.ui.language_row.selected());
        config.start_minimized = self.ui.start_minimized_switch.is_active();
        config.hotkey_start = self.ui.start_hotkey_entry.text().to_string();
        config.hotkey_stop = self.ui.stop_hotkey_entry.text().to_string();
    }
}

impl SettingsWindow {
    pub fn new(app: &Application, config: Config, sender: std::sync::mpsc::Sender<crate::AppMessage>) -> Self {
        let (default_width, default_height) = config
            .settings_window
            .as_ref()
            .and_then(|geom|
                if geom.width > 0 && geom.height > 0 { Some((geom.width, geom.height)) } else { None }
            )
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
                                if let Ok(x11_surface) = surface.downcast::<gdk4_x11::X11Surface>() {
                                    if let Ok((conn, _)) = x11rb::connect(None) {
                                        use x11rb::connection::Connection as _;
                                        use x11rb::protocol::xproto::ConnectionExt as _;
                                        let _ = conn.configure_window(x11_surface.xid() as u32, &x11rb::protocol::xproto::ConfigureWindowAux::new().x(position.0).y(position.1));
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
        let profile_dropdown = gtk4::DropDown::builder().model(&profile_model).valign(Align::Center).build();
        header_bar.set_title_widget(Some(&profile_dropdown));

        let save_button = gtk4::Button::builder().label(tr(lang, "Сохранить")).css_classes(["suggested-action"]).valign(Align::Center).build();
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
        let appearance_page = adw::PreferencesPage::new();
        view_stack.add_titled(&appearance_page, Some("appearance"), tr(lang, "Внешний вид"));
        let power_page = adw::PreferencesPage::new();
        view_stack.add_titled(&power_page, Some("power"), tr(lang, "Питание"));
        let advanced_page = adw::PreferencesPage::new();
        view_stack.add_titled(&advanced_page, Some("advanced"), tr(lang, "Система"));
        let profiles_page = adw::PreferencesPage::new();
        view_stack.add_titled(&profiles_page, Some("profiles"), tr(lang, "Профили"));

        let general_widgets = build_general_group(&config, lang);
        general_page.add(&general_widgets.general_group);
        general_page.add(&general_widgets.hotkeys_group);
        let GeneralWidgets { inactivity_spin, mouse_wake_spin, fade_switch, language_row, start_hotkey_entry, stop_hotkey_entry, .. } = general_widgets;

        let autostart_widgets = build_autostart_group(&config, lang);
        autostart_page.add(&autostart_widgets.group);
        let AutostartWidgets { autostart_switch, start_minimized_switch, .. } = autostart_widgets;

        let content_widgets = build_content_group(&config, lang);
        content_page.add(&content_widgets.group);
        content_page.add(&content_widgets.preview_group);
        let ContentWidgets { mode_selector, stack, color_button, gradient_start_button, gradient_end_button, pattern_row, web_url_row, stream_url_row, file_row, file_button, file_info_row, slideshow_interval_row, mute_row, volume_row, random_row, slideshow_interval_spin, mute_switch, video_volume_spin, random_media_switch, media_list_row, media_list_box, add_media_button, remove_media_button, preview_frame, media_files, selected_media_preview, .. } = content_widgets;

        let appearance_widgets = build_appearance_group(&config, lang);
        appearance_page.add(&appearance_widgets.group);
        appearance_page.add(&appearance_widgets.clock_preview_group);
        let AppearanceWidgets { clock_switch, clock_format_row, clock_format_entry, clock_position_row, clock_size_spin, clock_move_switch, clock_move_interval_row, clock_move_interval_spin, clock_preview_label, .. } = appearance_widgets;

        let power_widgets = build_power_group(&config, lang);
        power_page.add(&power_widgets.group);
        power_page.add(&power_widgets.apps_group);
        let PowerWidgets {
            inhibit_switch,
            power_integration_switch,
            lock_screen_switch,
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

        let profiles_group = adw::PreferencesGroup::builder().title(tr(lang, "Управление профилями")).build();
        let profile_name_row = adw::EntryRow::builder().title(tr(lang, "Название профиля")).text(config.active_profile().name.as_str()).build();
        let add_profile_row = adw::ActionRow::builder().title(tr(lang, "Добавить профиль")).build();
        let add_profile_btn_widget = gtk4::Button::with_label(tr(lang, "Добавить"));
        add_profile_btn_widget.add_css_class("suggested-action");
        add_profile_row.add_suffix(&add_profile_btn_widget);
        profiles_group.add(&profile_name_row);
        profiles_group.add(&add_profile_row);
        profiles_page.add(&profiles_group);

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

        let sidebar_page = adw::NavigationPage::builder().title(tr(lang, "Настройки")).tag("sidebar").build();
        let sidebar_list = ListBox::new();
        sidebar_list.add_css_class("navigation-sidebar");
        sidebar_list.set_selection_mode(SelectionMode::Single);
        let categories = [
            ("general", tr(lang, "Общие"), "preferences-system-symbolic"),
            ("autostart", tr(lang, "Автозапуск"), "system-run-symbolic"),
            ("content", tr(lang, "Контент"), "applications-multimedia-symbolic"),
            ("appearance", tr(lang, "Внешний вид"), "preferences-desktop-appearance-symbolic"),
            ("power", tr(lang, "Питание"), "power-profile-balanced-symbolic"),
            ("advanced", tr(lang, "Система"), "utilities-terminal-symbolic"),
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
            unsafe { row.set_data("page-tag", tag.to_string()); }
            sidebar_list.append(&row);
        }
        let scrolled = gtk4::ScrolledWindow::builder().child(&sidebar_list).propagate_natural_width(true).build();
        sidebar_page.set_child(Some(&scrolled));
        split_view.set_sidebar(Some(&sidebar_page));

        let content_nav_page = adw::NavigationPage::builder().title(tr(lang, "Настройки")).tag("content-nav").child(&content_box).build();
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
            let file_row = file_row.clone();
            let file_info_row = file_info_row.clone();
            let slideshow_interval_row = slideshow_interval_row.clone();
            let mute_row = mute_row.clone();
            let volume_row = volume_row.clone();
            let random_row = random_row.clone();
            let media_list_row = media_list_row.clone();
            let media_list_box = media_list_box.clone();
            move |flowbox| {
                let Some(child) = flowbox.selected_children().get(0).cloned() else { return; };
                let mode_idx = unsafe { child.data::<u32>("mode-index").unwrap().as_ref().clone() };
                set_stack_for_mode(&stack, mode_idx);
                set_content_mode_visibility(
                    &stream_url_row,
                    &file_row,
                    &file_info_row,
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
            let lock_screen_switch = lock_screen_switch.clone();
            let clock_switch = clock_switch.clone();
            let clock_format_entry = clock_format_entry.clone();
            let clock_position_row = clock_position_row.clone();
            let clock_move_switch = clock_move_switch.clone();
            let clock_move_interval_spin = clock_move_interval_spin.clone();
            let clock_size_spin = clock_size_spin.clone();
            let slideshow_interval_spin = slideshow_interval_spin.clone();
            let random_media_switch = random_media_switch.clone();
            let media_files = media_files.clone();
            let start_hotkey_entry = start_hotkey_entry.clone();
            let stop_hotkey_entry = stop_hotkey_entry.clone();
            move || {
                let selected_child = mode_selector.selected_children().get(0).cloned();
                let mode_idx = selected_child.map(|c| unsafe { c.data::<u32>("mode-index").unwrap().as_ref().clone() }).unwrap_or(0);
                let text = build_status_text(&profile_name_row, &inactivity_spin, &mouse_wake_spin, mode_idx, &color_button, &gradient_start_button, &gradient_end_button, &pattern_row, &web_url_row, &stream_url_row, &file_row, &file_info_row, &mute_switch, &video_volume_spin, &fade_switch, &inhibit_switch, &power_integration_switch, &lock_screen_switch, &clock_switch, &clock_format_entry, &clock_position_row, &clock_move_switch, &clock_move_interval_spin, &clock_size_spin, &slideshow_interval_spin, &random_media_switch, &media_files, &start_hotkey_entry, &stop_hotkey_entry, lang);
                status_row.set_subtitle(&text);
            }
        });

        let preview_media = Rc::new(RefCell::new(None));
        let preview_web = Rc::new(RefCell::new(None));
        let update_preview = Rc::new({
            let preview_frame = preview_frame.clone();
            let mode_selector = mode_selector.clone();
            let file_row = file_row.clone();
            let random_media_switch = random_media_switch.clone();
            let media_files = media_files.clone();
            let selected_media_preview = selected_media_preview.clone();
            let color_button = color_button.clone();
            let gradient_start_button = gradient_start_button.clone();
            let gradient_end_button = gradient_end_button.clone();
            let pattern_row = pattern_row.clone();
            let web_url_row = web_url_row.clone();
            let stream_url_row = stream_url_row.clone();
            let mute_switch = mute_switch.clone();
            let video_volume_spin = video_volume_spin.clone();
            let clock_switch = clock_switch.clone();
            let clock_format_row = clock_format_row.clone();
            let clock_format_entry = clock_format_entry.clone();
            let clock_position_row = clock_position_row.clone();
            let clock_size_spin = clock_size_spin.clone();
            let clock_preview_label = clock_preview_label.clone();
            let preview_media = preview_media.clone();
            let preview_web = preview_web.clone();
            move || {
                let selected_child = mode_selector.selected_children().get(0).cloned();
                let mode = selected_child.map(|c| unsafe { c.data::<u32>("mode-index").unwrap().as_ref().clone() }).unwrap_or(0);
                let preview_path = preview_media_path(mode, &file_row, random_media_switch.is_active(), &media_files.borrow(), selected_media_preview.borrow().as_deref());
                let clock_format = clock_format_from_ui(&clock_format_row, &clock_format_entry);
                let (widget, media, web) = build_preview_widget(mode, &color_button, &gradient_start_button, &gradient_end_button, &pattern_row, &web_url_row, &stream_url_row, &file_row, &mute_switch, video_volume_spin.value() as u8, preview_path.as_deref(), clock_switch.is_active(), &clock_format, appearance::clock_position_from_index(clock_position_row.selected()), clock_size_spin.value() as u32, lang);
                preview_frame.set_child(Some(&widget));
                update_preview_tooltip(&preview_frame, mode, preview_path.as_deref(), lang);
                *preview_media.borrow_mut() = media;
                *preview_web.borrow_mut() = web;

                clock_preview_label.set_text(&appearance::format_clock_text(&clock_format));
                appearance::apply_clock_size(&clock_preview_label, clock_size_spin.value() as u32);
                appearance::apply_clock_position(&clock_preview_label, appearance::clock_position_from_index(clock_position_row.selected()), 20);
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
                start_hotkey_entry: start_hotkey_entry.clone(),
                stop_hotkey_entry: stop_hotkey_entry.clone(),
                autostart_switch: autostart_switch.clone(),
                mode_selector: mode_selector.clone(),
                stack: stack.clone(),
                color_button: color_button.clone(),
                gradient_start_button: gradient_start_button.clone(),
                gradient_end_button: gradient_end_button.clone(),
                pattern_row: pattern_row.clone(),
                web_url_row: web_url_row.clone(),
                stream_url_row: stream_url_row.clone(),
                file_row: file_row.clone(),
                file_info_row: file_info_row.clone(),
                slideshow_interval_row: slideshow_interval_row.clone(),
                mute_row: mute_row.clone(),
                volume_row: volume_row.clone(),
                random_row: random_row.clone(),
                slideshow_interval_spin: slideshow_interval_spin.clone(),
                mute_switch: mute_switch.clone(),
                video_volume_spin: video_volume_spin.clone(),
                random_media_switch: random_media_switch.clone(),
                media_files: media_files.clone(),
                selected_media_preview: selected_media_preview.clone(),
                media_list_row: media_list_row.clone(),
                media_list_box: media_list_box.clone(),
                remove_media_button: remove_media_button.clone(),
                clock_switch: clock_switch.clone(),
                clock_format_row: clock_format_row.clone(),
                clock_format_entry: clock_format_entry.clone(),
                clock_position_row: clock_position_row.clone(),
                clock_move_switch: clock_move_switch.clone(),
                clock_move_interval_row: clock_move_interval_row.clone(),
                clock_move_interval_spin: clock_move_interval_spin.clone(),
                clock_size_spin: clock_size_spin.clone(),
                clock_preview_label: clock_preview_label.clone(),
                inhibit_switch: inhibit_switch.clone(),
                power_integration_switch: power_integration_switch.clone(),
                lock_screen_switch: lock_screen_switch.clone(),
                mpris_pause_switch: mpris_pause_switch.clone(),
                app_inhibit_list: app_inhibit_list.clone(),
                app_inhibit_entry: app_inhibit_entry.clone(),
                app_inhibit_apps: app_inhibit_apps.clone(),
                delete_button: delete_button.clone(),
                panel_commands_list: panel_commands_list.clone(),
            },
            update_status: update_status.clone(),
            update_preview: update_preview.clone(),
        });

        add_command_btn.connect_clicked({
            let controller = controller.clone();
            move |_| {
                let presets = PanelCommand::presets();
                if presets.is_empty() {
                    return;
                }
                let Some(window) = controller.window_weak.upgrade() else { return; };
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
                        if let Some(idx) = response.strip_prefix("preset-").and_then(|v| v.parse::<usize>().ok()) {
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
                populate_activation_log_group(&activation_group, &activation_list, &config.activation_log, controller.lang);
                clear_log_btn.set_sensitive(false);
                controller.mark_modified();
            }
        });

        export_btn.connect_clicked({
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            move |_| {
                let Some(window) = controller.window_weak.upgrade() else { return; };
                let dialog = gtk4::FileDialog::new();
                dialog.set_title(tr(controller.lang, "Экспорт настроек"));
                dialog.set_accept_label(Some(tr(controller.lang, "Сохранить")));
                dialog.set_initial_name(Some("rs-screensaver-settings.json"));
                dialog.save(Some(&window), None::<&gio::Cancellable>, {
                    let controller = controller.clone();
                    let toast_overlay = toast_overlay.clone();
                    move |res| {
                        let Ok(file) = res else { return; };
                        let Some(path) = file.path() else { return; };
                        let index = controller.active_profile_index();
                        controller.apply_ui_to_profile(index);
                        let config = controller.config.borrow().clone();
                        let toast_lang = config.language;
                        let content = match serde_json::to_string_pretty(&config) {
                            Ok(content) => content,
                            Err(err) => {
                                toast_overlay.add_toast(adw::Toast::new(&tr(toast_lang, "Ошибка экспорта: {err}").replace("{err}", &err.to_string())));
                                return;
                            }
                        };
                        if let Err(err) = std::fs::write(&path, content) {
                            toast_overlay.add_toast(adw::Toast::new(&tr(toast_lang, "Не удалось сохранить файл: {err}").replace("{err}", &err.to_string())));
                            return;
                        }
                        toast_overlay.add_toast(adw::Toast::new(tr(toast_lang, "Настройки экспортированы")));
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
                let Some(window) = controller.window_weak.upgrade() else { return; };
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
                        let Ok(file) = res else { return; };
                        let Some(path) = file.path() else { return; };
                        let content = match std::fs::read_to_string(&path) {
                            Ok(content) => content,
                            Err(err) => {
                                toast_overlay.add_toast(adw::Toast::new(&tr(controller.lang, "Не удалось прочитать файл: {err}").replace("{err}", &err.to_string())));
                                return;
                            }
                        };
                        let mut config: Config = match serde_json::from_str(&content) {
                            Ok(cfg) => cfg,
                            Err(err) => {
                                toast_overlay.add_toast(adw::Toast::new(&tr(controller.lang, "Ошибка формата JSON: {err}").replace("{err}", &err.to_string())));
                                return;
                            }
                        };
                        config.normalize();
                        if let Err(err) = config.save() {
                            toast_overlay.add_toast(adw::Toast::new(&tr(controller.lang, "Не удалось сохранить файл: {err}").replace("{err}", &err.to_string())));
                            return;
                        }
                        let toast_lang = config.language;
                        *controller.config.borrow_mut() = config;
                        refresh_profile_model(&controller);
                        let profile_index = controller.config.borrow().active_profile_index();
                        controller.apply_profile_to_ui(profile_index);
                        let config = controller.config.borrow();
                        populate_activation_log_group(&activation_group, &activation_list, &config.activation_log, controller.lang);
                        clear_log_btn.set_sensitive(!config.activation_log.is_empty());
                        runtime_row.set_subtitle(&format_runtime(config.total_runtime_seconds, controller.lang));
                        controller.set_modified(false);
                        toast_overlay.add_toast(adw::Toast::new(tr(toast_lang, "Настройки импортированы")));
                    }
                });
            }
        });

        about_button.connect_clicked({
            let controller = controller.clone();
            let window_weak = window.downgrade();
            move |_| {
                let Some(window) = window_weak.upgrade() else { return; };
                let version = env!("CARGO_PKG_VERSION");
                let display_version = version
                    .rsplit_once('.')
                    .map(|(base, patch)| if patch == "0" { base } else { version })
                    .unwrap_or(version);
                let dialog = adw::AboutDialog::builder()
                    .application_name("RS Screensaver")
                    .application_icon(crate::ui::app_icon_name())
                    .developer_name("leocallidus")
                    .version(display_version)
                    .comments(tr(controller.lang, "Простой скринсейвер сделанный на Rust и GTK4"))
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
                controller.ui.profile_model.splice(index as u32, 1, &[name.as_str()]);
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
                    toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Достигнут лимит профилей")));
                    return;
                }
                let new_index = config.profiles.len();
                let name = profile_name(controller.lang, new_index + 1);
                config.profiles.push(SettingsProfile::new(name.clone()));
                controller.ui.profile_model.append(&name);
                drop(config);
                controller.apply_profile_to_ui(new_index);
                controller.set_modified(true);
            }
        });

        let reset_handler = Rc::new({
            let controller = controller.clone();
            let window_weak = window.downgrade();
            move || {
                let Some(window) = window_weak.upgrade() else { return; };
                let dialog = adw::MessageDialog::builder()
                    .transient_for(&window)
                    .modal(true)
                    .heading(tr(controller.lang, "Сбросить профиль?"))
                    .body(tr(controller.lang, "Настройки текущего профиля будут сброшены к значениям по умолчанию."))
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
                    toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Нельзя удалить последний профиль")));
                    return;
                }
                let Some(window) = window_weak.upgrade() else { return; };
                let dialog = adw::MessageDialog::builder()
                    .transient_for(&window)
                    .modal(true)
                    .heading(tr(controller.lang, "Удалить профиль?"))
                    .body(tr(controller.lang, "Профиль будет удалён без возможности восстановления."))
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
                                toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Нельзя удалить последний профиль")));
                                dialog.close();
                                return;
                            }
                            let index = controller.active_profile_index();
                            if index < config.profiles.len() {
                                config.profiles.remove(index);
                                controller.ui.profile_model.remove(index as u32);
                                let new_index = if index > 0 { index - 1 } else { 0 };
                                config.active_profile = new_index as u8;
                                drop(config);
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
                    toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Язык будет применён после перезапуска приложения")));
                }
                controller.mark_modified();
            }
        });

        start_hotkey_entry.connect_notify_local(Some("text"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        stop_hotkey_entry.connect_notify_local(Some("text"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        setup_hotkey_capture(&start_hotkey_entry, controller.clone(), toast_overlay.clone());
        setup_hotkey_capture(&stop_hotkey_entry, controller.clone(), toast_overlay.clone());

        mode_selector.connect_selected_children_changed({
            let controller = controller.clone();
            move |_| {
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                if let Some(path) = file_row_path(&controller.ui.file_row) {
                    let expects_dir = mode_idx == 6;
                    let expects_file = matches!(mode_idx, 4 | 5);
                    let should_clear = (expects_dir && path.is_file())
                        || (expects_file && path.is_dir());
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
                update_file_info_row(&controller.ui.file_info_row, file_row_path(&controller.ui.file_row), mode_idx, controller.lang);
                if !matches!(mode_idx, 4 | 5) {
                    controller.ui.selected_media_preview.borrow_mut().take();
                    controller.ui.remove_media_button.set_sensitive(false);
                }
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

        video_volume_spin.connect_value_changed({
            let controller = controller.clone();
            move |_| {
                controller.mark_modified();
                (controller.update_preview)();
                (controller.update_status)();
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
                let Some(window) = window_weak.upgrade() else { return; };
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
                            let Ok(folder) = res else { return; };
                            let Some(path) = folder.path() else { return; };
                            if let Err(msg) = validate_media_path(&path, content::MediaKind::ImageFolder, false, controller.lang) {
                                toast_overlay.add_toast(adw::Toast::new(&msg));
                                return;
                            }
                            let path_str = path.to_string_lossy().to_string();
                            set_file_row_path(&controller.ui.file_row, &controller.ui.file_info_row, Some(path_str.as_str()), tr(controller.lang, "Папка не выбрана"), controller.lang);
                            update_file_info_row(&controller.ui.file_info_row, Some(path), mode_idx, controller.lang);
                            controller.mark_modified();
                            (controller.update_preview)();
                            (controller.update_status)();
                        }
                    });
                    return;
                }
                if !matches!(mode_idx, 4 | 5) {
                    toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Сначала выберите режим")));
                    return;
                }
                let kind = if mode_idx == 4 { content::MediaKind::Image } else { content::MediaKind::Video };
                let dialog = gtk4::FileDialog::new();
                dialog.set_title(tr(controller.lang, "Выбрать файл"));
                if let Some(path) = file_row_path(&controller.ui.file_row) {
                    dialog.set_initial_file(Some(&gio::File::for_path(path)));
                }
                dialog.open(Some(&window), None::<&gio::Cancellable>, {
                    let controller = controller.clone();
                    let toast_overlay = toast_overlay.clone();
                    move |res| {
                        let Ok(file) = res else { return; };
                        let Some(path) = file.path() else { return; };
                        if let Err(msg) = validate_media_path(&path, kind, false, controller.lang) {
                            toast_overlay.add_toast(adw::Toast::new(&msg));
                            return;
                        }
                        let path_str = path.to_string_lossy().to_string();
                        set_file_row_path(&controller.ui.file_row, &controller.ui.file_info_row, Some(path_str.as_str()), tr(controller.lang, "Файл не выбран"), controller.lang);
                        update_file_info_row(&controller.ui.file_info_row, Some(path), mode_idx, controller.lang);
                        controller.mark_modified();
                        (controller.update_preview)();
                        (controller.update_status)();
                    }
                });
            }
        });

        add_media_button.connect_clicked({
            let controller = controller.clone();
            let window_weak = window.downgrade();
            let toast_overlay = toast_overlay.clone();
            move |_| {
                let Some(window) = window_weak.upgrade() else { return; };
                let mode_idx = selected_mode_index(&controller.ui.mode_selector);
                if !matches!(mode_idx, 4 | 5) {
                    toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Список доступен только для изображения или видео")));
                    return;
                }
                let kind = if mode_idx == 4 { content::MediaKind::Image } else { content::MediaKind::Video };
                let dialog = gtk4::FileDialog::new();
                dialog.set_title(tr(controller.lang, "Добавить медиафайлы"));
                dialog.open_multiple(Some(&window), None::<&gio::Cancellable>, {
                    let controller = controller.clone();
                    let toast_overlay = toast_overlay.clone();
                    move |res| {
                        let Ok(list_model) = res else { return; };
                        let mut skipped = false;
                        let mut added = 0usize;
                        let mut files = controller.ui.media_files.borrow_mut();
                        for i in 0..list_model.n_items() {
                            let Some(item) = list_model.item(i) else { continue; };
                            let file = item.downcast::<gio::File>().ok();
                            let Some(file) = file else { continue; };
                            let Some(path) = file.path() else { continue; };
                            if let Err(_) = validate_media_path(&path, kind, false, controller.lang) {
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
                            sync_media_list_box(&controller.ui.media_list_box, &controller.ui.media_files.borrow(), controller.lang);
                            controller.mark_modified();
                            (controller.update_status)();
                            (controller.update_preview)();
                        }
                        if skipped {
                            toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Некоторые файлы пропущены")));
                        }
                    }
                });
            }
        });

        remove_media_button.connect_clicked({
            let controller = controller.clone();
            move |_| {
                let Some(row) = controller.ui.media_list_box.selected_row() else { return; };
                let Some(path) = media_list_row_path(&row) else { return; };
                let mut files = controller.ui.media_files.borrow_mut();
                if let Some(pos) = files.iter().position(|p| p == &path) {
                    files.remove(pos);
                }
                drop(files);
                *controller.ui.selected_media_preview.borrow_mut() = None;
                sync_media_list_box(&controller.ui.media_list_box, &controller.ui.media_files.borrow(), controller.lang);
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

        clock_format_row.connect_notify_local(Some("selected"), {
            let controller = controller.clone();
            move |row, _| {
                let selected = row.selected() as usize;
                if selected < CLOCK_FORMAT_PATTERNS.len() {
                    controller.ui.clock_format_entry.set_text(CLOCK_FORMAT_PATTERNS[selected]);
                    controller.ui.clock_format_entry.set_visible(false);
                } else {
                    controller.ui.clock_format_entry.set_visible(true);
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
                controller.ui.clock_move_interval_row.set_visible(controller.ui.clock_move_switch.is_active());
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

        power_integration_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
                controller.mark_modified();
                (controller.update_status)();
            }
        });

        lock_screen_switch.connect_notify_local(Some("active"), {
            let controller = controller.clone();
            move |_, _| {
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
                let Some(win) = window_weak.upgrade() else { return; };
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

    pub fn show(&self) { self.window.present(); }
}

fn update_preview_tooltip(preview_frame: &Frame, mode: u32, preview_path: Option<&std::path::Path>, lang: Language) {
    if matches!(mode, 4 | 5 | 6) {
        if let Some(path) = preview_path {
            preview_frame.set_tooltip_text(Some(&tr(lang, "{path}\nНажмите, чтобы скопировать").replace("{path}", &path.to_string_lossy())));
            return;
        }
    }
    preview_frame.set_tooltip_text(None);
}

fn capture_window_geometry(window: &adw::Window) -> Option<WindowGeometry> {
    let (w, h) = (window.width(), window.height());
    if w <= 0 || h <= 0 { return None; }
    let mut geom = WindowGeometry { width: w, height: h, x: None, y: None };
    if let Some(display) = Display::default() {
        if display.backend().is_x11() {
            if let Some(surface) = window.surface() {
                if let Ok(x11) = surface.downcast::<gdk4_x11::X11Surface>() {
                    if let Ok((conn, screen)) = x11rb::connect(None) {
                        use x11rb::connection::Connection as _;
                        use x11rb::protocol::xproto::ConnectionExt as _;
                        let root = conn.setup().roots[screen].root;
                        if let Ok(cookie) = conn.translate_coordinates(x11.xid() as u32, root, 0, 0) {
                            if let Ok(reply) = cookie.reply() {
                                geom.x = Some(reply.dst_x as i32); geom.y = Some(reply.dst_y as i32);
                            }
                        }
                    }
                }
            }
        }
    }
    Some(geom)
}

fn persist_window_geometry(window: &adw::Window, original_config: &Rc<RefCell<Config>>, modified: &Cell<bool>) {
    if let Some(geometry) = capture_window_geometry(window) {
        let mut config = original_config.borrow().clone();
        config.settings_window = Some(geometry);
        *original_config.borrow_mut() = config.clone();
        if !modified.get() {
            let _ = config.save();
        }
    }
}

fn build_status_text(profile_name_row: &adw::EntryRow, inactivity_spin: &SpinButton, mouse_wake_spin: &SpinButton, mode_index: u32, color_button: &ColorDialogButton, gradient_start_button: &ColorDialogButton, gradient_end_button: &ColorDialogButton, pattern_row: &adw::ComboRow, web_url_row: &adw::EntryRow, stream_url_row: &adw::EntryRow, file_row: &adw::ActionRow, file_info_row: &adw::ActionRow, mute_switch: &Switch, video_volume_spin: &SpinButton, fade_switch: &Switch, inhibit_switch: &Switch, power_integration_switch: &Switch, lock_screen_switch: &Switch, clock_switch: &Switch, clock_format_entry: &adw::EntryRow, clock_position_row: &adw::ComboRow, clock_move_switch: &Switch, clock_move_interval_spin: &SpinButton, clock_size_spin: &SpinButton, slideshow_interval_spin: &SpinButton, random_media_switch: &Switch, media_files: &Rc<RefCell<Vec<String>>>, start_hotkey_entry: &Entry, stop_hotkey_entry: &Entry, lang: Language) -> String {
    let name = profile_name_row.text().to_string();
    let mode_idx = mode_index;
    let mode_lbl = content::mode_label(mode_idx, lang);
    let mut detail = String::new();
    match mode_idx {
        0 => detail = content::rgba_to_hex(color_button.rgba()),
        1 => detail = format!("{} -> {}", content::rgba_to_hex(gradient_start_button.rgba()), content::rgba_to_hex(gradient_end_button.rgba())),
        2 => detail = content::pattern_label(pattern_row.selected(), lang).to_string(),
        3 => detail = web_url_row.text().to_string(),
        7 => detail = stream_url_row.text().to_string(),
        4 | 5 => detail = file_row.subtitle().map(|s| s.to_string()).unwrap_or_default(),
        _ => {}
    }
    let list_count = media_files.borrow().len();
    if random_media_switch.is_active() && list_count > 0 { detail = tr(lang, "Список: {list_count}").replace("{list_count}", &list_count.to_string()); }
    let info = file_info_row.subtitle().map(|s| s.to_string()).unwrap_or_default();
    let mode_txt = if detail.is_empty() { mode_lbl.to_string() } else { format!("{mode_lbl}: {detail}") };
    let mode_txt = if info.is_empty() || info == tr(lang, "Нет данных") { mode_txt } else { format!("{mode_txt} • {info}") };
    let ss_suffix = if mode_idx == 6 { format!(" • {}", tr(lang, "Интервал: {slideshow_interval}с").replace("{slideshow_interval}", &(slideshow_interval_spin.value() as u64).to_string())) } else { String::new() };
    let vol_suffix = if matches!(mode_idx, 5 | 7) { format!(" • {}", tr(lang, "Громкость: {volume}%").replace("{volume}", &(video_volume_spin.value() as u32).to_string())) } else { String::new() };
    let clock_txt = if clock_switch.is_active() {
        let pos = appearance::clock_position_label(appearance::clock_position_from_index(clock_position_row.selected()), lang);
        if clock_move_switch.is_active() { tr(lang, "Да ({clock_format}, {clock_position}, {clock_size}пт, {interval}с)").replace("{clock_format}", &clock_format_entry.text()).replace("{clock_position}", pos).replace("{clock_size}", &(clock_size_spin.value() as u32).to_string()).replace("{interval}", &(clock_move_interval_spin.value() as u64).to_string()) }
        else { tr(lang, "Да ({clock_format}, {clock_position}, {clock_size}пт)").replace("{clock_format}", &clock_format_entry.text()).replace("{clock_position}", pos).replace("{clock_size}", &(clock_size_spin.value() as u32).to_string()) }
    } else { tr(lang, "Нет").to_string() };
    tr(lang, "Профиль: {profile_name} • Режим: {mode_text}{slideshow_suffix} • Таймер: {inactivity}с • Задержка мыши: {mouse_delay_ms}мс • Без звука: {mute}{volume_suffix} • Сон: {inhibit} • Интеграция питания: {power_integration} • Блокировка: {lock_screen} • Часы: {clock} • Fade: {fade} • ГК: {start_hotkey}/{stop_hotkey}")
        .replace("{profile_name}", &name).replace("{mode_text}", &mode_txt).replace("{slideshow_suffix}", &ss_suffix).replace("{inactivity}", &(inactivity_spin.value() as u64).to_string()).replace("{mouse_delay_ms}", &(mouse_wake_spin.value() as u64).to_string()).replace("{mute}", yes_no(lang, mute_switch.is_active())).replace("{volume_suffix}", &vol_suffix).replace("{inhibit}", yes_no(lang, inhibit_switch.is_active())).replace("{power_integration}", yes_no(lang, power_integration_switch.is_active())).replace("{lock_screen}", yes_no(lang, lock_screen_switch.is_active())).replace("{clock}", &clock_txt).replace("{fade}", yes_no(lang, fade_switch.is_active())).replace("{start_hotkey}", &start_hotkey_entry.text()).replace("{stop_hotkey}", &stop_hotkey_entry.text())
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
    }
}

fn set_stack_for_mode(stack: &Stack, mode_idx: u32) {
    match mode_idx {
        0 => stack.set_visible_child_name("color_page"),
        1 => stack.set_visible_child_name("gradient_page"),
        2 => stack.set_visible_child_name("pattern_page"),
        3 => stack.set_visible_child_name("web_page"),
        4 | 5 | 6 | 7 => stack.set_visible_child_name("file_page"),
        _ => {}
    }
}

fn set_content_mode_visibility(
    stream_url_row: &adw::EntryRow,
    file_row: &adw::ActionRow,
    file_info_row: &adw::ActionRow,
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
    let is_file_path = is_image || is_video || is_slideshow;
    let list_visible = is_image || is_video;
    stream_url_row.set_visible(is_stream);
    file_row.set_visible(is_file_path);
    file_info_row.set_visible(is_file_path);
    slideshow_interval_row.set_visible(is_slideshow);
    mute_row.set_visible(is_video || is_stream);
    volume_row.set_visible(is_video || is_stream);
    random_row.set_visible(list_visible);
    media_list_row.set_visible(list_visible);
    media_list_box.set_visible(list_visible);
}

fn set_file_row_path(file_row: &adw::ActionRow, file_info_row: &adw::ActionRow, path: Option<&str>, empty_label: &str, lang: Language) {
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

fn update_file_info_row(file_info_row: &adw::ActionRow, path: Option<std::path::PathBuf>, mode_idx: u32, lang: Language) {
    let Some(path) = path else {
        file_info_row.set_subtitle(tr(lang, "Нет данных"));
        return;
    };
    let kind = match mode_idx {
        4 => content::MediaKind::Image,
        5 => content::MediaKind::Video,
        6 => content::MediaKind::ImageFolder,
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
            let dims = image::open(&path)
                .ok()
                .map(|img| {
                    let (w, h) = img.dimensions();
                    format!("{w}x{h}")
                });
            if let Some(dims) = dims {
                format!("{dims} - {size_txt}")
            } else {
                size_txt
            }
        }
        content::MediaKind::Video => {
            std::fs::metadata(&path)
                .ok()
                .map(|m| format_file_size(m.len(), lang))
                .unwrap_or_else(|| tr(lang, "Нет данных").to_string())
        }
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
        unsafe { row.set_data("media-path", path.to_string()); }
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
        toast_overlay.add_toast(adw::Toast::new(&tr(controller.lang, "Не удалось сохранить файл: {err}").replace("{err}", &err.to_string())));
        return;
    }
    let toast_lang = config.language;
    *controller.config.borrow_mut() = config.clone();
    controller.set_modified(false);
    let _ = sender.send(AppMessage::UpdateConfig(config));
    toast_overlay.add_toast(adw::Toast::new(tr(toast_lang, "Настройки сохранены")));
}

fn refresh_profile_model(controller: &SettingsController) {
    let names: Vec<String> = controller.config.borrow().profiles.iter().map(|p| p.name.clone()).collect();
    let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let total = controller.ui.profile_model.n_items();
    controller.ui.profile_model.splice(0, total, &name_refs);
    controller.ui.profile_dropdown.set_selected(controller.config.borrow().active_profile_index() as u32);
}

fn sync_panel_commands_list(controller: &Rc<SettingsController>) {
    let commands = controller.config.borrow().active_profile().panel_commands.clone();
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
        let enabled_switch = Switch::builder().valign(Align::Center).active(command.enabled).build();
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
                let Some(index) = command_index(switch) else { return; };
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
                let Some(index) = command_index(button) else { return; };
                open_panel_command_editor(&controller, index);
            }
        });

        delete_button.connect_clicked({
            let controller = controller.clone();
            move |button| {
                let Some(index) = command_index(button) else { return; };
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
        let switch = Switch::builder().valign(Align::Center).active(active).build();
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
    unsafe { widget.as_ref().data::<u32>("command-index").map(|v| *v.as_ref() as usize) }
}

fn open_panel_command_editor(controller: &Rc<SettingsController>, index: usize) {
    let Some(window) = controller.window_weak.upgrade() else { return; };
    let command = {
        let config = controller.config.borrow();
        let profile_idx = config.active_profile_index();
        config.profiles.get(profile_idx).and_then(|p| p.panel_commands.get(index)).cloned()
    };
    let Some(command) = command else { return; };

    let dialog = adw::MessageDialog::builder()
        .transient_for(&window)
        .modal(true)
        .heading(tr(controller.lang, "Редактировать"))
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    let name_row = adw::EntryRow::builder().title(tr(controller.lang, "Название")).build();
    name_row.set_text(&command.name);
    let show_row = adw::EntryRow::builder().title(tr(controller.lang, "Команда показа")).build();
    show_row.set_text(&command.show_command);
    let hide_row = adw::EntryRow::builder().title(tr(controller.lang, "Команда скрытия")).build();
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
    path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

fn clock_format_from_ui(clock_format_row: &adw::ComboRow, clock_format_entry: &adw::EntryRow) -> String {
    let selected = clock_format_row.selected() as usize;
    if selected < CLOCK_FORMAT_PATTERNS.len() {
        CLOCK_FORMAT_PATTERNS[selected].to_string()
    } else {
        clock_format_entry.text().to_string()
    }
}

fn setup_hotkey_capture(entry: &Entry, controller: Rc<SettingsController>, toast_overlay: adw::ToastOverlay) {
    let entry = entry.clone();
    let entry_clone = entry.clone();
    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed(move |_, keyval, _keycode, state| {
        if controller.profile_update_guard.get() {
            return glib::Propagation::Stop;
        }
        if keyval == Key::BackSpace {
            entry_clone.set_text("");
            controller.mark_modified();
            (controller.update_status)();
            return glib::Propagation::Stop;
        }
        if keyval == Key::Escape || is_modifier_key(keyval) {
            return glib::Propagation::Stop;
        }
        let mods = filtered_modifiers(state);
        let valid = is_function_key(keyval) || has_primary_modifier(mods);
        if !valid {
            toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Некорректный формат")));
            return glib::Propagation::Stop;
        }
        let accel = gtk4::accelerator_name(keyval, mods);
        if accel.is_empty() {
            toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Некорректный формат")));
            return glib::Propagation::Stop;
        }
        entry_clone.set_text(&accel);
        controller.mark_modified();
        (controller.update_status)();
        glib::Propagation::Stop
    });
    entry.add_controller(key_controller);
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
    let paintable = icon_theme.lookup_by_gicon(
        icon,
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
    Some(picture)
}
