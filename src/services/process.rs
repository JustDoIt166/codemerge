use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use futures::stream::{self, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::domain::{
    ArchiveEntrySource, FileDetail, Language, PreviewFileEntry, ProcessRecord, ProcessResult,
    ProcessStatus, ProcessingMode, ProcessingOptions,
};
use crate::error::AppError;
use crate::processor::archive::read_zip_entry_text;
use crate::processor::merger::{MergedFile, render_file_entry, render_prefix, render_suffix};
use crate::processor::reader::{compress_by_extension, count_chars_tokens, read_text_blocking};
use crate::processor::stats::ProcessingStats;
use crate::processor::walker::{
    CandidateFile, WalkerOptions, WalkerOutput, collect_candidates_with_progress,
};
use crate::services::runtime::RUNTIME;
use crate::services::tree::build_tree_nodes;
use crate::utils::i18n::tr;
use crate::utils::path::suggested_merge_result_name;
use crate::utils::temp_file;

#[derive(Debug)]
pub enum ProcessEvent {
    Scanning {
        scanned: usize,
        candidates: usize,
        skipped: usize,
    },
    Record(ProcessRecord),
    Completed(ProcessResult),
    Failed(AppError),
    Cancelled,
}

#[derive(Debug)]
pub struct ProcessHandle {
    pub receiver: Receiver<ProcessEvent>,
    pub cancel: CancellationToken,
}

#[derive(Debug, Clone)]
pub struct ProcessRequest {
    pub selected_folder: Option<PathBuf>,
    pub selected_files: Vec<PathBuf>,
    pub folder_blacklist: Vec<String>,
    pub ext_blacklist: Vec<String>,
    pub options: ProcessingOptions,
    pub language: Language,
}

pub fn start(request: ProcessRequest) -> ProcessHandle {
    let (tx, rx) = mpsc::channel();
    let cancel = CancellationToken::new();
    let thread_cancel = cancel.clone();
    thread::spawn(move || {
        let event_tx = tx.clone();
        let cancel_for_run = thread_cancel.clone();
        let result = match &*RUNTIME {
            Ok(runtime) => runtime
                .block_on(async move { run_process(request, cancel_for_run, event_tx).await }),
            Err(err) => Err(AppError::new(err.clone())),
        };

        match result {
            Ok(result) => {
                let _ = tx.send(ProcessEvent::Completed(result));
            }
            Err(_) if thread_cancel.is_cancelled() => {
                let _ = tx.send(ProcessEvent::Cancelled);
            }
            Err(err) => {
                let _ = tx.send(ProcessEvent::Failed(err));
            }
        }
    });
    ProcessHandle {
        receiver: rx,
        cancel,
    }
}

async fn run_process(
    request: ProcessRequest,
    cancel: CancellationToken,
    tx: mpsc::Sender<ProcessEvent>,
) -> Result<ProcessResult, AppError> {
    let lang = request.language;
    let progress_tx = tx.clone();
    let scan_cancel = cancel.clone();
    let walker = collect_candidates_with_progress(
        request.selected_folder.as_ref(),
        &request.selected_files,
        &request.folder_blacklist,
        &request.ext_blacklist,
        WalkerOptions {
            use_gitignore: request.options.use_gitignore,
            ignore_git: request.options.ignore_git,
        },
        move |scanned, candidates, skipped| {
            if !scan_cancel.is_cancelled() {
                let _ = progress_tx.send(ProcessEvent::Scanning {
                    scanned,
                    candidates,
                    skipped,
                });
            }
        },
    );

    if cancel.is_cancelled() {
        return Err(AppError::new(tr(lang, "cancelled")));
    }

    if walker.candidates.is_empty() {
        return Err(AppError::new(format!(
            "{}, skipped={}",
            tr(lang, "no_valid_files"),
            walker.skipped
        )));
    }

    run_process_with_walker(request, walker, cancel, tx).await
}

async fn run_process_with_walker(
    request: ProcessRequest,
    walker: WalkerOutput,
    cancel: CancellationToken,
    tx: mpsc::Sender<ProcessEvent>,
) -> Result<ProcessResult, AppError> {
    let lang = request.language;
    let tree_nodes = build_tree_nodes(&walker.candidates);
    let suggested_result_name = suggested_merge_result_name(
        request.selected_folder.as_deref(),
        &request.selected_files,
        request.options.output_format,
    );
    let mut stats = ProcessingStats {
        skipped_files: walker.skipped,
        ..ProcessingStats::default()
    };

    if matches!(request.options.mode, ProcessingMode::TreeOnly) {
        return Ok(ProcessResult {
            stats,
            tree_string: walker.tree,
            tree_nodes,
            merged_content_path: None,
            suggested_result_name,
            file_details: Vec::new(),
            preview_files: Vec::new(),
            preview_blob_dir: None,
        });
    }

    let output_format = request.options.output_format;
    let process_dir = temp_file::make_temp_process_dir()?;
    let result_path = temp_file::make_temp_result_path_in(&process_dir);
    let preview_dir = temp_file::make_temp_preview_dir_in(&process_dir)?;
    let mut output = match tokio::fs::File::create(&result_path).await {
        Ok(file) => file,
        Err(err) => {
            let _ = temp_file::cleanup_temp_dir(&process_dir);
            return Err(AppError::new(format!("create merged file failed: {err}")));
        }
    };
    if let Err(err) = output
        .write_all(render_prefix(output_format, &walker.tree).as_bytes())
        .await
    {
        cleanup_failed_run(&process_dir);
        return Err(AppError::new(format!("write merged prefix failed: {err}")));
    }

    let concurrency_limit = file_concurrency_limit(walker.candidates.len());
    let mut processed_stream = stream::iter(
        walker
            .candidates
            .into_iter()
            .map(|file| process_candidate_file(file, request.options.compress)),
    )
    .buffered(concurrency_limit);

    let mut preview_files = Vec::new();
    let mut file_details = Vec::new();

    while let Some(outcome) = processed_stream.next().await {
        if cancel.is_cancelled() {
            cleanup_failed_run(&process_dir);
            return Err(AppError::new(tr(lang, "cancelled")));
        }

        match outcome {
            FileProcessOutcome::Skipped { file, reason } => {
                stats.skipped_files += 1;
                let _ = tx.send(ProcessEvent::Record(ProcessRecord {
                    file_name: file,
                    status: ProcessStatus::Skipped,
                    chars: None,
                    tokens: None,
                    error: Some(reason),
                }));
            }
            FileProcessOutcome::Failed { file, error } => {
                stats.skipped_files += 1;
                let _ = tx.send(ProcessEvent::Record(ProcessRecord {
                    file_name: file,
                    status: ProcessStatus::Failed,
                    chars: None,
                    tokens: None,
                    error: Some(error),
                }));
            }
            FileProcessOutcome::Processed {
                detail,
                merged,
                chars,
                tokens,
                archive,
            } => {
                stats.processed_files += 1;
                stats.total_chars += chars;
                stats.total_tokens += tokens;
                if let Err(err) = output
                    .write_all(render_file_entry(output_format, &merged).as_bytes())
                    .await
                {
                    cleanup_failed_run(&process_dir);
                    return Err(AppError::new(format!("write merged content failed: {err}")));
                }
                let next_id = preview_files.len() as u32;
                let blob_path = preview_dir.join(format!("preview_{next_id}.txt"));
                if let Err(err) = tokio::fs::write(&blob_path, merged.content.as_bytes()).await {
                    cleanup_failed_run(&process_dir);
                    return Err(AppError::new(format!("write preview blob failed: {err}")));
                }
                preview_files.push(PreviewFileEntry {
                    id: next_id,
                    display_path: detail.path.clone(),
                    chars,
                    tokens,
                    preview_blob_path: blob_path,
                    byte_len: merged.content.len() as u64,
                    archive,
                });
                let _ = tx.send(ProcessEvent::Record(ProcessRecord {
                    file_name: detail.path.clone(),
                    status: ProcessStatus::Success,
                    chars: Some(chars),
                    tokens: Some(tokens),
                    error: None,
                }));
                file_details.push(detail);
            }
        }
    }

    let suffix = render_suffix(output_format);
    if !suffix.is_empty()
        && let Err(err) = output.write_all(suffix.as_bytes()).await
    {
        cleanup_failed_run(&process_dir);
        return Err(AppError::new(format!("write merged suffix failed: {err}")));
    }
    if let Err(err) = output.flush().await {
        cleanup_failed_run(&process_dir);
        return Err(AppError::new(format!("flush merged file failed: {err}")));
    }

    Ok(ProcessResult {
        stats,
        tree_string: walker.tree,
        tree_nodes,
        merged_content_path: Some(result_path),
        suggested_result_name,
        file_details,
        preview_files,
        preview_blob_dir: Some(preview_dir),
    })
}

fn cleanup_failed_run(process_dir: &std::path::Path) {
    let _ = temp_file::cleanup_temp_dir(process_dir);
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
        archive: Option<ArchiveEntrySource>,
    },
}

async fn process_candidate_file(file: CandidateFile, compress: bool) -> FileProcessOutcome {
    let absolute = file.absolute;
    let relative = file.relative;
    let archive_entry = file.archive_entry;
    let archive_path = file.archive_path;
    let rel_for_error = relative.clone();
    match tokio::task::spawn_blocking(move || {
        let archive = archive_entry_source(archive_path.as_deref(), &relative);
        let raw = match archive_entry.as_deref() {
            Some(entry_name) => read_zip_entry_text(&absolute, entry_name),
            None => read_text_blocking(&absolute),
        };
        let raw = match raw {
            Ok(content) => content,
            Err(error) => {
                return FileProcessOutcome::Skipped {
                    file: relative,
                    reason: format!("read failed: {error}"),
                };
            }
        };
        let compression_path = archive_entry
            .as_deref()
            .map(Path::new)
            .unwrap_or(absolute.as_path());
        let (compressed, _warn) = compress_by_extension(compression_path, &raw, compress);
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
            archive,
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

fn archive_entry_source(
    archive_path: Option<&str>,
    display_path: &str,
) -> Option<ArchiveEntrySource> {
    let archive_path = archive_path?;
    let entry_path = display_path
        .strip_prefix(archive_path)?
        .strip_prefix('/')?
        .to_string();
    if entry_path.is_empty() {
        return None;
    }
    Some(ArchiveEntrySource {
        archive_path: archive_path.to_string(),
        entry_path,
    })
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::time::Duration;

    use tempfile::tempdir;
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    use super::{ProcessEvent, ProcessRequest, cleanup_failed_run, start};
    use crate::domain::{Language, OutputFormat, ProcessingMode, ProcessingOptions};
    use crate::utils::temp_file::{
        make_temp_preview_dir_in, make_temp_process_dir, make_temp_result_path_in,
    };

    #[test]
    fn cleanup_failed_run_removes_process_dir_and_result_file() {
        let process_dir = make_temp_process_dir().expect("process dir");
        let result_path = make_temp_result_path_in(&process_dir);
        let preview_dir = make_temp_preview_dir_in(&process_dir).expect("preview dir");
        fs::write(&result_path, "merged").expect("write merged result");
        fs::write(preview_dir.join("preview_0.txt"), "content").expect("write preview");

        cleanup_failed_run(&process_dir);

        assert!(!process_dir.exists());
        assert!(!result_path.exists());
    }

    #[test]
    fn cleanup_failed_run_is_idempotent_on_missing_dir() {
        let dir = tempdir().expect("tempdir");
        let process_dir = dir.path().join("missing-process-dir");

        cleanup_failed_run(&process_dir);
        cleanup_failed_run(&process_dir);
    }

    #[test]
    fn start_selected_zip_honors_blacklist_inside_archive() {
        let dir = tempdir().expect("tempdir");
        let zip_path = dir.path().join("bundle.zip");
        write_test_zip(
            &zip_path,
            &[
                ("src/lib.rs", "pub fn from_zip() -> bool { true }\n"),
                ("README.md", "# zipped\n"),
                ("assets/logo.png", "binary"),
            ],
        );

        let handle = start(ProcessRequest {
            selected_folder: None,
            selected_files: vec![zip_path],
            folder_blacklist: vec!["src".to_string()],
            ext_blacklist: vec![".png".to_string()],
            options: ProcessingOptions {
                compress: false,
                use_gitignore: false,
                ignore_git: false,
                output_format: OutputFormat::Default,
                mode: ProcessingMode::Full,
            },
            language: Language::Zh,
        });

        let result = loop {
            match handle
                .receiver
                .recv_timeout(Duration::from_secs(10))
                .expect("process event")
            {
                ProcessEvent::Completed(result) => break result,
                ProcessEvent::Failed(error) => panic!("unexpected failure: {error}"),
                ProcessEvent::Cancelled => panic!("unexpected cancellation"),
                ProcessEvent::Scanning { .. } | ProcessEvent::Record(_) => {}
            }
        };

        assert_eq!(result.file_details.len(), 1);
        assert_eq!(result.preview_files.len(), 1);
        assert!(!result.tree_string.contains("bundle.zip/assets/logo.png"));
        assert!(!result.tree_string.contains("bundle.zip/src/lib.rs"));
        assert_eq!(
            result.preview_files[0]
                .archive
                .as_ref()
                .map(|archive| (archive.archive_path.as_str(), archive.entry_path.as_str())),
            Some(("bundle.zip", "README.md"))
        );
        let merged_path = result.merged_content_path.expect("merged path");
        let merged = fs::read_to_string(merged_path).expect("read merged");
        assert!(merged.contains("文件路径: bundle.zip/README.md"));
        assert!(!merged.contains("文件路径: bundle.zip/src/lib.rs"));
        assert!(!merged.contains("pub fn from_zip() -> bool { true }"));
        assert!(!merged.contains("文件路径: bundle.zip/assets/logo.png"));
    }

    fn write_test_zip(path: &std::path::Path, files: &[(&str, &str)]) {
        let file = fs::File::create(path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, content) in files {
            zip.start_file(name, options).expect("start file");
            zip.write_all(content.as_bytes()).expect("write zip entry");
        }
        zip.finish().expect("finish zip");
    }
}
