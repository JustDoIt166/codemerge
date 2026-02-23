use futures::stream::{self, StreamExt};
use iced::Task;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::app::message::{Message, ProcessContext, ProcessMessage, ProgressUpdate};
use crate::app::model::{
    FileDetail, Model, PreviewFileEntry, ProcessRecord, ProcessResult, ProcessStatus, ProcessingMode,
    ProcessingState, Toast, ToastStyle,
};
use crate::processor::merger::{MergedFile, render_file_entry, render_prefix, render_suffix};
use crate::processor::reader::{compress_by_extension, count_chars_tokens, read_text};
use crate::processor::stats::ProcessingStats;
use crate::processor::walker::{CandidateFile, WalkerOutput, collect_candidates_with_progress};
use crate::utils::i18n::tr;

pub fn update_process(model: &mut Model, msg: ProcessMessage) -> Task<Message> {
    match msg {
        ProcessMessage::Start => {
            let lang = model.language;
            if model.selected_folder.is_none() && model.selected_files.is_empty() {
                model.ui.toast = Some(Toast {
                    message: tr(lang, "no_input_selected").to_string(),
                    style: ToastStyle::Error,
                    duration: std::time::Duration::from_secs(4),
                });
                model.processing_state =
                    ProcessingState::Failed(tr(lang, "no_valid_files_short").to_string());
                return Task::none();
            }

            let token = CancellationToken::new();
            let ctx = ProcessContext::new(model, token.clone());
            model.cancel_token = Some(token);
            if let Some(prev_dir) = model
                .result
                .as_ref()
                .and_then(|r| r.preview_blob_dir.as_ref())
            {
                let _ = crate::utils::temp_file::cleanup_preview_dir(prev_dir);
            }
            model.result = None;
            model.ui.preview_content.clear();
            model.ui.preview_filter_input.clear();
            model.ui.selected_preview_file_id = None;
            model.ui.preview_total_bytes = 0;
            model.ui.preview_loaded_bytes = 0;
            model.ui.preview_offset = 0;
            model.ui.preview_loading = false;
            model.ui.preview_error = None;
            model.ui.show_guide = false;
            model.ui.show_cancel_confirmation = false;
            model.ui.processing_elapsed_ms = 0;
            model.processing_state = ProcessingState::InProgress {
                total: 1,
                processed: 0,
                skipped: 0,
                current_file: tr(lang, "scanning_files").to_string(),
                records: Vec::new(),
            };
            model.ui.toast = Some(Toast {
                message: tr(lang, "process_started").to_string(),
                style: ToastStyle::Info,
                duration: std::time::Duration::from_secs(2),
            });

            Task::run(spawn_process_stream(ctx), |update| Message::Process(update))
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
            let lang = model.language;
            model.cancel_token = None;
            model.ui.show_cancel_confirmation = false;
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
                    Task::none()
                }
                Err(e) => {
                    if e == tr(lang, "cancelled")
                        && matches!(model.processing_state, ProcessingState::Idle)
                    {
                        return Task::none();
                    }
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
                total,
                processed,
                skipped,
                current_file,
                records,
            } = &mut model.processing_state
            {
                match update {
                    ProgressUpdate::Scanning {
                        scanned,
                        candidates,
                        skipped: scan_skipped,
                    } => {
                        *total = (candidates + scan_skipped).max(1);
                        *skipped = scan_skipped;
                        *current_file =
                            format!("{} {}", tr(model.language, "scanning_files"), scanned);
                    }
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

fn spawn_process_stream(ctx: ProcessContext) -> impl futures::Stream<Item = ProcessMessage> {
    let (tx, rx) = mpsc::unbounded_channel::<ProcessMessage>();

    tokio::spawn(async move {
        let lang = ctx.language;
        let scan_tx = tx.clone();
        let scan_ctx = ctx.clone();
        let scan_join = tokio::task::spawn_blocking(move || {
            collect_candidates_with_progress(
                scan_ctx.selected_folder.as_ref(),
                &scan_ctx.selected_files,
                &scan_ctx.folder_blacklist,
                &scan_ctx.ext_blacklist,
                move |scanned, candidates, skipped| {
                    let _ = scan_tx.send(ProcessMessage::Record(ProgressUpdate::Scanning {
                        scanned,
                        candidates,
                        skipped,
                    }));
                },
            )
        });

        let walker = match scan_join.await {
            Ok(output) => output,
            Err(e) => {
                let _ = tx.send(ProcessMessage::Completed(Err(format!(
                    "scan task failed: {e}"
                ))));
                return;
            }
        };

        if walker.candidates.is_empty() {
            let reason = format!("{}, skipped={}", tr(lang, "no_valid_files"), walker.skipped);
            let _ = tx.send(ProcessMessage::Completed(Err(reason)));
            return;
        }

        let result = run_process_with_walker(ctx, walker, tx.clone()).await;
        let _ = tx.send(ProcessMessage::Completed(result));
    });

    stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}

async fn run_process_with_walker(
    ctx: ProcessContext,
    walker: WalkerOutput,
    tx: mpsc::UnboundedSender<ProcessMessage>,
) -> Result<ProcessResult, String> {
    let lang = ctx.language;

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
            preview_files: Vec::new(),
            preview_blob_dir: None,
        });
    }

    let mut details = Vec::new();
    let mut preview_files = Vec::new();
    let concurrency_limit = file_concurrency_limit(walker.candidates.len());
    let output_format = ctx.options.output_format;
    let path = crate::utils::temp_file::make_temp_result_path()?;
    let preview_dir = crate::utils::temp_file::make_temp_preview_dir()?;
    let mut output = tokio::fs::File::create(&path)
        .await
        .map_err(|e| format!("create merged file failed: {e}"))?;
    let prefix = render_prefix(output_format, &walker.tree);
    output
        .write_all(prefix.as_bytes())
        .await
        .map_err(|e| format!("write merged prefix failed: {e}"))?;

    let mut processed_stream = stream::iter(
        walker
            .candidates
            .into_iter()
            .map(|file| process_candidate_file(file, ctx.options.compress)),
    )
    .buffered(concurrency_limit);

    while let Some(outcome) = processed_stream.next().await {
        if ctx.cancel_token.is_cancelled() {
            let _ = tokio::fs::remove_file(&path).await;
            let _ = crate::utils::temp_file::cleanup_preview_dir(&preview_dir);
            return Err(tr(lang, "cancelled").to_string());
        }

        match outcome {
            FileProcessOutcome::Skipped { file, reason } => {
                stats.skipped_files += 1;
                let _ = tx.send(ProcessMessage::Record(ProgressUpdate::Skipped {
                    file,
                    reason,
                }));
            }
            FileProcessOutcome::Failed { file, error } => {
                stats.skipped_files += 1;
                let _ = tx.send(ProcessMessage::Record(ProgressUpdate::Failed {
                    file,
                    error,
                }));
            }
            FileProcessOutcome::Processed {
                detail,
                merged,
                chars,
                tokens,
            } => {
                stats.processed_files += 1;
                stats.total_chars += chars;
                stats.total_tokens += tokens;
                let chunk = render_file_entry(output_format, &merged);
                output
                    .write_all(chunk.as_bytes())
                    .await
                    .map_err(|e| format!("write merged content failed: {e}"))?;
                let next_id = preview_files.len() as u32;
                let blob_path = preview_dir.join(format!("preview_{next_id}.txt"));
                let byte_len = merged.content.len() as u64;
                tokio::fs::write(&blob_path, merged.content.as_bytes())
                    .await
                    .map_err(|e| format!("write preview blob failed: {e}"))?;
                preview_files.push(PreviewFileEntry {
                    id: next_id,
                    display_path: detail.path.clone(),
                    chars,
                    tokens,
                    preview_blob_path: blob_path,
                    byte_len,
                });
                let _ = tx.send(ProcessMessage::Record(ProgressUpdate::Success {
                    file: detail.path.clone(),
                    chars,
                    tokens,
                }));
                details.push(detail);
            }
        }
    }

    let suffix = render_suffix(output_format);
    if !suffix.is_empty() {
        output
            .write_all(suffix.as_bytes())
            .await
            .map_err(|e| format!("write merged suffix failed: {e}"))?;
    }
    output
        .flush()
        .await
        .map_err(|e| format!("flush merged file failed: {e}"))?;

    Ok(ProcessResult {
        stats,
        tree_string: Some(walker.tree),
        merged_content_path: Some(path),
        file_details: details,
        preview_files,
        preview_blob_dir: Some(preview_dir),
    })
}

#[derive(Debug)]
enum FileProcessOutcome {
    Skipped {
        file: String,
        reason: String,
    },
    Failed {
        file: String,
        error: String,
    },
    Processed {
        detail: FileDetail,
        merged: MergedFile,
        chars: usize,
        tokens: usize,
    },
}

async fn process_candidate_file(file: CandidateFile, compress: bool) -> FileProcessOutcome {
    let absolute = file.absolute;
    let relative = file.relative;
    let raw = match read_text(&absolute).await {
        Ok(v) => v,
        Err(e) => {
            return FileProcessOutcome::Skipped {
                file: relative,
                reason: format!("read failed: {e}"),
            };
        }
    };

    let rel_for_error = relative.clone();
    match tokio::task::spawn_blocking(move || {
        let (compressed, _warn) = compress_by_extension(&absolute, &raw, compress);
        let (chars, tokens) = count_chars_tokens(&compressed);
        FileProcessOutcome::Processed {
            detail: FileDetail {
                path: relative.clone(),
                chars,
                tokens,
            },
            merged: MergedFile {
                path: relative,
                chars,
                tokens,
                content: compressed,
            },
            chars,
            tokens,
        }
    })
    .await
    {
        Ok(outcome) => outcome,
        Err(e) => FileProcessOutcome::Failed {
            file: rel_for_error,
            error: format!("process failed: {e}"),
        },
    }
}

fn file_concurrency_limit(total_files: usize) -> usize {
    if total_files <= 1 {
        return 1;
    }

    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    workers.saturating_mul(4).clamp(4, 64).min(total_files)
}
