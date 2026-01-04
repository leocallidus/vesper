use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::thread;

pub struct IdleMonitor {
    idle_since: Arc<Mutex<Option<Instant>>>,
    dbus_idle_ms: Arc<AtomicU64>,
    x11_idle_ms: Arc<AtomicU64>,
}

impl IdleMonitor {
    pub fn new() -> Option<Self> {
        let idle_since = Arc::new(Mutex::new(None::<Instant>));
        let dbus_idle_ms = Arc::new(AtomicU64::new(0));
        let x11_idle_ms = Arc::new(AtomicU64::new(0));
        
        // Try Wayland ext-idle-notify
        let idle_since_wl = Arc::clone(&idle_since);
        thread::spawn(move || {
            let _ = run_wayland_idle(idle_since_wl);
        });
        
        // D-Bus fallback for GNOME
        let dbus_idle = Arc::clone(&dbus_idle_ms);
        thread::spawn(move || {
            let _ = run_dbus_idle(dbus_idle);
        });
        
        // X11 XScreenSaver extension
        let x11_idle = Arc::clone(&x11_idle_ms);
        thread::spawn(move || {
            let _ = run_x11_idle(x11_idle);
        });
        
        Some(Self { idle_since, dbus_idle_ms, x11_idle_ms })
    }
    
    pub fn get_idle_ms(&self) -> u64 {
        // Prefer Wayland idle time
        if let Some(since) = *self.idle_since.lock().unwrap() {
            return since.elapsed().as_millis() as u64;
        }
        // Then X11
        let x11 = self.x11_idle_ms.load(Ordering::Relaxed);
        if x11 > 0 {
            return x11;
        }
        // Fallback to D-Bus
        self.dbus_idle_ms.load(Ordering::Relaxed)
    }
    
    pub fn is_idle(&self) -> bool {
        self.get_idle_ms() > 0
    }
}

fn run_x11_idle(idle_ms: Arc<AtomicU64>) -> Result<(), Box<dyn std::error::Error>> {
    use x11rb::connection::Connection;
    use x11rb::protocol::screensaver;
    
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    
    // Check if XScreenSaver extension is available
    screensaver::query_version(&conn, 1, 1)?.reply()?;
    
    loop {
        if let Ok(reply) = screensaver::query_info(&conn, root)?.reply() {
            idle_ms.store(reply.ms_since_user_input as u64, Ordering::Relaxed);
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn run_dbus_idle(idle_ms: Arc<AtomicU64>) -> Result<(), Box<dyn std::error::Error>> {
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
            }
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn run_wayland_idle(idle_since: Arc<Mutex<Option<Instant>>>) -> Result<(), Box<dyn std::error::Error>> {
    use wayland_client::{Connection, Dispatch, QueueHandle, protocol::{wl_registry, wl_seat}};
    use wayland_protocols::ext::idle_notify::v1::client::{ext_idle_notifier_v1, ext_idle_notification_v1};

    struct State {
        seat: Option<wl_seat::WlSeat>,
        notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
        idle_since: Arc<Mutex<Option<Instant>>>,
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for State {
        fn event(state: &mut Self, reg: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
            if let wl_registry::Event::Global { name, interface, .. } = event {
                match interface.as_str() {
                    "wl_seat" => { state.seat = Some(reg.bind(name, 1, qh, ())); }
                    "ext_idle_notifier_v1" => { state.notifier = Some(reg.bind(name, 1, qh, ())); }
                    _ => {}
                }
            }
        }
    }

    impl Dispatch<wl_seat::WlSeat, ()> for State {
        fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    }

    impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for State {
        fn event(_: &mut Self, _: &ext_idle_notifier_v1::ExtIdleNotifierV1, _: ext_idle_notifier_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    }

    impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, ()> for State {
        fn event(state: &mut Self, _: &ext_idle_notification_v1::ExtIdleNotificationV1, event: ext_idle_notification_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            match event {
                ext_idle_notification_v1::Event::Idled => { *state.idle_since.lock().unwrap() = Some(Instant::now()); }
                ext_idle_notification_v1::Event::Resumed => { *state.idle_since.lock().unwrap() = None; }
                _ => {}
            }
        }
    }

    let conn = Connection::connect_to_env()?;
    let mut state = State { seat: None, notifier: None, idle_since };
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    
    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut state)?;
    
    if let (Some(notifier), Some(seat)) = (&state.notifier, &state.seat) {
        notifier.get_idle_notification(1000, seat, &qh, ());
    } else {
        return Err("ext-idle-notify not available".into());
    }
    
    loop { queue.blocking_dispatch(&mut state)?; }
}
