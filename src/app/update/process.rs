use std::io::Write;

use iced::Task;
use tokio_util::sync::CancellationToken;

use crate::app::message::{Message, ProcessContext, ProcessMessage, ProgressUpdate};
use crate::app::model::{
    FileDetail, Model, ProcessRecord, ProcessResult, ProcessStatus, ProcessingMode,
    ProcessingState, Toast, ToastStyle,
};
use crate::processor::merger::{MergedFile, merge_content};
use crate::processor::reader::{compress_by_extension, count_chars_tokens, read_text};
use crate::processor::stats::ProcessingStats;
use crate::processor::walker::collect_candidates;
use crate::utils::i18n::tr;

pub fn update_process(model: &mut Model, msg: ProcessMessage) -> Task<Message> {
    match msg {
        ProcessMessage::Start => {
            let token = CancellationToken::new();
            let ctx = ProcessContext::new(model, token.clone());
            let pre = collect_candidates(
                ctx.selected_folder.as_ref(),
                &ctx.selected_files,
                &ctx.folder_blacklist,
                &ctx.ext_blacklist,
            );
            let lang = ctx.language;

            if pre.candidates.is_empty() {
                let reason = if ctx.selected_folder.is_none() && ctx.selected_files.is_empty() {
                    tr(lang, "no_input_selected").to_string()
                } else {
                    format!("{}, skipped={}", tr(lang, "no_valid_files"), pre.skipped)
                };
                model.ui.toast = Some(Toast {
                    message: reason,
                    style: ToastStyle::Error,
                    duration: std::time::Duration::from_secs(4),
                });
                model.processing_state =
                    ProcessingState::Failed(tr(lang, "no_valid_files_short").to_string());
                return Task::none();
            }

            if matches!(ctx.options.mode, ProcessingMode::TreeOnly) {
                model.ui.toast = Some(Toast {
                    message: format!("{}{}", tr(lang, "tree_mode_hint"), pre.candidates.len()),
                    style: ToastStyle::Info,
                    duration: std::time::Duration::from_secs(4),
                });
            }

            model.cancel_token = Some(token);
            model.result = None;
            model.ui.preview_content.clear();
            model.ui.show_guide = false;
            model.ui.show_cancel_confirmation = false;
            model.ui.processing_elapsed_ms = 0;
            model.processing_state = ProcessingState::InProgress {
                total: pre.candidates.len(),
                processed: 0,
                skipped: pre.skipped,
                current_file: String::new(),
                records: Vec::new(),
            };
            model.ui.toast = Some(Toast {
                message: tr(lang, "process_started").to_string(),
                style: ToastStyle::Info,
                duration: std::time::Duration::from_secs(2),
            });

            Task::perform(run_process(ctx), |res| {
                Message::Process(ProcessMessage::Completed(res))
            })
        }
        ProcessMessage::Cancel => {
            if let Some(token) = model.cancel_token.take() {
                token.cancel();
                model.processing_state = ProcessingState::Idle;
                model.ui.show_cancel_confirmation = false;
                model.ui.processing_elapsed_ms = 0;
                model.ui.toast = Some(Toast {
                    message: tr(model.language, "cancelled").to_string(),
                    style: ToastStyle::Info,
                    duration: std::time::Duration::from_secs(2),
                });
            }
            Task::none()
        }
        ProcessMessage::Completed(res) => {
            model.cancel_token = None;
            model.ui.show_cancel_confirmation = false;
            let lang = model.language;
            match res {
                Ok(result) => {
                    let processed = result.stats.processed_files;
                    let skipped = result.stats.skipped_files;
                    model.processing_state = ProcessingState::Completed { processed, skipped };
                    model.result = Some(result);
                    model.ui.toast = Some(Toast {
                        message: format!(
                            "{} {}={}, {}{}",
                            tr(lang, "done"),
                            tr(lang, "processed"),
                            processed,
                            tr(lang, "skipped_label"),
                            skipped
                        ),
                        style: ToastStyle::Success,
                        duration: std::time::Duration::from_secs(3),
                    });
                    Task::perform(async {}, |_| {
                        Message::Ui(crate::app::message::UiMessage::LoadPreview)
                    })
                }
                Err(e) => {
                    model.processing_state = ProcessingState::Failed(e.clone());
                    model.ui.toast = Some(Toast {
                        message: e,
                        style: ToastStyle::Error,
                        duration: std::time::Duration::from_secs(3),
                    });
                    Task::none()
                }
            }
        }
        ProcessMessage::Record(update) => {
            if let ProcessingState::InProgress {
                total: _,
                processed,
                skipped,
                current_file,
                records,
            } = &mut model.processing_state
            {
                match update {
                    ProgressUpdate::Success {
                        file,
                        chars,
                        tokens,
                    } => {
                        *processed += 1;
                        *current_file = file.clone();
                        records.push(ProcessRecord {
                            file_name: file,
                            status: ProcessStatus::Success,
                            chars: Some(chars),
                            tokens: Some(tokens),
                            error: None,
                        });
                    }
                    ProgressUpdate::Skipped { file, reason } => {
                        *skipped += 1;
                        *current_file = file.clone();
                        records.push(ProcessRecord {
                            file_name: file,
                            status: ProcessStatus::Skipped,
                            chars: None,
                            tokens: None,
                            error: Some(reason),
                        });
                    }
                    ProgressUpdate::Failed { file, error } => {
                        *skipped += 1;
                        *current_file = file.clone();
                        records.push(ProcessRecord {
                            file_name: file,
                            status: ProcessStatus::Failed,
                            chars: None,
                            tokens: None,
                            error: Some(error),
                        });
                    }
                    ProgressUpdate::Finished(_) | ProgressUpdate::Cancelled => {}
                }
            }
            Task::none()
        }
    }
}

async fn run_process(ctx: ProcessContext) -> Result<ProcessResult, String> {
    let lang = ctx.language;
    let walker = collect_candidates(
        ctx.selected_folder.as_ref(),
        &ctx.selected_files,
        &ctx.folder_blacklist,
        &ctx.ext_blacklist,
    );

    if walker.candidates.is_empty() {
        return Err(tr(lang, "no_valid_files_short").to_string());
    }

    if ctx.cancel_token.is_cancelled() {
        return Err(tr(lang, "cancelled").to_string());
    }

    let mut stats = ProcessingStats {
        skipped_files: walker.skipped,
        ..ProcessingStats::default()
    };

    if matches!(ctx.options.mode, ProcessingMode::TreeOnly) {
        return Ok(ProcessResult {
            stats,
            tree_string: Some(walker.tree),
            merged_content_path: None,
            file_details: Vec::new(),
        });
    }

    let mut merged_files = Vec::new();
    let mut details = Vec::new();

    for file in walker.candidates {
        if ctx.cancel_token.is_cancelled() {
            return Err(tr(lang, "cancelled").to_string());
        }

        let raw = match read_text(&file.absolute).await {
            Ok(v) => v,
            Err(_) => {
                stats.skipped_files += 1;
                continue;
            }
        };

        let (compressed, _warn) = compress_by_extension(&file.absolute, &raw, ctx.options.compress);
        let (chars, tokens) = count_chars_tokens(&compressed);

        stats.processed_files += 1;
        stats.total_chars += chars;
        stats.total_tokens += tokens;

        details.push(FileDetail {
            path: file.relative.clone(),
            chars,
            tokens,
        });

        merged_files.push(MergedFile {
            path: file.relative,
            chars,
            tokens,
            content: compressed,
        });
    }

    let merged = merge_content(ctx.options.output_format, &walker.tree, &merged_files);
    let path = crate::utils::temp_file::make_temp_result_path()?;

    let mut f =
        std::fs::File::create(&path).map_err(|e| format!("create merged file failed: {e}"))?;
    f.write_all(merged.as_bytes())
        .map_err(|e| format!("write merged file failed: {e}"))?;

    Ok(ProcessResult {
        stats,
        tree_string: Some(walker.tree),
        merged_content_path: Some(path),
        file_details: details,
    })
}
