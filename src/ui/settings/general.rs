use gtk4::prelude::*;
use gtk4::{Align, SpinButton, Switch, Entry};
use libadwaita as adw;
use libadwaita::prelude::*;
use crate::config::Config;
use crate::i18n::{Language, tr, language_label, language_index};

pub struct GeneralWidgets {
    pub general_group: adw::PreferencesGroup,
    pub hotkeys_group: adw::PreferencesGroup,
    pub inactivity_spin: SpinButton,
    pub mouse_wake_spin: SpinButton,
    pub fade_switch: Switch,
    pub language_row: adw::ComboRow,
    pub start_hotkey_entry: Entry,
    pub stop_hotkey_entry: Entry,
}

pub fn build_general_group(config: &Config, lang: Language) -> GeneralWidgets {
    // Group 1: General
    let general_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Общие"))
        .build();

    let inactivity_row = adw::ActionRow::builder()
        .title(tr(lang, "Интервал неактивности"))
        .subtitle(tr(lang, "Время бездействия в секундах"))
        .build();
        
    let inactivity_spin = SpinButton::with_range(10.0, 3600.0, 10.0);
    inactivity_spin.set_value(config.active_profile().inactivity_seconds as f64);
    inactivity_spin.set_valign(Align::Center);
    inactivity_row.add_suffix(&inactivity_spin);

    let mouse_wake_row = adw::ActionRow::builder()
        .title(tr(lang, "Задержка реакции на мышь"))
        .subtitle(tr(lang, "Миллисекунды"))
        .build();
    let mouse_wake_spin = SpinButton::with_range(0.0, 5000.0, 100.0);
    mouse_wake_spin.set_value(config.active_profile().mouse_wake_delay_ms as f64);
    mouse_wake_spin.set_valign(Align::Center);
    mouse_wake_row.add_suffix(&mouse_wake_spin);

    let fade_row = adw::ActionRow::builder()
        .title(tr(lang, "Плавное появление/исчезновение"))
        .build();
    let fade_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().fade_enabled)
        .build();
    fade_row.add_suffix(&fade_switch);

    let language_labels = [
        language_label(lang, Language::Auto),
        language_label(lang, Language::Russian),
        language_label(lang, Language::English),
    ];
    let language_model = gtk4::StringList::new(&language_labels);
    let language_row = adw::ComboRow::builder()
        .title(tr(lang, "Язык интерфейса"))
        .model(&language_model)
        .build();
    language_row.set_selected(language_index(config.language));

    general_group.add(&inactivity_row);
    general_group.add(&mouse_wake_row);
    general_group.add(&fade_row);
    general_group.add(&language_row);

    // Group 1.5: Hotkeys
    let hotkeys_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Горячие клавиши"))
        .description(tr(lang, "Работают при активном окне приложения. Backspace — очистить"))
        .build();

    let start_hotkey_row = adw::ActionRow::builder()
        .title(tr(lang, "Запуск скринсейвера"))
        .build();
    let start_hotkey_entry = Entry::new();
    start_hotkey_entry.set_hexpand(true);
    start_hotkey_entry.set_valign(Align::Center);
    start_hotkey_entry.set_editable(false);
    start_hotkey_entry.set_can_focus(true);
    start_hotkey_entry.set_placeholder_text(Some(tr(lang, "Нажмите комбинацию")));
    start_hotkey_entry.set_text(&config.hotkey_start);
    start_hotkey_row.add_suffix(&start_hotkey_entry);
    start_hotkey_row.set_activatable_widget(Some(&start_hotkey_entry));

    let stop_hotkey_row = adw::ActionRow::builder()
        .title(tr(lang, "Остановка скринсейвера"))
        .build();
    let stop_hotkey_entry = Entry::new();
    stop_hotkey_entry.set_hexpand(true);
    stop_hotkey_entry.set_valign(Align::Center);
    stop_hotkey_entry.set_editable(false);
    stop_hotkey_entry.set_can_focus(true);
    stop_hotkey_entry.set_placeholder_text(Some(tr(lang, "Нажмите комбинацию")));
    stop_hotkey_entry.set_text(&config.hotkey_stop);
    stop_hotkey_row.add_suffix(&stop_hotkey_entry);
    stop_hotkey_row.set_activatable_widget(Some(&stop_hotkey_entry));

    hotkeys_group.add(&start_hotkey_row);
    hotkeys_group.add(&stop_hotkey_row);

    GeneralWidgets {
        general_group,
        hotkeys_group,
        inactivity_spin,
        mouse_wake_spin,
        fade_switch,
        language_row,
        start_hotkey_entry,
        stop_hotkey_entry,
    }
}
