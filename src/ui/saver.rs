use crate::config::{
    AnimatedPattern, ClockPosition, PatternDensity, PatternSpeed, PatternTheme, ScreensaverMode,
    SettingsProfile, CLOCK_MOVE_POSITIONS, DEFAULT_CLOCK_FORMAT,
};
use gdk4::{Display, Key, Monitor, RGBA};
use glib::DateTime;
use gtk4::cairo;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, ContentFit, DrawingArea, EventControllerKey, EventControllerMotion,
    Fixed, GraphicsOffload, GraphicsOffloadEnabled, Justification, Label, MediaControls, MediaFile,
    Overflow, Overlay, Picture, Video, Widget,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
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
    now_playing: Option<NowPlayingWidgets>,
    rss_ticker: Option<RssTickerWidgets>,
    system_stats: Option<SystemStatsWidgets>,
    clock_label: Option<Label>,
    clock_time_label: Option<Label>,
    clock_date_label: Option<Label>,
}

#[derive(Clone)]
struct NowPlayingWidgets {
    root: gtk4::Box,
    art: Picture,
    title: Label,
    meta: Label,
    last_art_url: Rc<RefCell<Option<String>>>,
}

#[derive(Clone)]
struct RssTickerWidgets {
    root: gtk4::Box,
    fixed: Fixed,
    label: Label,
    x: Rc<Cell<f64>>,
}

#[derive(Clone)]
struct SystemStatsWidgets {
    root: gtk4::Box,
    cpu_label: Label,
    ram_label: Label,
    graph: DrawingArea,
    history: Rc<RefCell<Vec<crate::sysstats::SystemStatsSample>>>,
}

pub struct ScreensaverWindow {
    windows: Vec<WindowData>,
    config: SettingsProfile,
    slideshow_source: RefCell<Option<glib::SourceId>>,
    clock_source: RefCell<Option<glib::SourceId>>,
    clock_move_source: RefCell<Option<glib::SourceId>>,
    now_playing_source: RefCell<Option<glib::SourceId>>,
    now_playing_move_source: RefCell<Option<glib::SourceId>>,
    rss_update_source: RefCell<Option<glib::SourceId>>,
    rss_scroll_source: RefCell<Option<glib::SourceId>>,
    rss_stop: Arc<AtomicBool>,
    system_stats_source: RefCell<Option<glib::SourceId>>,
    system_stats_move_source: RefCell<Option<glib::SourceId>>,
    system_stats_stop: Arc<AtomicBool>,
    now_playing_stop: Arc<AtomicBool>,
    activity_suppressed_until: Rc<Cell<Option<Instant>>>,
}

impl ScreensaverWindow {
    pub fn new<F>(
        config: &SettingsProfile,
        app: Option<gtk4::Application>,
        started_at: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
        on_activity: F,
    ) -> Self
    where
        F: Fn() + 'static + Clone,
    {
        Self::new_impl(config, app, started_at, on_activity, None)
    }

    pub fn new_for_monitors<F>(
        config: &SettingsProfile,
        app: Option<gtk4::Application>,
        started_at: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
        monitors: Vec<Monitor>,
        on_activity: F,
    ) -> Self
    where
        F: Fn() + 'static + Clone,
    {
        Self::new_impl(config, app, started_at, on_activity, Some(monitors))
    }

    fn new_impl<F>(
        config: &SettingsProfile,
        app: Option<gtk4::Application>,
        started_at: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
        on_activity: F,
        monitors_override: Option<Vec<Monitor>>,
    ) -> Self
    where
        F: Fn() + 'static + Clone,
    {
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
                    now_playing_source: RefCell::new(None),
                    now_playing_move_source: RefCell::new(None),
                    rss_update_source: RefCell::new(None),
                    rss_scroll_source: RefCell::new(None),
                    rss_stop: Arc::new(AtomicBool::new(true)),
                    system_stats_source: RefCell::new(None),
                    system_stats_move_source: RefCell::new(None),
                    system_stats_stop: Arc::new(AtomicBool::new(true)),
                    now_playing_stop: Arc::new(AtomicBool::new(true)),
                    activity_suppressed_until,
                }
            }
        };

        let monitors = monitors_override.unwrap_or_else(|| Self::get_monitors(&display));
        let is_x11 = display.backend().is_x11();
        let track_mouse_activity = match config_clone.mode {
            ScreensaverMode::Web(_) => !config_clone.web_interaction_enabled,
            ScreensaverMode::Pattern(AnimatedPattern::SmokeInk) => false,
            _ => true,
        };

        if monitors.is_empty() {
            let (window, media, slideshow_picture, clock_label, clock_time_label, clock_date_label) =
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
                track_mouse_activity,
            );
            let now_playing = if config_clone.show_now_playing {
                Self::attach_now_playing_overlay(&window, config_clone.now_playing_position)
            } else {
                None
            };
            let rss_ticker = if config_clone.show_rss_ticker {
                Self::attach_rss_ticker_overlay(&window)
            } else {
                None
            };
            let system_stats = if config_clone.show_system_stats {
                Self::attach_system_stats_overlay(&window, config_clone.system_stats_position)
            } else {
                None
            };
            windows.push(WindowData {
                window,
                media,
                slideshow_picture,
                now_playing,
                rss_ticker,
                system_stats,
                clock_label,
                clock_time_label,
                clock_date_label,
            });
        } else {
            for monitor in &monitors {
                let geometry = monitor.geometry();
                let (
                    window,
                    media,
                    slideshow_picture,
                    clock_label,
                    clock_time_label,
                    clock_date_label,
                ) = Self::create_window_with_content(app.as_ref(), config, is_x11, Some(geometry));
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
                    track_mouse_activity,
                );
                let now_playing = if config_clone.show_now_playing {
                    Self::attach_now_playing_overlay(&window, config_clone.now_playing_position)
                } else {
                    None
                };
                let rss_ticker = if config_clone.show_rss_ticker {
                    Self::attach_rss_ticker_overlay(&window)
                } else {
                    None
                };
                let system_stats = if config_clone.show_system_stats {
                    Self::attach_system_stats_overlay(&window, config_clone.system_stats_position)
                } else {
                    None
                };
                windows.push(WindowData {
                    window,
                    media,
                    slideshow_picture,
                    now_playing,
                    rss_ticker,
                    system_stats,
                    clock_label,
                    clock_time_label,
                    clock_date_label,
                });
            }
        }

        // If we render video on multiple monitors, we still want audio only once.
        // Otherwise, sound can "double" when multiple windows play the same (or different) videos.
        if !config_clone.mute_video {
            let mut audio_assigned = false;
            for wd in &windows {
                let Some(media) = wd.media.as_ref() else {
                    continue;
                };
                if !audio_assigned {
                    audio_assigned = true;
                } else {
                    media.set_muted(true);
                    media.set_volume(0.0);
                }
            }
        }

        let slideshow_source = RefCell::new(None);
        let clock_source = RefCell::new(None);
        let clock_move_source = RefCell::new(None);
        let now_playing_source = RefCell::new(None);
        let now_playing_move_source = RefCell::new(None);
        let rss_update_source = RefCell::new(None);
        let rss_scroll_source = RefCell::new(None);
        let rss_stop = Arc::new(AtomicBool::new(false));
        let system_stats_source = RefCell::new(None);
        let system_stats_move_source = RefCell::new(None);
        let system_stats_stop = Arc::new(AtomicBool::new(false));
        let now_playing_stop = Arc::new(AtomicBool::new(false));
        let instance = Self {
            windows,
            config: config_clone,
            slideshow_source,
            clock_source,
            clock_move_source,
            now_playing_source,
            now_playing_move_source,
            rss_update_source,
            rss_scroll_source,
            rss_stop,
            system_stats_source,
            system_stats_move_source,
            system_stats_stop,
            now_playing_stop,
            activity_suppressed_until,
        };
        instance.setup_slideshow_if_needed();
        instance.setup_clock_if_needed();
        instance.setup_clock_move_if_needed();
        instance.setup_now_playing_if_needed();
        instance.setup_now_playing_move_if_needed();
        instance.setup_rss_ticker_if_needed();
        instance.setup_system_stats_if_needed();
        instance.setup_system_stats_move_if_needed();
        instance
    }

    fn create_window_with_content(
        app: Option<&gtk4::Application>,
        config: &SettingsProfile,
        is_x11: bool,
        geometry: Option<gdk4::Rectangle>,
    ) -> (
        ApplicationWindow,
        Option<MediaFile>,
        Option<Picture>,
        Option<Label>,
        Option<Label>,
        Option<Label>,
    ) {
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

        let show_cursor =
            matches!(config.mode, ScreensaverMode::Web(_)) && config.web_interaction_enabled;
        if !show_cursor {
            if let Some(cursor) = gdk4::Cursor::from_name("none", None) {
                window.set_cursor(Some(&cursor));
            }
        }

        let (media, slideshow_picture) = Self::setup_window_content(&window, config);
        let (clock_label, clock_time_label, clock_date_label) = if config.show_clock {
            if config.clock_two_lines {
                let (t, d) = Self::attach_clock_overlay_two_lines(
                    &window,
                    if config.clock_time_format.trim().is_empty() {
                        crate::config::DEFAULT_CLOCK_TIME_FORMAT
                    } else {
                        &config.clock_time_format
                    },
                    if config.clock_date_format.trim().is_empty() {
                        crate::config::DEFAULT_CLOCK_DATE_FORMAT
                    } else {
                        &config.clock_date_format
                    },
                    config.clock_position,
                    config.clock_size,
                );
                (None, t, d)
            } else {
                (
                    Self::attach_clock_overlay(
                        &window,
                        &config.clock_format,
                        config.clock_position,
                        config.clock_size,
                    ),
                    None,
                    None,
                )
            }
        } else {
            (None, None, None)
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

        (
            window,
            media,
            slideshow_picture,
            clock_label,
            clock_time_label,
            clock_date_label,
        )
    }

    fn setup_activity_tracking<F>(
        window: &ApplicationWindow,
        on_activity: F,
        started_at: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
        activity_suppressed_until: Rc<Cell<Option<Instant>>>,
        mouse_wake_delay_ms: u64,
        track_mouse_activity: bool,
    ) where
        F: Fn() + 'static + Clone,
    {
        fn is_media_key(key: Key) -> bool {
            matches!(
                key,
                Key::AudioNext
                    | Key::AudioPrev
                    | Key::AudioPlay
                    | Key::AudioPause
                    | Key::AudioStop
                    | Key::AudioRaiseVolume
                    | Key::AudioLowerVolume
                    | Key::AudioMute
                    | Key::AudioMicMute
                    | Key::AudioMedia
                    | Key::AudioForward
                    | Key::AudioRewind
                    | Key::AudioCycleTrack
                    | Key::AudioRepeat
                    | Key::AudioRandomPlay
                    | Key::AudioRecord
                    | Key::AudioPreset
            )
        }

        let key_grace = Duration::from_millis(1500);
        let mouse_grace = Duration::from_millis(mouse_wake_delay_ms);
        let is_x11 = Display::default()
            .map(|d| d.backend().is_x11())
            .unwrap_or(false);

        let key_controller = EventControllerKey::new();
        let on_activity_key = on_activity.clone();
        let started_at_key = started_at.clone();
        let suppress_until_key = activity_suppressed_until.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if let Some(until) = suppress_until_key.get() {
                if Instant::now() < until {
                    return glib::Propagation::Stop;
                }
            }
            if is_media_key(key) {
                return glib::Propagation::Stop;
            }
            if started_at_key
                .lock()
                .unwrap()
                .map(|t| t.elapsed() >= key_grace)
                .unwrap_or(false)
            {
                on_activity_key();
            }
            glib::Propagation::Stop
        });
        window.add_controller(key_controller);

        if track_mouse_activity {
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
                if started_at_motion
                    .lock()
                    .unwrap()
                    .map(|t| t.elapsed() >= mouse_grace)
                    .unwrap_or(false)
                {
                    // Treat motion as activity only if the pointer actually moved.
                    //
                    // On Wayland, animations/relayout can sometimes cause motion-like notifications
                    // without real cursor movement. Using a small distance threshold avoids the
                    // screensaver closing by itself.
                    let threshold = if is_x11 { 50.0 } else { 6.0 };
                    if let Some((lx, ly)) = last_pos.get() {
                        let dist = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
                        if dist > 0.0 {
                            total_distance.set(total_distance.get() + dist);
                            if total_distance.get() > threshold {
                                on_activity_motion();
                            }
                        }
                    }
                    last_pos.set(Some((x, y)));
                }
            });
            window.add_controller(motion_controller);
        }
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
        if let Some(source) = self.now_playing_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(source) = self.now_playing_move_source.borrow_mut().take() {
            source.remove();
        }
        self.now_playing_stop.store(true, Ordering::Relaxed);
        if let Some(source) = self.rss_update_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(source) = self.rss_scroll_source.borrow_mut().take() {
            source.remove();
        }
        self.rss_stop.store(true, Ordering::Relaxed);
        if let Some(source) = self.system_stats_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(source) = self.system_stats_move_source.borrow_mut().take() {
            source.remove();
        }
        self.system_stats_stop.store(true, Ordering::Relaxed);

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
            if !cmd.enabled {
                continue;
            }
            let command = if hide {
                &cmd.hide_command
            } else {
                &cmd.show_command
            };
            if command.is_empty() {
                continue;
            }
            let _ = std::process::Command::new("sh")
                .args(["-c", command])
                .output();
        }
    }

    fn configure_x11_override_redirect(xid: u32, geom: Option<gdk4::Rectangle>) {
        let Ok((conn, _)) = x11rb::connect(None) else {
            return;
        };

        // Unmap first, then set override-redirect, then remap
        let _ = conn.unmap_window(xid);

        let _ = conn.change_window_attributes(
            xid,
            &x11rb::protocol::xproto::ChangeWindowAttributesAux::new().override_redirect(1),
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
            true,
            xid,
            x11rb::CURRENT_TIME,
            x11rb::protocol::xproto::GrabMode::ASYNC,
            x11rb::protocol::xproto::GrabMode::ASYNC,
        );
        let _ = conn.grab_pointer(
            true,
            xid,
            x11rb::protocol::xproto::EventMask::POINTER_MOTION
                | x11rb::protocol::xproto::EventMask::BUTTON_PRESS
                | x11rb::protocol::xproto::EventMask::BUTTON_RELEASE,
            x11rb::protocol::xproto::GrabMode::ASYNC,
            x11rb::protocol::xproto::GrabMode::ASYNC,
            xid,
            0u32,
            x11rb::CURRENT_TIME,
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
                let bg = if matches!(pattern, AnimatedPattern::WaterRipples) {
                    Some(config.water_ripples_background_image.as_str())
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                } else {
                    None
                };
                Self::setup_pattern_content(
                    window,
                    *pattern,
                    config.pattern_speed,
                    config.pattern_density,
                    config.pattern_theme,
                    bg,
                );
                (None, None)
            }
            ScreensaverMode::Web(url) => {
                Self::setup_web_content(
                    window,
                    url,
                    config.mute_video,
                    config.web_interaction_enabled,
                );
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
            ScreensaverMode::PythonScript(path) => {
                let path = path.trim();
                if path.is_empty() {
                    Self::setup_color_content(window, "#000000");
                    (None, None)
                } else if !std::path::Path::new(path).is_file() {
                    Self::setup_color_content(window, "#000000");
                    (None, None)
                } else {
                    Self::setup_python_script_content(
                        window,
                        path,
                        config.pattern_density,
                        config.pattern_theme,
                    );
                    (None, None)
                }
            }
            ScreensaverMode::Shadertoy(path) => {
                let path = path.trim();
                if path.is_empty() {
                    Self::setup_color_content(window, "#000000");
                    (None, None)
                } else if !std::path::Path::new(path).is_file() {
                    Self::setup_color_content(window, "#000000");
                    (None, None)
                } else {
                    Self::setup_shadertoy_content(window, path, config.pattern_density);
                    (None, None)
                }
            }
        }
    }

    fn setup_python_script_content(
        window: &ApplicationWindow,
        script_path: &str,
        density: PatternDensity,
        theme: PatternTheme,
    ) {
        let area = crate::ui::python_plugins::build_python_script_area(script_path, density, theme);
        window.set_child(Some(&area));
    }

    fn setup_shadertoy_content(window: &ApplicationWindow, shader_path: &str, density: PatternDensity) {
        let area = crate::ui::shadertoy::build_shadertoy_area(shader_path, density);
        window.set_child(Some(&area));
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
            cr.set_source_rgba(
                color.red() as f64,
                color.green() as f64,
                color.blue() as f64,
                1.0,
            );
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

    fn setup_pattern_content(
        window: &ApplicationWindow,
        pattern: AnimatedPattern,
        speed: PatternSpeed,
        density: PatternDensity,
        theme: PatternTheme,
        water_ripples_bg_path: Option<&str>,
    ) {
        let gl_area = crate::ui::gl_patterns::build_gl_pattern_area(
            pattern,
            speed,
            density,
            theme,
            water_ripples_bg_path,
        );
        window.set_child(Some(&gl_area));
    }

    fn setup_web_content(window: &ApplicationWindow, url: &str, mute: bool, interactive: bool) {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            Self::setup_color_content(window, "#000000");
            return;
        }
        let web_view = webkit6::WebView::new();
        web_view.load_uri(trimmed);
        web_view.set_can_focus(interactive);
        web_view.set_is_muted(mute);
        web_view.set_hexpand(true);
        web_view.set_vexpand(true);
        if interactive {
            web_view.grab_focus();
        }
        let window_for_crash = window.clone();
        web_view.connect_web_process_terminated(move |_, _| {
            Self::setup_color_content(&window_for_crash, "#000000");
        });
        window.set_child(Some(&web_view));
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
                (gray.powf(0.5), gray * 0.7, gray * 0.2)
            }
            PatternTheme::Cool => {
                let gray = r * 0.299 + g * 0.587 + b * 0.114;
                (gray * 0.2, gray * 0.7, gray.powf(0.5))
            }
            PatternTheme::Random => (g, b, r),
        }
    }

    fn draw_pattern(
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
            AnimatedPattern::Matrix => {
                Self::draw_matrix_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Stars => {
                Self::draw_stars_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Geometry => {
                Self::draw_geometry_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Flowfield => {
                Self::draw_flowfield_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Aurora => {
                Self::draw_aurora_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Plasma => {
                Self::draw_plasma_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Bokeh => {
                Self::draw_bokeh_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Constellation => {
                Self::draw_constellation_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Lissajous => {
                Self::draw_lissajous_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Waves => {
                Self::draw_waves_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Voronoi => {
                Self::draw_voronoi_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Scanline => {
                Self::draw_scanline_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Fireflies => {
                Self::draw_fireflies_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::SmokeInk => {
                // CPU fallback (unused in GPU mode): reuse plasma for now.
                Self::draw_plasma_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::WaterRipples => {
                // CPU fallback (unused in GPU mode): reuse waves for now.
                Self::draw_waves_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::MatrixRain3D => {
                // CPU fallback (unused in GPU mode): reuse Matrix for now.
                Self::draw_matrix_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Lcars => {
                // CPU fallback (unused in GPU mode): reuse Geometry for now.
                Self::draw_geometry_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Terminal => {
                // CPU fallback (unused in GPU mode): reuse Matrix for now.
                Self::draw_matrix_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::Fractals => {
                // CPU fallback (unused in GPU mode): reuse Plasma for now.
                Self::draw_plasma_pattern(cr, width, height, t, density, theme)
            }
            AnimatedPattern::ReactionDiffusion => {
                // CPU fallback (unused in GPU mode): reuse Plasma for now.
                Self::draw_plasma_pattern(cr, width, height, t, density, theme)
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
                let (r, g, b) = Self::apply_theme(0.0, green, 0.0, theme);
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
            let (r, g, b) = Self::apply_theme(brightness, brightness, 1.0, theme);
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
                let base = Self::hash_f64(seed);
                let cx = col as f64 * spacing + spacing * 0.5;
                let cy = row as f64 * spacing + spacing * 0.5;
                let size = spacing * (0.28 + 0.18 * base);
                let angle = time * 0.5 + base * std::f64::consts::PI * 2.0;
                let pulse = (time * 1.1 + base * std::f64::consts::PI * 2.0).sin() * 0.5 + 0.5;
                let r_base = 0.2 + 0.6 * pulse;
                let g_base = 0.4 + 0.4 * (1.0 - pulse);
                let b_base = 0.7 + 0.2 * pulse;
                let (r, g, b) = Self::apply_theme(r_base, g_base, b_base, theme);

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
                let (r, g, b) = Self::apply_theme(r_base, g_base, b_base, theme);

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
            let (r, g, b) = Self::apply_theme(r_base, g_base, b_base, theme);

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
                let (r, g, b) = Self::apply_theme(r_base, g_base, b_base, theme);

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

            let r_base = Self::hash_f64(seed);
            let g_base = Self::hash_f64(seed.wrapping_add(1));
            let b_base = Self::hash_f64(seed.wrapping_add(2));

            // Random properties
            let radius = 20.0 + Self::hash_f64(seed.wrapping_add(3)) * 80.0;
            let speed_x = -10.0 + Self::hash_f64(seed.wrapping_add(4)) * 20.0;
            let speed_y = -10.0 + Self::hash_f64(seed.wrapping_add(5)) * 20.0;
            let x_start = Self::hash_f64(seed.wrapping_add(6)) * width;
            let y_start = Self::hash_f64(seed.wrapping_add(7)) * height;

            // Parallax drift
            let x = (x_start + time * speed_x).rem_euclid(width + radius * 2.0) - radius;
            let y = (y_start + time * speed_y).rem_euclid(height + radius * 2.0) - radius;

            // Slow fade in/out
            let fade_speed = 0.2 + Self::hash_f64(seed.wrapping_add(8)) * 0.5;
            let offset = Self::hash_f64(seed.wrapping_add(9)) * 10.0;
            let alpha = 0.1 + ((time * fade_speed + offset).sin() * 0.5 + 0.5) * 0.15;

            // Color variation (warm/cool mix)
            let hue = Self::hash_f64(seed.wrapping_add(10));
            let (r_col, g_col, b_col) = if hue > 0.5 {
                (0.8 + r_base * 0.2, 0.4 + g_base * 0.3, 0.2) // Warm
            } else {
                (0.1, 0.5 + g_base * 0.3, 0.7 + b_base * 0.3) // Cool
            };
            let (r, g, b) = Self::apply_theme(r_col, g_col, b_col, theme);

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
            let x_start = Self::hash_f64(seed) * width;
            let y_start = Self::hash_f64(seed.wrapping_add(1)) * height;
            let vx = -20.0 + Self::hash_f64(seed.wrapping_add(2)) * 40.0;
            let vy = -20.0 + Self::hash_f64(seed.wrapping_add(3)) * 40.0;

            let x = (x_start + time * vx).rem_euclid(width);
            let y = (y_start + time * vy).rem_euclid(height);

            points.push((x, y));

            // Draw point
            let (r, g, b) = Self::apply_theme(1.0, 1.0, 1.0, theme);
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
                    let (r, g, b) = Self::apply_theme(0.6, 0.8, 1.0, theme);
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
            let (r, g, b) = Self::apply_theme(r_base, g_base, b_base, theme);

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
            let (r, g, b) = Self::apply_theme(r_base, g_base, b_base, theme);

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
        let step = 25.0; // Aggressively optimized: Increased to 25.0 to fix UI blocking

        // Compute seed positions
        let mut seeds = Vec::with_capacity(seed_count);
        for i in 0..seed_count {
            let idx = i as u32;
            let seed = idx.wrapping_mul(123456789);

            let x_base = Self::hash_f64(seed) * width;
            let y_base = Self::hash_f64(seed.wrapping_add(1)) * height;
            let vx = -15.0 + Self::hash_f64(seed.wrapping_add(2)) * 30.0;
            let vy = -15.0 + Self::hash_f64(seed.wrapping_add(3)) * 30.0;

            let x = (x_base + time * vx).rem_euclid(width);
            let y = (y_base + time * vy).rem_euclid(height);

            // Seed color (hue)
            let r_base = Self::hash_f64(seed.wrapping_add(4));
            let g_base = Self::hash_f64(seed.wrapping_add(5));
            let b_base = Self::hash_f64(seed.wrapping_add(6));
            let (r, g, b) = Self::apply_theme(r_base, g_base, b_base, theme);

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

        let (r, g, b) = Self::apply_theme(0.0, 0.1, 0.0, theme);
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
        let (br, bg, bb) = Self::apply_theme(0.5, 0.0, 0.0, theme);
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

            let x_base = Self::hash_f64(seed) * width;
            let y_base = Self::hash_f64(seed.wrapping_add(1)) * height;

            // Wandering motion
            let nx = Self::hash_f64(seed.wrapping_add(2)) * 100.0;
            let ny = Self::hash_f64(seed.wrapping_add(3)) * 100.0;

            // Use time to drive position
            let t_offset = Self::hash_f64(seed.wrapping_add(4)) * 10.0;
            let t = time * 0.5 + t_offset;

            let dx = (t * 0.7 + nx).sin() * 50.0 + (t * 1.3).cos() * 30.0;
            let dy = (t * 0.5 + ny).cos() * 50.0 + (t * 1.7).sin() * 30.0;

            let x = (x_base + dx).rem_euclid(width + 40.0) - 20.0;
            let y = (y_base + dy).rem_euclid(height + 40.0) - 20.0;

            // Pulsing brightness
            let pulse_speed = 1.0 + Self::hash_f64(seed.wrapping_add(5));
            let pulse = (t * pulse_speed).sin() * 0.5 + 0.5; // 0.0 to 1.0

            // Color: Yellow-Green/Gold
            let r_base = 0.8 + 0.2 * pulse;
            let g_base = 0.9 + 0.1 * pulse;
            let b_base = 0.2;
            let alpha = 0.3 + 0.7 * pulse;
            let (r, g, b) = Self::apply_theme(r_base, g_base, b_base, theme);

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

        Self::enable_cinemagraph_loop(&media);

        let video = Video::for_media_stream(Some(&media));
        video.set_autoplay(true);
        video.set_loop(true);
        video.set_graphics_offload(GraphicsOffloadEnabled::Enabled);
        video.set_can_focus(false);
        video.set_focusable(false);
        video.set_focus_on_click(false);
        video.set_can_target(false);
        video.set_hexpand(true);
        video.set_vexpand(true);
        Self::hide_media_controls(&video.clone().upcast::<Widget>());

        let offload = GraphicsOffload::new(Some(&video));
        offload.set_enabled(GraphicsOffloadEnabled::Enabled);
        offload.set_hexpand(true);
        offload.set_vexpand(true);
        window.set_child(Some(&offload));

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

        let video = Video::for_media_stream(Some(&media));
        video.set_autoplay(true);
        video.set_loop(false);
        video.set_graphics_offload(GraphicsOffloadEnabled::Enabled);
        video.set_can_focus(false);
        video.set_focusable(false);
        video.set_focus_on_click(false);
        video.set_can_target(false);
        video.set_hexpand(true);
        video.set_vexpand(true);
        Self::hide_media_controls(&video.clone().upcast::<Widget>());

        let offload = GraphicsOffload::new(Some(&video));
        offload.set_enabled(GraphicsOffloadEnabled::Enabled);
        offload.set_hexpand(true);
        offload.set_vexpand(true);
        window.set_child(Some(&offload));

        media
    }

    fn enable_cinemagraph_loop(media: &MediaFile) {
        const MARGIN_US: i64 = 500_000;
        const POLL_MS: u64 = 200;

        let weak_media = media.downgrade();
        glib::timeout_add_local(Duration::from_millis(POLL_MS), move || {
            let Some(media) = weak_media.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if media.error().is_some() || !media.is_playing() {
                return glib::ControlFlow::Continue;
            }
            if !media.is_prepared()
                || !media.has_video()
                || !media.is_seekable()
                || media.is_seeking()
            {
                return glib::ControlFlow::Continue;
            }
            let duration = media.duration();
            if duration <= 0 {
                return glib::ControlFlow::Continue;
            }
            let timestamp = media.timestamp();
            if timestamp >= duration.saturating_sub(MARGIN_US) {
                media.seek(0);
            }
            glib::ControlFlow::Continue
        });
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

        self.activity_suppressed_until
            .set(Some(Instant::now() + Duration::from_millis(250)));
        let interval_ms = self.config.slideshow_interval_seconds.max(1) * 1000;
        Self::set_slideshow_image(&pictures, &images[0]);
        if images.len() > 1 {
            let images = Rc::new(images);
            let pictures = Rc::new(pictures);
            let index = Rc::new(Cell::new(0usize));
            let suppress_until = self.activity_suppressed_until.clone();

            let source_id =
                glib::timeout_add_local(Duration::from_millis(interval_ms), move || {
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

        if self.config.clock_two_lines {
            let time_labels: Vec<Label> = self
                .windows
                .iter()
                .filter_map(|wd| wd.clock_time_label.clone())
                .collect();
            let date_labels: Vec<Label> = self
                .windows
                .iter()
                .filter_map(|wd| wd.clock_date_label.clone())
                .collect();
            if time_labels.is_empty() && date_labels.is_empty() {
                return;
            }

            let time_format = self.config.clock_time_format.clone();
            let date_format = self.config.clock_date_format.clone();
            let time_labels = Rc::new(time_labels);
            let date_labels = Rc::new(date_labels);
            let suppress_until = self.activity_suppressed_until.clone();
            let source_id = glib::timeout_add_local(Duration::from_secs(1), move || {
                suppress_until.set(Some(Instant::now() + Duration::from_millis(250)));
                let time_text = Self::format_clock_text(&time_format);
                let date_text = Self::format_clock_text(&date_format);
                for label in time_labels.iter() {
                    label.set_text(&time_text);
                }
                for label in date_labels.iter() {
                    label.set_text(&date_text);
                }
                glib::ControlFlow::Continue
            });
            *self.clock_source.borrow_mut() = Some(source_id);
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

        let labels: Vec<Label> = if self.config.clock_two_lines {
            self.windows
                .iter()
                .filter_map(|wd| wd.clock_time_label.clone())
                .collect()
        } else {
            self.windows
                .iter()
                .filter_map(|wd| wd.clock_label.clone())
                .collect()
        };
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

    fn setup_now_playing_if_needed(&self) {
        if !self.config.show_now_playing {
            return;
        }

        let widgets: Vec<NowPlayingWidgets> = self
            .windows
            .iter()
            .filter_map(|wd| wd.now_playing.clone())
            .collect();
        if widgets.is_empty() {
            return;
        }

        let stop = Arc::clone(&self.now_playing_stop);
        let (tx, rx) = std::sync::mpsc::channel::<Option<crate::mpris::NowPlayingInfo>>();
        std::thread::spawn(move || {
            let mut last: Option<crate::mpris::NowPlayingInfo> = None;
            while !stop.load(Ordering::Relaxed) {
                let current = crate::mpris::query_now_playing().ok().flatten();
                if current != last {
                    let _ = tx.send(current.clone());
                    last = current;
                }
                std::thread::sleep(Duration::from_millis(1000));
            }
        });

        let suppress_until = self.activity_suppressed_until.clone();
        let source_id = glib::timeout_add_local(Duration::from_millis(250), move || {
            while let Ok(info) = rx.try_recv() {
                suppress_until.set(Some(Instant::now() + Duration::from_millis(400)));
                for widget in &widgets {
                    Self::apply_now_playing_to_widgets(widget, info.as_ref());
                }
            }
            glib::ControlFlow::Continue
        });
        *self.now_playing_source.borrow_mut() = Some(source_id);
    }

    fn setup_now_playing_move_if_needed(&self) {
        if !self.config.show_now_playing || !self.config.now_playing_move_enabled {
            return;
        }

        let roots: Vec<gtk4::Box> = self
            .windows
            .iter()
            .filter_map(|wd| wd.now_playing.clone())
            .map(|w| w.root)
            .collect();
        if roots.is_empty() {
            return;
        }

        let start_index = CLOCK_MOVE_POSITIONS
            .iter()
            .position(|pos| *pos == self.config.now_playing_position)
            .unwrap_or(0);
        let index = Rc::new(Cell::new(start_index));
        let roots = Rc::new(roots);
        let suppress_until = self.activity_suppressed_until.clone();
        let interval = self.config.now_playing_move_interval_seconds.max(1);
        let source_id = glib::timeout_add_local(Duration::from_secs(interval), move || {
            let next = (index.get() + 1) % CLOCK_MOVE_POSITIONS.len();
            index.set(next);
            suppress_until.set(Some(Instant::now() + Duration::from_millis(250)));
            for root in roots.iter() {
                Self::apply_clock_position_widget(root, CLOCK_MOVE_POSITIONS[next], 24);
            }
            glib::ControlFlow::Continue
        });
        *self.now_playing_move_source.borrow_mut() = Some(source_id);
    }

    fn setup_rss_ticker_if_needed(&self) {
        if !self.config.show_rss_ticker {
            return;
        }

        let widgets: Vec<RssTickerWidgets> = self
            .windows
            .iter()
            .filter_map(|wd| wd.rss_ticker.clone())
            .collect();
        if widgets.is_empty() {
            return;
        }
        let widgets = Rc::new(widgets);

        let feeds = self.config.rss_feeds.clone();
        if feeds.is_empty() {
            for w in widgets.iter() {
                w.root.set_visible(false);
            }
            return;
        }

        let refresh = Duration::from_secs(self.config.rss_refresh_interval_minutes.max(1) * 60);
        let speed = self.config.rss_ticker_speed_px_s.max(10) as f64;

        let stop = Arc::clone(&self.rss_stop);
        let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let text = crate::rss::fetch_ticker_text(&feeds, 5, 50).ok().flatten();
                let _ = tx.send(text);

                let mut slept = Duration::from_secs(0);
                while slept < refresh && !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_secs(1));
                    slept += Duration::from_secs(1);
                }
            }
        });

        let suppress_until = self.activity_suppressed_until.clone();
        let widgets_for_update = Rc::clone(&widgets);
        let update_source = glib::timeout_add_local(Duration::from_millis(250), move || {
            while let Ok(text) = rx.try_recv() {
                suppress_until.set(Some(Instant::now() + Duration::from_millis(400)));
                for w in widgets_for_update.iter() {
                    let text = text.as_deref().map(str::trim).filter(|s| !s.is_empty());
                    if let Some(text) = text {
                        w.label.set_text(text);
                        w.root.set_visible(true);
                        let start = w.root.allocated_width().max(0) as f64;
                        w.x.set(start);
                        w.fixed.move_(&w.label, start, 0.0);
                    } else {
                        w.root.set_visible(false);
                    }
                }
            }
            glib::ControlFlow::Continue
        });
        *self.rss_update_source.borrow_mut() = Some(update_source);

        let widgets_for_scroll = Rc::clone(&widgets);
        let scroll_source = glib::timeout_add_local(Duration::from_millis(16), move || {
            let dx = speed * (16.0 / 1000.0);
            for w in widgets_for_scroll.iter() {
                if !w.root.is_visible() {
                    continue;
                }
                let container_width = w.root.allocated_width().max(0) as f64;
                if container_width <= 0.0 {
                    continue;
                }
                let (_, label_width, _, _) = w.label.measure(gtk4::Orientation::Horizontal, -1);
                let label_width = label_width.max(0) as f64;
                let mut x = w.x.get() - dx;
                if label_width > 0.0 && x + label_width < 0.0 {
                    x = container_width;
                }
                w.x.set(x);
                w.fixed.move_(&w.label, x, 0.0);
            }
            glib::ControlFlow::Continue
        });
        *self.rss_scroll_source.borrow_mut() = Some(scroll_source);
    }

    fn setup_system_stats_if_needed(&self) {
        if !self.config.show_system_stats {
            return;
        }

        let widgets: Vec<SystemStatsWidgets> = self
            .windows
            .iter()
            .filter_map(|wd| wd.system_stats.clone())
            .collect();
        if widgets.is_empty() {
            return;
        }
        let widgets = Rc::new(widgets);

        let stop = Arc::clone(&self.system_stats_stop);
        let (tx, rx) = std::sync::mpsc::channel::<crate::sysstats::SystemStatsSample>();
        std::thread::spawn(move || {
            let mut reader = crate::sysstats::SystemStatsReader::new();
            while !stop.load(Ordering::Relaxed) {
                if let Some(sample) = reader.sample() {
                    let _ = tx.send(sample);
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });

        let suppress_until = self.activity_suppressed_until.clone();
        let widgets_for_update = Rc::clone(&widgets);
        let update_source = glib::timeout_add_local(Duration::from_millis(250), move || {
            while let Ok(sample) = rx.try_recv() {
                suppress_until.set(Some(Instant::now() + Duration::from_millis(400)));
                for w in widgets_for_update.iter() {
                    let cpu_pct = (sample.cpu_usage * 100.0).round().clamp(0.0, 100.0) as u32;
                    let ram_pct = (sample.ram_usage * 100.0).round().clamp(0.0, 100.0) as u32;
                    w.cpu_label.set_text(&format!("CPU {cpu_pct:>3}%"));
                    w.ram_label.set_text(&format!("RAM {ram_pct:>3}%"));

                    {
                        let mut hist = w.history.borrow_mut();
                        hist.push(sample);
                        if hist.len() > 120 {
                            let overflow = hist.len() - 120;
                            hist.drain(0..overflow);
                        }
                    }
                    w.graph.queue_draw();
                }
            }
            glib::ControlFlow::Continue
        });
        *self.system_stats_source.borrow_mut() = Some(update_source);
    }

    fn setup_system_stats_move_if_needed(&self) {
        if !self.config.show_system_stats || !self.config.system_stats_move_enabled {
            return;
        }

        let roots: Vec<gtk4::Box> = self
            .windows
            .iter()
            .filter_map(|wd| wd.system_stats.clone())
            .map(|w| w.root)
            .collect();
        if roots.is_empty() {
            return;
        }

        let start_index = CLOCK_MOVE_POSITIONS
            .iter()
            .position(|pos| *pos == self.config.system_stats_position)
            .unwrap_or(0);
        let index = Rc::new(Cell::new(start_index));
        let roots = Rc::new(roots);
        let suppress_until = self.activity_suppressed_until.clone();
        let interval = self.config.system_stats_move_interval_seconds.max(1);
        let source_id = glib::timeout_add_local(Duration::from_secs(interval), move || {
            let next = (index.get() + 1) % CLOCK_MOVE_POSITIONS.len();
            index.set(next);
            suppress_until.set(Some(Instant::now() + Duration::from_millis(250)));
            for root in roots.iter() {
                Self::apply_clock_position_widget(root, CLOCK_MOVE_POSITIONS[next], 24);
            }
            glib::ControlFlow::Continue
        });
        *self.system_stats_move_source.borrow_mut() = Some(source_id);
    }

    fn apply_now_playing_to_widgets(
        widget: &NowPlayingWidgets,
        info: Option<&crate::mpris::NowPlayingInfo>,
    ) {
        if let Some(info) = info {
            let title = info.title.trim();
            let artist = info.artist.trim();
            let album = info.album.trim();

            widget
                .root
                .set_visible(!title.is_empty() || !artist.is_empty());
            widget.title.set_text(title);

            let mut meta = String::new();
            if !artist.is_empty() {
                meta.push_str(artist);
            }
            if !album.is_empty() {
                if !meta.is_empty() {
                    meta.push_str(" — ");
                }
                meta.push_str(album);
            }
            widget.meta.set_text(&meta);

            let art_url = info
                .art_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let mut last = widget.last_art_url.borrow_mut();
            if last.as_deref() != art_url {
                *last = art_url.map(|s| s.to_string());
                Self::set_picture_from_art_url(&widget.art, art_url);
            }
        } else {
            widget.root.set_visible(false);
        }
    }

    fn set_picture_from_art_url(picture: &Picture, art_url: Option<&str>) {
        let Some(url) = art_url else {
            picture.set_paintable(gdk4::Paintable::NONE);
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

        picture.set_paintable(gdk4::Paintable::NONE);
    }

    fn attach_now_playing_overlay(
        window: &ApplicationWindow,
        position: ClockPosition,
    ) -> Option<NowPlayingWidgets> {
        let child = window.child()?;
        let overlay = match child.clone().downcast::<Overlay>() {
            Ok(overlay) => overlay,
            Err(_) => {
                window.set_child(None::<&gtk4::Widget>);
                let overlay = Overlay::new();
                overlay.set_child(Some(&child));
                window.set_child(Some(&overlay));
                overlay
            }
        };

        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        root.add_css_class("osd");
        root.set_visible(false);
        Self::apply_clock_position_widget(&root, position, 24);

        let art = Picture::new();
        art.set_size_request(96, 96);
        art.set_can_shrink(false);
        art.set_keep_aspect_ratio(true);
        art.set_content_fit(ContentFit::Cover);

        let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        text_box.set_hexpand(true);

        let title = Label::new(None);
        title.add_css_class("title-3");
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_wrap_mode(pango::WrapMode::WordChar);
        title.set_max_width_chars(60);

        let meta = Label::new(None);
        meta.add_css_class("caption");
        meta.set_xalign(0.0);
        meta.set_wrap(true);
        meta.set_wrap_mode(pango::WrapMode::WordChar);
        meta.set_max_width_chars(60);
        meta.set_opacity(0.85);

        text_box.append(&title);
        text_box.append(&meta);

        root.append(&art);
        root.append(&text_box);

        overlay.add_overlay(&root);

        Some(NowPlayingWidgets {
            root,
            art,
            title,
            meta,
            last_art_url: Rc::new(RefCell::new(None)),
        })
    }

    fn attach_rss_ticker_overlay(window: &ApplicationWindow) -> Option<RssTickerWidgets> {
        let child = window.child()?;
        let overlay = match child.clone().downcast::<Overlay>() {
            Ok(overlay) => overlay,
            Err(_) => {
                window.set_child(None::<&gtk4::Widget>);
                let overlay = Overlay::new();
                overlay.set_child(Some(&child));
                window.set_child(Some(&overlay));
                overlay
            }
        };

        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        root.set_halign(Align::Fill);
        root.set_valign(Align::End);
        root.set_hexpand(true);
        root.set_margin_start(24);
        root.set_margin_end(24);
        root.set_margin_bottom(24);
        root.set_overflow(Overflow::Hidden);
        root.add_css_class("osd");
        root.set_visible(false);
        root.set_can_target(false);
        root.set_focusable(false);

        let fixed = Fixed::new();
        fixed.set_hexpand(true);
        fixed.set_vexpand(false);
        fixed.set_overflow(Overflow::Hidden);
        fixed.set_can_target(false);
        fixed.set_focusable(false);

        let label = Label::new(None);
        label.add_css_class("caption");
        label.set_xalign(0.0);
        label.set_wrap(false);
        label.set_single_line_mode(true);
        label.set_can_target(false);
        label.set_focusable(false);
        label.set_margin_top(6);
        label.set_margin_bottom(6);

        fixed.put(&label, 0.0, 0.0);
        root.append(&fixed);
        overlay.add_overlay(&root);

        Some(RssTickerWidgets {
            root,
            fixed,
            label,
            x: Rc::new(Cell::new(0.0)),
        })
    }

    fn attach_system_stats_overlay(
        window: &ApplicationWindow,
        position: ClockPosition,
    ) -> Option<SystemStatsWidgets> {
        let child = window.child()?;
        let overlay = match child.clone().downcast::<Overlay>() {
            Ok(overlay) => overlay,
            Err(_) => {
                window.set_child(None::<&gtk4::Widget>);
                let overlay = Overlay::new();
                overlay.set_child(Some(&child));
                window.set_child(Some(&overlay));
                overlay
            }
        };

        let history: Rc<RefCell<Vec<crate::sysstats::SystemStatsSample>>> =
            Rc::new(RefCell::new(Vec::new()));

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        root.add_css_class("osd");
        root.set_can_target(false);
        root.set_focusable(false);
        root.set_visible(true);
        Self::apply_clock_position_widget(&root, position, 24);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        header.set_hexpand(true);

        let cpu_label = Label::new(Some("CPU   0%"));
        cpu_label.add_css_class("caption");
        cpu_label.add_css_class("monospace");
        cpu_label.set_xalign(0.0);
        cpu_label.set_can_target(false);
        cpu_label.set_focusable(false);

        let ram_label = Label::new(Some("RAM   0%"));
        ram_label.add_css_class("caption");
        ram_label.add_css_class("monospace");
        ram_label.set_xalign(0.0);
        ram_label.set_can_target(false);
        ram_label.set_focusable(false);

        header.append(&cpu_label);
        header.append(&ram_label);

        let graph = DrawingArea::new();
        graph.set_size_request(260, 140);
        graph.set_can_target(false);
        graph.set_focusable(false);

        let history_for_draw = Rc::clone(&history);
        graph.set_draw_func(move |_, cr, width, height| {
            let width = width.max(1) as f64;
            let height = height.max(1) as f64;

            cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
            let _ = cr.paint();

            // Scanlines
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.08);
            let mut y = 0.0;
            while y < height {
                cr.rectangle(0.0, y, width, 1.0);
                y += 2.0;
            }
            let _ = cr.fill();

            // Border
            cr.set_source_rgba(0.0, 1.0, 0.4, 0.18);
            cr.set_line_width(1.0);
            cr.rectangle(0.5, 0.5, (width - 1.0).max(0.0), (height - 1.0).max(0.0));
            let _ = cr.stroke();

            let samples = history_for_draw.borrow();
            if samples.len() < 2 {
                return;
            }

            let section_h = height / 2.0;
            Self::draw_retro_series(
                cr,
                &samples,
                width,
                0.0,
                section_h,
                |s| s.cpu_usage,
                (0.1, 1.0, 0.3),
            );
            Self::draw_retro_series(
                cr,
                &samples,
                width,
                section_h,
                section_h,
                |s| s.ram_usage,
                (0.1, 0.9, 0.8),
            );
        });

        root.append(&header);
        root.append(&graph);
        overlay.add_overlay(&root);

        Some(SystemStatsWidgets {
            root,
            cpu_label,
            ram_label,
            graph,
            history,
        })
    }

    fn draw_retro_series(
        cr: &cairo::Context,
        samples: &[crate::sysstats::SystemStatsSample],
        width: f64,
        top: f64,
        height: f64,
        value: impl Fn(&crate::sysstats::SystemStatsSample) -> f64,
        color: (f64, f64, f64),
    ) {
        let width = width.max(1.0);
        let height = height.max(1.0);

        // Grid
        cr.set_source_rgba(color.0, color.1, color.2, 0.10);
        cr.set_line_width(1.0);
        for i in 1..4 {
            let y = top + height * (i as f64 / 4.0);
            cr.move_to(0.0, y);
            cr.line_to(width, y);
        }
        for i in 1..10 {
            let x = width * (i as f64 / 10.0);
            cr.move_to(x, top);
            cr.line_to(x, top + height);
        }
        let _ = cr.stroke();

        let max_points = 120usize;
        let n = samples.len().min(max_points);
        if n < 2 {
            return;
        }
        let step = if max_points > 1 {
            width / (max_points.saturating_sub(1) as f64)
        } else {
            width
        };
        let start_x = width - (n.saturating_sub(1) as f64) * step;
        let slice = &samples[samples.len() - n..];

        // Filled area
        cr.set_source_rgba(color.0, color.1, color.2, 0.10);
        cr.new_path();
        for (i, s) in slice.iter().enumerate() {
            let v = value(s).clamp(0.0, 1.0);
            let x = start_x + (i as f64) * step;
            let y = top + (1.0 - v) * height;
            if i == 0 {
                cr.move_to(x, y);
            } else {
                cr.line_to(x, y);
            }
        }
        cr.line_to(start_x + ((n - 1) as f64) * step, top + height);
        cr.line_to(start_x, top + height);
        cr.close_path();
        let _ = cr.fill();

        // Line
        cr.set_source_rgba(color.0, color.1, color.2, 0.90);
        cr.set_line_width(1.6);
        cr.new_path();
        for (i, s) in slice.iter().enumerate() {
            let v = value(s).clamp(0.0, 1.0);
            let x = start_x + (i as f64) * step;
            let y = top + (1.0 - v) * height;
            if i == 0 {
                cr.move_to(x, y);
            } else {
                cr.line_to(x, y);
            }
        }
        let _ = cr.stroke();
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
        label.set_justify(Justification::Center);
        label.set_wrap(true);
        label.set_wrap_mode(pango::WrapMode::WordChar);
        label.set_max_width_chars(40);
        label.set_text(&Self::format_clock_text(format));
        Self::apply_clock_size(&label, size);
        Self::apply_clock_position(&label, position, 24);
        overlay.add_overlay(&label);

        window.set_child(Some(&overlay));
        Some(label)
    }

    fn attach_clock_overlay_two_lines(
        window: &ApplicationWindow,
        time_format: &str,
        date_format: &str,
        position: ClockPosition,
        size: u32,
    ) -> (Option<Label>, Option<Label>) {
        let Some(child) = window.child() else {
            return (None, None);
        };
        window.set_child(None::<&gtk4::Widget>);

        let overlay = Overlay::new();
        overlay.set_child(Some(&child));

        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 2);

        let time_label = Label::new(Some(&Self::format_clock_text(time_format)));
        time_label.add_css_class("title-1");
        time_label.set_opacity(0.92);
        time_label.set_justify(Justification::Center);

        let date_label = Label::new(Some(&Self::format_clock_text(date_format)));
        date_label.add_css_class("title-3");
        date_label.set_opacity(0.85);
        date_label.set_justify(Justification::Center);

        Self::apply_clock_size(&time_label, size);
        Self::apply_clock_size(&date_label, (size.saturating_mul(55) / 100).max(8));

        container.append(&time_label);
        container.append(&date_label);

        Self::apply_clock_position_widget(&container, position, 24);
        overlay.add_overlay(&container);

        window.set_child(Some(&overlay));
        (Some(time_label), Some(date_label))
    }

    fn apply_clock_position_widget(
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

    fn apply_clock_position(label: &Label, position: ClockPosition, margin: i32) {
        Self::apply_clock_position_widget(label, position, margin);
        match position {
            ClockPosition::TopLeft => {
                label.set_xalign(0.0);
                label.set_yalign(0.0);
            }
            ClockPosition::TopCenter => {
                label.set_xalign(0.5);
                label.set_yalign(0.0);
            }
            ClockPosition::TopRight => {
                label.set_xalign(1.0);
                label.set_yalign(0.0);
            }
            ClockPosition::CenterLeft => {
                label.set_xalign(0.0);
                label.set_yalign(0.5);
            }
            ClockPosition::Center => {
                label.set_xalign(0.5);
                label.set_yalign(0.5);
            }
            ClockPosition::CenterRight => {
                label.set_xalign(1.0);
                label.set_yalign(0.5);
            }
            ClockPosition::BottomLeft => {
                label.set_xalign(0.0);
                label.set_yalign(1.0);
            }
            ClockPosition::BottomCenter => {
                label.set_xalign(0.5);
                label.set_yalign(1.0);
            }
            ClockPosition::BottomRight => {
                label.set_xalign(1.0);
                label.set_yalign(1.0);
            }
        }
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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
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

    fn is_allowed_video_ext(ext: &str) -> bool {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "mp4"
                | "mkv"
                | "webm"
                | "avi"
                | "mov"
                | "m4v"
                | "mpg"
                | "mpeg"
                | "ogv"
                | "wmv"
                | "flv"
                | "m2ts"
                | "mts"
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
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
        Some(RGBA::new(r, g, b, 1.0))
    }
}
