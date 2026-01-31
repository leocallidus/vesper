use crate::config::Config;
use crate::i18n::{resolve_language, tr};
use crate::AppMessage;
use ksni::{MenuItem, Tray};
use ksni::TrayMethods;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;

pub struct TrayHandler {
    sender: Sender<AppMessage>,
    is_enabled: Arc<Mutex<bool>>,
    inhibit_sleep: Arc<Mutex<bool>>,
}

#[derive(Clone, Copy, Debug)]
pub enum TrayCommand {
    SetVisible(bool),
    Quit,
}

impl TrayHandler {
    pub fn new(
        sender: Sender<AppMessage>,
        is_enabled: Arc<Mutex<bool>>,
        inhibit_sleep: Arc<Mutex<bool>>,
    ) -> Self {
        Self {
            sender,
            is_enabled,
            inhibit_sleep,
        }
    }
}

pub fn spawn_tray_controller(
    sender: Sender<AppMessage>,
    is_enabled: Arc<Mutex<bool>>,
    inhibit_sleep: Arc<Mutex<bool>>,
    initially_visible: bool,
) -> tokio_mpsc::UnboundedSender<TrayCommand> {
    let (tx, mut rx) = tokio_mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async move {
            let mut visible = initially_visible;
            let mut handle = if visible {
                spawn_tray_handle(sender.clone(), Arc::clone(&is_enabled), Arc::clone(&inhibit_sleep))
                    .await
            } else {
                None
            };

            while let Some(cmd) = rx.recv().await {
                match cmd {
                    TrayCommand::SetVisible(next) => {
                        if next == visible {
                            continue;
                        }
                        visible = next;
                        if visible {
                            if handle.is_none() {
                                handle = spawn_tray_handle(
                                    sender.clone(),
                                    Arc::clone(&is_enabled),
                                    Arc::clone(&inhibit_sleep),
                                )
                                .await;
                            }
                        } else if let Some(h) = handle.take() {
                            h.shutdown().await;
                        }
                    }
                    TrayCommand::Quit => break,
                }
            }

            if let Some(h) = handle {
                h.shutdown().await;
            }
        });
    });
    tx
}

async fn spawn_tray_handle(
    sender: Sender<AppMessage>,
    is_enabled: Arc<Mutex<bool>>,
    inhibit_sleep: Arc<Mutex<bool>>,
) -> Option<ksni::Handle<TrayHandler>> {
    let tray = TrayHandler::new(sender, is_enabled, inhibit_sleep);
    match tray.spawn().await {
        Ok(handle) => Some(handle),
        Err(e) => {
            eprintln!("System tray not available: {}", e);
            None
        }
    }
}

impl Tray for TrayHandler {
    fn icon_name(&self) -> String {
        "vesper".into()
    }

    fn icon_theme_path(&self) -> String {
        use crate::ui::get_local_icon_theme_path;
        get_local_icon_theme_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn overlay_icon_name(&self) -> String {
        if *self.is_enabled.lock().unwrap() {
            String::new()
        } else {
            "emblem-pause".into()
        }
    }

    fn title(&self) -> String {
        "Vesper".into()
    }

    fn id(&self) -> String {
        "vesper".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.send(AppMessage::ShowMainWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        use ksni::menu::*;

        let is_enabled = *self.is_enabled.lock().unwrap();
        let inhibit_sleep = *self.inhibit_sleep.lock().unwrap();
        let config = Config::load();
        let lang = resolve_language(config.language);
        let active_profile = config.active_profile_index();
        let profile_items: Vec<MenuItem<Self>> = config
            .profiles
            .iter()
            .enumerate()
            .map(|(index, profile)| {
                let index = index as u8;
                CheckmarkItem {
                    label: profile.name.clone(),
                    checked: index as usize == active_profile,
                    activate: Box::new(move |this: &mut Self| {
                        let _ = this.sender.send(AppMessage::SwitchProfile(index));
                    }),
                    ..Default::default()
                }
                .into()
            })
            .collect();
        let profiles_menu = SubMenu {
            label: tr(lang, "Профили").into(),
            submenu: profile_items,
            ..Default::default()
        };

        vec![
            CheckmarkItem {
                label: tr(lang, "Включено").into(),
                checked: is_enabled,
                activate: Box::new(|this: &mut Self| {
                    let current = *this.is_enabled.lock().unwrap();
                    *this.is_enabled.lock().unwrap() = !current;
                    let _ = this.sender.send(AppMessage::ToggleEnabled(!current));
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: tr(lang, "Блокировать сон").into(),
                checked: inhibit_sleep,
                activate: Box::new(|this: &mut Self| {
                    let current = *this.inhibit_sleep.lock().unwrap();
                    *this.inhibit_sleep.lock().unwrap() = !current;
                    let _ = this.sender.send(AppMessage::ToggleInhibitSleep(!current));
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            profiles_menu.into(),
            MenuItem::Separator,
            StandardItem {
                label: tr(lang, "Настройки").into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(AppMessage::OpenSettings);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: tr(lang, "Запустить").into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(AppMessage::StartScreensaver);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: tr(lang, "Выход").into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(AppMessage::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}
