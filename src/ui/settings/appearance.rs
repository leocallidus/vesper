use gtk4::prelude::*;
use gtk4::{Align, SpinButton, Switch, Label};
use libadwaita as adw;
use libadwaita::prelude::*;
use crate::config::{Config, ClockPosition, DEFAULT_CLOCK_FORMAT};
use crate::i18n::{Language, tr};

pub struct AppearanceWidgets {
    pub group: adw::PreferencesGroup,
    pub clock_switch: Switch,
    pub clock_format_row: adw::ComboRow,
    pub clock_format_entry: adw::EntryRow,
    pub clock_position_row: adw::ComboRow,
    pub clock_move_switch: Switch,
    pub clock_move_interval_row: adw::ActionRow,
    pub clock_move_interval_spin: SpinButton,
    pub clock_size_spin: SpinButton,
    pub clock_preview_group: adw::PreferencesGroup,
    pub clock_preview_label: Label,
}

pub const CLOCK_FORMAT_LABEL_KEYS: [&str; 8] = [
    "24ч: %H:%M",
    "24ч: %H:%M:%S",
    "12ч: %I:%M %p",
    "Дата: %d.%m.%Y",
    "Дата и время: %d.%m.%Y %H:%M",
    "День недели: %a %H:%M",
    "Полная дата: %A, %d %B %Y",
    "ISO: %F %T",
];

pub const CLOCK_FORMAT_PATTERNS: [&str; 8] = [
    "%H:%M",
    "%H:%M:%S",
    "%I:%M %p",
    "%d.%m.%Y",
    "%d.%m.%Y %H:%M",
    "%a %H:%M",
    "%A, %d %B %Y",
    "%F %T",
];

pub const CLOCK_POSITION_LABEL_KEYS: [&str; 9] = [
    "Сверху слева",
    "Сверху по центру",
    "Сверху справа",
    "По центру слева",
    "По центру",
    "По центру справа",
    "Снизу слева",
    "Снизу по центру",
    "Снизу справа",
];

pub fn build_appearance_group(config: &Config, lang: Language) -> AppearanceWidgets {
    let clock_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Часы"))
        .description(tr(lang, "Отображаются поверх скринсейвера"))
        .build();

    let clock_switch_row = adw::ActionRow::builder()
        .title(tr(lang, "Показывать часы"))
        .build();
    let clock_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().show_clock)
        .build();
    clock_switch_row.add_suffix(&clock_switch);
    clock_group.add(&clock_switch_row);

    let clock_format_labels: Vec<&str> = CLOCK_FORMAT_LABEL_KEYS
        .iter()
        .map(|label| tr(lang, label))
        .collect();
    let mut labels_with_custom = clock_format_labels.clone();
    labels_with_custom.push(tr(lang, "Пользовательский"));
    let clock_format_model = gtk4::StringList::new(&labels_with_custom);
    let clock_format_row = adw::ComboRow::builder()
        .title(tr(lang, "Формат"))
        .model(&clock_format_model)
        .build();
    
    let current_fmt = &config.active_profile().clock_format;
    let mut selected_fmt_idx = CLOCK_FORMAT_PATTERNS.len();
    for (i, pattern) in CLOCK_FORMAT_PATTERNS.iter().enumerate() {
        if pattern == current_fmt {
            selected_fmt_idx = i;
            break;
        }
    }
    clock_format_row.set_selected(selected_fmt_idx as u32);
    clock_group.add(&clock_format_row);

    let clock_format_entry = adw::EntryRow::builder()
        .title(tr(lang, "Строка формата"))
        .build();
    clock_format_entry.set_text(current_fmt);
    clock_format_entry.set_visible(selected_fmt_idx == CLOCK_FORMAT_PATTERNS.len());
    clock_group.add(&clock_format_entry);

    let clock_pos_labels: Vec<&str> = CLOCK_POSITION_LABEL_KEYS
        .iter()
        .map(|label| tr(lang, label))
        .collect();
    let clock_pos_model = gtk4::StringList::new(&clock_pos_labels);
    let clock_position_row = adw::ComboRow::builder()
        .title(tr(lang, "Положение"))
        .model(&clock_pos_model)
        .build();
    clock_position_row.set_selected(clock_position_index(config.active_profile().clock_position));
    clock_group.add(&clock_position_row);

    let clock_move_row = adw::ActionRow::builder()
        .title(tr(lang, "Перемещать часы"))
        .subtitle(tr(lang, "Меняет положение по кругу"))
        .build();
    let clock_move_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().clock_move_enabled)
        .build();
    clock_move_row.add_suffix(&clock_move_switch);
    clock_group.add(&clock_move_row);

    let clock_move_interval_row = adw::ActionRow::builder()
        .title(tr(lang, "Интервал перемещения"))
        .subtitle(tr(lang, "Секунды"))
        .visible(config.active_profile().clock_move_enabled)
        .build();
    let clock_move_interval_spin = SpinButton::with_range(1.0, 3600.0, 1.0);
    clock_move_interval_spin.set_value(config.active_profile().clock_move_interval_seconds as f64);
    clock_move_interval_spin.set_valign(Align::Center);
    clock_move_interval_row.add_suffix(&clock_move_interval_spin);
    clock_group.add(&clock_move_interval_row);

    let clock_size_row = adw::ActionRow::builder()
        .title(tr(lang, "Размер текста"))
        .subtitle(tr(lang, "Пункты"))
        .build();
    let clock_size_spin = SpinButton::with_range(8.0, 200.0, 1.0);
    clock_size_spin.set_value(config.active_profile().clock_size as f64);
    clock_size_spin.set_valign(Align::Center);
    clock_size_row.add_suffix(&clock_size_spin);
    clock_group.add(&clock_size_row);

    let clock_preview_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Макет часов"))
        .build();
    
    let clock_preview_frame = gtk4::Frame::new(None);
    clock_preview_frame.set_size_request(-1, 200);
    clock_preview_frame.add_css_class("preview-screen");
    
    let preview_overlay = gtk4::Overlay::new();
    preview_overlay.set_hexpand(true);
    preview_overlay.set_vexpand(true);
    
    let bg = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    bg.add_css_class("preview-bg");
    preview_overlay.set_child(Some(&bg));
    
    let clock_preview_label = Label::new(Some(&format_clock_text(current_fmt)));
    clock_preview_label.add_css_class("title-1");
    apply_clock_size(&clock_preview_label, config.active_profile().clock_size);
    apply_clock_position(&clock_preview_label, config.active_profile().clock_position, 20);
    
    preview_overlay.add_overlay(&clock_preview_label);
    clock_preview_frame.set_child(Some(&preview_overlay));
    clock_preview_group.add(&clock_preview_frame);

    AppearanceWidgets {
        group: clock_group,
        clock_switch,
        clock_format_row,
        clock_format_entry,
        clock_position_row,
        clock_move_switch,
        clock_move_interval_row,
        clock_move_interval_spin,
        clock_size_spin,
        clock_preview_group,
        clock_preview_label,
    }
}

pub fn clock_position_from_index(index: u32) -> ClockPosition {
    match index {
        0 => ClockPosition::TopLeft,
        1 => ClockPosition::TopCenter,
        2 => ClockPosition::TopRight,
        3 => ClockPosition::CenterLeft,
        4 => ClockPosition::Center,
        5 => ClockPosition::CenterRight,
        6 => ClockPosition::BottomLeft,
        7 => ClockPosition::BottomCenter,
        8 => ClockPosition::BottomRight,
        _ => ClockPosition::TopRight,
    }
}

pub fn clock_position_index(position: ClockPosition) -> u32 {
    match position {
        ClockPosition::TopLeft => 0,
        ClockPosition::TopCenter => 1,
        ClockPosition::TopRight => 2,
        ClockPosition::CenterLeft => 3,
        ClockPosition::Center => 4,
        ClockPosition::CenterRight => 5,
        ClockPosition::BottomLeft => 6,
        ClockPosition::BottomCenter => 7,
        ClockPosition::BottomRight => 8,
    }
}

pub fn clock_position_label(position: ClockPosition, lang: Language) -> &'static str {
    match position {
        ClockPosition::TopLeft => tr(lang, "Сверху слева"),
        ClockPosition::TopCenter => tr(lang, "Сверху по центру"),
        ClockPosition::TopRight => tr(lang, "Сверху справа"),
        ClockPosition::CenterLeft => tr(lang, "По центру слева"),
        ClockPosition::Center => tr(lang, "По центру"),
        ClockPosition::CenterRight => tr(lang, "По центру справа"),
        ClockPosition::BottomLeft => tr(lang, "Снизу слева"),
        ClockPosition::BottomCenter => tr(lang, "Снизу по центру"),
        ClockPosition::BottomRight => tr(lang, "Снизу справа"),
    }
}

pub fn apply_clock_size(label: &Label, size_points: u32) {
    let attr_list = gtk4::pango::AttrList::new();
    let size_attr = gtk4::pango::AttrSize::new(size_points as i32 * gtk4::pango::SCALE);
    attr_list.insert(size_attr);
    label.set_attributes(Some(&attr_list));
}

pub fn format_clock_text(format: &str) -> String {
    let now = glib::DateTime::now_local().unwrap_or_else(|_| glib::DateTime::now_utc().expect("Failed to get UTC time"));
    match now.format(format) {
        Ok(s) => s.to_string(),
        Err(_) => now.format(DEFAULT_CLOCK_FORMAT).unwrap().to_string(),
    }
}

pub fn clock_format_preset_index(format: &str) -> Option<u32> {
    let trimmed = format.trim();
    CLOCK_FORMAT_PATTERNS
        .iter()
        .position(|value| *value == trimmed)
        .map(|i| i as u32)
}

pub fn apply_clock_position(label: &Label, position: ClockPosition, margin: i32) {
    label.set_margin_top(0);
    label.set_margin_bottom(0);
    label.set_margin_start(0);
    label.set_margin_end(0);

    match position {
        ClockPosition::TopLeft => {
            label.set_halign(Align::Start);
            label.set_valign(Align::Start);
            label.set_xalign(0.0);
            label.set_yalign(0.0);
            label.set_margin_top(margin);
            label.set_margin_start(margin);
        }
        ClockPosition::TopCenter => {
            label.set_halign(Align::Center);
            label.set_valign(Align::Start);
            label.set_xalign(0.5);
            label.set_yalign(0.0);
            label.set_margin_top(margin);
        }
        ClockPosition::TopRight => {
            label.set_halign(Align::End);
            label.set_valign(Align::Start);
            label.set_xalign(1.0);
            label.set_yalign(0.0);
            label.set_margin_top(margin);
            label.set_margin_end(margin);
        }
        ClockPosition::CenterLeft => {
            label.set_halign(Align::Start);
            label.set_valign(Align::Center);
            label.set_xalign(0.0);
            label.set_yalign(0.5);
            label.set_margin_start(margin);
        }
        ClockPosition::Center => {
            label.set_halign(Align::Center);
            label.set_valign(Align::Center);
            label.set_xalign(0.5);
            label.set_yalign(0.5);
        }
        ClockPosition::CenterRight => {
            label.set_halign(Align::End);
            label.set_valign(Align::Center);
            label.set_xalign(1.0);
            label.set_yalign(0.5);
            label.set_margin_end(margin);
        }
        ClockPosition::BottomLeft => {
            label.set_halign(Align::Start);
            label.set_valign(Align::End);
            label.set_xalign(0.0);
            label.set_yalign(1.0);
            label.set_margin_bottom(margin);
            label.set_margin_start(margin);
        }
        ClockPosition::BottomCenter => {
            label.set_halign(Align::Center);
            label.set_valign(Align::End);
            label.set_xalign(0.5);
            label.set_yalign(1.0);
            label.set_margin_bottom(margin);
        }
        ClockPosition::BottomRight => {
            label.set_halign(Align::End);
            label.set_valign(Align::End);
            label.set_xalign(1.0);
            label.set_yalign(1.0);
            label.set_margin_bottom(margin);
            label.set_margin_end(margin);
        }
    }
}
