use crate::autostart;
use crate::config::Config;
use crate::i18n::{tr, Language};
use gtk4::{Align, Switch};
use libadwaita as adw;
use libadwaita::prelude::*;

pub struct AutostartWidgets {
    pub group: adw::PreferencesGroup,
    pub autostart_switch: Switch,
    pub start_minimized_switch: Switch,
}

pub fn build_autostart_group(config: &Config, lang: Language) -> AutostartWidgets {
    let group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Автозапуск"))
        .build();

    let autostart_row = adw::ActionRow::builder()
        .title(tr(lang, "Запускать при входе в систему"))
        .subtitle(tr(lang, "Создает запись в ~/.config/autostart"))
        .build();
    let autostart_switch = Switch::builder()
        .valign(Align::Center)
        .active(autostart::is_autostart_enabled())
        .build();
    autostart_row.add_suffix(&autostart_switch);
    group.add(&autostart_row);

    let start_minimized_row = adw::ActionRow::builder()
        .title(tr(lang, "Запускать в фоне"))
        .subtitle(tr(lang, "Не показывать главное окно при запуске"))
        .build();
    let start_minimized_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.start_minimized)
        .build();
    start_minimized_row.add_suffix(&start_minimized_switch);
    group.add(&start_minimized_row);

    AutostartWidgets {
        group,
        autostart_switch,
        start_minimized_switch,
    }
}
