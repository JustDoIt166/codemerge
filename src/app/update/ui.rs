use std::fs;
use std::path::PathBuf;

use chrono::Local;
use iced::Task;
use tokio::io::AsyncReadExt;

use crate::app::message::{Message, PreviewPayload, UiMessage};
use crate::app::model::{Model, ProcessingState, Toast, ToastStyle};
use crate::utils::i18n::tr;

const PREVIEW_LIMIT_BYTES: u64 = 1024 * 1024;

pub fn update_ui(model: &mut Model, msg: UiMessage) -> Task<Message> {
    let mut task = Task::none();

    match msg {
        UiMessage::Reset => {
            model.ui.show_reset_confirmation = true;
            model.ui.show_cancel_confirmation = false;
            model.ui.toast = Some(info_toast(tr(model.language, "reset_prompt")));
        }
        UiMessage::ConfirmReset => {
            let lang = model.language;
            let options = model.options.clone();
            let folder_blacklist = model.folder_blacklist.clone();
            let ext_blacklist = model.ext_blacklist.clone();
            *model = Model::default();
            model.language = lang;
            model.options = options;
            model.folder_blacklist = folder_blacklist;
            model.ext_blacklist = ext_blacklist;
            model.ui.show_reset_confirmation = false;
            model.ui.show_cancel_confirmation = false;
            model.ui.toast = Some(success_toast(tr(model.language, "reset_done")));
        }
        UiMessage::CancelReset => {
            model.ui.show_reset_confirmation = false;
            model.ui.toast = Some(info_toast(tr(model.language, "reset_cancelled")));
        }
        UiMessage::RequestCancel => {
            if matches!(model.processing_state, ProcessingState::InProgress { .. }) {
                model.ui.show_cancel_confirmation = true;
                model.ui.toast = Some(info_toast(tr(model.language, "cancel_prompt")));
            }
        }
        UiMessage::ConfirmCancel => {
            model.ui.show_cancel_confirmation = false;
            return Task::perform(async {}, |_| {
                Message::Process(crate::app::message::ProcessMessage::Cancel)
            });
        }
        UiMessage::CancelCancel => {
            model.ui.show_cancel_confirmation = false;
            model.ui.toast = Some(info_toast(tr(model.language, "cancel_cancelled")));
        }
        UiMessage::ExpandStats(v) => model.ui.expanded_stats = v,
        UiMessage::CopyTree => {
            if let Some(result) = &model.result {
                if let Some(tree) = &result.tree_string {
                    copy_to_clipboard(model, tree.clone());
                } else {
                    model.ui.toast = Some(info_toast(tr(model.language, "no_tree")));
                }
            } else {
                model.ui.toast = Some(info_toast(tr(model.language, "no_tree")));
            }
        }
        UiMessage::CopyContent => {
            if !model.ui.preview_content.is_empty() {
                copy_to_clipboard(model, model.ui.preview_content.clone());
            } else {
                model.ui.toast = Some(info_toast(tr(model.language, "no_content")));
            }
        }
        UiMessage::DownloadContent => {
            let Some(result) = &model.result else {
                model.ui.toast = Some(info_toast(tr(model.language, "no_result")));
                return Task::none();
            };
            let Some(path) = &result.merged_content_path else {
                model.ui.toast = Some(info_toast(tr(model.language, "no_result")));
                return Task::none();
            };
            let lang = model.language;

            if let Some(save_path) = default_save_path(model, path) {
                match fs::copy(path, save_path) {
                    Ok(_) => model.ui.toast = Some(success_toast(tr(lang, "saved"))),
                    Err(e) => {
                        model.ui.toast =
                            Some(error_toast(&format!("{}{}", tr(lang, "save_failed"), e)))
                    }
                }
            }
        }
        UiMessage::LoadPreview => {
            task = load_preview(model, false);
        }
        UiMessage::PreviewLoaded(res) => {
            apply_preview_result(model, res);
        }
        UiMessage::LoadAllPreview => {
            model.ui.show_load_all_confirm = true;
            model.ui.toast = Some(info_toast(tr(model.language, "load_all_prompt")));
        }
        UiMessage::ConfirmLoadAllPreview => {
            model.ui.show_load_all_confirm = false;
            task = load_preview(model, true);
        }
        UiMessage::CancelLoadAllPreview => {
            model.ui.show_load_all_confirm = false;
            model.ui.toast = Some(info_toast(tr(model.language, "load_all_cancelled")));
        }
        UiMessage::SwitchOutputTab(tab) => model.ui.active_output_tab = tab,
        UiMessage::ToggleConfigExpanded => model.ui.config_expanded = !model.ui.config_expanded,
        UiMessage::ToggleBlacklistExpanded => {
            model.ui.blacklist_expanded = !model.ui.blacklist_expanded
        }
        UiMessage::DismissToast => {
            model.ui.toast = None;
            model.ui.toast_elapsed_ms = 0;
            model.ui.toast_last_key.clear();
        }
        UiMessage::Resize(w, h) => model.window_size = (w, h),
    }

    super::refresh_preflight(model);
    task
}

pub fn on_tick(model: &mut Model) -> Task<Message> {
    model.ui.pulse_phase += 0.08;
    if model.ui.pulse_phase > 1.0 {
        model.ui.pulse_phase -= 1.0;
    }

    if matches!(model.processing_state, ProcessingState::InProgress { .. }) {
        model.ui.processing_elapsed_ms = model.ui.processing_elapsed_ms.saturating_add(120);
    }

    if let Some(toast) = &model.ui.toast {
        let key = format!("{:?}:{}", toast.style, toast.message);
        if model.ui.toast_last_key != key {
            model.ui.toast_last_key = key;
            model.ui.toast_elapsed_ms = 0;
        } else {
            model.ui.toast_elapsed_ms = model.ui.toast_elapsed_ms.saturating_add(120);
        }

        let ttl = toast.duration.as_millis() as u64;
        if ttl > 0 && model.ui.toast_elapsed_ms >= ttl {
            model.ui.toast = None;
            model.ui.toast_elapsed_ms = 0;
            model.ui.toast_last_key.clear();
        }
    } else {
        model.ui.toast_elapsed_ms = 0;
        model.ui.toast_last_key.clear();
    }

    Task::none()
}

fn load_preview(model: &mut Model, all: bool) -> Task<Message> {
    let Some(result) = &model.result else {
        model.ui.toast = Some(info_toast(tr(model.language, "no_result")));
        return Task::none();
    };
    let Some(path) = &result.merged_content_path else {
        model.ui.toast = Some(info_toast(tr(model.language, "no_result")));
        return Task::none();
    };
    let path = path.clone();

    Task::perform(read_preview_payload(path, all), |res| {
        Message::Ui(UiMessage::PreviewLoaded(res))
    })
}

fn apply_preview_result(model: &mut Model, res: Result<PreviewPayload, String>) {
    let lang = model.language;
    match res {
        Ok(payload) => {
            model.ui.preview_content = payload.content;
            model.ui.preview_loaded_all = payload.loaded_all;
            if payload.loaded_all {
                model.ui.toast = Some(info_toast(tr(lang, "preview_loaded_all")));
            } else {
                model.ui.toast = Some(info_toast(tr(lang, "preview_loaded")));
            }
        }
        Err(e) => {
            model.ui.toast = Some(error_toast(&format!("{}{}", tr(lang, "preview_failed"), e)));
        }
    }
}

async fn read_preview_payload(path: PathBuf, all: bool) -> Result<PreviewPayload, String> {
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("read metadata failed: {e}"))?;
    let file_len = metadata.len();
    let limit = if all {
        file_len
    } else {
        file_len.min(PREVIEW_LIMIT_BYTES)
    };

    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| format!("open preview file failed: {e}"))?;
    let mut bytes = Vec::with_capacity(limit as usize);
    file.take(limit)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| format!("read preview file failed: {e}"))?;

    Ok(PreviewPayload {
        content: String::from_utf8_lossy(&bytes).to_string(),
        loaded_all: all || file_len <= PREVIEW_LIMIT_BYTES,
    })
}

fn copy_to_clipboard(model: &mut Model, content: String) {
    let lang = model.language;
    match arboard::Clipboard::new().and_then(|mut c| c.set_text(content)) {
        Ok(_) => model.ui.toast = Some(success_toast(tr(lang, "copied"))),
        Err(e) => model.ui.toast = Some(error_toast(&format!("{}{}", tr(lang, "copy_failed"), e))),
    }
}

fn default_save_path(model: &Model, merged: &std::path::Path) -> Option<std::path::PathBuf> {
    let ext = match model.options.output_format {
        crate::app::model::OutputFormat::Xml => "xml",
        crate::app::model::OutputFormat::Markdown => "md",
        _ => "txt",
    };

    let prefix = match model.language {
        crate::app::model::Language::Zh => "文件内容汇总",
        crate::app::model::Language::En => "content_summary",
    };

    let name = format!(
        "{}_{}.{}",
        prefix,
        Local::now().format("%Y-%m-%dT%H-%M-%S"),
        ext
    );

    rfd::FileDialog::new()
        .set_file_name(&name)
        .set_directory(merged.parent().unwrap_or_else(|| std::path::Path::new(".")))
        .save_file()
}

fn success_toast(msg: &str) -> Toast {
    Toast {
        message: msg.to_string(),
        style: ToastStyle::Success,
        duration: std::time::Duration::from_secs(2),
    }
}

fn error_toast(msg: &str) -> Toast {
    Toast {
        message: msg.to_string(),
        style: ToastStyle::Error,
        duration: std::time::Duration::from_secs(3),
    }
}

fn info_toast(msg: &str) -> Toast {
    Toast {
        message: msg.to_string(),
        style: ToastStyle::Info,
        duration: std::time::Duration::from_secs(2),
    }
}

#[allow(dead_code)]
fn _is_idle(state: &ProcessingState) -> bool {
    matches!(state, ProcessingState::Idle)
}
