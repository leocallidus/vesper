pub fn desktop_targets() -> (bool, bool) {
    let desktop = desktop_name();
    let kde = desktop.contains("kde") || desktop.contains("plasma");
    let gnome = desktop.contains("gnome");
    (kde, gnome)
}

pub fn is_kde_or_gnome() -> bool {
    let (kde, gnome) = desktop_targets();
    kde || gnome
}

fn desktop_name() -> String {
    let raw = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
        .or_else(|_| std::env::var("DESKTOP_SESSION"))
        .unwrap_or_default();
    raw.to_ascii_lowercase()
}
