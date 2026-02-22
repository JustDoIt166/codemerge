use std::collections::HashSet;
use std::path::PathBuf;

use iced::Task;

use crate::app::message::{FileMessage, Message};
use crate::app::model::{FileEntry, Model, Toast, ToastStyle};
use crate::processor::walker::{auto_gitignore_path, parse_gitignore_rules, unique_paths};
use crate::utils::i18n::tr;
use crate::utils::path::filename;

pub fn update_file(model: &mut Model, msg: FileMessage) -> Task<Message> {
    let lang = model.language;
    let task = match msg {
        FileMessage::SelectFolder => {
            let folder = rfd::FileDialog::new().pick_folder();
            if let Some(path) = folder {
                model.selected_folder = Some(path.clone());
                model.ui.toast = Some(info_toast(tr(lang, "folder_selected")));

                if model.options.use_gitignore {
                    let p = auto_gitignore_path(&path);
                    if p.exists() {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            let mut changed = 0usize;
                            for rule in parse_gitignore_rules(&content) {
                                if !model.folder_blacklist.contains(&rule) {
                                    model.folder_blacklist.push(rule);
                                    changed += 1;
                                }
                            }
                            model.ui.toast = Some(Toast {
                                message: format!(
                                    "{}{}{}",
                                    tr(lang, "gitignore_applied"),
                                    changed,
                                    tr(lang, "gitignore_rules")
                                ),
                                style: ToastStyle::Info,
                                duration: std::time::Duration::from_secs(3),
                            });
                        }
                    }
                }
            } else {
                model.ui.toast = Some(info_toast(tr(lang, "folder_pick_cancelled")));
            }
            Task::none()
        }
        FileMessage::SelectFiles => {
            let files = rfd::FileDialog::new().pick_files();
            if let Some(picked) = files {
                let mut existing: HashSet<String> = model
                    .selected_files
                    .iter()
                    .map(|f| f.path.to_string_lossy().to_string())
                    .collect();

                let mut dup = 0usize;
                for p in picked {
                    let abs = p.clone();
                    let k = abs.to_string_lossy().to_string();
                    if model.dedupe_exact_path && !existing.insert(k) {
                        dup += 1;
                        continue;
                    }

                    let size = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
                    model.selected_files.push(FileEntry {
                        path: abs.clone(),
                        name: filename(&abs),
                        size,
                    });
                }

                if dup > 0 {
                    model.ui.toast = Some(Toast {
                        message: format!("{}{}", dup, tr(lang, "duplicate_files_ignored")),
                        style: ToastStyle::Info,
                        duration: std::time::Duration::from_secs(3),
                    });
                } else {
                    model.ui.toast = Some(info_toast(tr(lang, "files_added")));
                }
            } else {
                model.ui.toast = Some(info_toast(tr(lang, "files_pick_cancelled")));
            }

            let unique: Vec<PathBuf> = unique_paths(
                &model
                    .selected_files
                    .iter()
                    .map(|f| f.path.clone())
                    .collect::<Vec<_>>(),
            );
            if unique.len() != model.selected_files.len() && model.dedupe_exact_path {
                model
                    .selected_files
                    .retain(|f| unique.iter().any(|u| u == &f.path));
            }

            Task::none()
        }
        FileMessage::SelectGitignore => {
            model.gitignore_file = rfd::FileDialog::new()
                .add_filter("gitignore", &["gitignore"])
                .pick_file();
            if model.gitignore_file.is_some() {
                model.ui.toast = Some(info_toast(tr(lang, "gitignore_selected")));
            } else {
                model.ui.toast = Some(info_toast(tr(lang, "gitignore_pick_cancelled")));
            }
            Task::none()
        }
        FileMessage::ApplyGitignore => {
            if let Some(path) = &model.gitignore_file {
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        let mut added = 0usize;
                        for rule in parse_gitignore_rules(&content) {
                            if !model.folder_blacklist.contains(&rule) {
                                model.folder_blacklist.push(rule);
                                added += 1;
                            }
                        }
                        model.ui.toast = Some(Toast {
                            message: format!(
                                "{}{}{}",
                                tr(lang, "rules_added"),
                                added,
                                tr(lang, "blacklist_rules")
                            ),
                            style: ToastStyle::Success,
                            duration: std::time::Duration::from_secs(3),
                        });
                    }
                    Err(e) => {
                        model.ui.toast = Some(Toast {
                            message: format!("{}{}", tr(lang, "read_gitignore_failed"), e),
                            style: ToastStyle::Error,
                            duration: std::time::Duration::from_secs(3),
                        });
                    }
                }
            } else {
                model.ui.toast = Some(info_toast(tr(lang, "gitignore_required")));
            }
            Task::none()
        }
        FileMessage::RemoveFile(index) => {
            if index < model.selected_files.len() {
                model.selected_files.remove(index);
                model.ui.toast = Some(info_toast(tr(lang, "file_removed")));
            }
            Task::none()
        }
        FileMessage::ClearAllFiles => {
            model.selected_files.clear();
            model.selected_folder = None;
            model.gitignore_file = None;
            model.ui.toast = Some(info_toast(tr(lang, "files_cleared")));
            Task::none()
        }
    };
    super::refresh_preflight(model);
    task
}

fn info_toast(msg: &str) -> Toast {
    Toast {
        message: msg.to_string(),
        style: ToastStyle::Info,
        duration: std::time::Duration::from_secs(2),
    }
}
