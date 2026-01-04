use gtk4::prelude::*;
use gtk4::{ApplicationWindow, DrawingArea, Picture, ContentFit, EventControllerKey, EventControllerMotion, MediaFile, Label, Overlay, Align};
use gtk4::pango;
use gtk4::cairo;
use gdk4::{Display, Monitor, RGBA};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use crate::config::{AnimatedPattern, ClockPosition, SettingsProfile, ScreensaverMode, CLOCK_MOVE_POSITIONS, DEFAULT_CLOCK_FORMAT};
use glib::DateTime;
use webkit6::prelude::*;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as XprotoConnectionExt;

const FADE_DURATION_MS: u64 = 250;
const FADE_TICK_MS: u64 = 16;

#[derive(Clone, Copy)]
enum MediaKind {
    Image,
    Video,
}

struct WindowData {
    window: ApplicationWindow,
    media: Option<MediaFile>,
    slideshow_picture: Option<Picture>,
    clock_label: Option<Label>,
}

pub struct ScreensaverWindow {
    windows: Vec<WindowData>,
    config: SettingsProfile,
    slideshow_source: RefCell<Option<glib::SourceId>>,
    clock_source: RefCell<Option<glib::SourceId>>,
    clock_move_source: RefCell<Option<glib::SourceId>>,
    activity_suppressed_until: Rc<Cell<Option<Instant>>>,
}

impl ScreensaverWindow {
    pub fn new<F>(
        config: &SettingsProfile,
        app: Option<gtk4::Application>,
        started_at: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
        on_activity: F,
    ) -> Self
    where F: Fn() + 'static + Clone {
        let mut windows = Vec::new();
        let triggered = Rc::new(Cell::new(false));
        let config_clone = config.clone();
        let activity_suppressed_until = Rc::new(Cell::new(None));
        
        let on_activity_wrapped = {
            let triggered = triggered.clone();
            move || {
                if !triggered.get() {
                    triggered.set(true);
                    on_activity();
                }
            }
        };
        
        let display = match Display::default() {
            Some(d) => d,
            None => {
                return Self {
                    windows,
                    config: config_clone,
                    slideshow_source: RefCell::new(None),
                    clock_source: RefCell::new(None),
                    clock_move_source: RefCell::new(None),
                    activity_suppressed_until,
                }
            }
        };
        
        let monitors = Self::get_monitors(&display);
        let is_x11 = display.backend().is_x11();
        
        if monitors.is_empty() {
            let (window, media, slideshow_picture, clock_label) =
                Self::create_window_with_content(app.as_ref(), config, is_x11, None);
            if !is_x11 {
                window.fullscreen();
            }
            Self::setup_activity_tracking(
                &window,
                on_activity_wrapped.clone(),
                started_at.clone(),
                activity_suppressed_until.clone(),
                config_clone.mouse_wake_delay_ms,
            );
            windows.push(WindowData { window, media, slideshow_picture, clock_label });
        } else {
            for monitor in &monitors {
                let geometry = monitor.geometry();
                let (window, media, slideshow_picture, clock_label) =
                    Self::create_window_with_content(app.as_ref(), config, is_x11, Some(geometry));
                window.set_default_size(geometry.width(), geometry.height());
                if !is_x11 {
                    window.fullscreen_on_monitor(monitor);
                }
                Self::setup_activity_tracking(
                    &window,
                    on_activity_wrapped.clone(),
                    started_at.clone(),
                    activity_suppressed_until.clone(),
                    config_clone.mouse_wake_delay_ms,
                );
                windows.push(WindowData { window, media, slideshow_picture, clock_label });
            }
        }

        let slideshow_source = RefCell::new(None);
        let clock_source = RefCell::new(None);
        let clock_move_source = RefCell::new(None);
        let instance = Self {
            windows,
            config: config_clone,
            slideshow_source,
            clock_source,
            clock_move_source,
            activity_suppressed_until,
        };
        instance.setup_slideshow_if_needed();
        instance.setup_clock_if_needed();
        instance.setup_clock_move_if_needed();
        instance
    }

    fn create_window_with_content(
        app: Option<&gtk4::Application>,
        config: &SettingsProfile,
        is_x11: bool,
        geometry: Option<gdk4::Rectangle>,
    ) -> (ApplicationWindow, Option<MediaFile>, Option<Picture>, Option<Label>) {
        let mut builder = ApplicationWindow::builder()
            .title("Screensaver")
            .decorated(false)
            .modal(false)
            .deletable(false)
            .resizable(false);

        if let Some(app) = app {
            builder = builder.application(app);
        }

        let window = builder.build();

        if let Some(cursor) = gdk4::Cursor::from_name("none", None) {
            window.set_cursor(Some(&cursor));
        }

        let (media, slideshow_picture) = Self::setup_window_content(&window, config);
        let clock_label = if config.show_clock {
            Self::attach_clock_overlay(
                &window,
                &config.clock_format,
                config.clock_position,
                config.clock_size,
            )
        } else {
            None
        };

        if is_x11 {
            let geom = geometry;
            window.connect_realize(move |window| {
                if let Some(surface) = window.surface() {
                    if let Ok(x11_surface) = surface.downcast::<gdk4_x11::X11Surface>() {
                        let xid = x11_surface.xid() as u32;
                        Self::configure_x11_override_redirect(xid, geom);
                    }
                }
            });
        }

        (window, media, slideshow_picture, clock_label)
    }

    fn setup_activity_tracking<F>(
        window: &ApplicationWindow,
        on_activity: F,
        started_at: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
        activity_suppressed_until: Rc<Cell<Option<Instant>>>,
        mouse_wake_delay_ms: u64,
    )
    where F: Fn() + 'static + Clone {
        let key_grace = Duration::from_millis(1500);
        let mouse_grace = Duration::from_millis(mouse_wake_delay_ms);
        let is_x11 = Display::default().map(|d| d.backend().is_x11()).unwrap_or(false);
        
        let key_controller = EventControllerKey::new();
        let on_activity_key = on_activity.clone();
        let started_at_key = started_at.clone();
        let suppress_until_key = activity_suppressed_until.clone();
        key_controller.connect_key_pressed(move |_, _, _, _| {
            if let Some(until) = suppress_until_key.get() {
                if Instant::now() < until {
                    return glib::Propagation::Stop;
                }
            }
            if started_at_key.lock().unwrap()
                .map(|t| t.elapsed() >= key_grace)
                .unwrap_or(false)
            {
                on_activity_key();
            }
            glib::Propagation::Stop
        });
        window.add_controller(key_controller);

        let motion_controller = EventControllerMotion::new();
        let on_activity_motion = on_activity;
        let started_at_motion = started_at;
        let suppress_until_motion = activity_suppressed_until;
        let last_pos: Rc<Cell<Option<(f64, f64)>>> = Rc::new(Cell::new(None));
        let total_distance: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));
        
        motion_controller.connect_motion(move |_, x, y| {
            if let Some(until) = suppress_until_motion.get() {
                if Instant::now() < until {
                    return;
                }
            }
            if started_at_motion.lock().unwrap()
                .map(|t| t.elapsed() >= mouse_grace)
                .unwrap_or(false)
            {
                if is_x11 {
                    // X11: require significant mouse movement
                    if let Some((lx, ly)) = last_pos.get() {
                        let dist = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
                        total_distance.set(total_distance.get() + dist);
                        if total_distance.get() > 50.0 {
                            on_activity_motion();
                        }
                    }
                    last_pos.set(Some((x, y)));
                } else {
                    on_activity_motion();
                }
            }
        });
        window.add_controller(motion_controller);
    }

    pub fn show(&self) {
        Self::set_panels_autohide(&self.config, true);
        let windows: Vec<_> = self.windows.iter().map(|wd| wd.window.clone()).collect();
        for window in &windows {
            if !window.is_realized() {
                gtk4::prelude::WidgetExt::realize(window);
            }
        }
        if self.config.fade_enabled {
            for window in &windows {
                window.set_opacity(0.0);
                window.present();
            }
            Self::animate_opacity(windows, 0.0, 1.0, FADE_DURATION_MS, None);
        } else {
            for window in &windows {
                window.set_opacity(1.0);
                window.present();
            }
        }
    }

    pub fn hide(&self) {
        Self::set_panels_autohide(&self.config, false);
        Self::release_x11_grabs();
        if let Some(source) = self.slideshow_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(source) = self.clock_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(source) = self.clock_move_source.borrow_mut().take() {
            source.remove();
        }
        
        for wd in &self.windows {
            if let Some(ref media) = wd.media {
                media.set_playing(false);
            }
        }
        
        let windows: Vec<_> = self.windows.iter().map(|wd| wd.window.clone()).collect();
        if self.config.fade_enabled {
            let windows_for_close = windows.clone();
            Self::animate_opacity(
                windows,
                1.0,
                0.0,
                FADE_DURATION_MS,
                Some(Box::new(move || {
                    for window in windows_for_close {
                        window.close();
                    }
                })),
            );
        } else {
            glib::timeout_add_local_once(Duration::from_millis(50), move || {
                for window in windows {
                    window.close();
                }
            });
        }
    }

    fn animate_opacity(
        windows: Vec<ApplicationWindow>,
        from: f64,
        to: f64,
        duration_ms: u64,
        on_complete: Option<Box<dyn FnOnce() + 'static>>,
    ) {
        if windows.is_empty() {
            if let Some(cb) = on_complete {
                cb();
            }
            return;
        }
        if duration_ms == 0 {
            for window in &windows {
                window.set_opacity(to);
            }
            if let Some(cb) = on_complete {
                cb();
            }
            return;
        }

        let start = Instant::now();
        let delta = to - from;
        let mut on_complete = on_complete;

        glib::timeout_add_local(Duration::from_millis(FADE_TICK_MS), move || {
            let elapsed = start.elapsed().as_millis() as f64;
            let t = (elapsed / duration_ms as f64).min(1.0);
            let value = from + delta * t;
            for window in &windows {
                window.set_opacity(value);
            }
            if t >= 1.0 {
                if let Some(cb) = on_complete.take() {
                    cb();
                }
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    fn release_x11_grabs() {
        if let Ok((conn, _)) = x11rb::connect(None) {
            let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
            let _ = conn.ungrab_pointer(x11rb::CURRENT_TIME);
            let _ = conn.flush();
        }
    }

    fn set_panels_autohide(config: &SettingsProfile, hide: bool) {
        for cmd in &config.panel_commands {
            if !cmd.enabled { continue; }
            let command = if hide { &cmd.hide_command } else { &cmd.show_command };
            if command.is_empty() { continue; }
            let _ = std::process::Command::new("sh")
                .args(["-c", command])
                .output();
        }
    }

    fn configure_x11_override_redirect(xid: u32, geom: Option<gdk4::Rectangle>) {
        let Ok((conn, _)) = x11rb::connect(None) else { return };
        
        // Unmap first, then set override-redirect, then remap
        let _ = conn.unmap_window(xid);
        
        let _ = conn.change_window_attributes(
            xid,
            &x11rb::protocol::xproto::ChangeWindowAttributesAux::new()
                .override_redirect(1),
        );
        
        if let Some(g) = geom {
            let _ = conn.configure_window(
                xid,
                &x11rb::protocol::xproto::ConfigureWindowAux::new()
                    .x(g.x())
                    .y(g.y())
                    .width(g.width() as u32)
                    .height(g.height() as u32)
                    .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
            );
        }
        
        let _ = conn.map_window(xid);
        
        // Raise to top
        let _ = conn.configure_window(
            xid,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
        );
        
        // Grab keyboard and pointer
        let _ = conn.grab_keyboard(
            true, xid, x11rb::CURRENT_TIME,
            x11rb::protocol::xproto::GrabMode::ASYNC,
            x11rb::protocol::xproto::GrabMode::ASYNC,
        );
        let _ = conn.grab_pointer(
            true, xid,
            x11rb::protocol::xproto::EventMask::POINTER_MOTION 
                | x11rb::protocol::xproto::EventMask::BUTTON_PRESS,
            x11rb::protocol::xproto::GrabMode::ASYNC,
            x11rb::protocol::xproto::GrabMode::ASYNC,
            xid, 0u32, x11rb::CURRENT_TIME,
        );
        
        let _ = conn.flush();
    }

    fn setup_window_content(
        window: &ApplicationWindow,
        config: &SettingsProfile,
    ) -> (Option<MediaFile>, Option<Picture>) {
        match &config.mode {
            ScreensaverMode::Color(hex) => {
                Self::setup_color_content(window, hex);
                (None, None)
            }
            ScreensaverMode::Gradient { start, end } => {
                Self::setup_gradient_content(window, start, end);
                (None, None)
            }
            ScreensaverMode::Pattern(pattern) => {
                Self::setup_pattern_content(window, *pattern);
                (None, None)
            }
            ScreensaverMode::Web(url) => {
                Self::setup_web_content(window, url, config.mute_video);
                (None, None)
            }
            ScreensaverMode::Stream(url) => {
                let url = url.trim();
                if url.is_empty() || !Self::is_valid_stream_url(url) {
                    Self::setup_color_content(window, "#000000");
                    (None, None)
                } else {
                    (
                        Some(Self::setup_stream_content(
                            window,
                            url,
                            config.mute_video,
                            config.video_volume,
                        )),
                        None,
                    )
                }
            }
            ScreensaverMode::Image(path) => {
                let path = Self::resolve_random_media_path(config, MediaKind::Image, path);
                if let Some(path) = path {
                    Self::setup_image_content(window, &path);
                } else {
                    Self::setup_color_content(window, "#000000");
                }
                (None, None)
            }
            ScreensaverMode::Video(path) => {
                let path = Self::resolve_random_media_path(config, MediaKind::Video, path);
                if let Some(path) = path {
                    (
                        Some(Self::setup_video_content(
                            window,
                            &path,
                            config.mute_video,
                            config.video_volume,
                        )),
                        None,
                    )
                } else {
                    Self::setup_color_content(window, "#000000");
                    (None, None)
                }
            }
            ScreensaverMode::Slideshow(path) => {
                let picture = Self::setup_slideshow_content(window, path);
                (None, picture)
            }
        }
    }

    fn get_monitors(display: &Display) -> Vec<Monitor> {
        let list = display.monitors();
        (0..list.n_items())
            .filter_map(|i| list.item(i)?.downcast::<Monitor>().ok())
            .collect()
    }

    fn setup_color_content(window: &ApplicationWindow, hex: &str) {
        let drawing_area = DrawingArea::new();
        let color = Self::hex_to_rgba(hex).unwrap_or_else(|| RGBA::new(0.0, 0.0, 0.0, 1.0));
        drawing_area.set_draw_func(move |_, cr, _, _| {
            cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, 1.0);
            let _ = cr.paint();
        });
        window.set_child(Some(&drawing_area));
    }

    fn setup_gradient_content(window: &ApplicationWindow, start_hex: &str, end_hex: &str) {
        let drawing_area = DrawingArea::new();
        let start = Self::hex_to_rgba(start_hex).unwrap_or_else(|| RGBA::new(0.0, 0.0, 0.0, 1.0));
        let end = Self::hex_to_rgba(end_hex).unwrap_or_else(|| RGBA::new(0.0, 0.0, 0.0, 1.0));
        drawing_area.set_draw_func(move |_, cr, width, height| {
            let gradient = cairo::LinearGradient::new(0.0, 0.0, width as f64, height as f64);
            gradient.add_color_stop_rgba(
                0.0,
                start.red() as f64,
                start.green() as f64,
                start.blue() as f64,
                1.0,
            );
            gradient.add_color_stop_rgba(
                1.0,
                end.red() as f64,
                end.green() as f64,
                end.blue() as f64,
                1.0,
            );
            let _ = cr.set_source(&gradient);
            let _ = cr.paint();
        });
        window.set_child(Some(&drawing_area));
    }

    fn setup_pattern_content(window: &ApplicationWindow, pattern: AnimatedPattern) {
        let drawing_area = DrawingArea::new();
        drawing_area.set_hexpand(true);
        drawing_area.set_vexpand(true);
        let start = Instant::now();
        drawing_area.set_draw_func(move |_, cr, width, height| {
            let t = start.elapsed().as_secs_f64();
            Self::draw_pattern(cr, width as f64, height as f64, t, pattern);
        });
        drawing_area.add_tick_callback(|widget, _| {
            widget.queue_draw();
            glib::ControlFlow::Continue
        });
        window.set_child(Some(&drawing_area));
    }

    fn setup_web_content(window: &ApplicationWindow, url: &str, mute: bool) {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            Self::setup_color_content(window, "#000000");
            return;
        }
        let web_view = webkit6::WebView::new();
        web_view.load_uri(trimmed);
        web_view.set_can_focus(false);
        web_view.set_is_muted(mute);
        web_view.set_hexpand(true);
        web_view.set_vexpand(true);
        let window_for_crash = window.clone();
        web_view.connect_web_process_terminated(move |_, _| {
            Self::setup_color_content(&window_for_crash, "#000000");
        });
        window.set_child(Some(&web_view));
    }

    fn draw_pattern(
        cr: &cairo::Context,
        width: f64,
        height: f64,
        time: f64,
        pattern: AnimatedPattern,
    ) {
        if width <= 1.0 || height <= 1.0 {
            return;
        }
        match pattern {
            AnimatedPattern::Matrix => Self::draw_matrix_pattern(cr, width, height, time),
            AnimatedPattern::Stars => Self::draw_stars_pattern(cr, width, height, time),
            AnimatedPattern::Geometry => Self::draw_geometry_pattern(cr, width, height, time),
        }
    }

    fn draw_matrix_pattern(cr: &cairo::Context, width: f64, height: f64, time: f64) {
        cr.set_source_rgb(0.0, 0.0, 0.0);
        let _ = cr.paint();

        let cell = 16.0;
        let columns = (width / cell).ceil() as u32;
        for col in 0..columns {
            let seed = col.wrapping_mul(1103515245).wrapping_add(12345);
            let speed = 40.0 + (Self::hash_u32(seed) % 70) as f64;
            let offset = (Self::hash_u32(seed ^ 0x9e3779b9) % 1000) as f64;
            let tail = 6 + (Self::hash_u32(seed ^ 0x7f4a7c15) % 14) as i32;
            let head = (time * speed + offset) % (height + cell * tail as f64);

            for i in 0..tail {
                let y = head - i as f64 * cell;
                if y < -cell || y > height + cell {
                    continue;
                }
                let alpha = 1.0 - i as f64 / tail as f64;
                let green = 0.2 + 0.8 * alpha;
                cr.set_source_rgba(0.0, green, 0.0, 1.0);
                cr.rectangle(col as f64 * cell, y, cell * 0.75, cell * 0.75);
                let _ = cr.fill();
            }
        }
    }

    fn draw_stars_pattern(cr: &cairo::Context, width: f64, height: f64, time: f64) {
        cr.set_source_rgb(0.01, 0.01, 0.04);
        let _ = cr.paint();

        let density = ((width * height) / 8500.0).clamp(90.0, 240.0) as u32;
        for i in 0..density {
            let seed = i.wrapping_mul(2654435761);
            let x = Self::hash_f64(seed) * width;
            let y = Self::hash_f64(seed.wrapping_add(1)) * height;
            let speed = 20.0 + Self::hash_f64(seed.wrapping_add(2)) * 90.0;
            let radius = 0.6 + Self::hash_f64(seed.wrapping_add(3)) * 2.0;
            let y_pos = (y + time * speed) % height;
            let twinkle = (time * 2.0
                + Self::hash_f64(seed.wrapping_add(4)) * std::f64::consts::PI * 2.0)
                .sin()
                * 0.5
                + 0.5;
            let brightness = 0.6 + 0.4 * twinkle;
            cr.set_source_rgba(brightness, brightness, 1.0, 1.0);
            cr.arc(x, y_pos, radius, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
        }
    }

    fn draw_geometry_pattern(cr: &cairo::Context, width: f64, height: f64, time: f64) {
        cr.set_source_rgb(0.04, 0.04, 0.07);
        let _ = cr.paint();

        let spacing = 90.0;
        let cols = (width / spacing).ceil() as u32;
        let rows = (height / spacing).ceil() as u32;
        cr.set_line_width(2.0);

        for col in 0..cols {
            for row in 0..rows {
                let seed = col.wrapping_mul(73856093) ^ row.wrapping_mul(19349663);
                let base = Self::hash_f64(seed);
                let cx = col as f64 * spacing + spacing * 0.5;
                let cy = row as f64 * spacing + spacing * 0.5;
                let size = spacing * (0.28 + 0.18 * base);
                let angle = time * 0.5 + base * std::f64::consts::PI * 2.0;
                let pulse = (time * 1.1 + base * std::f64::consts::PI * 2.0).sin() * 0.5 + 0.5;
                let r = 0.2 + 0.6 * pulse;
                let g = 0.4 + 0.4 * (1.0 - pulse);
                let b = 0.7 + 0.2 * pulse;

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

    fn hash_u32(value: u32) -> u32 {
        let mut x = value;
        x ^= x >> 16;
        x = x.wrapping_mul(0x7feb352d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846ca68b);
        x ^= x >> 16;
        x
    }

    fn hash_f64(value: u32) -> f64 {
        Self::hash_u32(value) as f64 / u32::MAX as f64
    }

    fn setup_image_content(window: &ApplicationWindow, path: &str) {
        let path = std::path::Path::new(path);
        if path.exists() {
            let file = gio::File::for_path(path);
            let picture = Picture::for_file(&file);
            picture.set_can_shrink(true);
            picture.set_content_fit(ContentFit::Contain);
            window.set_child(Some(&picture));
        } else {
            Self::setup_color_content(window, "#000000");
        }
    }

    fn setup_slideshow_content(window: &ApplicationWindow, _path: &str) -> Option<Picture> {
        let picture = Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(ContentFit::Contain);
        window.set_child(Some(&picture));
        Some(picture)
    }

    fn setup_video_content(
        window: &ApplicationWindow,
        path: &str,
        mute: bool,
        volume_percent: u8,
    ) -> MediaFile {
        let file = gio::File::for_path(path);
        let media = MediaFile::for_file(&file);
        media.set_loop(true);
        media.set_muted(mute);
        let volume = (volume_percent as f64 / 100.0).clamp(0.0, 1.0);
        media.set_volume(volume);
        media.set_playing(true);
        
        let picture = Picture::for_paintable(&media);
        picture.set_can_shrink(false);
        picture.set_content_fit(ContentFit::Contain);
        window.set_child(Some(&picture));
        
        media
    }

    fn setup_stream_content(
        window: &ApplicationWindow,
        url: &str,
        mute: bool,
        volume_percent: u8,
    ) -> MediaFile {
        let file = gio::File::for_uri(url);
        let media = MediaFile::for_file(&file);
        media.set_loop(false);
        media.set_muted(mute);
        let volume = (volume_percent as f64 / 100.0).clamp(0.0, 1.0);
        media.set_volume(volume);
        media.set_playing(true);

        let picture = Picture::for_paintable(&media);
        picture.set_can_shrink(false);
        picture.set_content_fit(ContentFit::Contain);
        window.set_child(Some(&picture));

        media
    }

    fn setup_slideshow_if_needed(&self) {
        let ScreensaverMode::Slideshow(path) = &self.config.mode else {
            return;
        };

        let images = Self::collect_slideshow_images(path);
        if images.is_empty() {
            for wd in &self.windows {
                Self::setup_color_content(&wd.window, "#000000");
            }
            return;
        }

        let pictures: Vec<Picture> = self
            .windows
            .iter()
            .filter_map(|wd| wd.slideshow_picture.clone())
            .collect();
        if pictures.is_empty() {
            return;
        }

        self.activity_suppressed_until.set(Some(Instant::now() + Duration::from_millis(250)));
        Self::set_slideshow_image(&pictures, &images[0]);
        if images.len() > 1 {
            let interval_ms = self.config.slideshow_interval_seconds.max(1) * 1000;
            let images = Rc::new(images);
            let pictures = Rc::new(pictures);
            let index = Rc::new(Cell::new(0usize));
            let suppress_until = self.activity_suppressed_until.clone();

            let source_id = glib::timeout_add_local(Duration::from_millis(interval_ms), move || {
                let next = (index.get() + 1) % images.len();
                index.set(next);
                suppress_until.set(Some(Instant::now() + Duration::from_millis(250)));
                Self::set_slideshow_image(&pictures, &images[next]);
                glib::ControlFlow::Continue
            });
            *self.slideshow_source.borrow_mut() = Some(source_id);
        }
    }

    fn setup_clock_if_needed(&self) {
        if !self.config.show_clock {
            return;
        }
        let labels: Vec<Label> = self
            .windows
            .iter()
            .filter_map(|wd| wd.clock_label.clone())
            .collect();
        if labels.is_empty() {
            return;
        }
        let format = self.config.clock_format.clone();
        let labels = Rc::new(labels);
        let suppress_until = self.activity_suppressed_until.clone();
        let source_id = glib::timeout_add_local(Duration::from_secs(1), move || {
            suppress_until.set(Some(Instant::now() + Duration::from_millis(250)));
            let text = Self::format_clock_text(&format);
            for label in labels.iter() {
                label.set_text(&text);
            }
            glib::ControlFlow::Continue
        });
        *self.clock_source.borrow_mut() = Some(source_id);
    }

    fn setup_clock_move_if_needed(&self) {
        if !self.config.show_clock || !self.config.clock_move_enabled {
            return;
        }
        let labels: Vec<Label> = self
            .windows
            .iter()
            .filter_map(|wd| wd.clock_label.clone())
            .collect();
        if labels.is_empty() {
            return;
        }
        let start_index = CLOCK_MOVE_POSITIONS
            .iter()
            .position(|pos| *pos == self.config.clock_position)
            .unwrap_or(0);
        let index = Rc::new(Cell::new(start_index));
        let labels = Rc::new(labels);
        let suppress_until = self.activity_suppressed_until.clone();
        let interval = self.config.clock_move_interval_seconds.max(1);
        let source_id = glib::timeout_add_local(Duration::from_secs(interval), move || {
            let next = (index.get() + 1) % CLOCK_MOVE_POSITIONS.len();
            index.set(next);
            suppress_until.set(Some(Instant::now() + Duration::from_millis(250)));
            for label in labels.iter() {
                Self::apply_clock_position(label, CLOCK_MOVE_POSITIONS[next], 24);
            }
            glib::ControlFlow::Continue
        });
        *self.clock_move_source.borrow_mut() = Some(source_id);
    }

    fn attach_clock_overlay(
        window: &ApplicationWindow,
        format: &str,
        position: ClockPosition,
        size: u32,
    ) -> Option<Label> {
        let child = window.child()?;
        window.set_child(None::<&gtk4::Widget>);
        let overlay = Overlay::new();
        overlay.set_child(Some(&child));

        let label = Label::new(None);
        label.add_css_class("title-1");
        label.set_opacity(0.9);
        label.set_text(&Self::format_clock_text(format));
        Self::apply_clock_size(&label, size);
        Self::apply_clock_position(&label, position, 24);
        overlay.add_overlay(&label);

        window.set_child(Some(&overlay));
        Some(label)
    }

    fn format_clock_text(format: &str) -> String {
        let trimmed = format.trim();
        let format = if trimmed.is_empty() {
            DEFAULT_CLOCK_FORMAT
        } else {
            trimmed
        };
        let Ok(now) = DateTime::now_local() else {
            return String::new();
        };
        match now.format(format) {
            Ok(text) => text.to_string(),
            Err(_) => {
                if format != DEFAULT_CLOCK_FORMAT {
                    if let Ok(fallback) = now.format(DEFAULT_CLOCK_FORMAT) {
                        return fallback.to_string();
                    }
                }
                String::new()
            }
        }
    }

    fn apply_clock_position(label: &Label, position: ClockPosition, margin: i32) {
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

    fn apply_clock_size(label: &Label, size: u32) {
        let size = size.max(6) as i32;
        let scaled = size.saturating_mul(pango::SCALE);
        let attrs = pango::AttrList::new();
        let attr = pango::AttrSize::new(scaled);
        attrs.insert(attr);
        label.set_attributes(Some(&attrs));
    }

    fn is_valid_stream_url(url: &str) -> bool {
        let lower = url.trim().to_ascii_lowercase();
        lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("rtsp://")
            || lower.starts_with("rtmp://")
    }

    fn resolve_random_media_path(
        config: &SettingsProfile,
        kind: MediaKind,
        fallback: &str,
    ) -> Option<String> {
        if config.random_media {
            let candidates: Vec<String> = config
                .media_list
                .iter()
                .filter(|path| Self::is_allowed_media_path(path, kind))
                .cloned()
                .collect();
            if let Some(path) = Self::pick_random_path(&candidates) {
                return Some(path);
            }
        }

        let fallback = fallback.trim();
        if fallback.is_empty() {
            None
        } else {
            Some(fallback.to_string())
        }
    }

    fn pick_random_path(paths: &[String]) -> Option<String> {
        if paths.is_empty() {
            return None;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let idx = (now.as_nanos() % (paths.len() as u128)) as usize;
        Some(paths[idx].clone())
    }

    fn is_allowed_media_path(path: &str, kind: MediaKind) -> bool {
        let path = std::path::Path::new(path);
        if !path.is_file() {
            return false;
        }
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => ext.to_ascii_lowercase(),
            None => return false,
        };
        match kind {
            MediaKind::Image => Self::is_allowed_image_ext(&ext),
            MediaKind::Video => Self::is_allowed_video_ext(&ext),
        }
    }

    fn collect_slideshow_images(path: &str) -> Vec<std::path::PathBuf> {
        let dir = std::path::Path::new(path);
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return Vec::new(),
        };
        let mut images = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if Self::is_allowed_image_ext(ext) {
                        images.push(path);
                    }
                }
            }
        }
        images.sort();
        images
    }

    fn is_allowed_image_ext(ext: &str) -> bool {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tiff" | "tif" | "tga" | "ico"
                | "ppm" | "pgm" | "pbm" | "hdr" | "exr"
        )
    }

    fn is_allowed_video_ext(ext: &str) -> bool {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" | "mpg" | "mpeg" | "ogv" | "wmv"
                | "flv" | "m2ts" | "mts"
        )
    }

    fn set_slideshow_image(pictures: &[Picture], path: &std::path::Path) {
        let file = gio::File::for_path(path);
        for picture in pictures {
            picture.set_file(Some(&file));
        }
    }

    fn hex_to_rgba(hex: &str) -> Option<RGBA> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 { return None; }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
        Some(RGBA::new(r, g, b, 1.0))
    }
}
