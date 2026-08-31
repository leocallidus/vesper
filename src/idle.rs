use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

pub struct IdleMonitor {
    wayland_active: Arc<AtomicBool>,
    wayland_has_input_notify: Arc<AtomicBool>,
    wayland_input_idle_since: Arc<Mutex<Option<Instant>>>,
    wayland_standard_idle_since: Arc<Mutex<Option<Instant>>>,
    dbus_active: Arc<AtomicBool>,
    dbus_idle_ms: Arc<AtomicU64>,
    x11_active: Arc<AtomicBool>,
    x11_raw_idle_ms: Arc<AtomicU64>,
    x11_standard_idle_ms: Arc<AtomicU64>,
    x11_last_input: Arc<Mutex<Instant>>,
}

impl IdleMonitor {
    pub fn new() -> Option<Self> {
        let wayland_active = Arc::new(AtomicBool::new(false));
        let wayland_has_input_notify = Arc::new(AtomicBool::new(false));
        let wayland_input_idle_since = Arc::new(Mutex::new(None::<Instant>));
        let wayland_standard_idle_since = Arc::new(Mutex::new(None::<Instant>));
        let dbus_active = Arc::new(AtomicBool::new(false));
        let dbus_idle_ms = Arc::new(AtomicU64::new(0));
        let x11_active = Arc::new(AtomicBool::new(false));
        let x11_raw_idle_ms = Arc::new(AtomicU64::new(0));
        let x11_standard_idle_ms = Arc::new(AtomicU64::new(0));
        let x11_last_input = Arc::new(Mutex::new(Instant::now()));

        let is_wayland_env = std::env::var("WAYLAND_DISPLAY").is_ok();

        // 1. Wayland ext-idle-notify
        let wl_active = Arc::clone(&wayland_active);
        let wl_has_input = Arc::clone(&wayland_has_input_notify);
        let wl_input = Arc::clone(&wayland_input_idle_since);
        let wl_standard = Arc::clone(&wayland_standard_idle_since);
        thread::spawn(move || {
            let _ = run_wayland_idle(wl_active, wl_has_input, wl_input, wl_standard);
        });

        // 2. D-Bus fallback for GNOME
        let dbus_act = Arc::clone(&dbus_active);
        let dbus_idle = Arc::clone(&dbus_idle_ms);
        thread::spawn(move || {
            let _ = run_dbus_idle(dbus_act, dbus_idle);
        });

        // 3. X11 idle detection (only on native X11 or if Wayland is not active)
        let x11_act = Arc::clone(&x11_active);
        let x11_raw = Arc::clone(&x11_raw_idle_ms);
        let x11_std = Arc::clone(&x11_standard_idle_ms);
        let x11_input = Arc::clone(&x11_last_input);
        let wl_active_for_x11 = Arc::clone(&wayland_active);
        thread::spawn(move || {
            if is_wayland_env {
                thread::sleep(std::time::Duration::from_millis(500));
                if wl_active_for_x11.load(Ordering::Relaxed) {
                    return;
                }
            }
            let _ = run_x11_idle(x11_act, x11_raw, x11_std, x11_input, wl_active_for_x11);
        });

        Some(Self {
            wayland_active,
            wayland_has_input_notify,
            wayland_input_idle_since,
            wayland_standard_idle_since,
            dbus_active,
            dbus_idle_ms,
            x11_active,
            x11_raw_idle_ms,
            x11_standard_idle_ms,
            x11_last_input,
        })
    }

    pub fn get_idle_ms(&self, ignore_inhibitors: bool) -> u64 {
        if self.wayland_active.load(Ordering::Relaxed) {
            if ignore_inhibitors {
                if let Some(since) = *self.wayland_input_idle_since.lock().unwrap() {
                    return since.elapsed().as_millis() as u64;
                }
                if self.wayland_has_input_notify.load(Ordering::Relaxed) {
                    return 0;
                }
                if let Some(since) = *self.wayland_standard_idle_since.lock().unwrap() {
                    return since.elapsed().as_millis() as u64;
                }
                0
            } else {
                if let Some(since) = *self.wayland_standard_idle_since.lock().unwrap() {
                    return since.elapsed().as_millis() as u64;
                }
                0
            }
        } else if self.dbus_active.load(Ordering::Relaxed) {
            self.dbus_idle_ms.load(Ordering::Relaxed)
        } else if self.x11_active.load(Ordering::Relaxed) {
            if ignore_inhibitors {
                self.x11_raw_idle_ms.load(Ordering::Relaxed)
            } else {
                self.x11_standard_idle_ms.load(Ordering::Relaxed)
            }
        } else {
            0
        }
    }

    #[allow(dead_code)]
    pub fn is_idle(&self, ignore_inhibitors: bool) -> bool {
        self.get_idle_ms(ignore_inhibitors) > 0
    }

    pub fn reset(&self) {
        *self.wayland_input_idle_since.lock().unwrap() = None;
        *self.wayland_standard_idle_since.lock().unwrap() = None;
        *self.x11_last_input.lock().unwrap() = Instant::now();
        self.x11_raw_idle_ms.store(0, Ordering::Relaxed);
        self.x11_standard_idle_ms.store(0, Ordering::Relaxed);
        self.dbus_idle_ms.store(0, Ordering::Relaxed);
    }
}

fn run_x11_idle(
    x11_active: Arc<AtomicBool>,
    raw_idle_ms: Arc<AtomicU64>,
    standard_idle_ms: Arc<AtomicU64>,
    last_input: Arc<Mutex<Instant>>,
    wayland_active: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    use x11rb::connection::Connection;
    use x11rb::protocol::screensaver;
    use x11rb::protocol::xproto::ConnectionExt;

    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;

    let has_screensaver = screensaver::query_version(&conn, 1, 1)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some();

    x11_active.store(true, Ordering::Relaxed);

    let mut last_ptr: Option<(i16, i16, u16)> = None;
    let mut last_keys: Option<[u8; 32]> = None;

    loop {
        if wayland_active.load(Ordering::Relaxed) {
            x11_active.store(false, Ordering::Relaxed);
            break;
        }

        let mut activity_detected = false;

        // 1. Check pointer position & button mask
        if let Ok(ptr_cookie) = conn.query_pointer(root) {
            if let Ok(ptr_reply) = ptr_cookie.reply() {
                let current_ptr = (ptr_reply.root_x, ptr_reply.root_y, u16::from(ptr_reply.mask));
                if let Some(prev) = last_ptr {
                    if prev != current_ptr {
                        activity_detected = true;
                    }
                }
                last_ptr = Some(current_ptr);
            }
        }

        // 2. Check keyboard key states
        if let Ok(keymap_cookie) = conn.query_keymap() {
            if let Ok(keymap_reply) = keymap_cookie.reply() {
                let current_keys = keymap_reply.keys;
                if let Some(prev) = last_keys {
                    if prev != current_keys {
                        activity_detected = true;
                    }
                }
                if current_keys.iter().any(|&b| b != 0) {
                    activity_detected = true;
                }
                last_keys = Some(current_keys);
            }
        }

        if activity_detected {
            *last_input.lock().unwrap() = Instant::now();
        }

        let elapsed = last_input.lock().unwrap().elapsed().as_millis() as u64;
        raw_idle_ms.store(elapsed, Ordering::Relaxed);

        // 3. Query XScreenSaver extension
        if has_screensaver {
            if let Ok(reply) = screensaver::query_info(&conn, root)?.reply() {
                standard_idle_ms.store(reply.ms_since_user_input as u64, Ordering::Relaxed);
            }
        }

        thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(())
}

fn run_dbus_idle(
    dbus_active: Arc<AtomicBool>,
    idle_ms: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error>> {
    use zbus::blocking::Connection;

    let conn = Connection::session()?;

    loop {
        // Try GNOME Mutter IdleMonitor
        if let Ok(reply) = conn.call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        ) {
            if let Ok(ms) = reply.body().deserialize::<u64>() {
                idle_ms.store(ms, Ordering::Relaxed);
                dbus_active.store(true, Ordering::Relaxed);
            }
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WaylandIdleType {
    Input,
    Standard,
}

fn run_wayland_idle(
    wayland_active: Arc<AtomicBool>,
    wayland_has_input_notify: Arc<AtomicBool>,
    input_idle_since: Arc<Mutex<Option<Instant>>>,
    standard_idle_since: Arc<Mutex<Option<Instant>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use wayland_client::{
        protocol::{wl_registry, wl_seat},
        Connection, Dispatch, QueueHandle,
    };
    use wayland_protocols::ext::idle_notify::v1::client::{
        ext_idle_notification_v1, ext_idle_notifier_v1,
    };

    struct State {
        seat: Option<wl_seat::WlSeat>,
        notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
        notifier_version: u32,
        input_idle_since: Arc<Mutex<Option<Instant>>>,
        standard_idle_since: Arc<Mutex<Option<Instant>>>,
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for State {
        fn event(
            state: &mut Self,
            reg: &wl_registry::WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            {
                match interface.as_str() {
                    "wl_seat" => {
                        state.seat = Some(reg.bind(name, 1, qh, ()));
                    }
                    "ext_idle_notifier_v1" => {
                        let bind_version = version.min(2);
                        state.notifier = Some(reg.bind(name, bind_version, qh, ()));
                        state.notifier_version = bind_version;
                    }
                    _ => {}
                }
            }
        }
    }

    impl Dispatch<wl_seat::WlSeat, ()> for State {
        fn event(
            _: &mut Self,
            _: &wl_seat::WlSeat,
            _: wl_seat::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &ext_idle_notifier_v1::ExtIdleNotifierV1,
            _: ext_idle_notifier_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, WaylandIdleType> for State {
        fn event(
            state: &mut Self,
            _: &ext_idle_notification_v1::ExtIdleNotificationV1,
            event: ext_idle_notification_v1::Event,
            idle_type: &WaylandIdleType,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let target = match idle_type {
                WaylandIdleType::Input => &state.input_idle_since,
                WaylandIdleType::Standard => &state.standard_idle_since,
            };
            match event {
                ext_idle_notification_v1::Event::Idled => {
                    *target.lock().unwrap() = Some(Instant::now());
                }
                ext_idle_notification_v1::Event::Resumed => {
                    *target.lock().unwrap() = None;
                }
                _ => {}
            }
        }
    }

    let conn = Connection::connect_to_env()?;
    let mut state = State {
        seat: None,
        notifier: None,
        notifier_version: 1,
        input_idle_since,
        standard_idle_since,
    };
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();

    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut state)?;

    if let (Some(notifier), Some(seat)) = (&state.notifier, &state.seat) {
        // Standard idle notification (respects inhibitors)
        notifier.get_idle_notification(1000, seat, &qh, WaylandIdleType::Standard);
        // Input-only idle notification (ignores inhibitors, version >= 2)
        if state.notifier_version >= 2 {
            notifier.get_input_idle_notification(1000, seat, &qh, WaylandIdleType::Input);
            wayland_has_input_notify.store(true, Ordering::Relaxed);
        }
        wayland_active.store(true, Ordering::Relaxed);
    } else {
        return Err("ext-idle-notify not available".into());
    }

    loop {
        queue.blocking_dispatch(&mut state)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idle_monitor_structure_and_methods() {
        let monitor = IdleMonitor {
            wayland_active: Arc::new(AtomicBool::new(false)),
            wayland_has_input_notify: Arc::new(AtomicBool::new(false)),
            wayland_input_idle_since: Arc::new(Mutex::new(None)),
            wayland_standard_idle_since: Arc::new(Mutex::new(None)),
            dbus_active: Arc::new(AtomicBool::new(false)),
            dbus_idle_ms: Arc::new(AtomicU64::new(0)),
            x11_active: Arc::new(AtomicBool::new(true)),
            x11_raw_idle_ms: Arc::new(AtomicU64::new(0)),
            x11_standard_idle_ms: Arc::new(AtomicU64::new(0)),
            x11_last_input: Arc::new(Mutex::new(Instant::now())),
        };

        // When all sources are 0/None, get_idle_ms returns 0
        assert_eq!(monitor.get_idle_ms(true), 0);
        assert_eq!(monitor.get_idle_ms(false), 0);
        assert!(!monitor.is_idle(true));
        assert!(!monitor.is_idle(false));

        // When Wayland input idle is active (inhibitors ignored) but standard idle is None (inhibited)
        monitor.wayland_active.store(true, Ordering::Relaxed);
        monitor.wayland_has_input_notify.store(true, Ordering::Relaxed);
        *monitor.wayland_input_idle_since.lock().unwrap() =
            Some(Instant::now() - std::time::Duration::from_secs(10));
        *monitor.wayland_standard_idle_since.lock().unwrap() = None;

        assert!(monitor.get_idle_ms(true) >= 10000);
        assert_eq!(monitor.get_idle_ms(false), 0);
        assert!(monitor.is_idle(true));
        assert!(!monitor.is_idle(false));

        // Reset Wayland input idle (user resumed activity on Wayland)
        *monitor.wayland_input_idle_since.lock().unwrap() = None;
        assert_eq!(monitor.get_idle_ms(true), 0);
        assert_eq!(monitor.get_idle_ms(false), 0);
        assert!(!monitor.is_idle(true));

        // Switch to D-Bus active
        monitor.wayland_active.store(false, Ordering::Relaxed);
        monitor.dbus_active.store(true, Ordering::Relaxed);
        monitor.dbus_idle_ms.store(6000, Ordering::Relaxed);
        assert_eq!(monitor.get_idle_ms(true), 6000);
        assert_eq!(monitor.get_idle_ms(false), 6000);

        // Switch to X11 active
        monitor.dbus_active.store(false, Ordering::Relaxed);
        monitor.x11_active.store(true, Ordering::Relaxed);
        monitor.x11_raw_idle_ms.store(5000, Ordering::Relaxed);
        monitor.x11_standard_idle_ms.store(0, Ordering::Relaxed);

        assert_eq!(monitor.get_idle_ms(true), 5000);
        assert_eq!(monitor.get_idle_ms(false), 0);
        assert!(monitor.is_idle(true));
        assert!(!monitor.is_idle(false));

        // When standard idle is 3000, and raw idle is 8000
        monitor.x11_standard_idle_ms.store(3000, Ordering::Relaxed);
        monitor.x11_raw_idle_ms.store(8000, Ordering::Relaxed);

        assert_eq!(monitor.get_idle_ms(true), 8000);
        assert_eq!(monitor.get_idle_ms(false), 3000);

        // Test reset
        monitor.reset();
        assert_eq!(monitor.get_idle_ms(true), 0);
        assert_eq!(monitor.get_idle_ms(false), 0);
    }
}
