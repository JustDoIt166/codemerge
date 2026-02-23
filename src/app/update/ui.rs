use std::fs;
use std::path::PathBuf;

use chrono::Local;
use iced::Task;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::app::message::{Message, PreflightUpdate, PreviewPagePayload, UiMessage};
use crate::app::model::{Model, OutputTab, ProcessingState, Toast, ToastStyle};
use crate::utils::i18n::tr;

pub fn update_ui(model: &mut Model, msg: UiMessage) -> Task<Message> {
    let mut task = Task::none();
    let mut needs_preflight_refresh = false;

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
            needs_preflight_refresh = true;
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
        UiMessage::PreviewFilterChanged(v) => {
            model.ui.preview_filter_input = v;
        }
        UiMessage::SelectPreviewFile(file_id) => {
            model.ui.selected_preview_file_id = Some(file_id);
            model.ui.preview_content.clear();
            model.ui.preview_offset = 0;
            model.ui.preview_total_bytes = 0;
            model.ui.preview_loaded_bytes = 0;
            model.ui.preview_error = None;
            task = load_preview_page(model, file_id, 0);
        }
        UiMessage::LoadPreviewPage { file_id, offset } => {
            task = load_preview_page(model, file_id, offset);
        }
        UiMessage::PreviewPageLoaded(res) => {
            apply_preview_page_result(model, res);
        }
        UiMessage::PreviewNextPage => {
            if let Some(file_id) = model.ui.selected_preview_file_id {
                let next_offset = model
                    .ui
                    .preview_offset
                    .saturating_add(model.ui.preview_loaded_bytes);
                if next_offset < model.ui.preview_total_bytes {
                    task = load_preview_page(model, file_id, next_offset);
                }
            }
        }
        UiMessage::PreviewPrevPage => {
            if let Some(file_id) = model.ui.selected_preview_file_id {
                let prev_offset = model.ui.preview_offset.saturating_sub(model.ui.preview_page_bytes);
                if prev_offset != model.ui.preview_offset {
                    task = load_preview_page(model, file_id, prev_offset);
                } else if model.ui.preview_offset > 0 {
                    task = load_preview_page(model, file_id, 0);
                }
            }
        }
        UiMessage::SwitchOutputTab(tab) => {
            model.ui.active_output_tab = tab;
            if tab == OutputTab::MergedContent
                && model.ui.selected_preview_file_id.is_none()
                && let Some(result) = &model.result
                && let Some(first) = result.preview_files.first()
            {
                let first_id = first.id;
                task = Task::perform(async {}, move |_| {
                    Message::Ui(UiMessage::SelectPreviewFile(first_id))
                });
            }
        }
        UiMessage::ToggleConfigExpanded => model.ui.config_expanded = !model.ui.config_expanded,
        UiMessage::ToggleBlacklistExpanded => {
            model.ui.blacklist_expanded = !model.ui.blacklist_expanded
        }
        UiMessage::DismissToast => {
            model.ui.toast = None;
            model.ui.toast_elapsed_ms = 0;
            model.ui.toast_last_key.clear();
        }
        UiMessage::PreflightUpdate(update) => {
            apply_preflight_update(model, update);
        }
        UiMessage::Resize(w, h) => model.window_size = (w, h),
    }

    if needs_preflight_refresh {
        Task::batch([task, super::refresh_preflight(model)])
    } else {
        task
    }
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

fn load_preview_page(model: &mut Model, file_id: u32, offset: u64) -> Task<Message> {
    let Some(result) = &model.result else {
        model.ui.toast = Some(info_toast(tr(model.language, "no_result")));
        return Task::none();
    };
    let Some(entry) = result.preview_files.iter().find(|f| f.id == file_id) else {
        model.ui.toast = Some(info_toast(tr(model.language, "no_result")));
        return Task::none();
    };

    let path = entry.preview_blob_path.clone();
    let page_bytes = model.ui.preview_page_bytes.clamp(16 * 1024, 256 * 1024);
    let offset = offset.min(entry.byte_len);
    model.ui.preview_loading = true;
    model.ui.preview_error = None;

    Task::perform(read_preview_page(path, file_id, offset, page_bytes), |res| {
        Message::Ui(UiMessage::PreviewPageLoaded(res))
    })
}

fn apply_preview_page_result(model: &mut Model, res: Result<PreviewPagePayload, String>) {
    let lang = model.language;
    model.ui.preview_loading = false;

    match res {
        Ok(payload) => {
            model.ui.selected_preview_file_id = Some(payload.file_id);
            model.ui.preview_content = payload.content;
            model.ui.preview_offset = payload.offset;
            model.ui.preview_loaded_bytes = payload.loaded_bytes;
            model.ui.preview_total_bytes = payload.total_bytes;
            model.ui.preview_error = None;
        }
        Err(e) => {
            model.ui.preview_error = Some(e.clone());
            model.ui.toast = Some(error_toast(&format!("{}{}", tr(lang, "preview_failed"), e)));
        }
    }
}

fn apply_preflight_update(model: &mut Model, update: PreflightUpdate) {
    match update {
        PreflightUpdate::Started { revision } => {
            if revision != model.preflight_revision {
                return;
            }
            model.preflight.is_scanning = true;
            model.preflight.scanned_entries = 0;
        }
        PreflightUpdate::Progress {
            revision,
            scanned,
            candidates,
            skipped,
        } => {
            if revision != model.preflight_revision {
                return;
            }
            model.preflight.is_scanning = true;
            model.preflight.scanned_entries = scanned;
            model.preflight.to_process_files = candidates;
            model.preflight.skipped_files = skipped;
            model.preflight.total_files = candidates + skipped;
        }
        PreflightUpdate::Completed { revision, stats } => {
            if revision != model.preflight_revision {
                return;
            }
            model.preflight = stats;
        }
        PreflightUpdate::Failed { revision, error } => {
            if revision != model.preflight_revision {
                return;
            }
            model.preflight.is_scanning = false;
            model.ui.toast = Some(error_toast(&error));
        }
    }
}

async fn read_preview_page(
    path: PathBuf,
    file_id: u32,
    offset: u64,
    page_bytes: u64,
) -> Result<PreviewPagePayload, String> {
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("read metadata failed: {e}"))?;
    let total_bytes = metadata.len();
    let safe_offset = offset.min(total_bytes);
    let limit = page_bytes.min(total_bytes.saturating_sub(safe_offset));

    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| format!("open preview file failed: {e}"))?;
    file.seek(std::io::SeekFrom::Start(safe_offset))
        .await
        .map_err(|e| format!("seek preview file failed: {e}"))?;

    let mut bytes = Vec::with_capacity(limit as usize);
    file.take(limit)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| format!("read preview file failed: {e}"))?;

    Ok(PreviewPagePayload {
        file_id,
        offset: safe_offset,
        loaded_bytes: bytes.len() as u64,
        total_bytes,
        content: String::from_utf8_lossy(&bytes).to_string(),
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

    let folder_name = model
        .selected_folder
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.trim().is_empty());

    let name = if let Some(folder_name) = folder_name {
        format!(
            "{}_{}_{}.{}",
            folder_name,
            prefix,
            Local::now().format("%Y-%m-%dT%H-%M-%S"),
            ext
        )
    } else {
        format!(
            "{}_{}.{}",
            prefix,
            Local::now().format("%Y-%m-%dT%H-%M-%S"),
            ext
        )
    };

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

#[cfg(test)]
mod tests {
    use super::{apply_preview_page_result, read_preview_page};
    use crate::app::message::PreviewPagePayload;
    use crate::app::model::{Model, ProcessResult};
    use crate::processor::stats::ProcessingStats;

    #[test]
    fn apply_preview_selects_file_and_updates_page_state() {
        let mut model = Model::default();
        model.result = Some(ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: None,
            merged_content_path: None,
            file_details: Vec::new(),
            preview_files: Vec::new(),
            preview_blob_dir: None,
        });

        apply_preview_page_result(
            &mut model,
            Ok(PreviewPagePayload {
                file_id: 7,
                offset: 64,
                loaded_bytes: 10,
                total_bytes: 100,
                content: "0123456789".to_string(),
            }),
        );

        assert_eq!(model.ui.selected_preview_file_id, Some(7));
        assert_eq!(model.ui.preview_offset, 64);
        assert_eq!(model.ui.preview_loaded_bytes, 10);
        assert_eq!(model.ui.preview_total_bytes, 100);
        assert_eq!(model.ui.preview_content, "0123456789");
    }

    #[test]
    fn read_preview_page_handles_boundaries() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(async {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("preview.bin");
            tokio::fs::write(&path, b"abcdef").await.expect("write");

            let first = read_preview_page(path.clone(), 1, 0, 4).await.expect("first");
            assert_eq!(first.content, "abcd");
            assert_eq!(first.loaded_bytes, 4);
            assert_eq!(first.total_bytes, 6);

            let tail = read_preview_page(path.clone(), 1, 4, 4).await.expect("tail");
            assert_eq!(tail.content, "ef");
            assert_eq!(tail.loaded_bytes, 2);

            let overflow = read_preview_page(path, 1, 99, 4).await.expect("overflow");
            assert_eq!(overflow.offset, 6);
            assert_eq!(overflow.loaded_bytes, 0);
            assert!(overflow.content.is_empty());
        });
    }
}
