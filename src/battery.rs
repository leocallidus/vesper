use crate::AppMessage;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use zbus::blocking::{Connection as DbusConnection, Proxy as DbusProxy};

pub fn spawn_battery_monitor(sender: mpsc::Sender<AppMessage>) {
    thread::spawn(move || {
        let conn = match DbusConnection::system() {
            Ok(conn) => conn,
            Err(err) => {
                eprintln!("Battery: failed to connect to system bus: {err}");
                return;
            }
        };

        let proxy = match DbusProxy::new(
            &conn,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
        ) {
            Ok(proxy) => proxy,
            Err(err) => {
                eprintln!("Battery: UPower proxy failed: {err}");
                return;
            }
        };

        let mut last: Option<bool> = None;
        loop {
            match proxy.get_property::<bool>("OnBattery") {
                Ok(on_battery) => {
                    if last != Some(on_battery) {
                        last = Some(on_battery);
                        let _ = sender.send(AppMessage::BatteryStateChanged(on_battery));
                    }
                }
                Err(_) => {
                    // UPower might not be available in some environments.
                    return;
                }
            }
            thread::sleep(Duration::from_secs(5));
        }
    });
}
