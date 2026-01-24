mod apps;
mod autostart;
mod battery;
mod config;
mod desktop;
mod i18n;
mod idle;
mod monitors;
mod mpris;
mod rss;
mod sysstats;
mod tray;
mod ui;
mod wayland_lock;

use gio::{ApplicationFlags, Notification, SimpleAction};
use gtk4::prelude::*;
use gtk4::{Align, Application, Box as GtkBox, Orientation};
use ksni::TrayMethods;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zbus::blocking::{
    Connection as DbusConnection, ConnectionBuilder as DbusConnectionBuilder, Proxy as DbusProxy,
};
use zbus::interface;
use zbus::zvariant::OwnedObjectPath;

use config::{
    ActivationLogEntry, AnimatedPattern, Config, ScreensaverMode, SettingsProfile, ACTION_PANIC,
    ACTION_START, ACTION_STOP,
};
use i18n::{cli_usage, resolve_language, system_language, tr, yes_no, Language};
use idle::IdleMonitor;
use tray::TrayHandler;
use ui::{ScreensaverWindow, SettingsWindow};

pub enum AppMessage {
    IdleThresholdReached,
    OpenSettings,
    StartScreensaver,
    StopScreensaver,
    PanicStopScreensaver,
    StopScreensaverUserActivity,
    BatteryStateChanged(bool),
    SwitchProfile(u8),
    ToggleEnabled(bool),
    ToggleInhibitSleep(bool),
    ShowMainWindow,
    UpdateConfig(Config),
    Quit,
}

#[derive(Clone, Copy)]
enum MediaKind {
    Image,
    Video,
}

#[derive(Clone, Copy)]
enum StopReason {
    Requested,
    UserActivity,
}

const APP_ID: &str = "com.example.vesper";
const KDE_COMPONENT_FRIENDLY: &str = "Vesper";
const PROBLEM_NOTIFY_COOLDOWN: Duration = Duration::from_secs(10);
const DBUS_SERVICE_NAME: &str = "com.example.RSScreensaver";
const DBUS_OBJECT_PATH: &str = "/com/example/RSScreensaver";
const DBUS_INTERFACE_NAME: &str = "com.example.RSScreensaver.Control";
const CLI_AUTOSTART_TIMEOUT: Duration = Duration::from_secs(3);

struct AppState {
    config: Arc<Mutex<Config>>,
    screensaver_active: Arc<Mutex<bool>>,
    screensaver_started_at: Arc<Mutex<Option<Instant>>>,
    screensaver_windows: Arc<Mutex<Option<Vec<ScreensaverWindow>>>>,
    wayland_lock: Arc<Mutex<Option<wayland_lock::SessionLockController>>>,
    on_battery: Arc<Mutex<bool>>,
    battery_forced_black: Arc<Mutex<bool>>,
    main_window: Arc<Mutex<Option<adw::ApplicationWindow>>>,
    is_enabled: Arc<Mutex<bool>>,
    inhibit_sleep: Arc<Mutex<bool>>,
    inhibit_cookie: Arc<Mutex<Option<u32>>>,
    last_problem_notification: Arc<Mutex<Option<(String, Instant)>>>,
    last_warning_notification: Arc<Mutex<Option<(String, Instant)>>>,
    power_inhibit_state: Arc<Mutex<PowerInhibitState>>,
    status: Arc<Mutex<StatusSnapshot>>,
    paused_players: Arc<Mutex<Vec<String>>>,
    sender: mpsc::Sender<AppMessage>,
}

#[derive(Default)]
struct PowerInhibitState {
    gnome_cookie: Option<u32>,
    kde_cookie: Option<u32>,
}

#[derive(Clone)]
struct StatusSnapshot {
    screensaver_active: bool,
    enabled: bool,
    inhibit_sleep: bool,
    active_profile: u8,
    profile_name: String,
    mode: String,
}

impl StatusSnapshot {
    fn from_config(config: &Config, enabled: bool, inhibit_sleep: bool, lang: Language) -> Self {
        let profile = config.active_profile();
        Self {
            screensaver_active: false,
            enabled,
            inhibit_sleep,
            active_profile: config.active_profile,
            profile_name: profile.name.clone(),
            mode: format_activation_mode(profile, lang),
        }
    }
}

struct ControlInterface {
    sender: mpsc::Sender<AppMessage>,
    status: Arc<Mutex<StatusSnapshot>>,
}

#[interface(name = "com.example.RSScreensaver.Control")]
impl ControlInterface {
    fn start(&self) -> zbus::fdo::Result<()> {
        send_app_message(&self.sender, AppMessage::StartScreensaver)
    }

    fn stop(&self) -> zbus::fdo::Result<()> {
        send_app_message(&self.sender, AppMessage::StopScreensaver)
    }

    fn show_settings(&self) -> zbus::fdo::Result<()> {
        send_app_message(&self.sender, AppMessage::OpenSettings)
    }

    fn show_main_window(&self) -> zbus::fdo::Result<()> {
        send_app_message(&self.sender, AppMessage::ShowMainWindow)
    }

    fn set_enabled(&self, enabled: bool) -> zbus::fdo::Result<()> {
        send_app_message(&self.sender, AppMessage::ToggleEnabled(enabled))
    }

    fn set_inhibit_sleep(&self, inhibit: bool) -> zbus::fdo::Result<()> {
        send_app_message(&self.sender, AppMessage::ToggleInhibitSleep(inhibit))
    }

    fn switch_profile(&self, index: u8) -> zbus::fdo::Result<()> {
        send_app_message(&self.sender, AppMessage::SwitchProfile(index))
    }

    fn quit(&self) -> zbus::fdo::Result<()> {
        send_app_message(&self.sender, AppMessage::Quit)
    }

    fn status(&self) -> zbus::fdo::Result<(bool, bool, bool, u8, String, String)> {
        let snapshot = self.status.lock().unwrap().clone();
        Ok((
            snapshot.screensaver_active,
            snapshot.enabled,
            snapshot.inhibit_sleep,
            snapshot.active_profile,
            snapshot.profile_name,
            snapshot.mode,
        ))
    }
}

fn main() {
    if let Some(exit_code) = handle_cli_command() {
        std::process::exit(exit_code);
    }

    suppress_gtk_image_baseline_warning();

    let app = Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::default())
        .build();

    let config = Config::load();
    autostart::migrate_legacy_autostart();
    let inhibit_sleep_initial = config.active_profile().inhibit_sleep;
    let lang = resolve_language(config.language);
    let status = Arc::new(Mutex::new(StatusSnapshot::from_config(
        &config,
        true,
        inhibit_sleep_initial,
        lang,
    )));

    let (sender, receiver) = mpsc::channel();
    let state = Arc::new(AppState {
        config: Arc::new(Mutex::new(config)),
        screensaver_active: Arc::new(Mutex::new(false)),
        screensaver_started_at: Arc::new(Mutex::new(None)),
        screensaver_windows: Arc::new(Mutex::new(None)),
        wayland_lock: Arc::new(Mutex::new(None)),
        on_battery: Arc::new(Mutex::new(false)),
        battery_forced_black: Arc::new(Mutex::new(false)),
        main_window: Arc::new(Mutex::new(None)),
        is_enabled: Arc::new(Mutex::new(true)),
        inhibit_sleep: Arc::new(Mutex::new(inhibit_sleep_initial)),
        inhibit_cookie: Arc::new(Mutex::new(None)),
        last_problem_notification: Arc::new(Mutex::new(None)),
        last_warning_notification: Arc::new(Mutex::new(None)),
        power_inhibit_state: Arc::new(Mutex::new(PowerInhibitState::default())),
        status: Arc::clone(&status),
        paused_players: Arc::new(Mutex::new(Vec::new())),
        sender: sender.clone(),
    });
    setup_dbus_interface(sender.clone(), Arc::clone(&state.status));

    // Apply initial inhibit state
    if inhibit_sleep_initial {
        set_inhibit_sleep(Arc::clone(&state), true);
    }

    battery::spawn_battery_monitor(sender.clone());

    let state_clone = Arc::clone(&state);
    let state_startup = Arc::clone(&state);
    let startup_sender = sender.clone();
    app.connect_startup(move |app| {
        let prefer_dark = drain_gtk_prefer_dark_setting();
        adw::init().expect("Failed to initialize Libadwaita");
        apply_adwaita_color_scheme(prefer_dark);
        setup_hotkeys(app, Arc::clone(&state_startup), startup_sender.clone());
    });

    let activate_sender = sender.clone();
    app.connect_activate(move |app| {
        setup_app(app, Arc::clone(&state_clone), activate_sender.clone());
    });

    // Tray
    let tray_sender = sender.clone();
    let tray_enabled = Arc::clone(&state.is_enabled);
    let tray_inhibit = Arc::clone(&state.inhibit_sleep);
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tray = TrayHandler::new(tray_sender, tray_enabled, tray_inhibit);
            if let Err(e) = tray.spawn().await {
                eprintln!("System tray not available: {}", e);
            } else {
                std::future::pending::<()>().await;
            }
        });
    });

    // Activity monitor using Wayland ext-idle-notify
    let monitor_config = Arc::clone(&state.config);
    let monitor_screensaver_active = Arc::clone(&state.screensaver_active);
    let monitor_is_enabled = Arc::clone(&state.is_enabled);
    let monitor_sender = sender.clone();

    thread::spawn(move || {
        let idle_monitor = IdleMonitor::new();
        loop {
            let screensaver_active = *monitor_screensaver_active.lock().unwrap();
            let is_idle = idle_monitor.as_ref().map(|m| m.is_idle()).unwrap_or(false);

            if screensaver_active {
                // Rely on screensaver window input handlers to stop; idle monitor is for auto-start.
            } else {
                // Check idle threshold
                let is_enabled = *monitor_is_enabled.lock().unwrap();
                if is_enabled && is_idle {
                    let threshold_secs = monitor_config
                        .lock()
                        .unwrap()
                        .active_profile()
                        .inactivity_seconds;
                    let idle_ms = idle_monitor.as_ref().map(|m| m.get_idle_ms()).unwrap_or(0);
                    if idle_ms >= (threshold_secs as u64) * 1000 {
                        let _ = monitor_sender.send(AppMessage::IdleThresholdReached);
                        thread::sleep(Duration::from_millis(1000));
                    }
                }
            }

            thread::sleep(Duration::from_millis(100));
        }
    });

    // Message handler
    let state_receiver = Arc::clone(&state);
    glib::timeout_add_local(Duration::from_millis(200), move || {
        while let Ok(msg) = receiver.try_recv() {
            match msg {
                AppMessage::IdleThresholdReached => {
                    let config = state_receiver.config.lock().unwrap().clone();
                    let inhibit_apps = &config.active_profile().app_inhibit_list;
                    if apps::any_inhibit_app_running(inhibit_apps) {
                        continue;
                    }
                    start_screensaver(Arc::clone(&state_receiver));
                }
                AppMessage::OpenSettings => {
                    if let Some(main_window) = state_receiver.main_window.lock().unwrap().as_ref() {
                        if let Some(app) = main_window.application() {
                            let config = Config::load();
                            *state_receiver.config.lock().unwrap() = config.clone();
                            let settings = SettingsWindow::new(&app, config, sender.clone());
                            settings.show();
                        }
                    }
                }
                AppMessage::StartScreensaver => {
                    start_screensaver(Arc::clone(&state_receiver));
                }
                AppMessage::StopScreensaver => {
                    stop_screensaver(Arc::clone(&state_receiver), StopReason::Requested);
                }
                AppMessage::PanicStopScreensaver => {
                    stop_screensaver(Arc::clone(&state_receiver), StopReason::Requested);
                    let active = Arc::clone(&state_receiver.screensaver_active);
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(800));
                        let should_exit = match active.try_lock() {
                            Ok(guard) => *guard,
                            Err(_) => true,
                        };
                        if should_exit {
                            eprintln!("Panic hotkey: forcing process exit");
                            std::process::exit(1);
                        }
                    });
                }
                AppMessage::StopScreensaverUserActivity => {
                    stop_screensaver(Arc::clone(&state_receiver), StopReason::UserActivity);
                }
                AppMessage::BatteryStateChanged(on_battery) => {
                    *state_receiver.on_battery.lock().unwrap() = on_battery;
                    if *state_receiver.screensaver_active.lock().unwrap() {
                        handle_battery_state_change(Arc::clone(&state_receiver), on_battery);
                    }
                }
                AppMessage::ToggleEnabled(enabled) => {
                    *state_receiver.is_enabled.lock().unwrap() = enabled;
                    if let Ok(mut status) = state_receiver.status.lock() {
                        status.enabled = enabled;
                    }
                }
                AppMessage::ToggleInhibitSleep(inhibit) => {
                    set_inhibit_sleep(Arc::clone(&state_receiver), inhibit);
                    // Save to config
                    let mut config = state_receiver.config.lock().unwrap();
                    config.active_profile_mut().inhibit_sleep = inhibit;
                    let _ = config.save();
                }
                AppMessage::UpdateConfig(config) => {
                    let lang = resolve_language(config.language);
                    *state_receiver.config.lock().unwrap() = config.clone();
                    if let Ok(mut status) = state_receiver.status.lock() {
                        update_status_profile(&mut status, &config, lang);
                    }
                    if let Some(main_window) = state_receiver.main_window.lock().unwrap().as_ref() {
                        if let Some(app) = main_window.application() {
                            apply_app_hotkeys(&app, &config);
                        }
                    }
                }
                AppMessage::SwitchProfile(index) => {
                    let mut config = Config::load();
                    config.set_active_profile(index as usize);
                    let _ = config.save();
                    let inhibit = config.active_profile().inhibit_sleep;
                    let lang = resolve_language(config.language);
                    *state_receiver.config.lock().unwrap() = config.clone();
                    set_inhibit_sleep(Arc::clone(&state_receiver), inhibit);
                    if let Ok(mut status) = state_receiver.status.lock() {
                        update_status_profile(&mut status, &config, lang);
                    }
                }
                AppMessage::ShowMainWindow => {
                    if let Some(main_window) = state_receiver.main_window.lock().unwrap().as_ref() {
                        main_window.present();
                    }
                }
                AppMessage::Quit => {
                    stop_screensaver(Arc::clone(&state_receiver), StopReason::Requested);
                    if let Some(main_window) = state_receiver.main_window.lock().unwrap().as_ref() {
                        if let Some(app) = main_window.application() {
                            app.quit();
                        }
                    }
                }
            }
        }
        glib::ControlFlow::Continue
    });

    app.run();
}

fn setup_app(app: &Application, state: Arc<AppState>, sender: mpsc::Sender<AppMessage>) {
    ui::init_app_icon_theme();
    let (lang, start_minimized) = {
        let config = state.config.lock().unwrap();
        (resolve_language(config.language), config.start_minimized)
    };
    let app_icon = ui::app_icon_name();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Vesper")
        .default_width(400)
        .default_height(500)
        .build();
    window.set_icon_name(Some(app_icon));

    let content_box = GtkBox::new(Orientation::Vertical, 0);
    let header = adw::HeaderBar::new();

    let settings_btn = gtk4::Button::new();
    settings_btn.set_icon_name("emblem-system-symbolic");
    settings_btn.set_valign(gtk4::Align::Center);
    settings_btn.set_tooltip_text(Some(tr(lang, "Настройки")));
    let app_weak = app.downgrade();
    let state_settings = Arc::clone(&state);
    let sender_settings = sender.clone();
    settings_btn.connect_clicked(move |_| {
        if let Some(app) = app_weak.upgrade() {
            let config = Config::load();
            *state_settings.config.lock().unwrap() = config.clone();
            let settings = SettingsWindow::new(&app, config, sender_settings.clone());
            settings.show();
        }
    });
    header.pack_end(&settings_btn);
    content_box.append(&header);

    let status_page = adw::StatusPage::builder()
        .icon_name(app_icon)
        .title("Vesper")
        .description(tr(lang, "Программа работает в фоновом режиме"))
        .build();

    let buttons_box = GtkBox::new(Orientation::Vertical, 12);
    buttons_box.set_halign(Align::Center);
    buttons_box.set_margin_bottom(30);

    let start_btn = gtk4::Button::builder()
        .label(tr(lang, "Запустить сейчас"))
        .css_classes(["pill", "suggested-action"])
        .width_request(200)
        .height_request(50)
        .build();

    let state_start = Arc::clone(&state);
    start_btn.connect_clicked(move |_| {
        start_screensaver(Arc::clone(&state_start));
    });

    let quit_btn = gtk4::Button::builder()
        .label(tr(lang, "Выход"))
        .css_classes(["pill", "destructive-action"])
        .width_request(200)
        .build();

    let app_weak = app.downgrade();
    quit_btn.connect_clicked(move |_| {
        if let Some(app) = app_weak.upgrade() {
            app.quit();
        }
    });

    buttons_box.append(&start_btn);
    buttons_box.append(&quit_btn);
    status_page.set_child(Some(&buttons_box));
    content_box.append(&status_page);

    window.set_content(Some(&content_box));

    window.connect_close_request(move |window| {
        window.set_visible(false);
        glib::Propagation::Stop
    });

    *state.main_window.lock().unwrap() = Some(window.clone());
    if !start_minimized {
        window.present();
    }
}

fn setup_hotkeys(app: &Application, state: Arc<AppState>, sender: mpsc::Sender<AppMessage>) {
    let lang = resolve_language(state.config.lock().unwrap().language);
    let start_action = SimpleAction::new(ACTION_START, None);
    let stop_action = SimpleAction::new(ACTION_STOP, None);
    let panic_action = SimpleAction::new(ACTION_PANIC, None);

    {
        let state = Arc::clone(&state);
        start_action.connect_activate(move |_, _| {
            start_screensaver(Arc::clone(&state));
        });
    }
    {
        let state = Arc::clone(&state);
        stop_action.connect_activate(move |_, _| {
            stop_screensaver(Arc::clone(&state), StopReason::Requested);
        });
    }
    {
        let sender = sender.clone();
        panic_action.connect_activate(move |_, _| {
            let _ = sender.send(AppMessage::PanicStopScreensaver);
        });
    }

    app.add_action(&start_action);
    app.add_action(&stop_action);
    app.add_action(&panic_action);
    let config = state.config.lock().unwrap().clone();
    apply_app_hotkeys(app, &config);

    setup_kde_global_hotkeys(sender, lang);
}

fn setup_dbus_interface(sender: mpsc::Sender<AppMessage>, status: Arc<Mutex<StatusSnapshot>>) {
    thread::spawn(move || {
        if let Err(err) = register_dbus_interface(sender, status) {
            eprintln!("D-Bus interface not available: {}", err);
        }
    });
}

fn register_dbus_interface(
    sender: mpsc::Sender<AppMessage>,
    status: Arc<Mutex<StatusSnapshot>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let iface = ControlInterface { sender, status };
    let _conn = DbusConnectionBuilder::session()?
        .name(DBUS_SERVICE_NAME)?
        .serve_at(DBUS_OBJECT_PATH, iface)?
        .build()?;
    loop {
        thread::park();
    }
}

fn handle_cli_command() -> Option<i32> {
    let mut args = collect_cli_args()?;
    let Some(cmd) = args.pop() else {
        return None;
    };

    if matches!(cmd.as_str(), "-h" | "--help" | "help") {
        print_cli_usage();
        return Some(0);
    }

    let command = cmd.to_lowercase();
    let lang = cli_language();
    if matches!(command.as_str(), "status" | "--status") {
        return Some(handle_status_cli());
    }

    let result = match command.as_str() {
        "start" => run_cli_with_service(lang, || send_dbus_command("start", (), lang)),
        "stop" => run_cli_with_service(lang, || send_dbus_command("stop", (), lang)),
        "show-settings" | "settings" => {
            run_cli_with_service(lang, || send_dbus_command("show_settings", (), lang))
        }
        "show" | "show-main" | "main" => {
            run_cli_with_service(lang, || send_dbus_command("show_main_window", (), lang))
        }
        "enable" => run_cli_with_service(lang, || send_dbus_command("set_enabled", (true,), lang)),
        "disable" => {
            run_cli_with_service(lang, || send_dbus_command("set_enabled", (false,), lang))
        }
        "inhibit" => run_cli_with_service(lang, || {
            send_dbus_command("set_inhibit_sleep", (true,), lang)
        }),
        "uninhibit" => run_cli_with_service(lang, || {
            send_dbus_command("set_inhibit_sleep", (false,), lang)
        }),
        "set-enabled" => {
            let value = args.pop().unwrap_or_default();
            match parse_bool_arg(&value, lang) {
                Ok(enabled) => run_cli_with_service(lang, || {
                    send_dbus_command("set_enabled", (enabled,), lang)
                }),
                Err(err) => Err(err),
            }
        }
        "set-inhibit" => {
            let value = args.pop().unwrap_or_default();
            match parse_bool_arg(&value, lang) {
                Ok(inhibit) => run_cli_with_service(lang, || {
                    send_dbus_command("set_inhibit_sleep", (inhibit,), lang)
                }),
                Err(err) => Err(err),
            }
        }
        "switch-profile" | "profile" => {
            let index = match args.pop().and_then(|v| v.parse::<u8>().ok()) {
                Some(index) => index,
                None => return Some(print_cli_error(tr(lang, "Укажите индекс профиля (0-254)."))),
            };
            run_cli_with_service(lang, || send_dbus_command("switch_profile", (index,), lang))
        }
        "quit" | "exit" => run_cli_with_service(lang, || send_dbus_command("quit", (), lang)),
        _ => {
            print_cli_usage();
            return Some(1);
        }
    };

    match result {
        Ok(()) => Some(0),
        Err(err) => Some(print_cli_error(&err)),
    }
}

fn collect_cli_args() -> Option<Vec<String>> {
    let mut args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| !is_gapplication_arg(arg))
        .collect();
    if args.is_empty() {
        None
    } else {
        args.reverse();
        Some(args)
    }
}

fn is_gapplication_arg(arg: &str) -> bool {
    arg == "--gapplication-service" || arg.starts_with("--gapplication-app-id")
}

fn run_cli_with_service<F>(lang: Language, action: F) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    ensure_service_running(CLI_AUTOSTART_TIMEOUT, lang)?;
    action()
}

fn send_dbus_command<T>(method: &str, body: T, lang: Language) -> Result<(), String>
where
    T: zbus::zvariant::Type + serde::ser::Serialize,
{
    let conn = DbusConnection::session().map_err(|err| err.to_string())?;
    let proxy = DbusProxy::new(
        &conn,
        DBUS_SERVICE_NAME,
        DBUS_OBJECT_PATH,
        DBUS_INTERFACE_NAME,
    )
    .map_err(|err| err.to_string())?;
    call_dbus_method::<(), T>(&proxy, method, &body, lang).map(|_| ())
}

fn call_dbus_method<R, T>(
    proxy: &DbusProxy,
    method: &str,
    body: &T,
    lang: Language,
) -> Result<R, String>
where
    R: zbus::zvariant::Type + serde::de::DeserializeOwned,
    T: zbus::zvariant::Type + serde::ser::Serialize,
{
    let canonical = to_dbus_member_name(method);
    let mut candidates = Vec::new();
    candidates.push(canonical.clone());
    if canonical != method {
        candidates.push(method.to_string());
    }
    for (index, name) in candidates.iter().enumerate() {
        match proxy.call::<_, _, R>(name.as_str(), body) {
            Ok(value) => return Ok(value),
            Err(err) => {
                if is_unknown_method_error(&err) && index + 1 < candidates.len() {
                    continue;
                }
                return Err(err.to_string());
            }
        }
    }
    Err(tr(lang, "Не удалось выполнить D-Bus команду").to_string())
}

fn is_unknown_method_error(err: &zbus::Error) -> bool {
    matches!(
        err,
        zbus::Error::MethodError(name, _, _) if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod"
    )
}

fn to_dbus_member_name(method: &str) -> String {
    let mut out = String::new();
    let mut capitalize = true;
    for ch in method.chars() {
        if ch == '_' || ch == '-' {
            capitalize = true;
            continue;
        }
        if capitalize {
            for upper in ch.to_uppercase() {
                out.push(upper);
            }
            capitalize = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn ensure_service_running(timeout: Duration, lang: Language) -> Result<(), String> {
    if is_service_running()? {
        return Ok(());
    }
    spawn_service()?;
    let start = Instant::now();
    while start.elapsed() < timeout {
        if is_service_running()? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(tr(lang, "Не удалось запустить сервис").to_string())
}

fn spawn_service() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    std::process::Command::new(exe)
        .spawn()
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn is_service_running() -> Result<bool, String> {
    let conn = DbusConnection::session().map_err(|err| err.to_string())?;
    let proxy = DbusProxy::new(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .map_err(|err| err.to_string())?;
    proxy
        .call::<_, _, bool>("NameHasOwner", &(DBUS_SERVICE_NAME))
        .map_err(|err| err.to_string())
}

fn handle_status_cli() -> i32 {
    let lang = cli_language();
    match query_status(lang) {
        Ok(Some(status)) => {
            println!("{}", format_status_line(&status, lang));
            0
        }
        Ok(None) => {
            println!("{}", tr(lang, "Статус: не запущен"));
            2
        }
        Err(err) => print_cli_error(&err),
    }
}

fn query_status(lang: Language) -> Result<Option<StatusSnapshot>, String> {
    if !is_service_running()? {
        return Ok(None);
    }
    let conn = DbusConnection::session().map_err(|err| err.to_string())?;
    let proxy = DbusProxy::new(
        &conn,
        DBUS_SERVICE_NAME,
        DBUS_OBJECT_PATH,
        DBUS_INTERFACE_NAME,
    )
    .map_err(|err| err.to_string())?;
    let (active, enabled, inhibit, profile_index, profile_name, mode) =
        call_dbus_method::<(bool, bool, bool, u8, String, String), ()>(
            &proxy,
            "status",
            &(),
            lang,
        )?;
    Ok(Some(StatusSnapshot {
        screensaver_active: active,
        enabled,
        inhibit_sleep: inhibit,
        active_profile: profile_index,
        profile_name,
        mode,
    }))
}

fn format_status_line(status: &StatusSnapshot, lang: Language) -> String {
    let active = yes_no(lang, status.screensaver_active);
    let enabled = yes_no(lang, status.enabled);
    let inhibit = yes_no(lang, status.inhibit_sleep);
    tr(
        lang,
        "Активен: {active} • Включен: {enabled} • Сон: {inhibit} • Профиль: {} ({}) • Режим: {}",
    )
    .replace("{active}", active)
    .replace("{enabled}", enabled)
    .replace("{inhibit}", inhibit)
    .replacen("{}", &status.active_profile.to_string(), 1)
    .replacen("{}", &status.profile_name, 1)
    .replacen("{}", &status.mode, 1)
}

fn parse_bool_arg(value: &str, lang: Language) -> Result<bool, String> {
    match value.to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(tr(
            lang,
            "Неизвестное значение: {value} (ожидается true/false).",
        )
        .replace("{value}", value)),
    }
}

fn print_cli_usage() {
    println!("{}", cli_usage(cli_language()));
}

fn print_cli_error(message: &str) -> i32 {
    let lang = cli_language();
    eprintln!(
        "{}",
        tr(lang, "Ошибка: {message}").replace("{message}", message)
    );
    1
}

fn cli_language() -> Language {
    let config = Config::load();
    resolve_language(config.language)
}

fn apply_app_hotkeys(app: &Application, config: &Config) {
    set_action_accel(app, ACTION_START, &config.hotkey_start);
    set_action_accel(app, ACTION_STOP, &config.hotkey_stop);
    set_action_accel(app, ACTION_PANIC, &config.hotkey_panic);
}

fn send_app_message(sender: &mpsc::Sender<AppMessage>, msg: AppMessage) -> zbus::fdo::Result<()> {
    sender.send(msg).map_err(|_| {
        zbus::fdo::Error::Failed(tr(system_language(), "Не удалось отправить команду").into())
    })
}

fn set_action_accel(app: &Application, action: &str, accel: &str) {
    let action_name = format!("app.{action}");
    let accel = accel.trim();
    if accel.is_empty() {
        app.set_accels_for_action(&action_name, &[]);
    } else {
        app.set_accels_for_action(&action_name, &[accel]);
    }
}

fn setup_kde_global_hotkeys(sender: mpsc::Sender<AppMessage>, lang: Language) {
    thread::spawn(move || {
        if let Err(err) = register_kde_global_hotkeys(sender, lang) {
            eprintln!("KDE global shortcuts not available: {}", err);
        }
    });
}

fn drain_gtk_prefer_dark_setting() -> bool {
    let Some(settings) = gtk4::Settings::default() else {
        return false;
    };
    let prefer_dark = settings.property::<bool>("gtk-application-prefer-dark-theme");
    if prefer_dark {
        let _ = settings.set_property("gtk-application-prefer-dark-theme", &false);
    }
    prefer_dark
}

fn apply_adwaita_color_scheme(prefer_dark: bool) {
    let style_manager = adw::StyleManager::default();
    let scheme = if prefer_dark {
        adw::ColorScheme::PreferDark
    } else {
        adw::ColorScheme::Default
    };
    style_manager.set_color_scheme(scheme);
}

fn suppress_gtk_image_baseline_warning() {
    glib::log_set_handler(
        Some("Gtk"),
        glib::LogLevels::LEVEL_WARNING,
        false,
        false,
        |domain, level, message| {
            if message.contains("GtkImage")
                && message.contains("baselines")
                && message.contains("minimum -2147483648")
            {
                return;
            }
            glib::log_default_handler(domain, level, Some(message));
        },
    );
}

fn is_kde_unknown_method(err: &zbus::Error) -> bool {
    matches!(
        err,
        zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod"
    )
}

fn register_kde_global_hotkeys(
    sender: mpsc::Sender<AppMessage>,
    lang: Language,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = DbusConnection::session()?;
    let proxy = DbusProxy::new(
        &conn,
        "org.kde.kglobalaccel",
        "/kglobalaccel",
        "org.kde.KGlobalAccel",
    )?;

    let actions = [
        (ACTION_START, tr(lang, "Запустить скринсейвер")),
        (ACTION_STOP, tr(lang, "Остановить скринсейвер")),
        (ACTION_PANIC, tr(lang, "Принудительное закрытие")),
    ];

    for (action_unique, action_friendly) in actions {
        let action_id = vec![
            APP_ID.to_string(),
            action_unique.to_string(),
            KDE_COMPONENT_FRIENDLY.to_string(),
            action_friendly.to_string(),
        ];
        let _: () = proxy.call("doRegister", &(action_id.clone(),))?;
        let keys: Vec<Vec<i32>> = Vec::new();
        let result: Result<Vec<Vec<i32>>, zbus::Error> =
            proxy.call("setShortcutKeys", &(action_id, keys, 0u32));
        if let Err(err) = result {
            if is_kde_unknown_method(&err) {
                continue;
            }
            return Err(err.into());
        }
    }

    let component_path: OwnedObjectPath = proxy.call("getComponent", &(APP_ID,))?;
    let component_proxy = DbusProxy::new(
        &conn,
        "org.kde.kglobalaccel",
        component_path,
        "org.kde.kglobalaccel.Component",
    )?;
    let mut signals =
        component_proxy.receive_signal_with_args("globalShortcutPressed", &[(0, APP_ID)])?;

    for msg in &mut signals {
        let (_, action_unique, _) = match msg.body().deserialize::<(String, String, i64)>() {
            Ok(body) => body,
            Err(_) => continue,
        };
        match action_unique.as_str() {
            ACTION_START => {
                let _ = sender.send(AppMessage::StartScreensaver);
            }
            ACTION_STOP => {
                let _ = sender.send(AppMessage::StopScreensaver);
            }
            ACTION_PANIC => {
                let _ = sender.send(AppMessage::PanicStopScreensaver);
            }
            _ => {}
        }
    }

    Ok(())
}

fn stop_screensaver(state: Arc<AppState>, reason: StopReason) {
    let mut active = state.screensaver_active.lock().unwrap();
    if !*active {
        return;
    }
    *active = false;
    drop(active);

    disable_power_integration(&state);

    {
        let mut paused = state.paused_players.lock().unwrap();
        if !paused.is_empty() {
            resume_mpris_players(paused.clone());
            paused.clear();
        }
    }

    if let Some(started_at) = state.screensaver_started_at.lock().unwrap().take() {
        let elapsed = started_at.elapsed().as_secs();
        let mut config = state.config.lock().unwrap();
        config.total_runtime_seconds = config.total_runtime_seconds.saturating_add(elapsed);
        let _ = config.save();
    }

    if let Some(lock) = state.wayland_lock.lock().unwrap().take() {
        lock.stop();
    }
    *state.battery_forced_black.lock().unwrap() = false;

    if let Some(screensavers) = state.screensaver_windows.lock().unwrap().take() {
        for screensaver in &screensavers {
            screensaver.hide();
        }
    }
    if matches!(reason, StopReason::UserActivity) {
        let profile = state.config.lock().unwrap().active_profile().clone();
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            request_screen_lock(&profile);
        });
    }

    if let Ok(mut status) = state.status.lock() {
        status.screensaver_active = false;
    }
}

fn start_screensaver(state: Arc<AppState>) {
    if *state.screensaver_active.lock().unwrap() {
        return;
    }

    let (config_snapshot, lang) = {
        let config = state.config.lock().unwrap();
        (config.clone(), resolve_language(config.language))
    };
    let profile = config_snapshot.active_profile().clone();
    let forced_black = *state.on_battery.lock().unwrap();
    *state.battery_forced_black.lock().unwrap() = forced_black;
    let mut profile_for_activation = profile.clone();
    if forced_black {
        profile_for_activation.mode = ScreensaverMode::Color("#000000".to_string());
    }

    if profile.power_integration_enabled {
        enable_power_integration(&state);
    } else {
        disable_power_integration(&state);
    }
    if profile.mpris_pause_enabled {
        request_mpris_pause(Arc::clone(&state.paused_players));
    }
    {
        let mut config = state.config.lock().unwrap();
        let entry = ActivationLogEntry {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            profile_name: profile.name.clone(),
            mode: format_activation_mode(&profile_for_activation, lang),
        };
        config.push_activation_log(entry);
        let _ = config.save();
    }
    let app = state
        .main_window
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|w| w.application());

    let started_at = Arc::clone(&state.screensaver_started_at);
    let state_clone = Arc::clone(&state);

    // Prefer ext-session-lock-v1 on Wayland when available.
    let is_wayland = gdk4::Display::default()
        .map(|d| d.backend().is_wayland())
        .unwrap_or(false);
    let try_wayland_lock = matches!(
        profile_for_activation.mode,
        ScreensaverMode::Color(_) | ScreensaverMode::Gradient { .. }
    );
    if is_wayland && try_wayland_lock {
        if let Some(lock) = wayland_lock::start_wayland_session_lock_screensaver(
            &profile_for_activation,
            state.sender.clone(),
        ) {
            *state.screensaver_active.lock().unwrap() = true;
            *state.screensaver_started_at.lock().unwrap() = Some(Instant::now());
            *state.wayland_lock.lock().unwrap() = Some(lock);

            if let Ok(mut status) = state.status.lock() {
                status.screensaver_active = true;
                status.inhibit_sleep = *state.inhibit_sleep.lock().unwrap();
                status.enabled = *state.is_enabled.lock().unwrap();
                status.active_profile = state.config.lock().unwrap().active_profile;
                status.profile_name = profile.name.clone();
                status.mode = format_activation_mode(&profile_for_activation, lang);
            }
            return;
        }
    }

    let display = gdk4::Display::default();
    let monitors = display
        .as_ref()
        .map(crate::monitors::list_monitors)
        .unwrap_or_default();
    let mut groups: std::collections::BTreeMap<usize, Vec<gdk4::Monitor>> =
        std::collections::BTreeMap::new();
    if monitors.is_empty() {
        groups.insert(config_snapshot.active_profile_index(), Vec::new());
    } else {
        for monitor in monitors {
            let monitor_id = crate::monitors::monitor_id(&monitor);
            let profile_index = config_snapshot.resolved_profile_index_for_monitor(&monitor_id);
            groups.entry(profile_index).or_default().push(monitor);
        }
    }

    let mut effective: Vec<(SettingsProfile, Vec<gdk4::Monitor>)> = Vec::new();
    for (profile_index, group_monitors) in groups {
        let mut p = config_snapshot.profiles[profile_index].clone();
        if forced_black {
            p.mode = ScreensaverMode::Color("#000000".to_string());
        }
        effective.push((p, group_monitors));
    }

    // Validate all used profiles before starting (so we don't end up with partially created windows).
    for (p, _) in &effective {
        if let Err(message) = validate_profile_for_start(p, lang) {
            notify_problem(&state, &format!("{}: {message}", p.name), lang);
            return;
        }
    }

    // Warn about multi-pass user shaders: they can be heavy and may hang on buggy GPU drivers.
    {
        let panic_hotkey = config_snapshot.hotkey_panic.trim().to_string();
        let panic_txt = if panic_hotkey.is_empty() {
            tr(lang, "не настроено").to_string()
        } else {
            panic_hotkey
        };
        let has_multipass = effective.iter().any(|(p, _)| match &p.mode {
            ScreensaverMode::Shadertoy(path) => shadertoy_has_buffers(path),
            _ => false,
        });
        if has_multipass {
            let msg = tr(lang, "Обнаружены составные GLSL шейдеры. Если скринсейвер зависнет, используйте принудительное закрытие: {hotkey}")
                .replace("{hotkey}", &panic_txt);
            notify_warning(&state, &msg, lang);
        }
    }

    // Avoid audio "doubling" when multiple screensaver instances play video.
    let mut audio_assigned = false;
    for (p, _) in &mut effective {
        let wants_audio = !p.mute_video
            && matches!(
                p.mode,
                ScreensaverMode::Video(_) | ScreensaverMode::Stream(_)
            );
        if wants_audio && !audio_assigned {
            audio_assigned = true;
        } else if wants_audio {
            p.mute_video = true;
            p.video_volume = 0;
        }
    }

    let activity_triggered = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let on_activity = {
        let triggered = std::sync::Arc::clone(&activity_triggered);
        let state = Arc::clone(&state_clone);
        move || {
            if triggered
                .compare_exchange(
                    false,
                    true,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::Relaxed,
                )
                .is_ok()
            {
                let state = Arc::clone(&state);
                glib::idle_add_local_once(move || {
                    stop_screensaver(state, StopReason::UserActivity)
                });
            }
        }
    };

    let mut screensavers = Vec::new();
    for (p, group_monitors) in effective {
        let saver = if group_monitors.is_empty() {
            ScreensaverWindow::new(&p, app.clone(), started_at.clone(), on_activity.clone())
        } else {
            ScreensaverWindow::new_for_monitors(
                &p,
                app.clone(),
                started_at.clone(),
                group_monitors,
                on_activity.clone(),
            )
        };
        screensavers.push(saver);
    }

    *state.screensaver_active.lock().unwrap() = true;
    *state.screensaver_started_at.lock().unwrap() = Some(Instant::now());
    *state.screensaver_windows.lock().unwrap() = Some(screensavers);

    if let Some(ref screensavers) = *state.screensaver_windows.lock().unwrap() {
        for screensaver in screensavers {
            screensaver.show();
        }
    }

    let enabled = *state.is_enabled.lock().unwrap();
    let inhibit_sleep = *state.inhibit_sleep.lock().unwrap();
    let active_profile = state.config.lock().unwrap().active_profile;
    if let Ok(mut status) = state.status.lock() {
        status.screensaver_active = true;
        status.inhibit_sleep = inhibit_sleep;
        status.enabled = enabled;
        status.active_profile = active_profile;
        status.profile_name = profile.name.clone();
        status.mode = format_activation_mode(&profile_for_activation, lang);
    }
}

fn handle_battery_state_change(state: Arc<AppState>, on_battery: bool) {
    let mut forced = state.battery_forced_black.lock().unwrap();
    if on_battery == *forced {
        return;
    }
    *forced = on_battery;
    drop(forced);

    if let Some(lock) = state.wayland_lock.lock().unwrap().as_ref() {
        lock.set_battery_black(on_battery);
        return;
    }

    let lang = resolve_language(state.config.lock().unwrap().language);
    replace_active_screensaver_windows(state, on_battery, lang);
}

fn replace_active_screensaver_windows(state: Arc<AppState>, battery_black: bool, lang: Language) {
    if !*state.screensaver_active.lock().unwrap() {
        return;
    }
    if state.wayland_lock.lock().unwrap().is_some() {
        return;
    }

    let app = state
        .main_window
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|w| w.application());

    if let Some(old) = state.screensaver_windows.lock().unwrap().take() {
        for saver in &old {
            saver.hide();
        }
    }

    let started_at = Arc::clone(&state.screensaver_started_at);

    let config_snapshot = state.config.lock().unwrap().clone();
    let display = gdk4::Display::default();
    let monitors = display
        .as_ref()
        .map(crate::monitors::list_monitors)
        .unwrap_or_default();
    let mut groups: std::collections::BTreeMap<usize, Vec<gdk4::Monitor>> =
        std::collections::BTreeMap::new();
    if monitors.is_empty() {
        groups.insert(config_snapshot.active_profile_index(), Vec::new());
    } else {
        for monitor in monitors {
            let monitor_id = crate::monitors::monitor_id(&monitor);
            let profile_index = config_snapshot.resolved_profile_index_for_monitor(&monitor_id);
            groups.entry(profile_index).or_default().push(monitor);
        }
    }

    let mut effective: Vec<(SettingsProfile, Vec<gdk4::Monitor>)> = Vec::new();
    for (profile_index, group_monitors) in groups {
        let mut p = config_snapshot.profiles[profile_index].clone();
        if battery_black {
            p.mode = ScreensaverMode::Color("#000000".to_string());
        }
        effective.push((p, group_monitors));
    }

    // Avoid audio doubling across instances.
    let mut audio_assigned = false;
    for (p, _) in &mut effective {
        let wants_audio = !p.mute_video
            && matches!(
                p.mode,
                ScreensaverMode::Video(_) | ScreensaverMode::Stream(_)
            );
        if wants_audio && !audio_assigned {
            audio_assigned = true;
        } else if wants_audio {
            p.mute_video = true;
            p.video_volume = 0;
        }
    }

    let sender = state.sender.clone();
    let activity_triggered = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let on_activity = {
        let sender = sender.clone();
        let triggered = std::sync::Arc::clone(&activity_triggered);
        move || {
            if triggered
                .compare_exchange(
                    false,
                    true,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::Relaxed,
                )
                .is_ok()
            {
                let _ = sender.send(AppMessage::StopScreensaverUserActivity);
            }
        }
    };

    let mut screensavers = Vec::new();
    for (p, group_monitors) in effective {
        let saver = if group_monitors.is_empty() {
            ScreensaverWindow::new(&p, app.clone(), started_at.clone(), on_activity.clone())
        } else {
            ScreensaverWindow::new_for_monitors(
                &p,
                app.clone(),
                started_at.clone(),
                group_monitors,
                on_activity.clone(),
            )
        };
        screensavers.push(saver);
    }

    *state.screensaver_windows.lock().unwrap() = Some(screensavers);
    if let Some(ref screensavers) = *state.screensaver_windows.lock().unwrap() {
        for screensaver in screensavers {
            screensaver.show();
        }
    }

    if let Ok(mut status) = state.status.lock() {
        let mut profile = config_snapshot.active_profile().clone();
        if battery_black {
            profile.mode = ScreensaverMode::Color("#000000".to_string());
        }
        status.mode = format_activation_mode(&profile, lang);
    }
}

fn format_activation_mode(profile: &SettingsProfile, lang: Language) -> String {
    let (label, detail) = match &profile.mode {
        ScreensaverMode::Color(color) => (tr(lang, "Цвет"), Some(color.clone())),
        ScreensaverMode::Gradient { start, end } => {
            (tr(lang, "Градиент"), Some(format!("{start} -> {end}")))
        }
        ScreensaverMode::Pattern(pattern) => (
            tr(lang, "Паттерн"),
            Some(pattern_label(*pattern, lang).to_string()),
        ),
        ScreensaverMode::Web(url) => (tr(lang, "Веб-страница"), Some(url.clone())),
        ScreensaverMode::Stream(url) => (tr(lang, "Видео по URL"), Some(url.clone())),
        ScreensaverMode::Image(path) => (tr(lang, "Изображение"), Some(path_basename(path))),
        ScreensaverMode::Video(path) => (tr(lang, "Видео"), Some(path_basename(path))),
        ScreensaverMode::Slideshow(path) => (tr(lang, "Слайдшоу"), Some(path_basename(path))),
        ScreensaverMode::PythonScript(path) => {
            (tr(lang, "Python скрипт"), Some(path_basename(path)))
        }
        ScreensaverMode::Shadertoy(path) => (tr(lang, "GLSL шейдер"), Some(path_basename(path))),
    };
    let detail = if profile.random_media
        && matches!(
            &profile.mode,
            ScreensaverMode::Image(_) | ScreensaverMode::Video(_)
        )
        && !profile.media_list.is_empty()
    {
        Some(tr(lang, "Список: {}").replacen("{}", &profile.media_list.len().to_string(), 1))
    } else {
        detail
    };
    match detail {
        Some(detail) if !detail.trim().is_empty() => format!("{label}: {detail}"),
        _ => label.to_string(),
    }
}

fn update_status_profile(status: &mut StatusSnapshot, config: &Config, lang: Language) {
    let profile = config.active_profile();
    status.active_profile = config.active_profile;
    status.profile_name = profile.name.clone();
    status.mode = format_activation_mode(profile, lang);
}

fn pattern_label(pattern: AnimatedPattern, lang: Language) -> &'static str {
    match pattern {
        AnimatedPattern::Matrix => tr(lang, "Матрица"),
        AnimatedPattern::Stars => tr(lang, "Звезды"),
        AnimatedPattern::Geometry => tr(lang, "Геометрия"),
        AnimatedPattern::Flowfield => tr(lang, "Поле потока"),
        AnimatedPattern::Aurora => tr(lang, "Северное сияние"),
        AnimatedPattern::Plasma => tr(lang, "Плазма"),
        AnimatedPattern::Bokeh => tr(lang, "Боке"),
        AnimatedPattern::Constellation => tr(lang, "Созвездия"),
        AnimatedPattern::Lissajous => tr(lang, "Кривые Лиссажу"),
        AnimatedPattern::Waves => tr(lang, "Волны"),
        AnimatedPattern::Voronoi => tr(lang, "Ячейки Вороного"),
        AnimatedPattern::Scanline => tr(lang, "ЭЛТ-монитор"),
        AnimatedPattern::Fireflies => tr(lang, "Светлячки"),
        AnimatedPattern::SmokeInk => tr(lang, "Дым/Чернила"),
        AnimatedPattern::WaterRipples => tr(lang, "Водная рябь"),
        AnimatedPattern::MatrixRain3D => tr(lang, "Матрица 2.0"),
        AnimatedPattern::Lcars => tr(lang, "LCARS"),
        AnimatedPattern::Terminal => tr(lang, "Терминал"),
        AnimatedPattern::Fractals => tr(lang, "Фракталы"),
        AnimatedPattern::ReactionDiffusion => tr(lang, "Реакция-диффузия"),
    }
}

fn path_basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

fn enable_power_integration(state: &AppState) {
    disable_power_integration(state);
    let conn = match DbusConnection::session() {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("Power integration: failed to connect to D-Bus: {}", err);
            return;
        }
    };
    let (try_kde, try_gnome) = desktop::desktop_targets();
    let mut gnome_cookie = None;
    let mut kde_cookie = None;

    if try_gnome {
        match gnome_inhibit(&conn) {
            Ok(cookie) => gnome_cookie = Some(cookie),
            Err(err) => eprintln!("Power integration: GNOME inhibit failed: {}", err),
        }
    }
    if try_kde {
        match kde_inhibit(&conn) {
            Ok(cookie) => kde_cookie = Some(cookie),
            Err(err) => eprintln!("Power integration: KDE inhibit failed: {}", err),
        }
    }

    let mut guard = state.power_inhibit_state.lock().unwrap();
    guard.gnome_cookie = gnome_cookie;
    guard.kde_cookie = kde_cookie;
}

fn disable_power_integration(state: &AppState) {
    let (gnome_cookie, kde_cookie) = {
        let mut guard = state.power_inhibit_state.lock().unwrap();
        (guard.gnome_cookie.take(), guard.kde_cookie.take())
    };
    if gnome_cookie.is_none() && kde_cookie.is_none() {
        return;
    }
    let conn = match DbusConnection::session() {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("Power integration: failed to connect to D-Bus: {}", err);
            return;
        }
    };
    if let Some(cookie) = gnome_cookie {
        if let Err(err) = gnome_uninhibit(&conn, cookie) {
            eprintln!("Power integration: GNOME uninhibit failed: {}", err);
        }
    }
    if let Some(cookie) = kde_cookie {
        if let Err(err) = kde_uninhibit(&conn, cookie) {
            eprintln!("Power integration: KDE uninhibit failed: {}", err);
        }
    }
}

fn request_screen_lock(profile: &SettingsProfile) {
    if !profile.lock_screen_enabled {
        return;
    }
    let (try_kde, try_gnome) = desktop::desktop_targets();
    if !try_kde && !try_gnome {
        return;
    }
    let conn = match DbusConnection::session() {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("Lock screen: failed to connect to D-Bus: {}", err);
            return;
        }
    };
    let mut locked = false;
    let mut last_err = None;

    if try_gnome {
        if let Err(err) = lock_gnome_session(&conn) {
            last_err = Some(err.to_string());
        } else {
            locked = true;
        }
        if !locked {
            if let Err(err) = lock_gnome_screensaver(&conn) {
                last_err = Some(err.to_string());
            } else {
                locked = true;
            }
        }
    }

    if !locked {
        if let Err(err) = lock_freedesktop(&conn) {
            last_err = Some(err.to_string());
        } else {
            locked = true;
        }
    }

    if !locked {
        if let Some(err) = last_err {
            eprintln!("Lock screen: failed to lock: {}", err);
        } else {
            eprintln!("Lock screen: no compatible interface found");
        }
    }
}

fn gnome_inhibit(conn: &DbusConnection) -> Result<u32, Box<dyn std::error::Error>> {
    let proxy = DbusProxy::new(
        conn,
        "org.gnome.SessionManager",
        "/org/gnome/SessionManager",
        "org.gnome.SessionManager",
    )?;
    let app_id = "Vesper";
    let reason = "Screensaver active";
    let flags: u32 = 8;
    let cookie: u32 = proxy.call("Inhibit", &(app_id, 0u32, reason, flags))?;
    Ok(cookie)
}

fn gnome_uninhibit(conn: &DbusConnection, cookie: u32) -> Result<(), Box<dyn std::error::Error>> {
    let proxy = DbusProxy::new(
        conn,
        "org.gnome.SessionManager",
        "/org/gnome/SessionManager",
        "org.gnome.SessionManager",
    )?;
    let _: () = proxy.call("Uninhibit", &(cookie,))?;
    Ok(())
}

fn kde_inhibit(conn: &DbusConnection) -> Result<u32, Box<dyn std::error::Error>> {
    let proxy = DbusProxy::new(
        conn,
        "org.freedesktop.PowerManagement",
        "/org/freedesktop/PowerManagement/Inhibit",
        "org.freedesktop.PowerManagement.Inhibit",
    )?;
    let app_id = "Vesper";
    let reason = "Screensaver active";
    let cookie: u32 = proxy.call("Inhibit", &(app_id, reason))?;
    Ok(cookie)
}

fn kde_uninhibit(conn: &DbusConnection, cookie: u32) -> Result<(), Box<dyn std::error::Error>> {
    let proxy = DbusProxy::new(
        conn,
        "org.freedesktop.PowerManagement",
        "/org/freedesktop/PowerManagement/Inhibit",
        "org.freedesktop.PowerManagement.Inhibit",
    )?;
    let _: () = proxy.call("UnInhibit", &(cookie,))?;
    Ok(())
}

fn lock_freedesktop(conn: &DbusConnection) -> Result<(), Box<dyn std::error::Error>> {
    let proxy = DbusProxy::new(
        conn,
        "org.freedesktop.ScreenSaver",
        "/org/freedesktop/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    )?;
    let _: () = proxy.call("Lock", &())?;
    Ok(())
}

fn lock_gnome_screensaver(conn: &DbusConnection) -> Result<(), Box<dyn std::error::Error>> {
    let proxy = DbusProxy::new(
        conn,
        "org.gnome.ScreenSaver",
        "/org/gnome/ScreenSaver",
        "org.gnome.ScreenSaver",
    )?;
    let _: () = proxy.call("Lock", &())?;
    Ok(())
}

fn lock_gnome_session(conn: &DbusConnection) -> Result<(), Box<dyn std::error::Error>> {
    let proxy = DbusProxy::new(
        conn,
        "org.gnome.SessionManager",
        "/org/gnome/SessionManager",
        "org.gnome.SessionManager",
    )?;
    let _: () = proxy.call("Lock", &())?;
    Ok(())
}

fn request_mpris_pause(paused_players: Arc<Mutex<Vec<String>>>) {
    thread::spawn(move || match pause_mpris_players() {
        Ok(paused) => {
            *paused_players.lock().unwrap() = paused;
        }
        Err(err) => {
            eprintln!("MPRIS pause failed: {}", err);
        }
    });
}

fn pause_mpris_players() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let conn = DbusConnection::session()?;
    let dbus_proxy = DbusProxy::new(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    let names: Vec<String> = dbus_proxy.call("ListNames", &())?;
    let mut paused_list = Vec::new();

    for name in names {
        if !name.starts_with("org.mpris.MediaPlayer2.") {
            continue;
        }
        let player_proxy = match DbusProxy::new(
            &conn,
            name.as_str(),
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
        ) {
            Ok(proxy) => proxy,
            Err(err) => {
                eprintln!("MPRIS: failed to proxy {name}: {err}");
                continue;
            }
        };

        // Check status first
        let status: Result<String, _> = player_proxy.get_property("PlaybackStatus");
        if let Ok(s) = status {
            if s == "Playing" {
                let result: Result<(), zbus::Error> = player_proxy.call("Pause", &());
                if let Err(err) = result {
                    eprintln!("MPRIS: pause failed for {name}: {err}");
                } else {
                    paused_list.push(name.clone());
                }
            }
        }
    }
    Ok(paused_list)
}

fn resume_mpris_players(players: Vec<String>) {
    if players.is_empty() {
        return;
    }
    thread::spawn(move || {
        if let Err(err) = do_resume_mpris_players(players) {
            eprintln!("MPRIS resume failed: {}", err);
        }
    });
}

fn do_resume_mpris_players(players: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let conn = DbusConnection::session()?;
    for name in players {
        let player_proxy = match DbusProxy::new(
            &conn,
            name.as_str(),
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
        ) {
            Ok(proxy) => proxy,
            Err(err) => {
                eprintln!("MPRIS: failed to proxy {name}: {err}");
                continue;
            }
        };
        let result: Result<(), zbus::Error> = player_proxy.call("Play", &());
        if let Err(err) = result {
            eprintln!("MPRIS: resume failed for {name}: {err}");
        }
    }
    Ok(())
}

fn notify_problem(state: &AppState, message: &str, lang: Language) {
    if !should_notify_problem(state, message) {
        return;
    }
    let body = tr(lang, "Скринсейвер не запущен: {message}").replace("{message}", message);
    if let Some(app) = state
        .main_window
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|w| w.application())
    {
        let notification = Notification::new("Vesper");
        notification.set_body(Some(&body));
        app.send_notification(Some("vesper-problem"), &notification);
    } else {
        eprintln!("{body}");
    }
}

fn should_notify_problem(state: &AppState, message: &str) -> bool {
    let mut last = state.last_problem_notification.lock().unwrap();
    let now = Instant::now();
    if let Some((last_message, last_time)) = last.as_ref() {
        if last_message == message && now.duration_since(*last_time) < PROBLEM_NOTIFY_COOLDOWN {
            return false;
        }
    }
    *last = Some((message.to_string(), now));
    true
}

fn notify_warning(state: &AppState, message: &str, lang: Language) {
    if !should_notify_warning(state, message) {
        return;
    }
    let body = tr(lang, "Предупреждение: {message}").replace("{message}", message);
    if let Some(app) = state
        .main_window
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|w| w.application())
    {
        let notification = Notification::new("Vesper");
        notification.set_body(Some(&body));
        app.send_notification(Some("vesper-warning"), &notification);
    } else {
        eprintln!("{body}");
    }
}

fn should_notify_warning(state: &AppState, message: &str) -> bool {
    let mut last = state.last_warning_notification.lock().unwrap();
    let now = Instant::now();
    if let Some((last_message, last_time)) = last.as_ref() {
        if last_message == message && now.duration_since(*last_time) < PROBLEM_NOTIFY_COOLDOWN {
            return false;
        }
    }
    *last = Some((message.to_string(), now));
    true
}

fn validate_profile_for_start(profile: &SettingsProfile, lang: Language) -> Result<(), String> {
    match &profile.mode {
        ScreensaverMode::Image(path) => validate_media_selection(
            tr(lang, "Изображение"),
            MediaKind::Image,
            profile,
            path,
            lang,
        ),
        ScreensaverMode::Video(path) => {
            validate_media_selection(tr(lang, "Видео"), MediaKind::Video, profile, path, lang)
        }
        ScreensaverMode::Slideshow(path) => validate_slideshow_path(path, lang),
        ScreensaverMode::Web(url) => validate_web_url(url, lang),
        ScreensaverMode::Stream(url) => validate_stream_url(url, lang),
        ScreensaverMode::PythonScript(path) => validate_python_script_path(path, lang),
        ScreensaverMode::Shadertoy(path) => validate_shadertoy_shader_path(path, lang),
        _ => Ok(()),
    }
}

fn validate_python_script_path(path: &str, lang: Language) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(tr(lang, "Файл не выбран").to_string());
    }
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err(tr(lang, "Файл не найден: {path}").replace("{path}", path));
    }
    if !p.is_file() {
        return Err(tr(lang, "Путь не является файлом").to_string());
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if ext != "py" {
        return Err(tr(lang, "Неподдерживаемый формат").to_string());
    }
    Ok(())
}

fn validate_shadertoy_shader_path(path: &str, lang: Language) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(tr(lang, "Файл не выбран").to_string());
    }
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err(tr(lang, "Файл не найден: {path}").replace("{path}", path));
    }
    if p.is_dir() {
        if !dir_has_shadertoy_image_shader(p) {
            return Err(tr(lang, "В папке нет шейдера Image").to_string());
        }
        return Ok(());
    }
    if !p.is_file() {
        return Err(tr(lang, "Путь не является файлом").to_string());
    }
    let ext = p
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

fn shadertoy_has_buffers(path: &str) -> bool {
    let path = path.trim();
    if path.is_empty() {
        return false;
    }
    let p = std::path::Path::new(path);
    let dir = if p.is_dir() {
        p
    } else {
        p.parent().unwrap_or_else(|| std::path::Path::new("."))
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
        if matches!(key.as_str(), "buffera" | "bufferb" | "bufferc" | "bufferd") {
            return true;
        }
    }
    false
}

fn validate_media_selection(
    label: &str,
    kind: MediaKind,
    profile: &SettingsProfile,
    path: &str,
    lang: Language,
) -> Result<(), String> {
    if profile.random_media {
        let valid_count = count_valid_media(&profile.media_list, kind);
        if valid_count > 0 {
            return Ok(());
        }
        if !path.trim().is_empty() {
            return validate_media_path(path, kind, lang).map_err(|err| format!("{label}: {err}"));
        }
        return Err(format!(
            "{label}: {}",
            tr(lang, "список медиа пуст или недоступен")
        ));
    }
    validate_media_path(path, kind, lang).map_err(|err| format!("{label}: {err}"))
}

fn validate_media_path(path: &str, kind: MediaKind, lang: Language) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(tr(lang, "Файл не выбран").to_string());
    }
    let path_obj = Path::new(path);
    if !path_obj.exists() {
        return Err(tr(lang, "Файл не найден: {path}").replace("{path}", path));
    }
    if !path_obj.is_file() {
        return Err(tr(lang, "Путь не является файлом").to_string());
    }
    if !is_allowed_media_path(path_obj, kind) {
        return Err(tr(lang, "Неподдерживаемый формат").to_string());
    }
    Ok(())
}

fn validate_slideshow_path(path: &str, lang: Language) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(tr(lang, "Папка не выбрана").to_string());
    }
    let path_obj = Path::new(path);
    if !path_obj.exists() {
        return Err(tr(lang, "Папка не найдена: {path}").replace("{path}", path));
    }
    if !path_obj.is_dir() {
        return Err(tr(lang, "Путь не является папкой").to_string());
    }
    if !folder_has_images(path_obj) {
        return Err(tr(lang, "В папке нет изображений").to_string());
    }
    Ok(())
}

fn validate_web_url(url: &str, lang: Language) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err(tr(lang, "URL не указан").to_string());
    }
    Ok(())
}

fn validate_stream_url(url: &str, lang: Language) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(tr(lang, "URL не указан").to_string());
    }
    if !is_valid_stream_url(trimmed) {
        return Err(tr(lang, "Неверный URL потока").to_string());
    }
    Ok(())
}

fn count_valid_media(paths: &[String], kind: MediaKind) -> usize {
    paths
        .iter()
        .filter(|path| is_allowed_media_path(Path::new(path.trim()), kind))
        .count()
}

fn folder_has_images(path: &Path) -> bool {
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if is_allowed_image_ext(&ext.to_ascii_lowercase()) {
                    return true;
                }
            }
        }
    }
    false
}

fn is_allowed_media_path(path: &Path, kind: MediaKind) -> bool {
    if !path.is_file() {
        return false;
    }
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => ext.to_ascii_lowercase(),
        None => return false,
    };
    match kind {
        MediaKind::Image => is_allowed_image_ext(&ext),
        MediaKind::Video => is_allowed_video_ext(&ext),
    }
}

fn is_allowed_image_ext(ext: &str) -> bool {
    matches!(
        ext,
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
        ext,
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

fn is_valid_stream_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("rtsp://")
        || lower.starts_with("rtmp://")
}

fn set_inhibit_sleep(state: Arc<AppState>, inhibit: bool) {
    *state.inhibit_sleep.lock().unwrap() = inhibit;
    if let Ok(mut status) = state.status.lock() {
        status.inhibit_sleep = inhibit;
    }

    if inhibit {
        // Acquire inhibit lock via systemd-inhibit
        match std::process::Command::new("systemd-inhibit")
            .args([
                "--what=sleep:idle",
                "--who=Vesper",
                "--why=User requested",
                "--mode=block",
                "sleep",
                "infinity",
            ])
            .spawn()
        {
            Ok(child) => {
                *state.inhibit_cookie.lock().unwrap() = Some(child.id());
            }
            Err(_) => {}
        }
    } else {
        // Release inhibit lock
        if let Some(pid) = state.inhibit_cookie.lock().unwrap().take() {
            let _ = std::process::Command::new("kill")
                .arg(pid.to_string())
                .spawn();
        }
    }
}
