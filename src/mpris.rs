use std::collections::HashMap;

use zbus::blocking::{Connection as DbusConnection, Proxy as DbusProxy};
use zbus::zvariant::OwnedValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlayingInfo {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
}

pub fn query_now_playing() -> Result<Option<NowPlayingInfo>, Box<dyn std::error::Error>> {
    let conn = DbusConnection::session()?;
    let dbus_proxy = DbusProxy::new(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    let names: Vec<String> = dbus_proxy.call("ListNames", &())?;

    let mut playing: Option<NowPlayingInfo> = None;
    let mut fallback: Option<NowPlayingInfo> = None;

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
            Err(_) => continue,
        };

        let status: String = match player_proxy.get_property("PlaybackStatus") {
            Ok(value) => value,
            Err(_) => continue,
        };

        let metadata: HashMap<String, OwnedValue> = match player_proxy.get_property("Metadata") {
            Ok(value) => value,
            Err(_) => continue,
        };

        let info = NowPlayingInfo {
            player: name.clone(),
            title: get_metadata_string(&metadata, "xesam:title"),
            artist: get_metadata_artist(&metadata).unwrap_or_default(),
            album: get_metadata_string(&metadata, "xesam:album"),
            art_url: get_metadata_string_opt(&metadata, "mpris:artUrl"),
        };

        if status == "Playing" {
            playing = Some(info);
            break;
        }

        if fallback.is_none() && !info.title.is_empty() {
            fallback = Some(info);
        }
    }

    Ok(playing.or(fallback))
}

fn get_metadata_string_opt(meta: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let value = meta.get(key)?;
    let s: &str = value.try_into().ok()?;
    Some(s.trim().to_string())
}

fn get_metadata_string(meta: &HashMap<String, OwnedValue>, key: &str) -> String {
    get_metadata_string_opt(meta, key).unwrap_or_default()
}

fn get_metadata_artist(meta: &HashMap<String, OwnedValue>) -> Option<String> {
    let value = meta.get("xesam:artist")?;

    // MPRIS spec: xesam:artist is "as" (array of strings).
    let artists: Vec<String> = value.try_clone().ok()?.try_into().ok()?;
    let joined = artists
        .into_iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}
