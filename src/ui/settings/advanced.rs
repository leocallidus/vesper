use gtk4::prelude::*;
use gtk4::{Button, ListBox, SelectionMode};
use libadwaita as adw;
use libadwaita::prelude::*;
use crate::config::{Config, ActivationLogEntry};
use crate::i18n::{Language, tr};
use glib::DateTime;

pub struct AdvancedWidgets {
    pub panel_commands_group: adw::PreferencesGroup,
    pub status_group: adw::PreferencesGroup,
    pub status_row: adw::ActionRow, // Added
    pub export_import_group: adw::PreferencesGroup,
    pub about_group: adw::PreferencesGroup,
    pub about_button: Button,
    pub panel_commands_list: gtk4::ListBox,
    pub add_command_btn: Button,
    pub runtime_row: adw::ActionRow,
    pub activation_group: adw::PreferencesGroup, // Nested in status
    pub activation_list: ListBox,
    pub clear_log_btn: Button,
    pub export_btn: Button,
    pub import_btn: Button,
    pub reset_btn: Button,
}

const MAX_ACTIVATION_LOG_ROWS: usize = 10;

pub fn build_advanced_groups(config: &Config, lang: Language) -> AdvancedWidgets {
    // Group 4: Panel commands
    let panel_commands_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Команды панелей"))
        .description(tr(lang, "Команды для скрытия/показа панелей при активации скринсейвера"))
        .build();

    let panel_commands_list = gtk4::ListBox::new();
    panel_commands_list.add_css_class("boxed-list");
    panel_commands_group.add(&panel_commands_list);

    let add_command_row = adw::ActionRow::builder()
        .title(tr(lang, "Добавить из списка"))
        .build();
    let add_command_btn = Button::with_label(tr(lang, "Добавить"));
    add_command_row.add_suffix(&add_command_btn);
    panel_commands_group.add(&add_command_row);
    
    // Group 5: Status
    let status_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Статус"))
        .build();

    let status_row = adw::ActionRow::builder()
        .title(tr(lang, "Текущие настройки"))
        .subtitle("—")
        .use_markup(false)
        .build();
    status_group.add(&status_row);
    
    let runtime_row = adw::ActionRow::builder()
        .title(tr(lang, "Общее время работы"))
        .subtitle(format_runtime(config.total_runtime_seconds, lang))
        .build();
    status_group.add(&runtime_row);

    let activation_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "История активаций"))
        .build();
    let activation_list = ListBox::new();
    activation_list.add_css_class("boxed-list");
    activation_list.set_selection_mode(SelectionMode::None);
    activation_group.add(&activation_list);
    populate_activation_log_group(&activation_group, &activation_list, &config.activation_log, lang);
    status_group.add(&activation_group);

    let clear_log_row = adw::ActionRow::builder()
        .title(tr(lang, "История активаций"))
        .build();
    let clear_log_btn = Button::with_label(tr(lang, "Очистить"));
    clear_log_btn.set_sensitive(!config.activation_log.is_empty());
    clear_log_btn.add_css_class("destructive-action");
    clear_log_row.add_suffix(&clear_log_btn);
    status_group.add(&clear_log_row);

    // Group 6: Export/Import
    let export_import_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Экспорт/импорт"))
        .build();

    let export_row = adw::ActionRow::builder()
        .title(tr(lang, "Экспорт настроек"))
        .subtitle(tr(lang, "Сохранить настройки в JSON"))
        .build();
    let export_btn = Button::with_label(tr(lang, "Экспорт"));
    export_btn.add_css_class("suggested-action");
    export_row.add_suffix(&export_btn);
    export_import_group.add(&export_row);

    let import_row = adw::ActionRow::builder()
        .title(tr(lang, "Импорт настроек"))
        .subtitle(tr(lang, "Загрузить настройки из JSON"))
        .build();
    let import_btn = Button::with_label(tr(lang, "Импорт"));
    import_row.add_suffix(&import_btn);
    export_import_group.add(&import_row);

    let reset_row = adw::ActionRow::builder()
        .title(tr(lang, "Сбросить"))
        .subtitle(tr(lang, "Сбросить профиль?"))
        .build();
    let reset_btn = Button::with_label(tr(lang, "Сбросить"));
    reset_btn.add_css_class("destructive-action");
    reset_row.add_suffix(&reset_btn);
    export_import_group.add(&reset_row);

    // Group 7: About
    let about_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "О программе"))
        .build();
    let about_row = adw::ActionRow::builder()
        .title("RS Screensaver")
        .subtitle(tr(lang, "Простой скринсейвер сделанный на Rust и GTK4"))
        .build();
    let about_button = Button::with_label(tr(lang, "Открыть"));
    about_row.add_suffix(&about_button);
    about_group.add(&about_row);

    AdvancedWidgets {
        panel_commands_group,
        status_group,
        status_row, // Added
        export_import_group,
        about_group,
        about_button,
        panel_commands_list,
        add_command_btn,
        runtime_row,
        activation_group,
        activation_list,
        clear_log_btn,
        export_btn,
        import_btn,
        reset_btn,
    }
}

pub fn format_runtime(seconds: u64, lang: Language) -> String {
    if seconds == 0 {
        return format!("0{}", tr(lang, "с"));
    }
    let mut remaining = seconds;
    let days = remaining / 86_400;
    remaining %= 86_400;
    let hours = remaining / 3600;
    remaining %= 3600;
    let minutes = remaining / 60;
    let secs = remaining % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}{}", days, tr(lang, "д")));
    }
    if hours > 0 {
        parts.push(format!("{}{}", hours, tr(lang, "ч")));
    }
    if minutes > 0 {
        parts.push(format!("{}{}", minutes, tr(lang, "м")));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(format!("{}{}", secs, tr(lang, "с")));
    }
    parts.join(" ")
}

pub fn format_activation_timestamp(timestamp: u64) -> String {
    let safe_ts = timestamp.min(i64::MAX as u64) as i64;
    if let Ok(dt) = DateTime::from_unix_local(safe_ts) {
        if let Ok(text) = dt.format("%d.%m.%Y %H:%M:%S") {
            return text.to_string();
        }
    }
    timestamp.to_string()
}

pub fn populate_activation_log_group(
    group: &adw::PreferencesGroup,
    list: &ListBox,
    entries: &[ActivationLogEntry],
    lang: Language,
) {
    let mut child = list.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        list.remove(&widget);
        child = next;
    }

    let total = entries.len();
    let shown = total.min(MAX_ACTIVATION_LOG_ROWS);
    let description = if total == 0 {
        tr(lang, "Нет записей").to_string()
    } else if total <= MAX_ACTIVATION_LOG_ROWS {
        tr(lang, "Последние {shown} запусков").replace("{shown}", &shown.to_string())
    } else {
        tr(lang, "Последние {shown} из {total}")
            .replace("{shown}", &shown.to_string())
            .replace("{total}", &total.to_string())
    };
    group.set_description(Some(&description));

    if entries.is_empty() {
        let row = adw::ActionRow::builder()
            .title(tr(lang, "Записей пока нет"))
            .build();
        row.set_sensitive(false);
        list.append(&row);
        return;
    }

    for entry in entries.iter().rev().take(MAX_ACTIVATION_LOG_ROWS) {
        let title = format_activation_timestamp(entry.timestamp);
        let profile_name = entry.profile_name.trim();
        let profile_name = if profile_name.is_empty() { "—" } else { profile_name };
        let mode = entry.mode.trim();
        let mode = if mode.is_empty() { "—" } else { mode };
        let subtitle = format!("{profile_name} • {mode}");
        let row = adw::ActionRow::builder()
            .title(&title)
            .subtitle(&subtitle)
            .use_markup(false)
            .build();
        list.append(&row);
    }
}
