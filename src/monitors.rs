use crate::i18n::{tr, Language};
use gdk4::prelude::*;
use gdk4::{Display, Monitor};

pub fn list_monitors(display: &Display) -> Vec<Monitor> {
    let list = display.monitors();
    (0..list.n_items())
        .filter_map(|i| list.item(i)?.downcast::<Monitor>().ok())
        .collect()
}

pub fn monitor_id(monitor: &Monitor) -> String {
    if let Some(connector) = monitor.connector() {
        let connector = connector.to_string();
        if !connector.trim().is_empty() {
            return format!("connector:{connector}");
        }
    }

    let manufacturer = monitor.manufacturer().unwrap_or_default().to_string();
    let model = monitor.model().unwrap_or_default().to_string();
    let geom = monitor.geometry();
    format!(
        "fallback:{manufacturer}|{model}|{}x{}+{}+{}",
        geom.width(),
        geom.height(),
        geom.x(),
        geom.y()
    )
}

pub fn monitor_title(lang: Language, index: usize, monitor: &Monitor) -> String {
    let idx = index + 1;
    if let Some(connector) = monitor.connector() {
        let connector = connector.to_string();
        if !connector.trim().is_empty() {
            return format!("{} {idx} ({connector})", tr(lang, "Монитор"));
        }
    }
    format!("{} {idx}", tr(lang, "Монитор"))
}

pub fn monitor_subtitle(monitor: &Monitor) -> String {
    let geom = monitor.geometry();
    let mut parts = Vec::new();

    let manufacturer = monitor.manufacturer().unwrap_or_default().to_string();
    let model = monitor.model().unwrap_or_default().to_string();
    let name = format!("{manufacturer} {model}").trim().to_string();
    if !name.trim().is_empty() {
        parts.push(name);
    }

    parts.push(format!("{}×{}", geom.width(), geom.height()));

    if let Some(connector) = monitor.connector() {
        let connector = connector.to_string();
        if !connector.trim().is_empty() {
            parts.push(connector);
        }
    }

    parts.join(" • ")
}
