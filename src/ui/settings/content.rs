use crate::config::{
    AnimatedPattern, ClockPosition, Config, PatternDensity, PatternSpeed, PatternTheme,
    ScreensaverMode,
};
use crate::i18n::{tr, Language};
use crate::ui::settings::appearance::{apply_clock_position, apply_clock_size, format_clock_text};
use gdk4::RGBA;
use gtk4::cairo;
use gtk4::prelude::Cast;
use gtk4::prelude::*;
use gtk4::{
    Align, Button, ColorDialog, ColorDialogButton, ContentFit, DrawingArea, Frame, GraphicsOffload,
    GraphicsOffloadEnabled, IconLookupFlags, IconTheme, Label, ListBox, MediaControls, MediaFile,
    Orientation, Picture, SelectionMode, Stack, Switch, TextDirection, Video, Widget,
};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use webkit6::prelude::WebViewExt;

#[derive(Clone, Copy)]
pub enum MediaKind {
    Image,
    Video,
    ImageFolder,
}

pub struct ContentWidgets {
    pub group: adw::PreferencesGroup,
    pub preview_group: adw::PreferencesGroup,
    pub mode_selector: gtk4::FlowBox,
    pub stack: Stack,
    pub color_button: ColorDialogButton,
    pub gradient_start_button: ColorDialogButton,
    pub gradient_end_button: ColorDialogButton,
    pub pattern_row: adw::ComboRow,
    pub pattern_speed_row: adw::ComboRow,
    pub pattern_density_row: adw::ComboRow,
    pub pattern_theme_row: adw::ComboRow,
    pub water_ripples_bg_row: adw::ActionRow,
    pub water_ripples_bg_button: Button,
    pub water_ripples_bg_clear_button: Button,
    pub web_url_row: adw::EntryRow,
    pub web_interaction_switch: Switch,
    pub stream_url_row: adw::EntryRow,
    pub file_row: adw::ActionRow,
    pub file_button: Button,
    pub file_info_row: adw::ActionRow,
    pub shader_check_row: adw::ActionRow,
    pub shader_check_button: Button,
    pub slideshow_interval_row: adw::ActionRow,
    pub mute_row: adw::ActionRow,
    pub volume_row: adw::ActionRow,
    pub random_row: adw::ActionRow,
    pub slideshow_interval_spin: gtk4::SpinButton,
    pub mute_switch: Switch,
    pub video_volume_spin: gtk4::SpinButton,
    pub random_media_switch: Switch,
    pub media_list_row: adw::ActionRow,
    pub media_list_box: ListBox,
    pub add_media_button: Button,
    pub remove_media_button: Button,
    pub preview_frame: Frame,
    pub preview_pause_row: adw::ActionRow,
    pub preview_pause_switch: Switch,
    pub media_files: Rc<RefCell<Vec<String>>>,
    pub selected_media_preview: Rc<RefCell<Option<String>>>,
}

pub fn build_content_group(config: &Config, lang: Language) -> ContentWidgets {
    let content_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Контент"))
        .build();

    let mode_label_title = gtk4::Label::builder()
        .label(tr(lang, "Режим"))
        .halign(Align::Start)
        .margin_bottom(8)
        .build();
    mode_label_title.add_css_class("heading");
    content_group.add(&mode_label_title);

    let mode_selector = gtk4::FlowBox::builder()
        .valign(Align::Start)
        .max_children_per_line(4)
        .min_children_per_line(2)
        .selection_mode(gtk4::SelectionMode::Single)
        .homogeneous(true)
        .column_spacing(12)
        .row_spacing(12)
        .build();

    let modes = [
        (0, tr(lang, "Цвет"), mode_icon_name(0)),
        (1, tr(lang, "Градиент"), mode_icon_name(1)),
        (2, tr(lang, "Паттерны"), mode_icon_name(2)),
        (3, tr(lang, "Веб-страница"), mode_icon_name(3)),
        (4, tr(lang, "Изображение"), mode_icon_name(4)),
        (5, tr(lang, "Видео"), mode_icon_name(5)),
        (6, tr(lang, "Слайдшоу"), mode_icon_name(6)),
        (7, tr(lang, "Видео по URL"), mode_icon_name(7)),
        (8, tr(lang, "Python скрипт"), mode_icon_name(8)),
        (9, tr(lang, "GLSL шейдер"), mode_icon_name(9)),
    ];

    for (idx, name, icon) in modes {
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        card.set_margin_top(12);
        card.set_margin_bottom(12);
        card.set_margin_start(12);
        card.set_margin_end(12);
        let img = icon_picture(icon, 32);
        let lbl = gtk4::Label::new(Some(name));
        lbl.add_css_class("caption");
        card.append(&img);
        card.append(&lbl);

        let child = gtk4::FlowBoxChild::new();
        child.set_child(Some(&card));
        child.add_css_class("card");
        unsafe {
            child.set_data("mode-index", idx);
        }
        mode_selector.insert(&child, -1);
    }

    let initial_mode_idx = match config.active_profile().mode {
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
    };
    if let Some(child) = mode_selector.child_at_index(initial_mode_idx as i32) {
        mode_selector.select_child(&child);
    }

    content_group.add(&mode_selector);

    let stack = Stack::new();
    stack.set_margin_top(12);

    // --- Color Page ---
    let color_button = ColorDialogButton::new(Some(ColorDialog::new()));
    color_button.set_valign(Align::Center);
    let initial_rgba = match &config.active_profile().mode {
        ScreensaverMode::Color(hex) => hex_to_rgba(hex).unwrap_or(RGBA::new(0.0, 0.0, 0.0, 1.0)),
        _ => RGBA::new(0.0, 0.0, 0.0, 1.0),
    };
    color_button.set_rgba(&initial_rgba);
    let color_row = adw::ActionRow::builder()
        .title(tr(lang, "Выбор цвета"))
        .build();
    color_row.add_suffix(&color_button);
    let color_group = adw::PreferencesGroup::new();
    color_group.add(&color_row);
    stack.add_named(&color_group, Some("color_page"));

    // --- Gradient Page ---
    let (gradient_start_rgba, gradient_end_rgba) = match &config.active_profile().mode {
        ScreensaverMode::Gradient { start, end } => (
            hex_to_rgba(start).unwrap_or(RGBA::new(0.0, 0.0, 0.0, 1.0)),
            hex_to_rgba(end).unwrap_or(RGBA::new(0.0, 0.0, 0.0, 1.0)),
        ),
        _ => (RGBA::new(0.0, 0.0, 0.0, 1.0), RGBA::new(0.0, 0.0, 0.0, 1.0)),
    };
    let gradient_start_button = ColorDialogButton::new(Some(ColorDialog::new()));
    gradient_start_button.set_rgba(&gradient_start_rgba);
    let gradient_end_button = ColorDialogButton::new(Some(ColorDialog::new()));
    gradient_end_button.set_rgba(&gradient_end_rgba);
    let g_start_row = adw::ActionRow::builder().title(tr(lang, "Цвет 1")).build();
    g_start_row.add_suffix(&gradient_start_button);
    let g_end_row = adw::ActionRow::builder().title(tr(lang, "Цвет 2")).build();
    g_end_row.add_suffix(&gradient_end_button);
    let gradient_group = adw::PreferencesGroup::new();
    gradient_group.add(&g_start_row);
    gradient_group.add(&g_end_row);
    stack.add_named(&gradient_group, Some("gradient_page"));

    // --- Pattern Page ---
    let pattern_model = gtk4::StringList::new(&[
        tr(lang, "Матрица"),
        tr(lang, "Звёзды"),
        tr(lang, "Геометрия"),
        tr(lang, "Поле потока"),
        tr(lang, "Северное сияние"),
        tr(lang, "Плазма"),
        tr(lang, "Боке"),
        tr(lang, "Созвездия"),
        tr(lang, "Кривые Лиссажу"),
        tr(lang, "Волны"),
        tr(lang, "Ячейки Вороного"),
        tr(lang, "ЭЛТ-монитор"),
        tr(lang, "Светлячки"),
        tr(lang, "Дым/Чернила"),
        tr(lang, "Водная рябь"),
        tr(lang, "Матрица 2.0"),
        tr(lang, "LCARS"),
        tr(lang, "Терминал"),
        tr(lang, "Фракталы"),
        tr(lang, "Реакция-диффузия"),
    ]);
    let pattern_row = adw::ComboRow::builder()
        .title(tr(lang, "Паттерн"))
        .model(&pattern_model)
        .build();
    if let ScreensaverMode::Pattern(p) = config.active_profile().mode {
        pattern_row.set_selected(pattern_index(p));
    }

    let speed_model = gtk4::StringList::new(&[
        tr(lang, "Медленно"),
        tr(lang, "Нормально"),
        tr(lang, "Быстро"),
    ]);
    let pattern_speed_row = adw::ComboRow::builder()
        .title(tr(lang, "Скорость"))
        .model(&speed_model)
        .build();
    let initial_speed_idx = match config.active_profile().pattern_speed {
        PatternSpeed::Slow => 0,
        PatternSpeed::Normal => 1,
        PatternSpeed::Fast => 2,
    };
    pattern_speed_row.set_selected(initial_speed_idx);

    let density_model =
        gtk4::StringList::new(&[tr(lang, "Низкая"), tr(lang, "Средняя"), tr(lang, "Высокая")]);
    let pattern_density_row = adw::ComboRow::builder()
        .title(tr(lang, "Плотность"))
        .model(&density_model)
        .build();
    let initial_density_idx = match config.active_profile().pattern_density {
        PatternDensity::Low => 0,
        PatternDensity::Medium => 1,
        PatternDensity::High => 2,
    };
    pattern_density_row.set_selected(initial_density_idx);

    let theme_model = gtk4::StringList::new(&[
        tr(lang, "По умолчанию"),
        tr(lang, "Монохром"),
        tr(lang, "Теплая"),
        tr(lang, "Холодная"),
        tr(lang, "Случайная"),
    ]);
    let pattern_theme_row = adw::ComboRow::builder()
        .title(tr(lang, "Тема"))
        .model(&theme_model)
        .build();
    let initial_theme_idx = match config.active_profile().pattern_theme {
        PatternTheme::Default => 0,
        PatternTheme::Mono => 1,
        PatternTheme::Warm => 2,
        PatternTheme::Cool => 3,
        PatternTheme::Random => 4,
    };
    pattern_theme_row.set_selected(initial_theme_idx);

    let pattern_group = adw::PreferencesGroup::new();
    pattern_group.add(&pattern_row);
    pattern_group.add(&pattern_speed_row);
    pattern_group.add(&pattern_density_row);
    pattern_group.add(&pattern_theme_row);

    let water_ripples_bg_row = adw::ActionRow::builder()
        .title(tr(lang, "Фоновое изображение"))
        .subtitle(tr(lang, "Файл не выбран"))
        .build();
    let water_ripples_bg_button = Button::with_label(tr(lang, "Выбрать..."));
    let water_ripples_bg_clear_button = Button::with_label(tr(lang, "Очистить"));
    water_ripples_bg_clear_button.add_css_class("destructive-action");
    let bg_buttons = gtk4::Box::new(Orientation::Horizontal, 6);
    bg_buttons.append(&water_ripples_bg_button);
    bg_buttons.append(&water_ripples_bg_clear_button);
    water_ripples_bg_row.add_suffix(&bg_buttons);

    let is_water_ripples = matches!(
        config.active_profile().mode,
        ScreensaverMode::Pattern(AnimatedPattern::WaterRipples)
    );
    water_ripples_bg_row.set_visible(is_water_ripples);
    let bg_path = config
        .active_profile()
        .water_ripples_background_image
        .trim();
    if !bg_path.is_empty() {
        water_ripples_bg_row.set_subtitle(bg_path);
        water_ripples_bg_row.set_tooltip_text(Some(bg_path));
    }
    water_ripples_bg_clear_button.set_sensitive(!bg_path.is_empty());

    pattern_group.add(&water_ripples_bg_row);
    stack.add_named(&pattern_group, Some("pattern_page"));

    // --- Web Page ---
    let web_url_row = adw::EntryRow::builder().title("URL").build();
    if let ScreensaverMode::Web(url) = &config.active_profile().mode {
        web_url_row.set_text(url);
    }
    let web_interaction_row = adw::ActionRow::builder()
        .title(tr(lang, "Интерактивный веб"))
        .subtitle(tr(lang, "Разрешить управление мышью (курсор будет видим)"))
        .build();
    let web_interaction_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().web_interaction_enabled)
        .build();
    web_interaction_row.add_suffix(&web_interaction_switch);
    let web_group = adw::PreferencesGroup::new();
    web_group.add(&web_url_row);
    web_group.add(&web_interaction_row);
    stack.add_named(&web_group, Some("web_page"));

    // --- File Page ---
    let file_group = adw::PreferencesGroup::new();
    let stream_url_row = adw::EntryRow::builder()
        .title(tr(lang, "URL потока"))
        .build();
    if let ScreensaverMode::Stream(url) = &config.active_profile().mode {
        stream_url_row.set_text(url);
    }
    stream_url_row.set_visible(matches!(
        config.active_profile().mode,
        ScreensaverMode::Stream(_)
    ));
    let file_row = adw::ActionRow::builder()
        .title(tr(lang, "Путь к файлу"))
        .build();
    let file_button = Button::with_label(tr(lang, "Выбрать..."));
    file_row.add_suffix(&file_button);
    let file_info_row = adw::ActionRow::builder()
        .title(tr(lang, "Информация"))
        .subtitle(tr(lang, "Нет данных"))
        .build();
    let shader_check_row = adw::ActionRow::builder()
        .title(tr(lang, "Проверить шейдеры"))
        .subtitle(tr(lang, "Показать найденные BufferA-D"))
        .build();
    shader_check_row.set_visible(initial_mode_idx == 9);
    let shader_check_button = Button::with_label(tr(lang, "Проверить"));
    shader_check_row.add_suffix(&shader_check_button);
    let slideshow_interval_spin = gtk4::SpinButton::with_range(1.0, 3600.0, 1.0);
    slideshow_interval_spin.set_value(config.active_profile().slideshow_interval_seconds as f64);
    let slideshow_interval_row = adw::ActionRow::builder()
        .title(tr(lang, "Интервал слайдшоу"))
        .build();
    slideshow_interval_row.add_suffix(&slideshow_interval_spin);
    let mute_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().mute_video)
        .build();
    let mute_row = adw::ActionRow::builder()
        .title(tr(lang, "Без звука"))
        .build();
    mute_row.add_suffix(&mute_switch);
    let video_volume_spin = gtk4::SpinButton::with_range(0.0, 100.0, 1.0);
    video_volume_spin.set_value(config.active_profile().video_volume as f64);
    let volume_row = adw::ActionRow::builder()
        .title(tr(lang, "Громкость"))
        .build();
    volume_row.add_suffix(&video_volume_spin);
    let random_media_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().random_media)
        .build();
    let random_row = adw::ActionRow::builder()
        .title(tr(lang, "Случайный выбор"))
        .build();
    random_row.add_suffix(&random_media_switch);
    let media_list_row = adw::ActionRow::builder()
        .title(tr(lang, "Список медиа"))
        .build();
    let media_buttons = gtk4::Box::new(Orientation::Horizontal, 6);
    let add_media_button = Button::with_label(tr(lang, "Добавить"));
    let remove_media_button = Button::with_label(tr(lang, "Удалить"));
    remove_media_button.add_css_class("destructive-action");
    media_buttons.append(&add_media_button);
    media_buttons.append(&remove_media_button);
    media_list_row.add_suffix(&media_buttons);
    let media_list_box = ListBox::new();
    media_list_box.add_css_class("boxed-list");
    media_list_box.set_selection_mode(SelectionMode::Single);
    let list_visible = matches!(
        config.active_profile().mode,
        ScreensaverMode::Image(_) | ScreensaverMode::Video(_)
    );
    media_list_row.set_visible(list_visible);
    media_list_box.set_visible(list_visible);
    file_group.add(&stream_url_row);
    file_group.add(&file_row);
    file_group.add(&file_info_row);
    file_group.add(&shader_check_row);
    file_group.add(&slideshow_interval_row);
    file_group.add(&mute_row);
    file_group.add(&volume_row);
    file_group.add(&random_row);
    file_group.add(&media_list_row);
    file_group.add(&media_list_box);
    stack.add_named(&file_group, Some("file_page"));

    content_group.add(&stack);

    let preview_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Предпросмотр"))
        .build();
    let preview_frame = Frame::new(None);
    preview_frame.set_size_request(360, 200);
    preview_group.add(&preview_frame);

    let preview_pause_row = adw::ActionRow::builder()
        .title(tr(lang, "Пауза предпросмотра"))
        .build();
    preview_pause_row.set_visible(initial_mode_idx == 5 || initial_mode_idx == 7);
    let preview_pause_switch = Switch::builder().valign(Align::Center).active(true).build();
    preview_pause_row.add_suffix(&preview_pause_switch);
    preview_group.add(&preview_pause_row);

    let media_files = Rc::new(RefCell::new(config.active_profile().media_list.clone()));
    let selected_media_preview = Rc::new(RefCell::new(None));

    ContentWidgets {
        group: content_group,
        preview_group,
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
        file_row,
        file_button,
        file_info_row,
        shader_check_row,
        shader_check_button,
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
    }
}

pub fn mode_icon_name(mode_index: u32) -> &'static str {
    match mode_index {
        0 => "color-select-symbolic",
        1 => "applications-graphics-symbolic",
        2 => "view-grid-symbolic",
        3 => "internet-web-browser-symbolic",
        4 => "image-x-generic-symbolic",
        5 => "video-x-generic-symbolic",
        6 => "folder-pictures-symbolic",
        7 => "media-playback-start-symbolic",
        8 => "applications-development-symbolic",
        9 => "preferences-desktop-theme-symbolic",
        _ => "media-optical-symbolic",
    }
}

pub fn mode_label(mode_index: u32, lang: Language) -> &'static str {
    match mode_index {
        0 => tr(lang, "Цвет"),
        1 => tr(lang, "Градиент"),
        2 => tr(lang, "Паттерны"),
        3 => tr(lang, "Веб-страница"),
        4 => tr(lang, "Изображение"),
        5 => tr(lang, "Видео"),
        6 => tr(lang, "Слайдшоу"),
        7 => tr(lang, "Видео по URL"),
        8 => tr(lang, "Python скрипт"),
        9 => tr(lang, "GLSL шейдер"),
        _ => tr(lang, "Неизвестно"),
    }
}

fn icon_picture(icon_name: &str, size: i32) -> Picture {
    let Some(display) = gdk4::Display::default() else {
        let picture = Picture::new();
        picture.set_size_request(size, size);
        return picture;
    };
    let icon_theme = IconTheme::for_display(&display);
    let icon_name = if icon_theme.has_icon(icon_name) {
        icon_name
    } else {
        "text-x-generic-symbolic"
    };
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

pub fn pattern_index(pattern: AnimatedPattern) -> u32 {
    match pattern {
        AnimatedPattern::Matrix => 0,
        AnimatedPattern::Stars => 1,
        AnimatedPattern::Geometry => 2,
        AnimatedPattern::Flowfield => 3,
        AnimatedPattern::Aurora => 4,
        AnimatedPattern::Plasma => 5,
        AnimatedPattern::Bokeh => 6,
        AnimatedPattern::Constellation => 7,
        AnimatedPattern::Lissajous => 8,
        AnimatedPattern::Waves => 9,
        AnimatedPattern::Voronoi => 10,
        AnimatedPattern::Scanline => 11,
        AnimatedPattern::Fireflies => 12,
        AnimatedPattern::SmokeInk => 13,
        AnimatedPattern::WaterRipples => 14,
        AnimatedPattern::MatrixRain3D => 15,
        AnimatedPattern::Lcars => 16,
        AnimatedPattern::Terminal => 17,
        AnimatedPattern::Fractals => 18,
        AnimatedPattern::ReactionDiffusion => 19,
    }
}

pub fn pattern_from_index(index: u32) -> AnimatedPattern {
    match index {
        1 => AnimatedPattern::Stars,
        2 => AnimatedPattern::Geometry,
        3 => AnimatedPattern::Flowfield,
        4 => AnimatedPattern::Aurora,
        5 => AnimatedPattern::Plasma,
        6 => AnimatedPattern::Bokeh,
        7 => AnimatedPattern::Constellation,
        8 => AnimatedPattern::Lissajous,
        9 => AnimatedPattern::Waves,
        10 => AnimatedPattern::Voronoi,
        11 => AnimatedPattern::Scanline,
        12 => AnimatedPattern::Fireflies,
        13 => AnimatedPattern::SmokeInk,
        14 => AnimatedPattern::WaterRipples,
        15 => AnimatedPattern::MatrixRain3D,
        16 => AnimatedPattern::Lcars,
        17 => AnimatedPattern::Terminal,
        18 => AnimatedPattern::Fractals,
        19 => AnimatedPattern::ReactionDiffusion,
        _ => AnimatedPattern::Matrix,
    }
}

pub fn pattern_label(index: u32, lang: Language) -> &'static str {
    match index {
        1 => tr(lang, "Звёзды"),
        2 => tr(lang, "Геометрия"),
        3 => tr(lang, "Поле потока"),
        4 => tr(lang, "Северное сияние"),
        5 => tr(lang, "Плазма"),
        6 => tr(lang, "Боке"),
        7 => tr(lang, "Созвездия"),
        8 => tr(lang, "Кривые Лиссажу"),
        9 => tr(lang, "Волны"),
        10 => tr(lang, "Ячейки Вороного"),
        11 => tr(lang, "ЭЛТ-монитор"),
        12 => tr(lang, "Светлячки"),
        13 => tr(lang, "Дым/Чернила"),
        14 => tr(lang, "Водная рябь"),
        15 => tr(lang, "Матрица 2.0"),
        16 => tr(lang, "LCARS"),
        17 => tr(lang, "Терминал"),
        18 => tr(lang, "Фракталы"),
        19 => tr(lang, "Реакция-диффузия"),
        _ => tr(lang, "Матрица"),
    }
}

pub fn hex_to_rgba(hex: &str) -> Option<RGBA> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(RGBA::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            1.0,
        ))
    } else {
        None
    }
}

pub fn rgba_to_hex(rgba: RGBA) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (rgba.red() * 255.0) as u8,
        (rgba.green() * 255.0) as u8,
        (rgba.blue() * 255.0) as u8
    )
}

pub fn collect_image_paths(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut images = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if is_allowed_image_ext(ext) {
                        images.push(path);
                    }
                }
            }
        }
    }
    images.sort();
    images
}

pub fn is_allowed_image_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "bmp"
            | "gif"
            | "webp"
            | "tiff"
            | "tif"
            | "tga"
            | "ico"
            | "ppm"
            | "pgm"
            | "pbm"
            | "hdr"
            | "exr"
    )
}

pub fn format_file_size(bytes: u64, lang: Language) -> String {
    const UNITS: [&str; 5] = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
    let mut size = bytes as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    let unit_label = tr(lang, UNITS[unit]);
    if unit == 0 {
        format!("{bytes} {unit_label}")
    } else {
        format!("{:.1} {unit_label}", size)
    }
}

pub fn hash_u32(x: u32) -> u32 {
    let mut x = x;
    x = ((x >> 16) ^ x).wrapping_mul(0x45d9f3b);
    x = ((x >> 16) ^ x).wrapping_mul(0x45d9f3b);
    (x >> 16) ^ x
}

pub fn hash_f64(x: u32) -> f64 {
    (hash_u32(x) as f64) / (u32::MAX as f64)
}

pub fn draw_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    pattern: AnimatedPattern,
    speed_mult: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    let t = time * speed_mult;
    if width <= 1.0 || height <= 1.0 {
        return;
    }
    match pattern {
        AnimatedPattern::Matrix => draw_matrix_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Stars => draw_stars_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Geometry => draw_geometry_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Flowfield => draw_flowfield_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Aurora => draw_aurora_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Plasma => draw_plasma_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Bokeh => draw_bokeh_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Constellation => {
            draw_constellation_pattern(cr, width, height, t, density, theme)
        }
        AnimatedPattern::Lissajous => draw_lissajous_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Waves => draw_waves_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Voronoi => draw_voronoi_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Scanline => draw_scanline_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Fireflies => draw_fireflies_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::SmokeInk => draw_plasma_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::WaterRipples => draw_waves_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::MatrixRain3D => draw_matrix_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Lcars => draw_geometry_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Terminal => draw_matrix_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::Fractals => draw_plasma_pattern(cr, width, height, t, density, theme),
        AnimatedPattern::ReactionDiffusion => {
            draw_plasma_pattern(cr, width, height, t, density, theme)
        }
    }
}

fn apply_theme(r: f64, g: f64, b: f64, theme: PatternTheme) -> (f64, f64, f64) {
    match theme {
        PatternTheme::Default => (r, g, b),
        PatternTheme::Mono => {
            let gray = r * 0.299 + g * 0.587 + b * 0.114;
            (gray, gray, gray)
        }
        PatternTheme::Warm => {
            let gray = r * 0.299 + g * 0.587 + b * 0.114;
            // Shift towards orange/red
            (gray.powf(0.5), gray * 0.7, gray * 0.2)
        }
        PatternTheme::Cool => {
            let gray = r * 0.299 + g * 0.587 + b * 0.114;
            // Shift towards cyan/blue
            (gray * 0.2, gray * 0.7, gray.powf(0.5))
        }
        PatternTheme::Random => {
            // How to make this stable? Use fixed hue shift based on 'r' maybe?
            // Or just swap channels?
            // Swap channels based on brightness
            (g, b, r)
        }
    }
}

fn draw_matrix_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    cr.set_source_rgb(0.0, 0.0, 0.0);

    let _ = cr.paint();

    let cell = match density {
        PatternDensity::Low => 24.0,

        PatternDensity::Medium => 16.0,

        PatternDensity::High => 12.0,
    };

    let columns = (width / cell).ceil() as u32;

    for col in 0..columns {
        let seed = col.wrapping_mul(1103515245).wrapping_add(12345);
        let speed = 40.0 + (hash_u32(seed) % 70) as f64;
        let offset = (hash_u32(seed ^ 0x9e3779b9) % 1000) as f64;
        let tail = 6 + (hash_u32(seed ^ 0x7f4a7c15) % 14) as i32;
        let head = (time * speed + offset) % (height + cell * tail as f64);
        for i in 0..tail {
            let y = head - i as f64 * cell;
            if y < -cell || y > height + cell {
                continue;
            }
            let alpha = 1.0 - i as f64 / tail as f64;
            let green = 0.2 + 0.8 * alpha;
            let (r, g, b) = apply_theme(0.0, green, 0.0, theme);
            cr.set_source_rgba(r, g, b, 1.0);
            cr.rectangle(col as f64 * cell, y, cell * 0.75, cell * 0.75);
            let _ = cr.fill();
        }
    }
}
fn draw_stars_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    cr.set_source_rgb(0.01, 0.01, 0.04);
    let _ = cr.paint();

    let base_density = (width * height) / 8500.0;
    let count = match density {
        PatternDensity::Low => (base_density * 0.5) as u32,
        PatternDensity::Medium => base_density as u32,
        PatternDensity::High => (base_density * 2.0) as u32,
    };
    let density = count.clamp(20, 1000);

    for i in 0..density {
        let seed = i.wrapping_mul(2654435761);
        let x = hash_f64(seed) * width;
        let y = hash_f64(seed.wrapping_add(1)) * height;
        let speed = 20.0 + hash_f64(seed.wrapping_add(2)) * 90.0;
        let radius = 0.6 + hash_f64(seed.wrapping_add(3)) * 2.0;
        let y_pos = (y + time * speed) % height;
        let twinkle =
            (time * 2.0 + hash_f64(seed.wrapping_add(4)) * std::f64::consts::PI * 2.0).sin() * 0.5
                + 0.5;
        let brightness = 0.6 + 0.4 * twinkle;
        let (r, g, b) = apply_theme(brightness, brightness, 1.0, theme);
        cr.set_source_rgba(r, g, b, 1.0);
        cr.arc(x, y_pos, radius, 0.0, std::f64::consts::PI * 2.0);
        let _ = cr.fill();
    }
}

fn draw_geometry_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    cr.set_source_rgb(0.04, 0.04, 0.07);
    let _ = cr.paint();

    let spacing = match density {
        PatternDensity::Low => 120.0,
        PatternDensity::Medium => 90.0,
        PatternDensity::High => 60.0,
    };
    let cols = (width / spacing).ceil() as u32;
    let rows = (height / spacing).ceil() as u32;
    cr.set_line_width(2.0);

    for col in 0..cols {
        for row in 0..rows {
            let seed = col.wrapping_mul(73856093) ^ row.wrapping_mul(19349663);
            let base = hash_f64(seed);
            let cx = col as f64 * spacing + spacing * 0.5;
            let cy = row as f64 * spacing + spacing * 0.5;
            let size = spacing * (0.28 + 0.18 * base);
            let angle = time * 0.5 + base * std::f64::consts::PI * 2.0;
            let pulse = (time * 1.1 + base * std::f64::consts::PI * 2.0).sin() * 0.5 + 0.5;
            let r_base = 0.2 + 0.6 * pulse;
            let g_base = 0.4 + 0.4 * (1.0 - pulse);
            let b_base = 0.7 + 0.2 * pulse;
            let (r, g, b) = apply_theme(r_base, g_base, b_base, theme);

            let _ = cr.save();
            cr.translate(cx, cy);
            cr.rotate(angle);
            cr.set_source_rgba(r, g, b, 0.9);
            cr.rectangle(-size * 0.5, -size * 0.5, size, size);
            let _ = cr.stroke();

            let tri = size * 0.7;
            cr.move_to(0.0, -tri * 0.5);
            cr.line_to(tri * 0.5, tri * 0.5);
            cr.line_to(-tri * 0.5, tri * 0.5);
            cr.close_path();
            let _ = cr.stroke();
            let _ = cr.restore();
        }
    }
}

fn draw_flowfield_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    // Dark background
    cr.set_source_rgb(0.02, 0.02, 0.05);
    let _ = cr.paint();

    let spacing = match density {
        PatternDensity::Low => 70.0,
        PatternDensity::Medium => 50.0,
        PatternDensity::High => 35.0,
    };
    // Cover slightly more than the area to avoid edge artifacts
    let cols = (width / spacing).ceil() as i32 + 1;
    let rows = (height / spacing).ceil() as i32 + 1;

    cr.set_line_cap(cairo::LineCap::Round);

    for col in -1..cols {
        for row in -1..rows {
            let x = col as f64 * spacing;
            let y = row as f64 * spacing;

            // Value noise approximation
            let sx = x * 0.003;
            let sy = y * 0.003;
            let t = time * 0.15;

            // Layered sine waves for organic look
            let n1 = (sx * 2.0 + t).sin() * (sy * 2.0 - t * 0.8).cos();
            let n2 = (sx * 5.0 - t * 1.2).sin() * (sy * 5.0 + t * 1.5).cos();
            let n3 = (sx * 11.0 + t * 2.0).cos() * (sy * 11.0 - t * 2.5).sin();

            let noise = n1 + n2 * 0.5 + n3 * 0.25;
            let angle = noise * std::f64::consts::PI;

            let length = 24.0 + noise * 10.0;
            let dx = angle.cos() * length;
            let dy = angle.sin() * length;

            // Color palette: Cyan/Blue/Purple drift
            let r_base = 0.1 + 0.4 * (angle.sin() * 0.5 + 0.5);
            let g_base = 0.4 + 0.4 * (angle.cos() * 0.5 + 0.5);
            let b_base = 0.8;
            let a = 0.2 + 0.5 * (noise.abs().min(1.0));
            let (r, g, b) = apply_theme(r_base, g_base, b_base, theme);

            cr.set_source_rgba(r, g, b, a);
            cr.set_line_width(2.0 + 1.0 * noise.abs());

            cr.move_to(x - dx * 0.5, y - dy * 0.5);
            cr.line_to(x + dx * 0.5, y + dy * 0.5);
            let _ = cr.stroke();
        }
    }
}

fn draw_aurora_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    // Dark background
    let gradient = cairo::LinearGradient::new(0.0, 0.0, 0.0, height);
    gradient.add_color_stop_rgb(0.0, 0.0, 0.01, 0.05);
    gradient.add_color_stop_rgb(1.0, 0.0, 0.05, 0.15);
    let _ = cr.set_source(&gradient);
    let _ = cr.paint();

    let layers = match density {
        PatternDensity::Low => 3,
        PatternDensity::Medium => 5,
        PatternDensity::High => 8,
    };
    let step = 20.0;
    let steps = (width / step).ceil() as i32 + 2;

    for i in 0..layers {
        let layer = i as f64;
        let t = time * 0.2 + layer * 10.0;

        // Color shift: Green -> Blue -> Purple
        let hue = (t * 0.05 + layer * 0.1).sin() * 0.5 + 0.5;
        let r_base = 0.1 + 0.4 * hue;
        let g_base = 0.6 + 0.3 * (1.0 - hue);
        let b_base = 0.5 + 0.5 * hue;
        let a = 0.15 + 0.1 * (t.sin() * 0.5 + 0.5);
        let (r, g, b) = apply_theme(r_base, g_base, b_base, theme);

        cr.set_source_rgba(r, g, b, a);
        cr.new_path();

        // Start from bottom left
        cr.move_to(0.0, height);
        cr.line_to(0.0, height * 0.6);

        for s in 0..=steps {
            let x = s as f64 * step;
            let nx = x * 0.003;

            // Layered sine waves for organic "curtain" feel
            let y_norm = (nx * 3.0 + t).sin()
                + (nx * 7.0 - t * 0.8).cos() * 0.5
                + (nx * 13.0 + t * 1.5).sin() * 0.25;

            // Map to screen height with some variation
            let y_center = height * (0.5 + 0.1 * layer);
            let amplitude = height * 0.2;
            let y = y_center + y_norm * amplitude;

            cr.line_to(x, y);
        }

        // Close shape to bottom right
        cr.line_to(width, height);
        cr.close_path();
        let _ = cr.fill();
    }
}

fn draw_plasma_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    // Optimized step sizes to prevent UI blocking
    let step = match density {
        PatternDensity::Low => 60.0,
        PatternDensity::Medium => 40.0,
        PatternDensity::High => 20.0,
    };
    let cols = (width / step).ceil() as i32;
    let rows = (height / step).ceil() as i32;

    let t = time * 0.5;

    for col in 0..cols {
        for row in 0..rows {
            let x = col as f64 * step;
            let y = row as f64 * step;

            // Normalized coordinates
            let nx = x / width * 4.0;
            let ny = y / height * 4.0;

            let mut v = 0.0;
            v += (nx + t).sin();
            v += ((ny + t) * 0.5).sin();
            v += ((nx + ny + t) * 0.5).sin();
            let cx = nx + t * 0.2;
            let cy = ny + t * 0.3;
            v += (cx * cx + cy * cy + t).sin();

            // Map v (-4..4) to 0..1 range roughly
            let v = (v + 4.0) / 8.0;

            let r_base = (v * std::f64::consts::PI).sin() * 0.5 + 0.5;
            let g_base = (v * std::f64::consts::PI + 2.0).sin() * 0.5 + 0.5;
            let b_base = (v * std::f64::consts::PI + 4.0).sin() * 0.5 + 0.5;
            let (r, g, b) = apply_theme(r_base, g_base, b_base, theme);

            cr.set_source_rgb(r, g, b);
            cr.rectangle(x, y, step, step);
            let _ = cr.fill();
        }
    }
}

fn draw_bokeh_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    // Dark blurred background
    let gradient = cairo::LinearGradient::new(0.0, 0.0, width, height);
    gradient.add_color_stop_rgb(0.0, 0.05, 0.02, 0.05);
    gradient.add_color_stop_rgb(1.0, 0.1, 0.05, 0.1);
    let _ = cr.set_source(&gradient);
    let _ = cr.paint();

    let count = match density {
        PatternDensity::Low => 20u32,
        PatternDensity::Medium => 45u32,
        PatternDensity::High => 80u32,
    };

    for i in 0..count {
        let seed = i.wrapping_mul(982451653);

        let r_base = hash_f64(seed);
        let g_base = hash_f64(seed.wrapping_add(1));
        let b_base = hash_f64(seed.wrapping_add(2));

        // Random properties
        let radius = 20.0 + hash_f64(seed.wrapping_add(3)) * 80.0;
        let speed_x = -10.0 + hash_f64(seed.wrapping_add(4)) * 20.0;
        let speed_y = -10.0 + hash_f64(seed.wrapping_add(5)) * 20.0;
        let x_start = hash_f64(seed.wrapping_add(6)) * width;
        let y_start = hash_f64(seed.wrapping_add(7)) * height;

        // Parallax drift
        let x = (x_start + time * speed_x).rem_euclid(width + radius * 2.0) - radius;
        let y = (y_start + time * speed_y).rem_euclid(height + radius * 2.0) - radius;

        // Slow fade in/out
        let fade_speed = 0.2 + hash_f64(seed.wrapping_add(8)) * 0.5;
        let offset = hash_f64(seed.wrapping_add(9)) * 10.0;
        let alpha = 0.1 + ((time * fade_speed + offset).sin() * 0.5 + 0.5) * 0.15;

        // Color variation (warm/cool mix)
        let hue = hash_f64(seed.wrapping_add(10));
        let (r_col, g_col, b_col) = if hue > 0.5 {
            (0.8 + r_base * 0.2, 0.4 + g_base * 0.3, 0.2) // Warm
        } else {
            (0.1, 0.5 + g_base * 0.3, 0.7 + b_base * 0.3) // Cool
        };
        let (r, g, b) = apply_theme(r_col, g_col, b_col, theme);

        cr.set_source_rgba(r, g, b, alpha);
        cr.arc(x, y, radius, 0.0, std::f64::consts::PI * 2.0);
        let _ = cr.fill();
    }
}

fn draw_constellation_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    // Dark background
    cr.set_source_rgb(0.05, 0.05, 0.08);
    let _ = cr.paint();

    let count = match density {
        PatternDensity::Low => 40u32,
        PatternDensity::Medium => 80u32,
        PatternDensity::High => 140u32,
    };
    let connect_distance = match density {
        PatternDensity::Low => 200.0,
        PatternDensity::Medium => 150.0,
        PatternDensity::High => 100.0,
    };

    // Store positions to draw connections
    let mut points = Vec::with_capacity(count as usize);

    for i in 0..count {
        let seed = i.wrapping_mul(742938285);

        // Random start pos and velocity
        let x_start = hash_f64(seed) * width;
        let y_start = hash_f64(seed.wrapping_add(1)) * height;
        let vx = -20.0 + hash_f64(seed.wrapping_add(2)) * 40.0;
        let vy = -20.0 + hash_f64(seed.wrapping_add(3)) * 40.0;

        let x = (x_start + time * vx).rem_euclid(width);
        let y = (y_start + time * vy).rem_euclid(height);

        points.push((x, y));

        // Draw point
        let (r, g, b) = apply_theme(1.0, 1.0, 1.0, theme);
        cr.set_source_rgba(r, g, b, 0.6);
        cr.arc(x, y, 2.0, 0.0, std::f64::consts::PI * 2.0);
        let _ = cr.fill();
    }

    // Draw connections
    cr.set_line_width(1.0);
    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            let (x1, y1) = points[i];
            let (x2, y2) = points[j];

            let dx = x2 - x1;
            let dy = y2 - y1;
            let dist_sq = dx * dx + dy * dy;

            if dist_sq < connect_distance * connect_distance {
                let dist = dist_sq.sqrt();
                let alpha = 1.0 - (dist / connect_distance);

                // Subtle blue/white lines
                let (r, g, b) = apply_theme(0.6, 0.8, 1.0, theme);
                cr.set_source_rgba(r, g, b, alpha * 0.4);
                cr.move_to(x1, y1);
                cr.line_to(x2, y2);
                let _ = cr.stroke();
            }
        }
    }
}

fn draw_lissajous_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    // Dark background
    cr.set_source_rgb(0.0, 0.0, 0.05);
    let _ = cr.paint();

    let cx = width / 2.0;
    let cy = height / 2.0;
    let scale = width.min(height) * 0.4;

    // Slow evolution of parameters
    let a = 3.0 + (time * 0.05).sin() * 2.0;
    let b = 4.0 + (time * 0.03).cos() * 3.0;
    let delta = time * 0.2;

    // Draw a "trail" by iterating t backwards
    let tail_length = match density {
        PatternDensity::Low => 10.0,
        PatternDensity::Medium => 20.0,
        PatternDensity::High => 40.0,
    };
    let step = 0.05;
    let steps = (tail_length / step) as i32;

    cr.set_line_width(2.0);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);

    let mut prev_x = cx + scale * (a * time + delta).sin();
    let mut prev_y = cy + scale * (b * time).sin();

    for i in 0..steps {
        let t_offset = i as f64 * step;
        let t = time - t_offset;

        let alpha = 1.0 - (i as f64 / steps as f64);
        if alpha <= 0.0 {
            break;
        }

        let x = cx + scale * (a * t + delta).sin();
        let y = cy + scale * (b * t).sin();

        // Color logic: shifting hue
        let r_base = 0.5 + 0.5 * (t * 0.5).sin();
        let g_base = 0.5 + 0.5 * (t * 0.3 + 2.0).sin();
        let b_base = 0.5 + 0.5 * (t * 0.7 + 4.0).sin();
        let (r, g, b) = apply_theme(r_base, g_base, b_base, theme);

        cr.set_source_rgba(r, g, b, alpha);

        cr.move_to(prev_x, prev_y);
        cr.line_to(x, y);
        let _ = cr.stroke();

        prev_x = x;
        prev_y = y;
    }
}

fn draw_waves_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    // Very dark blue-grey background
    cr.set_source_rgb(0.02, 0.03, 0.05);
    let _ = cr.paint();

    let lines = match density {
        PatternDensity::Low => 15,
        PatternDensity::Medium => 25,
        PatternDensity::High => 40,
    };
    let spacing = height / (lines as f64);
    let step = 50.0;
    let steps = (width / step).ceil() as i32 + 1;

    cr.set_line_width(2.0);
    cr.set_line_cap(cairo::LineCap::Round);

    for i in 0..lines {
        let y_base = i as f64 * spacing + spacing * 0.5;
        let layer = i as f64;

        // Subtle color shift
        let r_base = 0.3 + 0.2 * ((layer * 0.1 + time * 0.2).sin() * 0.5 + 0.5);
        let g_base = 0.5 + 0.3 * ((layer * 0.15 + time * 0.3).cos() * 0.5 + 0.5);
        let b_base = 0.8;
        let alpha = 0.6;
        let (r, g, b) = apply_theme(r_base, g_base, b_base, theme);

        cr.set_source_rgba(r, g, b, alpha);
        cr.new_path();

        // Start left
        cr.move_to(0.0, y_base);

        for s in 0..=steps {
            let x = s as f64 * step;

            // Multiple interference terms
            let nx = x * 0.005;
            let ny = y_base * 0.005;
            let t = time * 0.5;

            let w1 = (nx * 5.0 + t).sin();
            let w2 = (nx * 12.0 - t * 1.5 + ny * 3.0).cos();
            let w3 = (nx * 2.0 + t * 0.5).sin();

            let displacement = (w1 * 20.0 + w2 * 10.0 + w3 * 30.0)
                * (0.5 + 0.5 * ((layer / lines as f64) * std::f64::consts::PI).sin());

            // Use curve_to for smoother lines with fewer segments?
            // For now, straight lines with fewer segments is safer for perf.
            cr.line_to(x, y_base + displacement);
        }

        let _ = cr.stroke();
    }
}

fn draw_voronoi_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    let seed_count = match density {
        PatternDensity::Low => 6,
        PatternDensity::Medium => 12,
        PatternDensity::High => 20,
    };
    let step = 25.0; // Aggressively optimized

    // Compute seed positions
    let mut seeds = Vec::with_capacity(seed_count);
    for i in 0..seed_count {
        let idx = i as u32;
        let seed = idx.wrapping_mul(123456789);

        let x_base = hash_f64(seed) * width;
        let y_base = hash_f64(seed.wrapping_add(1)) * height;
        let vx = -15.0 + hash_f64(seed.wrapping_add(2)) * 30.0;
        let vy = -15.0 + hash_f64(seed.wrapping_add(3)) * 30.0;

        let x = (x_base + time * vx).rem_euclid(width);
        let y = (y_base + time * vy).rem_euclid(height);

        // Seed color (hue)
        let r_base = hash_f64(seed.wrapping_add(4));
        let g_base = hash_f64(seed.wrapping_add(5));
        let b_base = hash_f64(seed.wrapping_add(6));
        let (r, g, b) = apply_theme(r_base, g_base, b_base, theme);

        seeds.push((x, y, r, g, b));
    }

    let cols = (width / step).ceil() as i32;
    let rows = (height / step).ceil() as i32;

    for col in 0..cols {
        for row in 0..rows {
            let px = col as f64 * step + step * 0.5;
            let py = row as f64 * step + step * 0.5;

            let mut min_dist_sq = f64::MAX;
            let mut closest_idx = 0;

            for (i, (sx, sy, _, _, _)) in seeds.iter().enumerate() {
                let dx = px - sx;
                let dy = py - sy;
                // Toroidal distance for seamless wrapping?
                // Or just clamp. Let's do simple distance for now.
                let d_sq = dx * dx + dy * dy;
                if d_sq < min_dist_sq {
                    min_dist_sq = d_sq;
                    closest_idx = i;
                }
            }

            let (_, _, r, g, b) = seeds[closest_idx];

            // Distance shading
            let dist = min_dist_sq.sqrt();
            let shade = (1.0 - dist / 300.0).max(0.2);

            cr.set_source_rgb(r * shade, g * shade, b * shade);
            cr.rectangle(col as f64 * step, row as f64 * step, step, step);
            let _ = cr.fill();
        }
    }

    // Draw seed points
    for (x, y, _, _, _) in seeds {
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.arc(x, y, 3.0, 0.0, std::f64::consts::PI * 2.0);
        let _ = cr.fill();
    }
}

fn draw_scanline_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    // 1. Base dark background
    cr.set_source_rgb(0.02, 0.02, 0.03);
    let _ = cr.paint();

    // 2. Moving scanlines
    let scan_height = match density {
        PatternDensity::Low => 16.0,
        PatternDensity::Medium => 10.0,
        PatternDensity::High => 6.0,
    };
    let scan_gap = 4.0;
    let total_h = scan_height + scan_gap;
    let count = (height / total_h).ceil() as i32 + 1;

    let roll_offset = (time * 20.0).rem_euclid(total_h);

    let (r, g, b) = apply_theme(0.0, 0.1, 0.0, theme);
    cr.set_source_rgba(r, g, b, 0.2);
    cr.new_path();
    for i in -1..count {
        let y = i as f64 * total_h + roll_offset;
        cr.rectangle(0.0, y, width, scan_height);
    }
    let _ = cr.fill();

    // 3. Scan beam - Simplified
    let beam_y = (time * 200.0).rem_euclid(height + 200.0) - 100.0;
    // Simple translucent rect instead of gradient for speed
    let (br, bg, bb) = apply_theme(0.5, 0.0, 0.0, theme);
    cr.set_source_rgba(br, bg, bb, 0.1);
    cr.rectangle(0.0, beam_y, width, 80.0);
    let _ = cr.fill();

    // 4. Fake Vignette (Border rects instead of radial gradient)
    // Drawing 4 opaque/semi-opaque rectangles is much faster than a radial gradient.
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.5);
    let border = 60.0;

    // Top
    cr.rectangle(0.0, 0.0, width, border);
    // Bottom
    cr.rectangle(0.0, height - border, width, border);
    // Left (minus corners)
    cr.rectangle(0.0, border, border, height - 2.0 * border);
    // Right (minus corners)
    cr.rectangle(width - border, border, border, height - 2.0 * border);

    let _ = cr.fill();
}

fn draw_fireflies_pattern(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    time: f64,
    density: PatternDensity,
    theme: PatternTheme,
) {
    // Dark night background
    let gradient = cairo::LinearGradient::new(0.0, 0.0, 0.0, height);
    gradient.add_color_stop_rgb(0.0, 0.01, 0.01, 0.02);
    gradient.add_color_stop_rgb(1.0, 0.02, 0.03, 0.05);
    let _ = cr.set_source(&gradient);
    let _ = cr.paint();

    let count = match density {
        PatternDensity::Low => 20u32,
        PatternDensity::Medium => 50u32,
        PatternDensity::High => 100u32,
    };

    for i in 0..count {
        let seed = i.wrapping_mul(864213579);

        let x_base = hash_f64(seed) * width;
        let y_base = hash_f64(seed.wrapping_add(1)) * height;

        // Wandering motion
        let nx = hash_f64(seed.wrapping_add(2)) * 100.0;
        let ny = hash_f64(seed.wrapping_add(3)) * 100.0;

        // Use time to drive position
        let t_offset = hash_f64(seed.wrapping_add(4)) * 10.0;
        let t = time * 0.5 + t_offset;

        let dx = (t * 0.7 + nx).sin() * 50.0 + (t * 1.3).cos() * 30.0;
        let dy = (t * 0.5 + ny).cos() * 50.0 + (t * 1.7).sin() * 30.0;

        let x = (x_base + dx).rem_euclid(width + 40.0) - 20.0;
        let y = (y_base + dy).rem_euclid(height + 40.0) - 20.0;

        // Pulsing brightness
        let pulse_speed = 1.0 + hash_f64(seed.wrapping_add(5));
        let pulse = (t * pulse_speed).sin() * 0.5 + 0.5; // 0.0 to 1.0

        // Color: Yellow-Green/Gold
        let r_base = 0.8 + 0.2 * pulse;
        let g_base = 0.9 + 0.1 * pulse;
        let b_base = 0.2;
        let alpha = 0.3 + 0.7 * pulse;
        let (r, g, b) = apply_theme(r_base, g_base, b_base, theme);

        // Glow (larger, faint)
        cr.set_source_rgba(r, g, b, alpha * 0.3);
        cr.arc(x, y, 6.0 + 4.0 * pulse, 0.0, std::f64::consts::PI * 2.0);
        let _ = cr.fill();

        // Core (small, bright)
        cr.set_source_rgba(1.0, 1.0, 0.8, alpha);
        cr.arc(x, y, 2.0, 0.0, std::f64::consts::PI * 2.0);
        let _ = cr.fill();
    }
}

pub fn build_preview_placeholder(text: &str) -> gtk4::Widget {
    let label = Label::new(Some(text));
    label.add_css_class("dim-label");
    label.set_hexpand(true);
    label.set_vexpand(true);
    label.upcast()
}

pub fn build_preview_widget(
    mode: u32,
    color_button: &ColorDialogButton,
    g_start: &ColorDialogButton,
    g_end: &ColorDialogButton,
    pattern_row: &adw::ComboRow,
    pattern_speed_row: &adw::ComboRow,
    pattern_density_row: &adw::ComboRow,
    pattern_theme_row: &adw::ComboRow,
    water_ripples_bg_path: Option<&std::path::Path>,
    web_url_row: &adw::EntryRow,
    stream_url_row: &adw::EntryRow,
    file_row: &adw::ActionRow,
    mute_switch: &Switch,
    video_volume: u8,
    preview_paused: bool,
    preview_path: Option<&std::path::Path>,
    clock_enabled: bool,
    clock_two_lines: bool,
    clock_format: &str,
    clock_time_format: &str,
    clock_date_format: &str,
    clock_position: ClockPosition,
    clock_size: u32,
    lang: Language,
) -> (gtk4::Widget, Option<MediaFile>, Option<webkit6::WebView>) {
    let (base_widget, media, web_view) = match mode {
        0 => {
            let rgba = color_button.rgba();
            let drawing_area = DrawingArea::new();
            drawing_area.set_draw_func(move |_, cr, _, _| {
                cr.set_source_rgba(
                    rgba.red() as f64,
                    rgba.green() as f64,
                    rgba.blue() as f64,
                    rgba.alpha() as f64,
                );
                let _ = cr.paint();
            });
            (drawing_area.upcast(), None, None)
        }
        1 => {
            let start = g_start.rgba();
            let end = g_end.rgba();
            let drawing_area = DrawingArea::new();
            drawing_area.set_draw_func(move |_, cr, width, height| {
                let gradient = cairo::LinearGradient::new(0.0, 0.0, width as f64, height as f64);
                gradient.add_color_stop_rgba(
                    0.0,
                    start.red() as f64,
                    start.green() as f64,
                    start.blue() as f64,
                    start.alpha() as f64,
                );
                gradient.add_color_stop_rgba(
                    1.0,
                    end.red() as f64,
                    end.green() as f64,
                    end.blue() as f64,
                    end.alpha() as f64,
                );
                let _ = cr.set_source(&gradient);
                let _ = cr.paint();
            });
            (drawing_area.upcast(), None, None)
        }
        2 => {
            let speed = match pattern_speed_row.selected() {
                0 => PatternSpeed::Slow,
                2 => PatternSpeed::Fast,
                _ => PatternSpeed::Normal,
            };
            let density = match pattern_density_row.selected() {
                0 => PatternDensity::Low,
                2 => PatternDensity::High,
                _ => PatternDensity::Medium,
            };
            let theme = match pattern_theme_row.selected() {
                0 => PatternTheme::Default,
                1 => PatternTheme::Mono,
                2 => PatternTheme::Warm,
                3 => PatternTheme::Cool,
                4 => PatternTheme::Random,
                _ => PatternTheme::Default,
            };
            (
                build_pattern_preview(
                    pattern_from_index(pattern_row.selected()),
                    speed,
                    density,
                    theme,
                    water_ripples_bg_path,
                ),
                None,
                None,
            )
        }
        3 => {
            let url = web_url_row.text().trim().to_string();
            if url.is_empty() {
                return (
                    build_preview_placeholder(tr(lang, "URL не указан")),
                    None,
                    None,
                );
            }
            let web_view = webkit6::WebView::new();
            web_view.load_uri(&url);
            web_view.set_can_focus(false);
            web_view.set_is_muted(mute_switch.is_active());
            (web_view.clone().upcast(), None, Some(web_view))
        }
        7 => {
            let url = stream_url_row.text().trim().to_string();
            if url.is_empty() {
                return (
                    build_preview_placeholder(tr(lang, "URL не указан")),
                    None,
                    None,
                );
            }
            let media = MediaFile::for_file(&gio::File::for_uri(&url));
            media.set_muted(mute_switch.is_active());
            media.set_volume(video_volume as f64 / 100.0);
            media.set_playing(!preview_paused);
            let video = Video::for_media_stream(Some(&media));
            video.set_autoplay(!preview_paused);
            video.set_loop(false);
            video.set_graphics_offload(GraphicsOffloadEnabled::Enabled);
            video.set_can_focus(false);
            video.set_focusable(false);
            video.set_focus_on_click(false);
            video.set_can_target(false);
            hide_media_controls(&video.clone().upcast::<Widget>());
            let offload = GraphicsOffload::new(Some(&video));
            offload.set_enabled(GraphicsOffloadEnabled::Enabled);
            (offload.upcast(), Some(media), None)
        }
        4 | 5 => {
            let path = preview_path
                .map(|p| p.to_path_buf())
                .or_else(|| file_row_path(file_row));
            let Some(path) = path else {
                return (
                    build_preview_placeholder(tr(lang, "Файл не выбран")),
                    None,
                    None,
                );
            };
            if let Err(msg) = validate_media_path(
                &path,
                if mode == 4 {
                    MediaKind::Image
                } else {
                    MediaKind::Video
                },
                false,
                lang,
            ) {
                return (build_preview_placeholder(&msg), None, None);
            }
            let file = gio::File::for_path(path);
            if mode == 4 {
                let picture = Picture::for_file(&file);
                picture.set_content_fit(ContentFit::Contain);
                (picture.upcast(), None, None)
            } else {
                let media = MediaFile::for_file(&file);
                media.set_loop(true);
                media.set_muted(mute_switch.is_active());
                media.set_volume(video_volume as f64 / 100.0);
                media.set_playing(!preview_paused);
                let video = Video::for_media_stream(Some(&media));
                video.set_autoplay(!preview_paused);
                video.set_loop(true);
                video.set_graphics_offload(GraphicsOffloadEnabled::Enabled);
                video.set_can_focus(false);
                video.set_focusable(false);
                video.set_focus_on_click(false);
                video.set_can_target(false);
                hide_media_controls(&video.clone().upcast::<Widget>());
                let offload = GraphicsOffload::new(Some(&video));
                offload.set_enabled(GraphicsOffloadEnabled::Enabled);
                (offload.upcast(), Some(media), None)
            }
        }
        6 => {
            let path = file_row_path(file_row);
            let Some(path) = path else {
                return (
                    build_preview_placeholder(tr(lang, "Папка не выбрана")),
                    None,
                    None,
                );
            };
            if let Err(msg) = validate_media_path(&path, MediaKind::ImageFolder, false, lang) {
                return (build_preview_placeholder(&msg), None, None);
            }
            let images = collect_image_paths(&path);
            let picture = Picture::for_file(&gio::File::for_path(&images[0]));
            picture.set_content_fit(ContentFit::Contain);
            (picture.upcast(), None, None)
        }
        8 => {
            let path = preview_path
                .map(|p| p.to_path_buf())
                .or_else(|| file_row_path(file_row));
            let Some(path) = path else {
                return (
                    build_preview_placeholder(tr(lang, "Файл не выбран")),
                    None,
                    None,
                );
            };
            if let Err(msg) = validate_python_script_path(&path, lang) {
                return (build_preview_placeholder(&msg), None, None);
            }
            (
                build_preview_placeholder(tr(lang, "Предпросмотр не поддерживается")),
                None,
                None,
            )
        }
        9 => {
            let path = preview_path
                .map(|p| p.to_path_buf())
                .or_else(|| file_row_path(file_row));
            let Some(path) = path else {
                return (
                    build_preview_placeholder(tr(lang, "Файл не выбран")),
                    None,
                    None,
                );
            };
            if let Err(msg) = validate_shadertoy_shader_path(&path, lang) {
                return (build_preview_placeholder(&msg), None, None);
            }
            (
                build_preview_placeholder(tr(lang, "Предпросмотр не поддерживается")),
                None,
                None,
            )
        }
        _ => (
            build_preview_placeholder(tr(lang, "Режим не поддерживается")),
            None,
            None,
        ),
    };

    if !clock_enabled {
        return (base_widget, media, web_view);
    }

    // If two-line mode is requested, it fully overrides the single-line format.
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&base_widget));

    if clock_two_lines {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 2);

        let time_label = Label::new(Some(&format_clock_text(clock_time_format)));
        time_label.add_css_class("title-2");
        time_label.set_opacity(0.92);

        let date_label = Label::new(Some(&format_clock_text(clock_date_format)));
        date_label.add_css_class("title-4");
        date_label.set_opacity(0.85);

        apply_clock_size(&time_label, clock_size);
        apply_clock_size(&date_label, (clock_size.saturating_mul(55) / 100).max(8));

        container.append(&time_label);
        container.append(&date_label);

        // Apply positioning to the container.
        container.set_margin_top(0);
        container.set_margin_bottom(0);
        container.set_margin_start(0);
        container.set_margin_end(0);
        match clock_position {
            ClockPosition::TopLeft => {
                container.set_halign(Align::Start);
                container.set_valign(Align::Start);
                container.set_margin_top(12);
                container.set_margin_start(12);
            }
            ClockPosition::TopCenter => {
                container.set_halign(Align::Center);
                container.set_valign(Align::Start);
                container.set_margin_top(12);
            }
            ClockPosition::TopRight => {
                container.set_halign(Align::End);
                container.set_valign(Align::Start);
                container.set_margin_top(12);
                container.set_margin_end(12);
            }
            ClockPosition::CenterLeft => {
                container.set_halign(Align::Start);
                container.set_valign(Align::Center);
                container.set_margin_start(12);
            }
            ClockPosition::Center => {
                container.set_halign(Align::Center);
                container.set_valign(Align::Center);
            }
            ClockPosition::CenterRight => {
                container.set_halign(Align::End);
                container.set_valign(Align::Center);
                container.set_margin_end(12);
            }
            ClockPosition::BottomLeft => {
                container.set_halign(Align::Start);
                container.set_valign(Align::End);
                container.set_margin_bottom(12);
                container.set_margin_start(12);
            }
            ClockPosition::BottomCenter => {
                container.set_halign(Align::Center);
                container.set_valign(Align::End);
                container.set_margin_bottom(12);
            }
            ClockPosition::BottomRight => {
                container.set_halign(Align::End);
                container.set_valign(Align::End);
                container.set_margin_bottom(12);
                container.set_margin_end(12);
            }
        }

        // And align text within the labels.
        let xalign = match clock_position {
            ClockPosition::TopLeft | ClockPosition::CenterLeft | ClockPosition::BottomLeft => 0.0,
            ClockPosition::TopCenter | ClockPosition::Center | ClockPosition::BottomCenter => 0.5,
            ClockPosition::TopRight | ClockPosition::CenterRight | ClockPosition::BottomRight => {
                1.0
            }
        };
        time_label.set_xalign(xalign);
        date_label.set_xalign(xalign);

        overlay.add_overlay(&container);

        let time_format = clock_time_format.to_string();
        let date_format = clock_date_format.to_string();
        let time_weak = time_label.downgrade();
        let date_weak = date_label.downgrade();
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(time_label) = time_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let Some(date_label) = date_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            time_label.set_text(&format_clock_text(&time_format));
            date_label.set_text(&format_clock_text(&date_format));
            glib::ControlFlow::Continue
        });

        return (overlay.upcast(), media, web_view);
    }

    let label = Label::new(None);
    label.set_text(&format_clock_text(clock_format));
    label.add_css_class("title-2");
    label.set_opacity(0.9);
    apply_clock_size(&label, clock_size);
    apply_clock_position(&label, clock_position, 12);
    overlay.add_overlay(&label);
    let format = clock_format.to_string();
    let label_weak = label.downgrade();
    glib::timeout_add_local(Duration::from_secs(1), move || {
        let Some(label) = label_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        label.set_text(&format_clock_text(&format));
        glib::ControlFlow::Continue
    });
    (overlay.upcast(), media, web_view)
}

fn hide_media_controls(root: &Widget) {
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        if widget.clone().downcast::<MediaControls>().is_ok() {
            widget.set_visible(false);
            continue;
        }

        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            stack.push(current);
        }
    }
}

pub fn build_pattern_preview(
    pattern: AnimatedPattern,
    speed: PatternSpeed,
    density: PatternDensity,
    theme: PatternTheme,
    water_ripples_bg_path: Option<&std::path::Path>,
) -> gtk4::Widget {
    let bg = water_ripples_bg_path
        .and_then(|p| p.to_str())
        .filter(|_| matches!(pattern, AnimatedPattern::WaterRipples));
    crate::ui::gl_patterns::build_gl_pattern_area(pattern, speed, density, theme, bg).upcast()
}

pub fn file_row_path(file_row: &adw::ActionRow) -> Option<std::path::PathBuf> {
    let path = file_row.tooltip_text()?.trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(path))
    }
}

pub fn validate_media_path(
    path: &std::path::Path,
    kind: MediaKind,
    check_content: bool,
    lang: Language,
) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err(tr(lang, "Файл не выбран").to_string());
    }
    if !path.exists() {
        return Err(tr(lang, "Файл не найден: {path}").replace("{path}", &path.to_string_lossy()));
    }
    if matches!(kind, MediaKind::ImageFolder) {
        if !path.is_dir() {
            return Err(tr(lang, "Путь не является папкой").to_string());
        }
        if collect_image_paths(path).is_empty() {
            return Err(tr(lang, "В папке нет изображений").to_string());
        }
        return Ok(());
    }
    if !path.is_file() {
        return Err(tr(lang, "Путь не является файлом").to_string());
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let allowed = match kind {
        MediaKind::Image => &[
            "png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff", "tif", "tga", "ico", "ppm", "pgm",
            "pbm", "hdr", "exr",
        ][..],
        MediaKind::Video => &[
            "mp4", "mkv", "webm", "avi", "mov", "m4v", "mpg", "mpeg", "ogv", "wmv", "flv", "m2ts",
            "mts",
        ][..],
        MediaKind::ImageFolder => &[][..],
    };
    if !allowed.contains(&ext.as_str()) {
        return Err(tr(lang, "Неподдерживаемый формат").to_string());
    }
    if check_content && image::open(path).is_err() {
        return Err(tr(lang, "Файл не является корректным изображением").to_string());
    }
    Ok(())
}

pub fn validate_python_script_path(path: &std::path::Path, lang: Language) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err(tr(lang, "Файл не выбран").to_string());
    }
    if !path.exists() {
        return Err(
            tr(lang, "Файл не найден: {path}").replace("{path}", &path.to_string_lossy())
        );
    }
    if !path.is_file() {
        return Err(tr(lang, "Путь не является файлом").to_string());
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if ext != "py" {
        return Err(tr(lang, "Неподдерживаемый формат").to_string());
    }
    Ok(())
}

pub fn validate_shadertoy_shader_path(
    path: &std::path::Path,
    lang: Language,
) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err(tr(lang, "Файл не выбран").to_string());
    }
    if !path.exists() {
        return Err(
            tr(lang, "Файл не найден: {path}").replace("{path}", &path.to_string_lossy())
        );
    }
    if path.is_dir() {
        if !dir_has_shadertoy_image_shader(path) {
            return Err(tr(lang, "В папке нет шейдера Image").to_string());
        }
        return Ok(());
    }
    if !path.is_file() {
        return Err(tr(lang, "Путь не является файлом").to_string());
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(ext.as_str(), "glsl" | "frag" | "fs") {
        return Err(tr(lang, "Неподдерживаемый формат").to_string());
    }
    Ok(())
}

fn dir_has_shadertoy_image_shader(dir: &std::path::Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
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
        if key == "image" || key == "mainimage" {
            return true;
        }
    }
    false
}

pub struct ShadertoyDetectedFiles {
    pub base_dir: PathBuf,
    pub common: Option<PathBuf>,
    pub image: Option<PathBuf>,
    pub buffers: [Option<PathBuf>; 4],
}

pub fn detect_shadertoy_files(selection: &Path) -> ShadertoyDetectedFiles {
    let (base_dir, selected_file) = if selection.is_dir() {
        (selection.to_path_buf(), None)
    } else {
        (
            selection
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".")),
            Some(selection.to_path_buf()),
        )
    };

    let mut image: Option<PathBuf> = None;
    let mut common: Option<PathBuf> = None;
    let mut buffers: [Option<PathBuf>; 4] = [None, None, None, None];

    if let Ok(rd) = std::fs::read_dir(&base_dir) {
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

            match key.as_str() {
                "image" | "mainimage" => {
                    image.get_or_insert(path);
                }
                "common" => {
                    common.get_or_insert(path);
                }
                "buffera" => {
                    buffers[0].get_or_insert(path);
                }
                "bufferb" => {
                    buffers[1].get_or_insert(path);
                }
                "bufferc" => {
                    buffers[2].get_or_insert(path);
                }
                "bufferd" => {
                    buffers[3].get_or_insert(path);
                }
                _ => {}
            }
        }
    }

    // If the user selected a file explicitly, prefer it as Image when it is Image/MainImage.
    if let Some(file) = selected_file.as_ref() {
        if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
            let key: String = stem
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .map(|c| c.to_ascii_lowercase())
                .collect();
            if key == "image" || key == "mainimage" {
                image = Some(file.clone());
            }
        }
    }

    ShadertoyDetectedFiles {
        base_dir,
        common,
        image,
        buffers,
    }
}

pub fn preview_media_path(
    mode: u32,
    file_row: &adw::ActionRow,
    random_media: bool,
    media_files: &[String],
    selected_path: Option<&str>,
) -> Option<std::path::PathBuf> {
    match mode {
        4 | 5 => {
            if let Some(selected) = selected_path {
                if !selected.trim().is_empty() {
                    return Some(std::path::PathBuf::from(selected));
                }
            }
            if random_media && !media_files.is_empty() {
                Some(std::path::PathBuf::from(&media_files[0]))
            } else {
                file_row_path(file_row)
            }
        }
        6 => file_row_path(file_row),
        _ => None,
    }
}
