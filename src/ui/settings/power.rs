use crate::config::Config;
use crate::desktop;
use crate::i18n::{tr, Language};
use gtk4::prelude::*;
use gtk4::{Align, Button, ListBox, Switch};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct PowerWidgets {
    pub group: adw::PreferencesGroup,
    pub apps_group: adw::PreferencesGroup,
    pub inhibit_switch: Switch,
    pub power_integration_switch: Switch,
    pub lock_screen_switch: Switch,
    pub mpris_pause_switch: Switch,
    pub app_inhibit_list: ListBox,
    pub app_inhibit_entry: adw::EntryRow,
    pub app_inhibit_add_button: Button,
    pub app_inhibit_refresh_button: Button,
    pub app_inhibit_apps: Rc<RefCell<Vec<String>>>,
}

pub fn build_power_group(config: &Config, lang: Language) -> PowerWidgets {
    let power_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Питание"))
        .build();
    let show_desktop_integrations = desktop::is_kde_or_gnome();

    let inhibit_row = adw::ActionRow::builder()
        .title(tr(lang, "Блокировать спящий режим"))
        .subtitle(tr(lang, "Предотвращает переход системы в сон"))
        .build();

    let inhibit_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().inhibit_sleep)
        .build();
    inhibit_row.add_suffix(&inhibit_switch);

    power_group.add(&inhibit_row);

    let power_integration_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().power_integration_enabled)
        .build();
    if show_desktop_integrations {
        let power_integration_row = adw::ActionRow::builder()
            .title(tr(lang, "Интеграция с настройками питания"))
            .subtitle(tr(
                lang,
                "KDE/GNOME: управление энергосбережением при скринсейвере",
            ))
            .build();
        power_integration_row.add_suffix(&power_integration_switch);
        power_group.add(&power_integration_row);
    }

    let lock_screen_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().lock_screen_enabled)
        .build();
    if show_desktop_integrations {
        let lock_screen_row = adw::ActionRow::builder()
            .title(tr(lang, "Блокировать экран при активации"))
            .subtitle(tr(
                lang,
                "KDE/GNOME: интеграция с системным экраном блокировки",
            ))
            .build();
        lock_screen_row.add_suffix(&lock_screen_switch);
        power_group.add(&lock_screen_row);
    }

    let mpris_pause_row = adw::ActionRow::builder()
        .title(tr(lang, "Приостанавливать медиаплееры (MPRIS)"))
        .subtitle(tr(
            lang,
            "Останавливает воспроизведение при запуске скринсейвера",
        ))
        .build();
    let mpris_pause_switch = Switch::builder()
        .valign(Align::Center)
        .active(config.active_profile().mpris_pause_enabled)
        .build();
    mpris_pause_row.add_suffix(&mpris_pause_switch);
    power_group.add(&mpris_pause_row);

    let apps_group = adw::PreferencesGroup::builder()
        .title(tr(lang, "Исключения приложений"))
        .description(tr(
            lang,
            "Скринсейвер не запускается автоматически, если эти приложения запущены",
        ))
        .build();
    let app_inhibit_list = ListBox::new();
    app_inhibit_list.add_css_class("boxed-list");
    apps_group.add(&app_inhibit_list);

    let app_inhibit_entry = adw::EntryRow::builder()
        .title(tr(lang, "Добавить приложение"))
        .build();
    let app_inhibit_add_button = Button::with_label(tr(lang, "Добавить"));
    app_inhibit_entry.add_suffix(&app_inhibit_add_button);
    apps_group.add(&app_inhibit_entry);

    let app_inhibit_refresh_row = adw::ActionRow::builder()
        .title(tr(lang, "Обновить список"))
        .build();
    let app_inhibit_refresh_button = Button::with_label(tr(lang, "Обновить"));
    app_inhibit_refresh_row.add_suffix(&app_inhibit_refresh_button);
    apps_group.add(&app_inhibit_refresh_row);

    let app_inhibit_apps = Rc::new(RefCell::new(
        config.active_profile().app_inhibit_list.clone(),
    ));

    PowerWidgets {
        group: power_group,
        apps_group,
        inhibit_switch,
        power_integration_switch,
        lock_screen_switch,
        mpris_pause_switch,
        app_inhibit_list,
        app_inhibit_entry,
        app_inhibit_add_button,
        app_inhibit_refresh_button,
        app_inhibit_apps,
    }
}
