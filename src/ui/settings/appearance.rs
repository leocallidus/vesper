use crate::config::{
    ClockPosition, Config, DEFAULT_CLOCK_DATE_FORMAT, DEFAULT_CLOCK_FORMAT,
    DEFAULT_CLOCK_TIME_FORMAT,
};
use crate::i18n::{tr, Language};
use crate::mpris::NowPlayingInfo;
use gio;
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{
    Align, Button, Justification, Label, ListBox, Picture, SelectionMode, SpinButton, Switch,
};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub struct AppearanceWidgets {
    pub group: adw::PreferencesGroup,
    pub now_playing_group: adw::PreferencesGroup,
    pub now_playing_switch: Switch,
    pub now_playing_position_row: adw::ComboRow,
    pub now_playing_move_switch: Switch,
    pub now_playing_move_interval_row: adw::ActionRow,
    pub now_playing_move_interval_spin: SpinButton,
    pub now_playing_preview_group: adw::PreferencesGroup,
    pub now_playing_preview_box: gtk4::Box,
    pub rss_group: adw::PreferencesGroup,
    pub rss_switch: Switch,
    pub rss_speed_spin: SpinButton,
    pub rss_refresh_spin: SpinButton,
    pub rss_feeds: Rc<RefCell<Vec<String>>>,
    pub rss_feeds_list: ListBox,
    pub rss_feed_entry: adw::EntryRow,
    pub rss_add_button: Button,
    pub system_stats_group: adw::PreferencesGroup,
    pub system_stats_switch: Switch,
    pub system_stats_position_row: adw::ComboRow,
    pub system_stats_move_switch: Switch,
    pub system_stats_move_interval_row: adw::ActionRow,
    pub system_stats_move_interval_spin: SpinButton,
    pub clock_switch: Switch,
    pub clock_two_lines_switch: Switch,
    pub clock_format_row: adw::ComboRow,
    pub clock_format_entry: adw::EntryRow,
    pub clock_time_format_entry: adw::EntryRow,
    pub clock_date_format_entry: adw::EntryRow,
    pub clock_position_row: adw::ComboRow,
    pub clock_move_switch: Switch,
    pub clock_move_interval_row: adw::ActionRow,
    pub clock_move_interval_spin: SpinButton,
    pub clock_size_spin: SpinButton,
    pub clock_preview_group: adw::PreferencesGroup,
    pub clock_preview_label: Label,
}

pub const CLOCK_FORMAT_LABEL_KEYS: [&str; 18] = [
    "24ч: %H:%M",
    "24ч: %H:%M:%S",
    "12ч: %I:%M %p",
    "Дата: %d.%m.%Y",
    "Дата и время: %d.%m.%Y %H:%M",
    "День недели: %a %H:%M",
    "Полная дата: %A, %d %B %Y",
    "ISO: %F %T",
    "24ч (без нуля): %-H:%M",
    "24ч с секундами и датой: %H:%M:%S  %d.%m",
    "12ч с секундами: %I:%M:%S %p",
    "День недели и дата: %a, %d.%m.%Y",
    "Полный день и дата: %A  %d.%m",
    "ISO дата: %F",
    "ISO дата и время: %F %R",
    "Номер недели и день: Неделя %V, %a",
    "День года: День %j",
    "Время и часовой пояс: %H:%M %z",
];

pub const CLOCK_FORMAT_PATTERNS: [&str; 18] = [
    "%H:%M",
    "%H:%M:%S",
    "%I:%M %p",
    "%d.%m.%Y",
    "%d.%m.%Y %H:%M",
    "%a %H:%M",
    "%A, %d %B %Y",
    "%F %T",
    "%-H:%M",
    "%H:%M:%S  %d.%m",
    "%I:%M:%S %p",
    "%a, %d.%m.%Y",
    "%A  %d.%m",
    "%F",
    "%F %R",
    "Week %V, %a",
    "Day %j",
    "%H:%M %z",
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
    let now_playing_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Сейчас играет"))
        .description(tr(lang, "Обложка и название трека (MPRIS)"))
        .build();

    let now_playing_switch_row = adw::ActionRow::builder()
        .title(tr(lang, "Показывать «Сейчас играет»"))
        .build();
    let now_playing_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().show_now_playing)
        .build();
    now_playing_switch_row.add_suffix(&now_playing_switch);
    now_playing_group.add(&now_playing_switch_row);

    let now_playing_pos_labels: Vec<&str> = CLOCK_POSITION_LABEL_KEYS
        .iter()
        .map(|label| tr(lang, label))
        .collect();
    let now_playing_pos_model = gtk4::StringList::new(&now_playing_pos_labels);
    let now_playing_position_row = adw::ComboRow::builder()
        .title(tr(lang, "Положение"))
        .model(&now_playing_pos_model)
        .build();
    now_playing_position_row.set_selected(clock_position_index(
        config.active_profile().now_playing_position,
    ));
    now_playing_group.add(&now_playing_position_row);

    let now_playing_move_row = adw::ActionRow::builder()
        .title(tr(lang, "Перемещать виджет"))
        .subtitle(tr(lang, "Меняет положение по кругу"))
        .build();
    let now_playing_move_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().now_playing_move_enabled)
        .build();
    now_playing_move_row.add_suffix(&now_playing_move_switch);
    now_playing_group.add(&now_playing_move_row);

    let now_playing_move_interval_row = adw::ActionRow::builder()
        .title(tr(lang, "Интервал перемещения"))
        .subtitle(tr(lang, "Секунды"))
        .visible(config.active_profile().now_playing_move_enabled)
        .build();
    let now_playing_move_interval_spin = SpinButton::with_range(1.0, 3600.0, 1.0);
    now_playing_move_interval_spin
        .set_value(config.active_profile().now_playing_move_interval_seconds as f64);
    now_playing_move_interval_spin.set_valign(Align::Center);
    now_playing_move_interval_row.add_suffix(&now_playing_move_interval_spin);
    now_playing_group.add(&now_playing_move_interval_row);

    let now_playing_preview_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Превью"))
        .build();

    let now_playing_preview_frame = gtk4::Frame::new(None);
    now_playing_preview_frame.set_size_request(-1, 140);
    now_playing_preview_frame.add_css_class("preview-screen");

    let preview_overlay = gtk4::Overlay::new();
    preview_overlay.set_hexpand(true);
    preview_overlay.set_vexpand(true);

    let bg = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    bg.add_css_class("preview-bg");
    preview_overlay.set_child(Some(&bg));

    let preview_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);

    let preview_art = Picture::new();
    preview_art.set_size_request(96, 96);
    preview_art.set_content_fit(gtk4::ContentFit::Cover);
    preview_art.set_can_shrink(false);

    let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    text_box.set_hexpand(true);

    let preview_title = Label::new(Some(tr(lang, "Нет воспроизведения")));
    preview_title.add_css_class("title-3");
    preview_title.set_xalign(0.0);
    preview_title.set_wrap(true);
    preview_title.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    preview_title.set_max_width_chars(60);

    let preview_artist = Label::new(None);
    preview_artist.add_css_class("caption");
    preview_artist.set_xalign(0.0);
    preview_artist.set_wrap(true);
    preview_artist.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    preview_artist.set_max_width_chars(60);
    preview_artist.set_opacity(0.85);

    text_box.append(&preview_title);
    text_box.append(&preview_artist);

    preview_box.append(&preview_art);
    preview_box.append(&text_box);

    apply_widget_position(
        &preview_box,
        config.active_profile().now_playing_position,
        16,
    );
    preview_overlay.add_overlay(&preview_box);

    now_playing_preview_frame.set_child(Some(&preview_overlay));
    now_playing_preview_group.add(&now_playing_preview_frame);

    spawn_now_playing_preview_updater(lang, &preview_title, &preview_artist, &preview_art);

    let rss_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "RSS/Новости"))
        .description(tr(lang, "Бегущая строка из RSS лент"))
        .build();

    let rss_switch_row = adw::ActionRow::builder()
        .title(tr(lang, "Показывать RSS-строку"))
        .build();
    let rss_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().show_rss_ticker)
        .build();
    rss_switch_row.add_suffix(&rss_switch);
    rss_group.add(&rss_switch_row);

    let rss_speed_row = adw::ActionRow::builder()
        .title(tr(lang, "Скорость прокрутки"))
        .subtitle(tr(lang, "пикс/с"))
        .build();
    let rss_speed_spin = SpinButton::with_range(10.0, 600.0, 1.0);
    rss_speed_spin.set_value(config.active_profile().rss_ticker_speed_px_s as f64);
    rss_speed_spin.set_valign(Align::Center);
    rss_speed_row.add_suffix(&rss_speed_spin);
    rss_group.add(&rss_speed_row);

    let rss_refresh_row = adw::ActionRow::builder()
        .title(tr(lang, "Интервал обновления"))
        .subtitle(tr(lang, "Минуты"))
        .build();
    let rss_refresh_spin = SpinButton::with_range(1.0, 360.0, 1.0);
    rss_refresh_spin.set_value(config.active_profile().rss_refresh_interval_minutes as f64);
    rss_refresh_spin.set_valign(Align::Center);
    rss_refresh_row.add_suffix(&rss_refresh_spin);
    rss_group.add(&rss_refresh_row);

    let rss_feeds_list = ListBox::new();
    rss_feeds_list.add_css_class("boxed-list");
    rss_feeds_list.set_selection_mode(SelectionMode::None);
    rss_group.add(&rss_feeds_list);

    let rss_feed_entry = adw::EntryRow::builder()
        .title(tr(lang, "Добавить RSS ленту"))
        .build();
    let rss_add_button = Button::with_label(tr(lang, "Добавить"));
    rss_feed_entry.add_suffix(&rss_add_button);
    rss_group.add(&rss_feed_entry);

    let rss_feeds = Rc::new(RefCell::new(config.active_profile().rss_feeds.clone()));

    let system_stats_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Системные показатели"))
        .description(tr(lang, "Графики CPU/RAM (retro-tech)"))
        .build();

    let system_stats_switch_row = adw::ActionRow::builder()
        .title(tr(lang, "Показывать System Stats"))
        .build();
    let system_stats_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().show_system_stats)
        .build();
    system_stats_switch_row.add_suffix(&system_stats_switch);
    system_stats_group.add(&system_stats_switch_row);

    let stats_pos_labels: Vec<&str> = CLOCK_POSITION_LABEL_KEYS
        .iter()
        .map(|label| tr(lang, label))
        .collect();
    let stats_pos_model = gtk4::StringList::new(&stats_pos_labels);
    let system_stats_position_row = adw::ComboRow::builder()
        .title(tr(lang, "Положение"))
        .model(&stats_pos_model)
        .build();
    system_stats_position_row.set_selected(clock_position_index(
        config.active_profile().system_stats_position,
    ));
    system_stats_group.add(&system_stats_position_row);

    let system_stats_move_row = adw::ActionRow::builder()
        .title(tr(lang, "Перемещать виджет"))
        .subtitle(tr(lang, "Меняет положение по кругу"))
        .build();
    let system_stats_move_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().system_stats_move_enabled)
        .build();
    system_stats_move_row.add_suffix(&system_stats_move_switch);
    system_stats_group.add(&system_stats_move_row);

    let system_stats_move_interval_row = adw::ActionRow::builder()
        .title(tr(lang, "Интервал перемещения"))
        .subtitle(tr(lang, "Секунды"))
        .visible(config.active_profile().system_stats_move_enabled)
        .build();
    let system_stats_move_interval_spin = SpinButton::with_range(1.0, 3600.0, 1.0);
    system_stats_move_interval_spin
        .set_value(config.active_profile().system_stats_move_interval_seconds as f64);
    system_stats_move_interval_spin.set_valign(Align::Center);
    system_stats_move_interval_row.add_suffix(&system_stats_move_interval_spin);
    system_stats_group.add(&system_stats_move_interval_row);

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

    let clock_two_lines_row = adw::ActionRow::builder()
        .title(tr(lang, "Часы в две строки"))
        .build();
    let clock_two_lines_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().clock_two_lines)
        .build();
    clock_two_lines_row.add_suffix(&clock_two_lines_switch);
    clock_group.add(&clock_two_lines_row);

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
    clock_format_row.set_sensitive(!config.active_profile().clock_two_lines);
    clock_group.add(&clock_format_row);

    let clock_format_entry = adw::EntryRow::builder()
        .title(tr(lang, "Строка формата"))
        .build();
    clock_format_entry.set_text(current_fmt);
    clock_format_entry.set_visible(
        !config.active_profile().clock_two_lines && selected_fmt_idx == CLOCK_FORMAT_PATTERNS.len(),
    );
    clock_group.add(&clock_format_entry);

    let clock_time_format_entry = adw::EntryRow::builder()
        .title(tr(lang, "Время (формат)"))
        .build();
    let time_text = if config.active_profile().clock_time_format.trim().is_empty() {
        DEFAULT_CLOCK_TIME_FORMAT
    } else {
        &config.active_profile().clock_time_format
    };
    clock_time_format_entry.set_text(time_text);
    clock_time_format_entry.set_visible(config.active_profile().clock_two_lines);
    clock_group.add(&clock_time_format_entry);

    let clock_date_format_entry = adw::EntryRow::builder()
        .title(tr(lang, "Дата (формат)"))
        .build();
    let date_text = if config.active_profile().clock_date_format.trim().is_empty() {
        DEFAULT_CLOCK_DATE_FORMAT
    } else {
        &config.active_profile().clock_date_format
    };
    clock_date_format_entry.set_text(date_text);
    clock_date_format_entry.set_visible(config.active_profile().clock_two_lines);
    clock_group.add(&clock_date_format_entry);

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

    let preview_text = if config.active_profile().clock_two_lines {
        format!(
            "{}\n{}",
            format_clock_text(&config.active_profile().clock_time_format),
            format_clock_text(&config.active_profile().clock_date_format)
        )
    } else {
        format_clock_text(current_fmt)
    };

    let clock_preview_label = Label::new(Some(&preview_text));
    clock_preview_label.add_css_class("title-1");
    clock_preview_label.set_justify(Justification::Center);
    clock_preview_label.set_wrap(true);
    clock_preview_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    clock_preview_label.set_max_width_chars(40);
    apply_clock_size(&clock_preview_label, config.active_profile().clock_size);
    apply_clock_position(
        &clock_preview_label,
        config.active_profile().clock_position,
        20,
    );

    preview_overlay.add_overlay(&clock_preview_label);
    clock_preview_frame.set_child(Some(&preview_overlay));
    clock_preview_group.add(&clock_preview_frame);

    AppearanceWidgets {
        group: clock_group,
        now_playing_group,
        now_playing_switch,
        now_playing_position_row,
        now_playing_move_switch,
        now_playing_move_interval_row,
        now_playing_move_interval_spin,
        now_playing_preview_group,
        now_playing_preview_box: preview_box,
        rss_group,
        rss_switch,
        rss_speed_spin,
        rss_refresh_spin,
        rss_feeds,
        rss_feeds_list,
        rss_feed_entry,
        rss_add_button,
        system_stats_group,
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
        clock_move_switch,
        clock_move_interval_row,
        clock_move_interval_spin,
        clock_size_spin,
        clock_preview_group,
        clock_preview_label,
    }
}

fn spawn_now_playing_preview_updater(lang: Language, title: &Label, artist: &Label, art: &Picture) {
    let title_weak = title.downgrade();
    let artist_weak = artist.downgrade();
    let art_weak = art.downgrade();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);

    let (tx, rx) = std::sync::mpsc::channel::<Option<NowPlayingInfo>>();
    thread::spawn(move || {
        let mut last: Option<NowPlayingInfo> = None;
        while !stop_thread.load(Ordering::Relaxed) {
            let current = crate::mpris::query_now_playing().ok().flatten();
            if current != last {
                let _ = tx.send(current.clone());
                last = current;
            }
            thread::sleep(Duration::from_millis(1000));
        }
    });

    glib::timeout_add_local(Duration::from_millis(250), move || {
        let Some(title) = title_weak.upgrade() else {
            stop.store(true, Ordering::Relaxed);
            return glib::ControlFlow::Break;
        };
        let Some(artist) = artist_weak.upgrade() else {
            stop.store(true, Ordering::Relaxed);
            return glib::ControlFlow::Break;
        };
        let Some(art) = art_weak.upgrade() else {
            stop.store(true, Ordering::Relaxed);
            return glib::ControlFlow::Break;
        };

        while let Ok(info) = rx.try_recv() {
            apply_now_playing_to_widgets(lang, info.as_ref(), &title, &artist, &art);
        }
        glib::ControlFlow::Continue
    });
}

fn apply_now_playing_to_widgets(
    lang: Language,
    info: Option<&NowPlayingInfo>,
    title: &Label,
    artist: &Label,
    art: &Picture,
) {
    if let Some(info) = info {
        let t = info.title.trim();
        if t.is_empty() {
            title.set_text(tr(lang, "Нет воспроизведения"));
        } else {
            title.set_text(t);
        }
        artist.set_text(info.artist.trim());
        set_picture_from_art_url(art, info.art_url.as_deref());
    } else {
        title.set_text(tr(lang, "Нет воспроизведения"));
        artist.set_text("");
        art.set_paintable(gdk::Paintable::NONE);
    }
}

fn set_picture_from_art_url(picture: &Picture, art_url: Option<&str>) {
    let Some(url) = art_url.map(str::trim).filter(|s| !s.is_empty()) else {
        picture.set_paintable(gdk::Paintable::NONE);
        return;
    };

    if url.starts_with("file://") {
        picture.set_file(Some(&gio::File::for_uri(url)));
        return;
    }

    let path = std::path::Path::new(url);
    if path.is_absolute() && path.is_file() {
        picture.set_file(Some(&gio::File::for_path(path)));
        return;
    }

    picture.set_paintable(gdk::Paintable::NONE);
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
    let now = glib::DateTime::now_local()
        .unwrap_or_else(|_| glib::DateTime::now_utc().expect("Failed to get UTC time"));
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

pub fn apply_widget_position(
    widget: &impl IsA<gtk4::Widget>,
    position: ClockPosition,
    margin: i32,
) {
    widget.set_margin_top(0);
    widget.set_margin_bottom(0);
    widget.set_margin_start(0);
    widget.set_margin_end(0);

    match position {
        ClockPosition::TopLeft => {
            widget.set_halign(Align::Start);
            widget.set_valign(Align::Start);
            widget.set_margin_top(margin);
            widget.set_margin_start(margin);
        }
        ClockPosition::TopCenter => {
            widget.set_halign(Align::Center);
            widget.set_valign(Align::Start);
            widget.set_margin_top(margin);
        }
        ClockPosition::TopRight => {
            widget.set_halign(Align::End);
            widget.set_valign(Align::Start);
            widget.set_margin_top(margin);
            widget.set_margin_end(margin);
        }
        ClockPosition::CenterLeft => {
            widget.set_halign(Align::Start);
            widget.set_valign(Align::Center);
            widget.set_margin_start(margin);
        }
        ClockPosition::Center => {
            widget.set_halign(Align::Center);
            widget.set_valign(Align::Center);
        }
        ClockPosition::CenterRight => {
            widget.set_halign(Align::End);
            widget.set_valign(Align::Center);
            widget.set_margin_end(margin);
        }
        ClockPosition::BottomLeft => {
            widget.set_halign(Align::Start);
            widget.set_valign(Align::End);
            widget.set_margin_bottom(margin);
            widget.set_margin_start(margin);
        }
        ClockPosition::BottomCenter => {
            widget.set_halign(Align::Center);
            widget.set_valign(Align::End);
            widget.set_margin_bottom(margin);
        }
        ClockPosition::BottomRight => {
            widget.set_halign(Align::End);
            widget.set_valign(Align::End);
            widget.set_margin_bottom(margin);
            widget.set_margin_end(margin);
        }
    }
}
