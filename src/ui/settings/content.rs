use gtk4::prelude::*;
use gtk4::{Align, Button, ColorDialog, ColorDialogButton, ContentFit, DrawingArea, Frame, IconLookupFlags, IconTheme, Label, ListBox, Picture, Stack, Switch, TextDirection, MediaFile, Orientation, SelectionMode};
use libadwaita as adw;
use libadwaita::prelude::*;
use gdk4::RGBA;
use crate::config::{Config, ScreensaverMode, AnimatedPattern, ClockPosition};
use crate::i18n::{Language, tr};
use crate::ui::settings::appearance::{format_clock_text, apply_clock_size, apply_clock_position};
use std::time::Duration;
use std::rc::Rc;
use std::cell::RefCell;
use gtk4::cairo;
use gtk4::prelude::Cast;
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
    pub web_url_row: adw::EntryRow,
    pub stream_url_row: adw::EntryRow,
    pub file_row: adw::ActionRow,
    pub file_button: Button,
    pub file_info_row: adw::ActionRow,
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
        unsafe { child.set_data("mode-index", idx); }
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
    let color_row = adw::ActionRow::builder().title(tr(lang, "Выбор цвета")).build();
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
    let pattern_model = gtk4::StringList::new(&[tr(lang, "Матрица"), tr(lang, "Звёзды"), tr(lang, "Геометрия")]);
    let pattern_row = adw::ComboRow::builder().title(tr(lang, "Паттерн")).model(&pattern_model).build();
    if let ScreensaverMode::Pattern(p) = config.active_profile().mode { pattern_row.set_selected(pattern_index(p)); }
    let pattern_group = adw::PreferencesGroup::new();
    pattern_group.add(&pattern_row);
    stack.add_named(&pattern_group, Some("pattern_page"));

    // --- Web Page ---
    let web_url_row = adw::EntryRow::builder().title("URL").build();
    if let ScreensaverMode::Web(url) = &config.active_profile().mode { web_url_row.set_text(url); }
    let web_group = adw::PreferencesGroup::new();
    web_group.add(&web_url_row);
    stack.add_named(&web_group, Some("web_page"));

    // --- File Page ---
    let file_group = adw::PreferencesGroup::new();
    let stream_url_row = adw::EntryRow::builder().title(tr(lang, "URL потока")).build();
    if let ScreensaverMode::Stream(url) = &config.active_profile().mode { stream_url_row.set_text(url); }
    stream_url_row.set_visible(matches!(config.active_profile().mode, ScreensaverMode::Stream(_)));
    let file_row = adw::ActionRow::builder().title(tr(lang, "Путь к файлу")).build();
    let file_button = Button::with_label(tr(lang, "Выбрать..."));
    file_row.add_suffix(&file_button);
    let file_info_row = adw::ActionRow::builder().title(tr(lang, "Информация")).subtitle(tr(lang, "Нет данных")).build();
    let slideshow_interval_spin = gtk4::SpinButton::with_range(1.0, 3600.0, 1.0);
    slideshow_interval_spin.set_value(config.active_profile().slideshow_interval_seconds as f64);
    let slideshow_interval_row = adw::ActionRow::builder().title(tr(lang, "Интервал слайдшоу")).build();
    slideshow_interval_row.add_suffix(&slideshow_interval_spin);
    let mute_switch = Switch::builder().valign(Align::Center).active(config.active_profile().mute_video).build();
    let mute_row = adw::ActionRow::builder().title(tr(lang, "Без звука")).build();
    mute_row.add_suffix(&mute_switch);
    let video_volume_spin = gtk4::SpinButton::with_range(0.0, 100.0, 1.0);
    video_volume_spin.set_value(config.active_profile().video_volume as f64);
    let volume_row = adw::ActionRow::builder().title(tr(lang, "Громкость")).build();
    volume_row.add_suffix(&video_volume_spin);
    let random_media_switch = Switch::builder().valign(Align::Center).active(config.active_profile().random_media).build();
    let random_row = adw::ActionRow::builder().title(tr(lang, "Случайный выбор")).build();
    random_row.add_suffix(&random_media_switch);
    let media_list_row = adw::ActionRow::builder().title(tr(lang, "Список медиа")).build();
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
    let list_visible = matches!(config.active_profile().mode, ScreensaverMode::Image(_) | ScreensaverMode::Video(_));
    media_list_row.set_visible(list_visible);
    media_list_box.set_visible(list_visible);
    file_group.add(&stream_url_row);
    file_group.add(&file_row);
    file_group.add(&file_info_row);
    file_group.add(&slideshow_interval_row);
    file_group.add(&mute_row);
    file_group.add(&volume_row);
    file_group.add(&random_row);
    file_group.add(&media_list_row);
    file_group.add(&media_list_box);
    stack.add_named(&file_group, Some("file_page"));

    content_group.add(&stack);

    let preview_group = adw::PreferencesGroup::builder().title(tr(lang, "Предпросмотр")).build();
    let preview_frame = Frame::new(None);
    preview_frame.set_size_request(360, 200);
    preview_group.add(&preview_frame);

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
        web_url_row,
        stream_url_row,
        file_row,
        file_button,
        file_info_row,
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
        media_files,
        selected_media_preview,
    }
}

pub fn mode_icon_name(mode_index: u32) -> &'static str {
    match mode_index {
        0 => "color-select-symbolic", 1 => "applications-graphics-symbolic",
        2 => "view-grid-symbolic", 3 => "internet-web-browser-symbolic",
        4 => "image-x-generic-symbolic", 5 => "video-x-generic-symbolic",
        6 => "folder-pictures-symbolic", 7 => "media-playback-start-symbolic",
        _ => "media-optical-symbolic",
    }
}

pub fn mode_label(mode_index: u32, lang: Language) -> &'static str {
    match mode_index {
        0 => tr(lang, "Цвет"), 1 => tr(lang, "Градиент"), 2 => tr(lang, "Паттерны"),
        3 => tr(lang, "Веб-страница"), 4 => tr(lang, "Изображение"), 5 => tr(lang, "Видео"),
        6 => tr(lang, "Слайдшоу"), 7 => tr(lang, "Видео по URL"), _ => tr(lang, "Неизвестно"),
    }
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

pub fn pattern_index(pattern: AnimatedPattern) -> u32 {
    match pattern { AnimatedPattern::Matrix => 0, AnimatedPattern::Stars => 1, AnimatedPattern::Geometry => 2 }
}

pub fn pattern_from_index(index: u32) -> AnimatedPattern {
    match index { 1 => AnimatedPattern::Stars, 2 => AnimatedPattern::Geometry, _ => AnimatedPattern::Matrix }
}

pub fn pattern_label(index: u32, lang: Language) -> &'static str {
    match index { 1 => tr(lang, "Звёзды"), 2 => tr(lang, "Геометрия"), _ => tr(lang, "Матрица") }
}

pub fn hex_to_rgba(hex: &str) -> Option<RGBA> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(RGBA::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0))
    } else { None }
}

pub fn rgba_to_hex(rgba: RGBA) -> String {
    format!("#{:02X}{:02X}{:02X}", (rgba.red() * 255.0) as u8, (rgba.green() * 255.0) as u8, (rgba.blue() * 255.0) as u8)
}

pub fn collect_image_paths(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut images = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if is_allowed_image_ext(ext) { images.push(path); }
                }
            }
        }
    }
    images.sort();
    images
}

pub fn is_allowed_image_ext(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tiff" | "tif" | "tga" | "ico" | "ppm" | "pgm" | "pbm" | "hdr" | "exr")
}

pub fn format_file_size(bytes: u64, lang: Language) -> String {
    const UNITS: [&str; 5] = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
    let mut size = bytes as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit < UNITS.len() - 1 { size /= 1024.0; unit += 1; }
    let unit_label = tr(lang, UNITS[unit]);
    if unit == 0 { format!("{bytes} {unit_label}") } else { format!("{:.1} {unit_label}", size) }
}

pub fn hash_u32(x: u32) -> u32 {
    let mut x = x;
    x = ((x >> 16) ^ x).wrapping_mul(0x45d9f3b);
    x = ((x >> 16) ^ x).wrapping_mul(0x45d9f3b);
    (x >> 16) ^ x
}

pub fn hash_f64(x: u32) -> f64 { (hash_u32(x) as f64) / (u32::MAX as f64) }

pub fn draw_pattern(cr: &cairo::Context, width: f64, height: f64, time: f64, pattern: AnimatedPattern) {
    if width <= 1.0 || height <= 1.0 { return; }
    match pattern {
        AnimatedPattern::Matrix => draw_matrix_pattern(cr, width, height, time),
        AnimatedPattern::Stars => draw_stars_pattern(cr, width, height, time),
        AnimatedPattern::Geometry => draw_geometry_pattern(cr, width, height, time),
    }
}

fn draw_matrix_pattern(cr: &cairo::Context, width: f64, height: f64, time: f64) {
    cr.set_source_rgb(0.0, 0.0, 0.0); let _ = cr.paint();
    let cell = 16.0; let columns = (width / cell).ceil() as u32;
    for col in 0..columns {
        let seed = col.wrapping_mul(1103515245).wrapping_add(12345);
        let speed = 40.0 + (hash_u32(seed) % 70) as f64;
        let offset = (hash_u32(seed ^ 0x9e3779b9) % 1000) as f64;
        let tail = 6 + (hash_u32(seed ^ 0x7f4a7c15) % 14) as i32;
        let head = (time * speed + offset) % (height + cell * tail as f64);
        for i in 0..tail {
            let y = head - i as f64 * cell;
            if y < -cell || y > height + cell { continue; }
            let alpha = 1.0 - i as f64 / tail as f64;
            let green = 0.2 + 0.8 * alpha;
            cr.set_source_rgba(0.0, green, 0.0, 1.0);
            cr.rectangle(col as f64 * cell, y, cell * 0.75, cell * 0.75); let _ = cr.fill();
        }
    }
}

fn draw_stars_pattern(cr: &cairo::Context, width: f64, height: f64, time: f64) {
    cr.set_source_rgb(0.01, 0.01, 0.04); let _ = cr.paint();
    let density = ((width * height) / 8500.0).clamp(90.0, 240.0) as u32;
    for i in 0..density {
        let seed = i.wrapping_mul(2654435761);
        let x = hash_f64(seed) * width; let y = hash_f64(seed.wrapping_add(1)) * height;
        let speed = 20.0 + hash_f64(seed.wrapping_add(2)) * 90.0;
        let radius = 0.6 + hash_f64(seed.wrapping_add(3)) * 2.0;
        let y_pos = (y + time * speed) % height;
        let twinkle = (time * 2.0 + hash_f64(seed.wrapping_add(4)) * std::f64::consts::PI * 2.0).sin() * 0.5 + 0.5;
        let brightness = 0.6 + 0.4 * twinkle;
        cr.set_source_rgba(brightness, brightness, 1.0, 1.0);
        cr.arc(x, y_pos, radius, 0.0, std::f64::consts::PI * 2.0); let _ = cr.fill();
    }
}

fn draw_geometry_pattern(cr: &cairo::Context, width: f64, height: f64, time: f64) {
    cr.set_source_rgb(0.04, 0.04, 0.07); let _ = cr.paint();
    let spacing = 90.0; let cols = (width / spacing).ceil() as u32; let rows = (height / spacing).ceil() as u32;
    cr.set_line_width(2.0);
    for col in 0..cols {
        for row in 0..rows {
            let seed = col.wrapping_mul(73856093) ^ row.wrapping_mul(19349663);
            let base = hash_f64(seed); let cx = col as f64 * spacing + spacing * 0.5; let cy = row as f64 * spacing + spacing * 0.5;
            let size = spacing * (0.28 + 0.18 * base); let angle = time * 0.5 + base * std::f64::consts::PI * 2.0;
            let pulse = (time * 1.1 + base * std::f64::consts::PI * 2.0).sin() * 0.5 + 0.5;
            let r = 0.2 + 0.6 * pulse; let g = 0.4 + 0.4 * (1.0 - pulse); let b = 0.7 + 0.2 * pulse;
            let _ = cr.save(); cr.translate(cx, cy); cr.rotate(angle); cr.set_source_rgba(r, g, b, 0.9);
            cr.rectangle(-size * 0.5, -size * 0.5, size, size); let _ = cr.stroke(); let _ = cr.restore();
        }
    }
}

pub fn build_preview_placeholder(text: &str) -> gtk4::Widget {
    let label = Label::new(Some(text)); label.add_css_class("dim-label");
    label.set_hexpand(true); label.set_vexpand(true); label.upcast()
}

pub fn build_preview_widget(
    mode: u32, color_button: &ColorDialogButton, g_start: &ColorDialogButton, g_end: &ColorDialogButton, pattern_row: &adw::ComboRow,
    web_url_row: &adw::EntryRow, stream_url_row: &adw::EntryRow, file_row: &adw::ActionRow, mute_switch: &Switch,
    video_volume: u8, preview_path: Option<&std::path::Path>, clock_enabled: bool, clock_format: &str,
    clock_position: ClockPosition, clock_size: u32, lang: Language,
) -> (gtk4::Widget, Option<MediaFile>, Option<webkit6::WebView>) {
    let (base_widget, media, web_view) = match mode {
        0 => {
            let rgba = color_button.rgba(); let drawing_area = DrawingArea::new();
            drawing_area.set_draw_func(move |_, cr, _, _| {
                cr.set_source_rgba(rgba.red() as f64, rgba.green() as f64, rgba.blue() as f64, rgba.alpha() as f64);
                let _ = cr.paint();
            });
            (drawing_area.upcast(), None, None)
        }
        1 => {
            let start = g_start.rgba(); let end = g_end.rgba(); let drawing_area = DrawingArea::new();
            drawing_area.set_draw_func(move |_, cr, width, height| {
                let gradient = cairo::LinearGradient::new(0.0, 0.0, width as f64, height as f64);
                gradient.add_color_stop_rgba(0.0, start.red() as f64, start.green() as f64, start.blue() as f64, start.alpha() as f64);
                gradient.add_color_stop_rgba(1.0, end.red() as f64, end.green() as f64, end.blue() as f64, end.alpha() as f64);
                let _ = cr.set_source(&gradient); let _ = cr.paint();
            });
            (drawing_area.upcast(), None, None)
        }
        2 => (build_pattern_preview(pattern_from_index(pattern_row.selected())), None, None),
        3 => {
            let url = web_url_row.text().trim().to_string();
            if url.is_empty() { return (build_preview_placeholder(tr(lang, "URL не указан")), None, None); }
            let web_view = webkit6::WebView::new(); web_view.load_uri(&url);
            web_view.set_can_focus(false); web_view.set_is_muted(mute_switch.is_active());
            (web_view.clone().upcast(), None, Some(web_view))
        }
        7 => {
            let url = stream_url_row.text().trim().to_string();
            if url.is_empty() { return (build_preview_placeholder(tr(lang, "URL не указан")), None, None); }
            let media = MediaFile::for_file(&gio::File::for_uri(&url));
            media.set_muted(mute_switch.is_active()); media.set_volume(video_volume as f64 / 100.0);
            media.set_playing(true); let picture = Picture::for_paintable(&media);
            picture.set_content_fit(ContentFit::Contain); (picture.upcast(), Some(media), None)
        }
        4 | 5 => {
            let path = preview_path.map(|p| p.to_path_buf()).or_else(|| file_row_path(file_row));
            let Some(path) = path else { return (build_preview_placeholder(tr(lang, "Файл не выбран")), None, None); };
            if let Err(msg) = validate_media_path(&path, if mode == 4 { MediaKind::Image } else { MediaKind::Video }, false, lang) {
                return (build_preview_placeholder(&msg), None, None);
            }
            let file = gio::File::for_path(path);
            if mode == 4 {
                let picture = Picture::for_file(&file); picture.set_content_fit(ContentFit::Contain); (picture.upcast(), None, None)
            } else {
                let media = MediaFile::for_file(&file); media.set_loop(true);
                media.set_muted(mute_switch.is_active()); media.set_volume(video_volume as f64 / 100.0);
                media.set_playing(true); let picture = Picture::for_paintable(&media);
                picture.set_content_fit(ContentFit::Contain); (picture.upcast(), Some(media), None)
            }
        }
        6 => {
            let path = file_row_path(file_row);
            let Some(path) = path else { return (build_preview_placeholder(tr(lang, "Папка не выбрана")), None, None); };
            if let Err(msg) = validate_media_path(&path, MediaKind::ImageFolder, false, lang) {
                return (build_preview_placeholder(&msg), None, None);
            }
            let images = collect_image_paths(&path);
            let picture = Picture::for_file(&gio::File::for_path(&images[0]));
            picture.set_content_fit(ContentFit::Contain); (picture.upcast(), None, None)
        }
        _ => (build_preview_placeholder(tr(lang, "Режим не поддерживается")), None, None),
    };

    if !clock_enabled { return (base_widget, media, web_view); }
    let overlay = gtk4::Overlay::new(); overlay.set_child(Some(&base_widget));
    let label = Label::new(None); label.set_text(&format_clock_text(clock_format));
    label.add_css_class("title-2"); label.set_opacity(0.9);
    apply_clock_size(&label, clock_size); apply_clock_position(&label, clock_position, 12);
    overlay.add_overlay(&label);
    let format = clock_format.to_string(); let label_weak = label.downgrade();
    glib::timeout_add_local(Duration::from_secs(1), move || {
        let Some(label) = label_weak.upgrade() else { return glib::ControlFlow::Break; };
        label.set_text(&format_clock_text(&format)); glib::ControlFlow::Continue
    });
    (overlay.upcast(), media, web_view)
}

pub fn build_pattern_preview(pattern: AnimatedPattern) -> gtk4::Widget {
    let drawing_area = DrawingArea::new();
    let start = std::time::Instant::now();
    drawing_area.set_draw_func(move |_, cr, width, height| {
        draw_pattern(cr, width as f64, height as f64, start.elapsed().as_secs_f64(), pattern);
    });
    drawing_area.add_tick_callback(|widget, _| { widget.queue_draw(); glib::ControlFlow::Continue });
    drawing_area.upcast()
}

pub fn file_row_path(file_row: &adw::ActionRow) -> Option<std::path::PathBuf> {
    let path = file_row.tooltip_text()?.trim().to_string();
    if path.is_empty() { None } else { Some(std::path::PathBuf::from(path)) }
}

pub fn validate_media_path(path: &std::path::Path, kind: MediaKind, check_content: bool, lang: Language) -> Result<(), String> {
    if path.as_os_str().is_empty() { return Err(tr(lang, "Файл не выбран").to_string()); }
    if !path.exists() { return Err(tr(lang, "Файл не найден: {path}").replace("{path}", &path.to_string_lossy())); }
    if matches!(kind, MediaKind::ImageFolder) {
        if !path.is_dir() { return Err(tr(lang, "Путь не является папкой").to_string()); }
        if collect_image_paths(path).is_empty() { return Err(tr(lang, "В папке нет изображений").to_string()); }
        return Ok(());
    }
    if !path.is_file() { return Err(tr(lang, "Путь не является файлом").to_string()); }
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let allowed = match kind {
        MediaKind::Image => &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff", "tif", "tga", "ico", "ppm", "pgm", "pbm", "hdr", "exr"][..],
        MediaKind::Video => &["mp4", "mkv", "webm", "avi", "mov", "m4v", "mpg", "mpeg", "ogv", "wmv", "flv", "m2ts", "mts"][..],
        MediaKind::ImageFolder => &[][..],
    };
    if !allowed.contains(&ext.as_str()) { return Err(tr(lang, "Неподдерживаемый формат").to_string()); }
    if check_content && image::open(path).is_err() { return Err(tr(lang, "Файл не является корректным изображением").to_string()); }
    Ok(())
}

pub fn preview_media_path(mode: u32, file_row: &adw::ActionRow, random_media: bool, media_files: &[String], selected_path: Option<&str>) -> Option<std::path::PathBuf> {
    match mode {
        4 | 5 => {
            if let Some(selected) = selected_path { if !selected.trim().is_empty() { return Some(std::path::PathBuf::from(selected)); } }
            if random_media && !media_files.is_empty() { Some(std::path::PathBuf::from(&media_files[0])) }
            else { file_row_path(file_row) }
        }
        6 => file_row_path(file_row),
        _ => None,
    }
}
