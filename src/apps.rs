use gio::prelude::*;
use gio::DesktopAppInfo;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
use x11rb::rust_connection::RustConnection;

#[derive(Clone, Debug)]
pub struct RunningGuiApp {
    pub id: String,
    pub display_name: String,
    pub icon: Option<gio::Icon>,
}

pub fn normalize_app_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut candidate = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string();
    if candidate.is_empty() {
        return None;
    }
    if let Some(file_name) = Path::new(&candidate).file_name().and_then(|n| n.to_str()) {
        candidate = file_name.to_string();
    }
    if let Some(stripped) = candidate.strip_suffix(".desktop") {
        candidate = stripped.to_string();
    }
    let candidate = candidate.trim().to_ascii_lowercase();
    if candidate.is_empty() {
        None
    } else {
        Some(candidate)
    }
}

pub fn list_running_gui_apps() -> Vec<RunningGuiApp> {
    let gui_pids = gui_process_ids();
    let uid = current_uid();
    let mut apps: HashMap<String, RunningGuiApp> = HashMap::new();
    for pid in gui_pids {
        let pid_path = pid_path(pid);
        if let Some(uid) = uid {
            if let Some(proc_uid) = process_uid(&pid_path) {
                if proc_uid != uid {
                    continue;
                }
            }
        }
        collect_app_entries(&pid_path, &mut apps);
    }
    for name in fallback_gui_app_names(true, false)
        .into_iter()
        .chain(fallback_gui_app_names(false, true))
    {
        add_app_by_id(&mut apps, &name);
    }
    let mut list: Vec<RunningGuiApp> = apps.into_values().collect();
    list.sort_by(|a, b| {
        a.display_name
            .to_ascii_lowercase()
            .cmp(&b.display_name.to_ascii_lowercase())
    });
    list
}

pub fn any_inhibit_app_running(apps: &[String]) -> bool {
    if apps.is_empty() {
        return false;
    }
    let mut targets: HashSet<String> = HashSet::new();
    for name in apps {
        if let Some(norm) = normalize_app_name(name) {
            targets.insert(norm);
        }
    }
    if targets.is_empty() {
        return false;
    }
    for app in list_running_gui_apps() {
        if targets.contains(&app.id) {
            return true;
        }
    }
    false
}

fn gui_process_ids() -> HashSet<u32> {
    let mut pids = HashSet::new();
    if let Some(x11) = x11_window_pids() {
        pids.extend(x11);
    }
    if let Some(x11) = x11_socket_pids() {
        pids.extend(x11);
    }
    if let Some(wayland) = wayland_client_pids() {
        pids.extend(wayland);
    }
    pids
}

fn x11_window_pids() -> Option<HashSet<u32>> {
    if std::env::var("DISPLAY").is_err() {
        return None;
    }
    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let screen = &conn.setup().roots[screen_num];
    let net_client_list = intern_atom(&conn, b"_NET_CLIENT_LIST")?;
    let net_wm_pid = intern_atom(&conn, b"_NET_WM_PID")?;
    let reply = conn
        .get_property(
            false,
            screen.root,
            net_client_list,
            AtomEnum::WINDOW,
            0,
            u32::MAX,
        )
        .ok()?
        .reply()
        .ok()?;
    let mut pids = HashSet::new();
    let Some(windows) = reply.value32() else {
        return Some(pids);
    };
    for win in windows {
        let pid_reply = conn
            .get_property(false, win, net_wm_pid, AtomEnum::CARDINAL, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        if let Some(pid) = pid_reply.value32().and_then(|mut vals| vals.next()) {
            pids.insert(pid);
        }
    }
    Some(pids)
}

fn intern_atom(conn: &RustConnection, name: &[u8]) -> Option<u32> {
    conn.intern_atom(false, name)
        .ok()?
        .reply()
        .ok()
        .map(|r| r.atom)
}

fn x11_socket_pids() -> Option<HashSet<u32>> {
    if std::env::var("DISPLAY").is_err() {
        return None;
    }
    let inodes = socket_inodes_for_path_prefix("/tmp/.X11-unix/X");
    if inodes.is_empty() {
        return None;
    }
    Some(process_ids_with_socket_inodes(&inodes))
}

fn wayland_client_pids() -> Option<HashSet<u32>> {
    let inodes = wayland_socket_inodes();
    if inodes.is_empty() {
        return None;
    }
    Some(process_ids_with_socket_inodes(&inodes))
}

fn wayland_socket_inodes() -> HashSet<u64> {
    let mut inodes = HashSet::new();
    if let Ok(display) = std::env::var("WAYLAND_DISPLAY") {
        if display.starts_with('/') {
            if let Some(inode) = socket_inode_for_path(&display) {
                inodes.insert(inode);
            }
        } else if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
            let path = format!("{runtime}/{display}");
            if let Some(inode) = socket_inode_for_path(&path) {
                inodes.insert(inode);
            }
        }
    }
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        let prefix = format!("{runtime}/wayland-");
        inodes.extend(socket_inodes_for_path_prefix(&prefix));
    }
    if inodes.is_empty() {
        inodes.extend(socket_inodes_for_path_substring("/wayland-"));
    }
    inodes
}

fn socket_inode_for_path(path: &str) -> Option<u64> {
    let Ok(text) = fs::read_to_string("/proc/net/unix") else {
        return None;
    };
    for line in text.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let _ = parts.next()?;
        let _ = parts.next()?;
        let _ = parts.next()?;
        let _ = parts.next()?;
        let _ = parts.next()?;
        let _ = parts.next()?;
        let inode = parts.next()?;
        let path_part = parts.next().unwrap_or("");
        if path_part == path {
            return inode.parse().ok();
        }
    }
    None
}

fn socket_inodes_for_path_prefix(prefix: &str) -> HashSet<u64> {
    let mut inodes = HashSet::new();
    let Ok(text) = fs::read_to_string("/proc/net/unix") else {
        return inodes;
    };
    for line in text.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();
        let inode = parts.next();
        let path_part = parts.next().unwrap_or("");
        if path_part.starts_with(prefix) {
            if let Some(inode) = inode.and_then(|v| v.parse::<u64>().ok()) {
                inodes.insert(inode);
            }
        }
    }
    inodes
}

fn socket_inodes_for_path_substring(needle: &str) -> HashSet<u64> {
    let mut inodes = HashSet::new();
    let Ok(text) = fs::read_to_string("/proc/net/unix") else {
        return inodes;
    };
    for line in text.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();
        let inode = parts.next();
        let path_part = parts.next().unwrap_or("");
        if path_part.contains(needle) {
            if let Some(inode) = inode.and_then(|v| v.parse::<u64>().ok()) {
                inodes.insert(inode);
            }
        }
    }
    inodes
}

fn process_ids_with_socket_inodes(inodes: &HashSet<u64>) -> HashSet<u32> {
    let uid = current_uid();
    let mut pids = HashSet::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return pids;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let pid_str = file_name.to_string_lossy();
        if !pid_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let pid_path = entry.path();
        if let Some(uid) = uid {
            if let Some(proc_uid) = process_uid(&pid_path) {
                if proc_uid != uid {
                    continue;
                }
            }
        }
        if process_has_socket_inodes(&pid_path, inodes) {
            if let Ok(pid) = pid_str.parse::<u32>() {
                pids.insert(pid);
            }
        }
    }
    pids
}

fn process_has_socket_inodes(pid_path: &Path, inodes: &HashSet<u64>) -> bool {
    let fd_dir = pid_path.join("fd");
    let Ok(entries) = fs::read_dir(fd_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(target) = fs::read_link(entry.path()) else {
            continue;
        };
        let Some(target) = target.to_str() else {
            continue;
        };
        let Some(stripped) = target.strip_prefix("socket:[") else {
            continue;
        };
        let Some(stripped) = stripped.strip_suffix(']') else {
            continue;
        };
        if let Ok(inode) = stripped.parse::<u64>() {
            if inodes.contains(&inode) {
                return true;
            }
        }
    }
    false
}

fn fallback_gui_app_names(require_hint: bool, require_no_tty: bool) -> Vec<String> {
    let uid = current_uid();
    let mut names = HashSet::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let pid_str = file_name.to_string_lossy();
        if !pid_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let pid_path = entry.path();
        if let Some(uid) = uid {
            if let Some(proc_uid) = process_uid(&pid_path) {
                if proc_uid != uid {
                    continue;
                }
            }
        }
        let env = match fs::read(pid_path.join("environ")) {
            Ok(env) => env,
            Err(_) => continue,
        };
        let env_text = String::from_utf8_lossy(&env);
        if !env_has_key(&env_text, "DISPLAY") && !env_has_key(&env_text, "WAYLAND_DISPLAY") {
            continue;
        }
        let has_hint = has_gui_hint(&env_text);
        if require_hint && !has_hint {
            continue;
        }
        if require_no_tty {
            if process_has_tty(&pid_path).unwrap_or(true) {
                continue;
            }
        }
        if let Some(name) = candidate_id_from_env(&env_text) {
            names.insert(name);
        }
        if let Some(name) = process_name(&pid_path).and_then(|name| normalize_app_name(&name)) {
            names.insert(name);
        }
    }
    let mut list: Vec<String> = names.into_iter().collect();
    list.sort();
    list
}

fn env_has_key(env_text: &str, key: &str) -> bool {
    let prefix = format!("{key}=");
    env_text.split('\0').any(|entry| entry.starts_with(&prefix))
}

fn has_gui_hint(env_text: &str) -> bool {
    env_has_key(env_text, "GIO_LAUNCHED_DESKTOP_FILE")
        || env_has_key(env_text, "DESKTOP_STARTUP_ID")
        || env_has_key(env_text, "GTK_APPLICATION_ID")
        || env_has_key(env_text, "QT_QPA_PLATFORM")
        || env_has_key(env_text, "MOZ_ENABLE_WAYLAND")
        || env_has_key(env_text, "SDL_VIDEODRIVER")
}

fn process_has_tty(pid_path: &Path) -> Option<bool> {
    let text = fs::read_to_string(pid_path.join("stat")).ok()?;
    let idx = text.rfind(") ")?;
    let rest = &text[idx + 2..];
    let mut fields = rest.split_whitespace();
    let _state = fields.next()?;
    let _ppid = fields.next()?;
    let _pgrp = fields.next()?;
    let _session = fields.next()?;
    let tty_nr: i64 = fields.next()?.parse().ok()?;
    Some(tty_nr != 0)
}

fn process_name(pid_path: &Path) -> Option<String> {
    let cmdline = pid_path.join("cmdline");
    if let Ok(bytes) = fs::read(cmdline) {
        if !bytes.is_empty() {
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            let first = String::from_utf8_lossy(&bytes[..end]).to_string();
            if !first.trim().is_empty() {
                return Some(first);
            }
        }
    }
    let comm = pid_path.join("comm");
    if let Ok(bytes) = fs::read(comm) {
        let text = String::from_utf8_lossy(&bytes).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

fn collect_app_entries(pid_path: &Path, apps: &mut HashMap<String, RunningGuiApp>) {
    let env_text = fs::read(pid_path.join("environ"))
        .ok()
        .map(|env| String::from_utf8_lossy(&env).to_string());
    if let Some(env_text) = env_text.as_deref() {
        if let Some(entry) = app_entry_from_env(env_text) {
            insert_app_entry(apps, entry);
        }
    }
    if let Some(entry) = app_entry_from_process(pid_path) {
        insert_app_entry(apps, entry);
    }
}

fn insert_app_entry(apps: &mut HashMap<String, RunningGuiApp>, entry: RunningGuiApp) {
    let key = entry.id.clone();
    apps.entry(key)
        .and_modify(|existing| {
            if existing.display_name == existing.id && entry.display_name != entry.id {
                existing.display_name = entry.display_name.clone();
            }
            if existing.icon.is_none() {
                existing.icon = entry.icon.clone();
            }
        })
        .or_insert(entry);
}

fn add_app_by_id(apps: &mut HashMap<String, RunningGuiApp>, id: &str) {
    let Some(normalized) = normalize_app_name(id) else {
        return;
    };
    if apps.contains_key(&normalized) {
        return;
    }
    if let Some(entry) = resolve_app_info(&normalized) {
        insert_app_entry(apps, entry);
        return;
    }
    if is_background_service(&normalized, None, None) {
        return;
    }
    insert_app_entry(
        apps,
        RunningGuiApp {
            id: normalized.clone(),
            display_name: normalized,
            icon: None,
        },
    );
}

fn app_entry_from_env(env_text: &str) -> Option<RunningGuiApp> {
    if let Some(desktop_id) = desktop_id_from_env(env_text) {
        return app_entry_from_desktop_id(&desktop_id)
            .or_else(|| entry_from_id_fallback(&desktop_id));
    }
    if let Some(app_id) = app_id_from_env(env_text) {
        return resolve_app_info(&app_id).or_else(|| entry_from_id_fallback(&app_id));
    }
    None
}

fn app_entry_from_process(pid_path: &Path) -> Option<RunningGuiApp> {
    let name = process_name(pid_path)?;
    resolve_app_info(&name).or_else(|| entry_from_id_fallback(&name))
}

pub fn resolve_app_info(id: &str) -> Option<RunningGuiApp> {
    let candidate = id.trim();
    if candidate.is_empty() {
        return None;
    }
    if let Some(entry) = app_entry_from_desktop_id(candidate) {
        return Some(entry);
    }
    let normalized = normalize_app_name(candidate)?;
    let desktop_id = format!("{normalized}.desktop");
    if let Some(entry) = app_entry_from_desktop_id(&desktop_id) {
        return Some(entry);
    }
    entry_from_id_fallback(&normalized)
}

fn app_entry_from_desktop_id(desktop_id: &str) -> Option<RunningGuiApp> {
    let app = DesktopAppInfo::new(desktop_id)?;
    if app.is_hidden() || app.is_nodisplay() {
        return None;
    }
    let display_name = app.display_name();
    let display_name = if display_name.is_empty() {
        app.name().to_string()
    } else {
        display_name.to_string()
    };
    let id_source = app
        .id()
        .map(|value| value.to_string())
        .unwrap_or_else(|| desktop_id.to_string());
    let id = normalize_app_name(&id_source)?;
    if is_background_service(&id, Some(desktop_id), Some(&display_name)) {
        return None;
    }
    Some(RunningGuiApp {
        id,
        display_name,
        icon: app.icon(),
    })
}

fn entry_from_id_fallback(id: &str) -> Option<RunningGuiApp> {
    let normalized = normalize_app_name(id)?;
    if is_background_service(&normalized, None, None) {
        return None;
    }
    Some(RunningGuiApp {
        id: normalized.clone(),
        display_name: normalized,
        icon: None,
    })
}

fn candidate_id_from_env(env_text: &str) -> Option<String> {
    if let Some(desktop_id) = desktop_id_from_env(env_text) {
        return normalize_app_name(&desktop_id);
    }
    if let Some(app_id) = app_id_from_env(env_text) {
        return normalize_app_name(&app_id);
    }
    None
}

fn desktop_id_from_env(env_text: &str) -> Option<String> {
    if let Some(value) = env_value(env_text, "GIO_LAUNCHED_DESKTOP_FILE") {
        return desktop_id_from_value(value);
    }
    if let Some(value) = env_value(env_text, "DESKTOP_STARTUP_ID") {
        if let Some(desktop) = desktop_id_from_startup(value) {
            return desktop_id_from_value(desktop);
        }
    }
    None
}

fn app_id_from_env(env_text: &str) -> Option<String> {
    if let Some(value) = env_value(env_text, "FLATPAK_ID") {
        return Some(value.to_string());
    }
    if let Some(value) = env_value(env_text, "GTK_APPLICATION_ID") {
        return Some(value.to_string());
    }
    if let Some(value) = env_value(env_text, "SNAP_INSTANCE_NAME") {
        return Some(value.to_string());
    }
    None
}

fn desktop_id_from_value(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_matches('"').trim_matches('\'');
    if trimmed.is_empty() {
        return None;
    }
    let mut candidate = trimmed;
    if let Some(pos) = candidate.find(".desktop") {
        candidate = &candidate[..pos + ".desktop".len()];
    }
    if let Some(file_name) = Path::new(candidate).file_name().and_then(|n| n.to_str()) {
        candidate = file_name;
    }
    if candidate.is_empty() {
        return None;
    }
    let candidate = if candidate.ends_with(".desktop") {
        candidate.to_string()
    } else {
        format!("{candidate}.desktop")
    };
    Some(candidate)
}

fn env_value<'a>(env_text: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}=");
    env_text
        .split('\0')
        .find_map(|entry| entry.strip_prefix(&prefix))
}

fn desktop_id_from_startup(value: &str) -> Option<&str> {
    let Some(pos) = value.find(".desktop") else {
        return None;
    };
    Some(&value[..pos + ".desktop".len()])
}

fn is_background_service(id: &str, desktop_id: Option<&str>, display_name: Option<&str>) -> bool {
    let id = id.to_ascii_lowercase();
    if is_background_service_id(&id) {
        return true;
    }
    if let Some(desktop_id) = desktop_id {
        let desktop_id = desktop_id.to_ascii_lowercase();
        if is_background_service_id(&desktop_id) {
            return true;
        }
        if desktop_id.starts_with("org.freedesktop.portal")
            || desktop_id.starts_with("org.freedesktop.impl.portal")
        {
            return true;
        }
    }
    if let Some(name) = display_name {
        let name = name.to_ascii_lowercase();
        if is_background_service_id(&name) {
            return true;
        }
    }
    false
}

fn is_background_service_id(value: &str) -> bool {
    const SERVICE_IDS: [&str; 20] = [
        "xdg-desktop-portal",
        "xdg-document-portal",
        "xdg-permission-store",
        "xdg-desktop-portal-gtk",
        "xdg-desktop-portal-gnome",
        "xdg-desktop-portal-kde",
        "xdg-desktop-portal-wlr",
        "xdg-desktop-portal-xapp",
        "at-spi2-registryd",
        "at-spi-bus-launcher",
        "dbus-daemon",
        "dbus-broker",
        "gvfsd",
        "gvfsd-fuse",
        "pipewire",
        "wireplumber",
        "pulseaudio",
        "rtkit-daemon",
        "gnome-keyring-daemon",
        "dconf-service",
    ];
    if SERVICE_IDS.contains(&value) {
        return true;
    }
    value.starts_with("xdg-desktop-portal")
}

fn pid_path(pid: u32) -> PathBuf {
    PathBuf::from("/proc").join(pid.to_string())
}

fn process_uid(pid_path: &Path) -> Option<u32> {
    let status = pid_path.join("status");
    let Ok(text) = fs::read_to_string(status) else {
        return None;
    };
    parse_uid(&text)
}

fn current_uid() -> Option<u32> {
    let Ok(text) = fs::read_to_string("/proc/self/status") else {
        return None;
    };
    parse_uid(&text)
}

fn parse_uid(status_text: &str) -> Option<u32> {
    for line in status_text.lines() {
        if line.starts_with("Uid:") {
            let mut parts = line.split_whitespace();
            let _ = parts.next()?;
            return parts.next()?.parse().ok();
        }
    }
    None
}
