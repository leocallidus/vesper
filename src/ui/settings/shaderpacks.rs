use crate::i18n::{tr, Language};
use gdk_pixbuf::Pixbuf;
use gio::prelude::*;
use gtk4::prelude::*;
use gtk4::{Align, ContentFit, Orientation};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::shaderpacks::{
    delete_installed_shaderpack_and_clean_config, discover_installed_shaderpacks,
    import_shaderpack_from_path,
    shaderpacks_root_dir, ImportConflictPolicy, ImportShaderpackError,
};

pub(super) struct ShaderpacksWidgets {
    pub page: gtk4::ScrolledWindow,
    empty_area: gtk4::Frame,
    pack_list: gtk4::ListBox,
    import_button: gtk4::Button,
    refresh_button: gtk4::Button,
}

pub(super) fn build_shaderpacks_page(lang: Language) -> ShaderpacksWidgets {
    let root = gtk4::Box::new(Orientation::Vertical, 12);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let toolbar = gtk4::Box::new(Orientation::Horizontal, 8);
    toolbar.set_halign(Align::End);

    let import_button = gtk4::Button::with_label(tr(lang, "Импортировать"));
    import_button.add_css_class("suggested-action");
    toolbar.append(&import_button);

    let refresh_button = gtk4::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text(tr(lang, "Обновить"))
        .build();
    refresh_button.add_css_class("flat");
    toolbar.append(&refresh_button);

    root.append(&toolbar);

    let empty_area = gtk4::Frame::new(None);
    empty_area.add_css_class("card");
    empty_area.set_hexpand(true);
    empty_area.set_vexpand(true);

    let empty_box = gtk4::Box::new(Orientation::Vertical, 10);
    empty_box.set_halign(Align::Center);
    empty_box.set_valign(Align::Center);
    empty_box.set_margin_top(36);
    empty_box.set_margin_bottom(36);
    empty_box.set_margin_start(24);
    empty_box.set_margin_end(24);

    let icon = gtk4::Image::from_icon_name("applications-graphics-symbolic");
    icon.set_pixel_size(64);
    empty_box.append(&icon);

    let title = gtk4::Label::new(Some(tr(lang, "Шейдерпаки")));
    title.add_css_class("title-1");
    title.set_wrap(true);
    title.set_justify(gtk4::Justification::Center);
    empty_box.append(&title);

    let hint = gtk4::Label::new(Some(tr(lang, "Перетащите папку шейдерпака сюда…")));
    hint.add_css_class("dim-label");
    hint.set_wrap(true);
    hint.set_justify(gtk4::Justification::Center);
    empty_box.append(&hint);

    let hint2 = gtk4::Label::new(Some(tr(lang, "Или нажмите «Импортировать».")));
    hint2.add_css_class("dim-label");
    hint2.set_wrap(true);
    hint2.set_justify(gtk4::Justification::Center);
    empty_box.append(&hint2);

    empty_area.set_child(Some(&empty_box));
    root.append(&empty_area);

    let pack_list = gtk4::ListBox::new();
    pack_list.add_css_class("boxed-list");
    pack_list.set_selection_mode(gtk4::SelectionMode::None);
    root.append(&pack_list);

    let clamp = adw::Clamp::new();
    clamp.set_child(Some(&root));
    clamp.set_maximum_size(900);
    clamp.set_tightening_threshold(700);

    let page = gtk4::ScrolledWindow::new();
    page.set_child(Some(&clamp));
    page.set_vexpand(true);
    page.set_hexpand(true);

    ShaderpacksWidgets {
        page,
        empty_area,
        pack_list,
        import_button,
        refresh_button,
    }
}

pub(super) fn connect_shaderpacks(
    widgets: &ShaderpacksWidgets,
    controller: Rc<super::SettingsController>,
    toast_overlay: adw::ToastOverlay,
) {
    let pack_list_weak = widgets.pack_list.downgrade();
    let empty_area_weak = widgets.empty_area.downgrade();
    let refresh: Rc<dyn Fn()> = Rc::new({
        let controller = controller.clone();
        let toast_overlay = toast_overlay.clone();
        move || {
            refresh_from_weaks(
                pack_list_weak.clone(),
                empty_area_weak.clone(),
                controller.clone(),
                toast_overlay.clone(),
            );
        }
    });

    widgets.refresh_button.connect_clicked({
        let refresh = refresh.clone();
        move |_| refresh()
    });

    widgets.import_button.connect_clicked({
        let controller = controller.clone();
        let toast_overlay = toast_overlay.clone();
        let refresh = refresh.clone();
        move |_| {
            let Some(window) = controller.window_weak.upgrade() else {
                return;
            };
            let choose = adw::MessageDialog::builder()
                .transient_for(&window)
                .modal(true)
                .heading(tr(controller.lang, "Импортировать"))
                .body(tr(controller.lang, "Выберите источник импорта"))
                .build();
            choose.add_response("cancel", tr(controller.lang, "Отмена"));
            choose.add_response("folder", tr(controller.lang, "Папка"));
            choose.add_response("zip", tr(controller.lang, ".zip"));
            choose.set_default_response(Some("folder"));
            choose.connect_response(None, {
                let controller = controller.clone();
                let toast_overlay = toast_overlay.clone();
                let refresh = refresh.clone();
                move |d, response| {
                    let Some(window) = controller.window_weak.upgrade() else {
                        d.close();
                        return;
                    };
                    if response == "folder" {
                        let dialog = gtk4::FileDialog::new();
                        dialog.set_title(tr(controller.lang, "Импортировать шейдерпак"));
                        dialog.select_folder(Some(&window), None::<&gio::Cancellable>, {
                            let controller = controller.clone();
                            let toast_overlay = toast_overlay.clone();
                            let refresh = refresh.clone();
                            move |res| {
                                let Ok(folder) = res else { return };
                                let Some(path) = folder.path() else { return };
                                handle_import_path(&controller, &toast_overlay, &refresh, &path);
                            }
                        });
                    } else if response == "zip" {
                        let dialog = gtk4::FileDialog::new();
                        dialog.set_title(tr(controller.lang, "Импортировать .zip"));
                        let filter = gtk4::FileFilter::new();
                        filter.set_name(Some("ZIP"));
                        filter.add_suffix("zip");
                        let filters = gio::ListStore::new::<gtk4::FileFilter>();
                        filters.append(&filter);
                        dialog.set_filters(Some(&filters));
                        dialog.set_default_filter(Some(&filter));
                        dialog.open(Some(&window), None::<&gio::Cancellable>, {
                            let controller = controller.clone();
                            let toast_overlay = toast_overlay.clone();
                            let refresh = refresh.clone();
                            move |res| {
                                let Ok(file) = res else { return };
                                let Some(path) = file.path() else { return };
                                handle_import_path(&controller, &toast_overlay, &refresh, &path);
                            }
                        });
                    }
                    d.close();
                }
            });
            choose.present();
        }
    });

    install_drop_target(&widgets.empty_area, &controller, &toast_overlay, &refresh);
    install_drop_target(&widgets.pack_list, &controller, &toast_overlay, &refresh);

    // Live reload (best-effort): periodically check for changes in the installed shaderpacks dir.
    let last_fingerprint = Rc::new(Cell::new(shaderpacks_fingerprint()));
    glib::timeout_add_seconds_local(2, {
        let pack_list_weak = widgets.pack_list.downgrade();
        let empty_area_weak = widgets.empty_area.downgrade();
        let last_fingerprint = last_fingerprint.clone();
        let refresh = refresh.clone();
        move || {
            if pack_list_weak.upgrade().is_none() && empty_area_weak.upgrade().is_none() {
                return glib::ControlFlow::Break;
            }
            let fp = shaderpacks_fingerprint();
            if fp != last_fingerprint.get() {
                last_fingerprint.set(fp);
                refresh();
            }
            glib::ControlFlow::Continue
        }
    });

    refresh();
}

fn shaderpacks_fingerprint() -> u64 {
    let root = shaderpacks_root_dir();
    let mut hasher = DefaultHasher::new();
    root.to_string_lossy().hash(&mut hasher);

    if !root.is_dir() {
        0u8.hash(&mut hasher);
        return hasher.finish();
    }

    let Ok(rd) = fs::read_dir(&root) else {
        1u8.hash(&mut hasher);
        return hasher.finish();
    };

    for entry in rd.flatten() {
        let path = entry.path();
        entry.file_name().hash(&mut hasher);
        hash_metadata(&path, &mut hasher);
        if path.is_dir() {
            hash_pack_dir(&path, &mut hasher);
        }
    }

    hasher.finish()
}

fn hash_pack_dir(pack_dir: &Path, hasher: &mut DefaultHasher) {
    for file in ["shaderpack.toml", "shaderpacklogo.png"] {
        hash_metadata(&pack_dir.join(file), hasher);
    }

    let shaders_dir = pack_dir.join("shaders");
    if !shaders_dir.is_dir() {
        return;
    }
    let Ok(rd) = fs::read_dir(&shaders_dir) else {
        return;
    };
    for entry in rd.flatten() {
        let shader_dir = entry.path();
        entry.file_name().hash(hasher);
        hash_metadata(&shader_dir, hasher);
        if shader_dir.is_dir() {
            hash_shader_dir(&shader_dir, hasher);
        }
    }
}

fn hash_shader_dir(shader_dir: &Path, hasher: &mut DefaultHasher) {
    let Ok(rd) = fs::read_dir(shader_dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        entry.file_name().hash(hasher);
        hash_metadata(&path, hasher);

        if path.is_dir() && path.file_name().and_then(|s| s.to_str()) == Some("assets") {
            if let Ok(rd2) = fs::read_dir(&path) {
                for entry2 in rd2.flatten() {
                    let p2 = entry2.path();
                    entry2.file_name().hash(hasher);
                    hash_metadata(&p2, hasher);
                }
            }
        }
    }
}

fn hash_metadata(path: &Path, hasher: &mut DefaultHasher) {
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    meta.len().hash(hasher);
    if let Ok(modified) = meta.modified() {
        hash_system_time(modified, hasher);
    }
}

fn hash_system_time(t: SystemTime, hasher: &mut DefaultHasher) {
    let Ok(dur) = t.duration_since(UNIX_EPOCH) else {
        return;
    };
    dur.as_secs().hash(hasher);
    dur.subsec_nanos().hash(hasher);
}

fn refresh_from_weaks(
    pack_list_weak: glib::WeakRef<gtk4::ListBox>,
    empty_area_weak: glib::WeakRef<gtk4::Frame>,
    controller: Rc<super::SettingsController>,
    toast_overlay: adw::ToastOverlay,
) {
    let (Some(pack_list), Some(empty_area)) = (pack_list_weak.upgrade(), empty_area_weak.upgrade())
    else {
        return;
    };
    rebuild_pack_list(
        &pack_list,
        &empty_area,
        controller,
        toast_overlay,
        pack_list_weak,
        empty_area_weak,
    );
}

fn rebuild_pack_list(
    pack_list: &gtk4::ListBox,
    empty_area: &gtk4::Frame,
    controller: Rc<super::SettingsController>,
    toast_overlay: adw::ToastOverlay,
    pack_list_weak: glib::WeakRef<gtk4::ListBox>,
    empty_area_weak: glib::WeakRef<gtk4::Frame>,
) {
    while let Some(child) = pack_list.first_child() {
        pack_list.remove(&child);
    }

    let packs = match discover_installed_shaderpacks() {
        Ok(v) => v,
        Err(err) => {
            toast_overlay.add_toast(adw::Toast::new(&err));
            Vec::new()
        }
    };

    empty_area.set_visible(packs.is_empty());
    pack_list.set_visible(!packs.is_empty());

    for pack in packs {
        let row = build_pack_row(&pack, controller.lang);

        let pack_dir = pack.dir.clone();
        row.open_button.connect_clicked({
            let toast_overlay = toast_overlay.clone();
            let lang = controller.lang;
            move |_| {
                if let Err(err) = open_path_in_file_manager(lang, &pack_dir) {
                    toast_overlay.add_toast(adw::Toast::new(&err));
                }
            }
        });

        let pack_for_check = pack.clone();
        row.check_button.connect_clicked({
            let controller = controller.clone();
            move |_| {
                show_pack_check_dialog(&controller, &pack_for_check);
            }
        });

        let pack_id = pack.id.clone();
        let pack_dir = pack.dir.clone();
        row.delete_button.connect_clicked({
            let controller = controller.clone();
            let toast_overlay = toast_overlay.clone();
            let pack_list_weak = pack_list_weak.clone();
            let empty_area_weak = empty_area_weak.clone();
            move |_| {
                confirm_delete_pack(
                    controller.clone(),
                    toast_overlay.clone(),
                    pack_id.clone(),
                    pack_dir.clone(),
                    pack_list_weak.clone(),
                    empty_area_weak.clone(),
                );
            }
        });

        pack_list.append(&row.row);
    }
}

struct PackRow {
    row: adw::ActionRow,
    open_button: gtk4::Button,
    check_button: gtk4::Button,
    delete_button: gtk4::Button,
}

fn build_pack_row(pack: &crate::shaderpacks::Shaderpack, lang: Language) -> PackRow {
    let title = pack.name.trim();
    let desc = pack.description.trim();
    let desc = truncate_350(desc);
    let subtitle = if desc.is_empty() {
        format!("{}: {}", tr(lang, "Шейдеров"), pack.shaders.len())
    } else {
        format!("{} • {}: {}", desc, tr(lang, "Шейдеров"), pack.shaders.len())
    };

    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(&subtitle)
        .build();

    if pack.logo_path.is_file() {
        let picture = gtk4::Picture::new();
        picture.set_content_fit(ContentFit::Contain);
        picture.set_can_shrink(true);
        picture.set_halign(Align::Center);
        picture.set_valign(Align::Center);
        picture.set_size_request(48, 48);
        if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&pack.logo_path, 96, 96, true) {
            let texture = gdk4::Texture::for_pixbuf(&pixbuf);
            picture.set_paintable(Some(&texture));
        } else {
            picture.set_filename(Some(&pack.logo_path));
        }
        row.add_prefix(&picture);
    }

    let actions = gtk4::Box::new(Orientation::Horizontal, 6);

    let open_button = gtk4::Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text(tr(lang, "Открыть папку"))
        .build();
    open_button.add_css_class("flat");
    actions.append(&open_button);

    let check_button = gtk4::Button::builder()
        .icon_name("document-properties-symbolic")
        .tooltip_text(tr(lang, "Проверить"))
        .build();
    check_button.add_css_class("flat");
    actions.append(&check_button);

    let delete_button = gtk4::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text(tr(lang, "Удалить"))
        .build();
    delete_button.add_css_class("flat");
    delete_button.add_css_class("destructive-action");
    actions.append(&delete_button);

    row.add_suffix(&actions);

    PackRow {
        row,
        open_button,
        check_button,
        delete_button,
    }
}

fn truncate_350(s: &str) -> String {
    if s.chars().count() <= 350 {
        return s.to_string();
    }
    let out: String = s.chars().take(350).collect();
    format!("{out}…")
}

fn open_path_in_file_manager(lang: Language, path: &Path) -> Result<(), String> {
    let file = gio::File::for_path(path);
    let uri = file.uri();
    gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
        .map_err(|e| format!("{}: {e}", tr(lang, "Не удалось открыть папку")))?;
    Ok(())
}

fn show_pack_check_dialog(
    controller: &Rc<super::SettingsController>,
    pack: &crate::shaderpacks::Shaderpack,
) {
    let Some(window) = controller.window_weak.upgrade() else {
        return;
    };

    let mut lines = Vec::new();
    lines.push(format!("{}: {}", tr(controller.lang, "Шейдеров"), pack.shaders.len()));
    for shader in &pack.shaders {
        let mut passes = Vec::new();
        if shader.detected.image.is_some() {
            passes.push("Image");
        }
        if shader.detected.common.is_some() {
            passes.push("Common");
        }
        for (idx, label) in ["BufferA", "BufferB", "BufferC", "BufferD"].iter().enumerate() {
            if shader.detected.buffers[idx].is_some() {
                passes.push(label);
            }
        }
        if shader.detected.sound.is_some() {
            passes.push("Sound");
        }
        let pass_text = if passes.is_empty() {
            tr(controller.lang, "Нет данных").to_string()
        } else {
            passes.join(", ")
        };
        lines.push(format!("- {}: {}", shader.name, pass_text));
    }

    let body = lines.join("\n");
    let dialog = adw::MessageDialog::builder()
        .transient_for(&window)
        .modal(true)
        .heading(pack.name.as_str())
        .body(&body)
        .build();
    dialog.add_response("ok", tr(controller.lang, "ОК"));
    dialog.set_default_response(Some("ok"));
    dialog.connect_response(None, |d, _| d.close());
    dialog.present();
}

fn confirm_delete_pack(
    controller: Rc<super::SettingsController>,
    toast_overlay: adw::ToastOverlay,
    pack_id: String,
    pack_dir: PathBuf,
    pack_list_weak: glib::WeakRef<gtk4::ListBox>,
    empty_area_weak: glib::WeakRef<gtk4::Frame>,
) {
    let Some(window) = controller.window_weak.upgrade() else {
        return;
    };

    let body = pack_dir.to_string_lossy().to_string();
    let dialog = adw::MessageDialog::builder()
        .transient_for(&window)
        .modal(true)
        .heading(tr(controller.lang, "Удалить шейдерпак?"))
        .body(&body)
        .build();
    dialog.add_response("cancel", tr(controller.lang, "Отмена"));
    dialog.add_response("delete", tr(controller.lang, "Удалить"));
    dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));

    dialog.connect_response(None, {
        let controller = controller.clone();
        let toast_overlay = toast_overlay.clone();
        let pack_id = pack_id.clone();
        let pack_dir = pack_dir.clone();
        let pack_list_weak = pack_list_weak.clone();
        let empty_area_weak = empty_area_weak.clone();
        move |d, response| {
            if response == "delete" {
                let cleared = {
                    let mut config = controller.config.borrow_mut();
                    match delete_installed_shaderpack_and_clean_config(&pack_id, &mut config) {
                        Ok(v) => {
                            let _ = config.save();
                            v
                        }
                        Err(err) => {
                            toast_overlay.add_toast(adw::Toast::new(&err));
                            d.close();
                            return;
                        }
                    }
                };
                toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Удалено")));
                if cleared > 0 {
                    toast_overlay.add_toast(adw::Toast::new(tr(
                        controller.lang,
                        "Сброшен выбор GLSL шейдера",
                    )));
                }

                // If the current UI points into that pack, clear it too.
                let deleted_root = pack_dir.canonicalize().unwrap_or(pack_dir.clone());
                let mode_idx = super::selected_mode_index(&controller.ui.mode_selector);
                if mode_idx == 9 {
                    if let Some(path) = crate::ui::settings::content::file_row_path(&controller.ui.file_row)
                    {
                        let canon = path.canonicalize().unwrap_or(path);
                        if canon.starts_with(&deleted_root) {
                            super::set_file_row_path(
                                &controller.ui.file_row,
                                &controller.ui.file_info_row,
                                None,
                                tr(controller.lang, "Файл не выбран"),
                                controller.lang,
                            );
                            super::update_file_info_row(
                                &controller.ui.file_info_row,
                                None,
                                mode_idx,
                                controller.lang,
                            );
                            (controller.update_preview)();
                            (controller.update_status)();
                        }
                    }
                }

                refresh_from_weaks(
                    pack_list_weak.clone(),
                    empty_area_weak.clone(),
                    controller.clone(),
                    toast_overlay.clone(),
                );
            }
            d.close();
        }
    });
    dialog.present();
}

fn handle_import_path(
    controller: &Rc<super::SettingsController>,
    toast_overlay: &adw::ToastOverlay,
    refresh: &Rc<dyn Fn()>,
    path: &Path,
) {
    let Some(window) = controller.window_weak.upgrade() else {
        return;
    };
    match import_shaderpack_from_path(path, ImportConflictPolicy::Abort) {
        Ok(_) => {
            toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Импортировано")));
            refresh();
        }
        Err(ImportShaderpackError::Conflict { .. }) => {
            let dialog = adw::MessageDialog::builder()
                .transient_for(&window)
                .modal(true)
                .heading(tr(controller.lang, "Шейдерпак уже существует"))
                .body(tr(controller.lang, "Выберите действие при конфликте"))
                .build();
            dialog.add_response("cancel", tr(controller.lang, "Отмена"));
            dialog.add_response("replace", tr(controller.lang, "Заменить"));
            dialog.add_response("rename", tr(controller.lang, "Переименовать"));
            dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("cancel"));
            dialog.connect_response(None, {
                let controller = controller.clone();
                let toast_overlay = toast_overlay.clone();
                let refresh = refresh.clone();
                let path = path.to_path_buf();
                move |d, response| {
                    let policy = match response {
                        "replace" => Some(ImportConflictPolicy::Replace),
                        "rename" => Some(ImportConflictPolicy::Rename),
                        _ => None,
                    };
                    if let Some(policy) = policy {
                        match import_shaderpack_from_path(&path, policy) {
                            Ok(_) => {
                                toast_overlay.add_toast(adw::Toast::new(tr(controller.lang, "Импортировано")));
                                refresh();
                            }
                            Err(err) => toast_overlay.add_toast(adw::Toast::new(&err.to_string())),
                        }
                    }
                    d.close();
                }
            });
            dialog.present();
        }
        Err(err) => {
            toast_overlay.add_toast(adw::Toast::new(&err.to_string()));
        }
    }
}

fn install_drop_target(
    widget: &impl IsA<gtk4::Widget>,
    controller: &Rc<super::SettingsController>,
    toast_overlay: &adw::ToastOverlay,
    refresh: &Rc<dyn Fn()>,
) {
    let target = gtk4::DropTarget::new(gdk4::FileList::static_type(), gdk4::DragAction::COPY);
    target.connect_drop({
        let controller = controller.clone();
        let toast_overlay = toast_overlay.clone();
        let refresh = refresh.clone();
        move |_, value, _, _| {
            let Ok(list) = value.get::<gdk4::FileList>() else {
                return false;
            };

            let files = list.files();
            let Some(file) = files.first() else {
                return false;
            };
            let Some(path) = file.path() else {
                return false;
            };
            handle_import_path(&controller, &toast_overlay, &refresh, &path);
            true
        }
    });
    widget.add_controller(target);
}
