use std::collections::HashSet;
use std::fs;

use iced::Task;

use crate::app::message::{BlacklistMessage, Message};
use crate::app::model::{
    Model, Toast, ToastStyle, default_ext_blacklist, default_folder_blacklist,
};
use crate::processor::walker::normalize_ext;
use crate::utils::i18n::tr;

pub fn update_blacklist(model: &mut Model, msg: BlacklistMessage) -> Task<Message> {
    let lang = model.language;
    match msg {
        BlacklistMessage::SharedInputChanged(v) => {
            model.ui.folder_blacklist_input = v.clone();
            model.ui.ext_blacklist_input = v;
        }
        BlacklistMessage::FolderInputChanged(v) => model.ui.folder_blacklist_input = v,
        BlacklistMessage::ExtInputChanged(v) => model.ui.ext_blacklist_input = v,
        BlacklistMessage::FilterInputChanged(v) => model.ui.blacklist_filter_input = v,
        BlacklistMessage::AddFolder => {
            let mut added = 0usize;
            for v in split_entries(&model.ui.folder_blacklist_input) {
                if !model.folder_blacklist.contains(&v) {
                    model.folder_blacklist.push(v);
                    added += 1;
                }
            }
            model.ui.toast = Some(if added > 0 {
                info_toast(tr(lang, "blacklist_added"))
            } else {
                info_toast(tr(lang, "blacklist_empty"))
            });
            model.ui.folder_blacklist_input.clear();
            model.ui.ext_blacklist_input.clear();
            sync_blacklist_selection(model);
        }
        BlacklistMessage::RemoveFolder(v) => {
            model.folder_blacklist.retain(|x| x != &v);
            model.ui.blacklist_selected.remove(&folder_key(&v));
            update_select_all_flag(model);
            model.ui.toast = Some(info_toast(tr(lang, "blacklist_removed")));
        }
        BlacklistMessage::AddExt => {
            let mut added = 0usize;
            for raw in split_entries(&model.ui.ext_blacklist_input) {
                let v = normalize_ext(&raw);
                if !v.is_empty() && !model.ext_blacklist.contains(&v) {
                    model.ext_blacklist.push(v);
                    added += 1;
                }
            }
            model.ui.toast = Some(if added > 0 {
                info_toast(tr(lang, "blacklist_added"))
            } else {
                info_toast(tr(lang, "blacklist_empty"))
            });
            model.ui.ext_blacklist_input.clear();
            model.ui.folder_blacklist_input.clear();
            sync_blacklist_selection(model);
        }
        BlacklistMessage::RemoveExt(v) => {
            model.ext_blacklist.retain(|x| x != &v);
            model.ui.blacklist_selected.remove(&ext_key(&v));
            update_select_all_flag(model);
            model.ui.toast = Some(info_toast(tr(lang, "blacklist_removed")));
        }
        BlacklistMessage::ToggleSelectAll => {
            let visible = filtered_entry_keys(model);
            if !visible.is_empty() {
                let all_selected = visible
                    .iter()
                    .all(|k| model.ui.blacklist_selected.contains(k));
                if all_selected {
                    for key in visible {
                        model.ui.blacklist_selected.remove(&key);
                    }
                } else {
                    for key in visible {
                        model.ui.blacklist_selected.insert(key);
                    }
                }
            }
            update_select_all_flag(model);
        }
        BlacklistMessage::ToggleInvertSelection => {
            let visible = filtered_entry_keys(model);
            for key in visible {
                if model.ui.blacklist_selected.contains(&key) {
                    model.ui.blacklist_selected.remove(&key);
                } else {
                    model.ui.blacklist_selected.insert(key);
                }
            }
            update_select_all_flag(model);
        }
        BlacklistMessage::ToggleSelect(key) => {
            if model.ui.blacklist_selected.contains(&key) {
                model.ui.blacklist_selected.remove(&key);
            } else {
                model.ui.blacklist_selected.insert(key);
            }
            update_select_all_flag(model);
        }
        BlacklistMessage::DeleteSelected => {
            let before = model.folder_blacklist.len() + model.ext_blacklist.len();
            if before == 0 || model.ui.blacklist_selected.is_empty() {
                model.ui.toast = Some(info_toast(tr(lang, "blacklist_empty")));
            } else {
                let selected = model.ui.blacklist_selected.clone();
                model
                    .folder_blacklist
                    .retain(|v| !selected.contains(&folder_key(v)));
                model
                    .ext_blacklist
                    .retain(|v| !selected.contains(&ext_key(v)));
                sync_blacklist_selection(model);
                let after = model.folder_blacklist.len() + model.ext_blacklist.len();
                let removed = before.saturating_sub(after);
                model.ui.toast = Some(info_toast(&format!(
                    "{} {removed}",
                    tr(lang, "blacklist_deleted_selected")
                )));
            }
        }
        BlacklistMessage::ResetToDefault => {
            model.folder_blacklist = default_folder_blacklist();
            model.ext_blacklist = default_ext_blacklist();
            model.ui.blacklist_selected.clear();
            model.ui.blacklist_selected_all = false;
            model.ui.toast = Some(info_toast(tr(lang, "blacklist_reset_default")));
        }
        BlacklistMessage::ClearAll => {
            model.folder_blacklist.clear();
            model.ext_blacklist.clear();
            model.ui.blacklist_selected.clear();
            model.ui.blacklist_selected_all = false;
            model.ui.toast = Some(info_toast(tr(lang, "blacklist_cleared")));
        }
        BlacklistMessage::Export => {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("codemerge_blacklist.txt")
                .save_file()
            {
                let content = render_blacklist_export(model);
                match fs::write(path, content) {
                    Ok(_) => {
                        model.ui.toast = Some(Toast {
                            message: tr(lang, "blacklist_exported").to_string(),
                            style: ToastStyle::Success,
                            duration: std::time::Duration::from_secs(2),
                        })
                    }
                    Err(e) => {
                        model.ui.toast = Some(Toast {
                            message: format!("{}{}", tr(lang, "save_failed"), e),
                            style: ToastStyle::Error,
                            duration: std::time::Duration::from_secs(3),
                        });
                    }
                }
            }
        }
        BlacklistMessage::ImportAppend => {
            import_blacklist(model, false);
        }
        BlacklistMessage::ImportReplace => {
            import_blacklist(model, true);
        }
        BlacklistMessage::SaveSettings => {
            let cfg = crate::utils::config_store::AppConfigV1 {
                language: model.language,
                options: model.options.clone(),
                folder_blacklist: model.folder_blacklist.clone(),
                ext_blacklist: model.ext_blacklist.clone(),
            };
            match crate::utils::config_store::save_config(&cfg) {
                Ok(_) => {
                    model.ui.toast = Some(Toast {
                        message: "blacklist_saved".to_string(),
                        style: ToastStyle::Success,
                        duration: std::time::Duration::from_secs(3),
                    });
                }
                Err(e) => {
                    model.ui.toast = Some(Toast {
                        message: format!("save failed: {e}"),
                        style: ToastStyle::Error,
                        duration: std::time::Duration::from_secs(3),
                    });
                }
            }
        }
    }

    super::refresh_preflight(model);
    Task::none()
}

fn split_entries(raw: &str) -> Vec<String> {
    raw.split(|c: char| c == '\n' || c == '\r' || c == ',' || c == ';' || c.is_whitespace())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn import_blacklist(model: &mut Model, replace: bool) {
    let lang = model.language;
    let Some(path) = rfd::FileDialog::new().pick_file() else {
        return;
    };

    match fs::read_to_string(path) {
        Ok(content) => {
            let (folders, exts) = parse_blacklist_import(&content);
            if replace {
                model.folder_blacklist.clear();
                model.ext_blacklist.clear();
            }

            let mut added = 0usize;
            for v in folders {
                if !model.folder_blacklist.contains(&v) {
                    model.folder_blacklist.push(v);
                    added += 1;
                }
            }
            for raw in exts {
                let v = normalize_ext(&raw);
                if !v.is_empty() && !model.ext_blacklist.contains(&v) {
                    model.ext_blacklist.push(v);
                    added += 1;
                }
            }

            sync_blacklist_selection(model);
            model.ui.toast = Some(info_toast(&format!(
                "{} {added}",
                tr(lang, "blacklist_imported")
            )));
        }
        Err(e) => {
            model.ui.toast = Some(Toast {
                message: format!("{}{}", tr(lang, "read_gitignore_failed"), e),
                style: ToastStyle::Error,
                duration: std::time::Duration::from_secs(3),
            });
        }
    }
}

fn parse_blacklist_import(content: &str) -> (Vec<String>, Vec<String>) {
    enum Section {
        Unknown,
        Folders,
        Exts,
    }

    let mut section = Section::Unknown;
    let mut folders = Vec::new();
    let mut exts = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower == "[folders]" {
            section = Section::Folders;
            continue;
        }
        if lower == "[extensions]" || lower == "[exts]" {
            section = Section::Exts;
            continue;
        }
        if lower.starts_with("folder:") {
            for v in split_entries(trimmed[7..].trim()) {
                folders.push(v);
            }
            continue;
        }
        if lower.starts_with("ext:") {
            for v in split_entries(trimmed[4..].trim()) {
                exts.push(v);
            }
            continue;
        }

        let entries = split_entries(trimmed);
        for v in entries {
            match section {
                Section::Folders => folders.push(v),
                Section::Exts => exts.push(v),
                Section::Unknown => {
                    if v.starts_with('.') {
                        exts.push(v);
                    } else {
                        folders.push(v);
                    }
                }
            }
        }
    }

    (folders, exts)
}

fn render_blacklist_export(model: &Model) -> String {
    let mut lines = vec![
        "# CodeMerge blacklist export".to_string(),
        "[folders]".to_string(),
    ];
    lines.extend(model.folder_blacklist.iter().cloned());
    lines.push(String::new());
    lines.push("[extensions]".to_string());
    lines.extend(model.ext_blacklist.iter().cloned());
    lines.join("\n")
}

fn filtered_entry_keys(model: &Model) -> Vec<String> {
    let filter = model.ui.blacklist_filter_input.trim().to_ascii_lowercase();
    let mut keys = Vec::new();
    for v in &model.folder_blacklist {
        if filter.is_empty() || v.to_ascii_lowercase().contains(&filter) {
            keys.push(folder_key(v));
        }
    }
    for v in &model.ext_blacklist {
        if filter.is_empty() || v.to_ascii_lowercase().contains(&filter) {
            keys.push(ext_key(v));
        }
    }
    keys
}

fn folder_key(v: &str) -> String {
    format!("folder:{v}")
}

fn ext_key(v: &str) -> String {
    format!("ext:{v}")
}

fn all_entry_keys(model: &Model) -> HashSet<String> {
    let mut keys = HashSet::new();
    for v in &model.folder_blacklist {
        keys.insert(folder_key(v));
    }
    for v in &model.ext_blacklist {
        keys.insert(ext_key(v));
    }
    keys
}

fn sync_blacklist_selection(model: &mut Model) {
    let valid = all_entry_keys(model);
    model.ui.blacklist_selected.retain(|k| valid.contains(k));
    update_select_all_flag(model);
}

fn update_select_all_flag(model: &mut Model) {
    let visible = filtered_entry_keys(model);
    model.ui.blacklist_selected_all = !visible.is_empty()
        && visible
            .iter()
            .all(|k| model.ui.blacklist_selected.contains(k));
}

fn info_toast(msg: &str) -> Toast {
    Toast {
        message: msg.to_string(),
        style: ToastStyle::Info,
        duration: std::time::Duration::from_secs(2),
    }
}
