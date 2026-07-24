use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::stream::{self, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::domain::{
    ArchiveEntrySource, FileDetail, Language, PreviewFileEntry, ProcessRecord, ProcessResult,
    ProcessStatus, ProcessingMode, ProcessingOptions, TemporaryWhitelistMode,
};
use crate::error::AppError;
use crate::processor::archive::read_zip_entry_text;
use crate::processor::merger::{
    MergedFile, render_file_entry_with_metadata, render_prefix_with_metadata,
    render_suffix_with_metadata,
};
use crate::processor::reader::{compress_by_extension, count_chars_tokens, read_text_blocking};
use crate::processor::stats::ProcessingStats;
use crate::processor::walker::{
    CandidateFile, WalkerFilterRules, WalkerOptions, WalkerOutput,
    collect_candidates_with_progress_and_cancel,
};
use crate::services::tree::build_tree_nodes;
use crate::utils::i18n::tr;
use crate::utils::path::suggested_merge_result_name;
use crate::utils::temp_file;

pub type ProcessRunId = u64;
pub type ProcessEventSink = Arc<dyn Fn(ProcessEvent) + Send + Sync>;

#[derive(Debug)]
pub struct ProcessEvent {
    pub run_id: ProcessRunId,
    pub kind: ProcessEventKind,
}

#[derive(Debug)]
pub enum ProcessEventKind {
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

impl ProcessEvent {
    pub fn scanning(
        run_id: ProcessRunId,
        scanned: usize,
        candidates: usize,
        skipped: usize,
    ) -> Self {
        Self {
            run_id,
            kind: ProcessEventKind::Scanning {
                scanned,
                candidates,
                skipped,
            },
        }
    }

    pub fn record(run_id: ProcessRunId, record: ProcessRecord) -> Self {
        Self {
            run_id,
            kind: ProcessEventKind::Record(record),
        }
    }

    pub fn completed(run_id: ProcessRunId, result: ProcessResult) -> Self {
        Self {
            run_id,
            kind: ProcessEventKind::Completed(result),
        }
    }

    pub fn failed(run_id: ProcessRunId, error: AppError) -> Self {
        Self {
            run_id,
            kind: ProcessEventKind::Failed(error),
        }
    }

    pub fn cancelled(run_id: ProcessRunId) -> Self {
        Self {
            run_id,
            kind: ProcessEventKind::Cancelled,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessRequest {
    pub selected_folder: Option<PathBuf>,
    pub selected_files: Vec<PathBuf>,
    pub folder_blacklist: Vec<String>,
    pub ext_blacklist: Vec<String>,
    pub excluded_files: Vec<String>,
    pub folder_whitelist: Vec<String>,
    pub ext_whitelist: Vec<String>,
    pub whitelist_mode: TemporaryWhitelistMode,
    pub options: ProcessingOptions,
    pub language: Language,
}

pub async fn execute(
    request: ProcessRequest,
    run_id: ProcessRunId,
    cancel: CancellationToken,
    emit: ProcessEventSink,
) -> Result<ProcessResult, AppError> {
    let lang = request.language;
    let progress_emit = Arc::clone(&emit);
    let scan_cancel = cancel.clone();
    let cancel_for_walker = cancel.clone();
    let walker = collect_candidates_with_progress_and_cancel(
        request.selected_folder.as_ref(),
        &request.selected_files,
        WalkerFilterRules {
            folder_blacklist: &request.folder_blacklist,
            ext_blacklist: &request.ext_blacklist,
            excluded_files: &request.excluded_files,
            folder_whitelist: &request.folder_whitelist,
            ext_whitelist: &request.ext_whitelist,
            whitelist_mode: request.whitelist_mode,
        },
        WalkerOptions {
            use_gitignore: request.options.use_gitignore,
            ignore_git: request.options.ignore_git,
        },
        move |scanned, candidates, skipped| {
            if !scan_cancel.is_cancelled() {
                progress_emit(ProcessEvent::scanning(run_id, scanned, candidates, skipped));
            }
        },
        move || cancel_for_walker.is_cancelled(),
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

    run_process_with_walker(request, run_id, walker, cancel, emit).await
}

async fn run_process_with_walker(
    request: ProcessRequest,
    run_id: ProcessRunId,
    walker: WalkerOutput,
    cancel: CancellationToken,
    emit: ProcessEventSink,
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
            process_dir: None,
            merged_content_path: None,
            merged_content_bytes: 0,
            suggested_result_name,
            file_details: Vec::new(),
            preview_files: Vec::new(),
            preview_blob_dir: None,
        });
    }

    let output_format = request.options.output_format;
    let output_metadata = &request.options.output_metadata;
    let process_dir = temp_file::ProcessTempDir::create()?;
    let result_path = temp_file::make_temp_result_path_in(process_dir.path());
    let preview_dir = temp_file::make_temp_preview_dir_in(process_dir.path())?;
    let mut output = match tokio::fs::File::create(&result_path).await {
        Ok(file) => file,
        Err(err) => {
            return Err(AppError::new(format!("create merged file failed: {err}")));
        }
    };
    if let Err(err) = output
        .write_all(
            render_prefix_with_metadata(output_format, &walker.tree, lang, output_metadata)
                .as_bytes(),
        )
        .await
    {
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
            return Err(AppError::new(tr(lang, "cancelled")));
        }

        match outcome {
            FileProcessOutcome::Skipped { file, reason } => {
                stats.skipped_files += 1;
                emit(ProcessEvent::record(
                    run_id,
                    ProcessRecord {
                        file_name: file,
                        status: ProcessStatus::Skipped,
                        chars: None,
                        tokens: None,
                        error: Some(reason),
                    },
                ));
            }
            FileProcessOutcome::Failed { file, error } => {
                stats.skipped_files += 1;
                emit(ProcessEvent::record(
                    run_id,
                    ProcessRecord {
                        file_name: file,
                        status: ProcessStatus::Failed,
                        chars: None,
                        tokens: None,
                        error: Some(error),
                    },
                ));
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
                    .write_all(
                        render_file_entry_with_metadata(
                            output_format,
                            &merged,
                            lang,
                            output_metadata,
                        )
                        .as_bytes(),
                    )
                    .await
                {
                    return Err(AppError::new(format!("write merged content failed: {err}")));
                }
                let next_id = preview_files.len() as u32;
                let blob_path = preview_dir.join(format!("preview_{next_id}.txt"));
                if let Err(err) = tokio::fs::write(&blob_path, merged.content.as_bytes()).await {
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
                emit(ProcessEvent::record(
                    run_id,
                    ProcessRecord {
                        file_name: detail.path.clone(),
                        status: ProcessStatus::Success,
                        chars: Some(chars),
                        tokens: Some(tokens),
                        error: None,
                    },
                ));
                file_details.push(detail);
            }
        }
    }

    if stats.processed_files == 0 {
        return Err(AppError::new(tr(lang, "no_content_generated")));
    }

    let suffix = render_suffix_with_metadata(output_format, output_metadata);
    if !suffix.is_empty()
        && let Err(err) = output.write_all(suffix.as_bytes()).await
    {
        return Err(AppError::new(format!("write merged suffix failed: {err}")));
    }
    if let Err(err) = output.flush().await {
        return Err(AppError::new(format!("flush merged file failed: {err}")));
    }

    let merged_content_bytes = match output.metadata().await {
        Ok(metadata) => metadata.len(),
        Err(err) => {
            return Err(AppError::new(format!(
                "read merged file metadata failed: {err}"
            )));
        }
    };

    let process_dir = process_dir.into_persisted_path();

    Ok(ProcessResult {
        stats,
        tree_string: walker.tree,
        tree_nodes,
        process_dir: Some(process_dir),
        merged_content_path: Some(result_path),
        merged_content_bytes,
        suggested_result_name,
        file_details,
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
            archive: archive_path
                .as_deref()
                .and_then(|archive_path| archive_entry_source(archive_path, &relative)),
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

fn archive_entry_source(archive_path: &str, display_path: &str) -> Option<ArchiveEntrySource> {
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
    workers.saturating_mul(2).clamp(2, 32).min(total_files)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use tempfile::tempdir;
    use tokio_util::sync::CancellationToken;
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    use super::{
        ProcessEventSink, ProcessRequest, archive_entry_source, execute, file_concurrency_limit,
        run_process_with_walker,
    };
    use crate::domain::{
        Language, OutputFormat, OutputMetadataOptions, ProcessingMode, ProcessingOptions,
        TemporaryWhitelistMode,
    };
    use std::sync::Arc;

    fn run_request(request: ProcessRequest) -> crate::domain::ProcessResult {
        let runtime = crate::services::runtime::RUNTIME
            .as_ref()
            .expect("runtime available");
        runtime
            .block_on(execute(
                request,
                1,
                CancellationToken::new(),
                Arc::new(|_| {}),
            ))
            .expect("process execution")
    }

    #[test]
    fn full_run_fails_when_no_candidate_content_can_be_read() {
        let dir = tempdir().expect("tempdir");
        let missing = dir.path().join("missing.rs");
        let request = ProcessRequest {
            selected_folder: None,
            selected_files: vec![missing.clone()],
            folder_blacklist: Vec::new(),
            ext_blacklist: Vec::new(),
            excluded_files: Vec::new(),
            folder_whitelist: Vec::new(),
            ext_whitelist: Vec::new(),
            whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            options: ProcessingOptions {
                compress: false,
                use_gitignore: false,
                ignore_git: false,
                output_format: OutputFormat::Default,
                output_metadata: Default::default(),
                mode: ProcessingMode::Full,
            },
            language: Language::En,
        };
        let walker = crate::processor::walker::WalkerOutput {
            candidates: vec![crate::processor::walker::CandidateFile {
                absolute: missing,
                relative: "missing.rs".into(),
                archive_entry: None,
                archive_path: None,
            }],
            skipped: 0,
            tree: "selected_files/\n  ├── missing.rs".into(),
        };
        let (tx, _rx) = std::sync::mpsc::channel();
        let emit: ProcessEventSink = Arc::new(move |event| {
            let _ = tx.send(event);
        });
        let runtime = match &*crate::services::runtime::RUNTIME {
            Ok(runtime) => runtime,
            Err(error) => panic!("runtime unavailable: {error}"),
        };

        let error = runtime
            .block_on(run_process_with_walker(
                request,
                1,
                walker,
                CancellationToken::new(),
                emit,
            ))
            .expect_err("all unreadable candidates must fail");

        assert_eq!(
            error.to_string(),
            "All candidate files failed; no usable content was generated"
        );
    }

    #[test]
    fn archive_entry_source_only_builds_archive_metadata_for_archive_paths() {
        assert_eq!(
            archive_entry_source("bundle.zip", "bundle.zip/src/lib.rs")
                .map(|source| (source.archive_path, source.entry_path)),
            Some(("bundle.zip".to_string(), "src/lib.rs".to_string()))
        );
        assert!(archive_entry_source("bundle.zip", "src/lib.rs").is_none());
    }

    #[test]
    fn file_concurrency_limit_stays_bounded_for_large_runs() {
        assert_eq!(file_concurrency_limit(0), 1);
        assert_eq!(file_concurrency_limit(1), 1);
        assert!(file_concurrency_limit(10_000) <= 32);
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

        let result = run_request(ProcessRequest {
            selected_folder: None,
            selected_files: vec![zip_path],
            folder_blacklist: vec!["src".to_string()],
            ext_blacklist: vec![".png".to_string()],
            excluded_files: Vec::new(),
            folder_whitelist: Vec::new(),
            ext_whitelist: Vec::new(),
            whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            options: ProcessingOptions {
                compress: false,
                use_gitignore: false,
                ignore_git: false,
                output_format: OutputFormat::Default,
                output_metadata: Default::default(),
                mode: ProcessingMode::Full,
            },
            language: Language::Zh,
        });

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

    #[test]
    fn start_whitelist_then_blacklist_limits_processed_files() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::create_dir_all(root.join("docs")).expect("mkdir docs");
        fs::write(root.join("src/lib.rs"), "pub fn scoped() {}\n").expect("write lib");
        fs::write(root.join("docs/guide.md"), "# guide\n").expect("write guide");

        let result = run_request(ProcessRequest {
            selected_folder: Some(root.to_path_buf()),
            selected_files: Vec::new(),
            folder_blacklist: Vec::new(),
            ext_blacklist: Vec::new(),
            excluded_files: Vec::new(),
            folder_whitelist: vec!["src".to_string()],
            ext_whitelist: Vec::new(),
            whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            options: ProcessingOptions {
                compress: false,
                use_gitignore: false,
                ignore_git: false,
                output_format: OutputFormat::Default,
                output_metadata: Default::default(),
                mode: ProcessingMode::Full,
            },
            language: Language::Zh,
        });

        assert_eq!(result.file_details.len(), 1);
        assert_eq!(result.file_details[0].path, "src/lib.rs");
        let merged_path = result.merged_content_path.expect("merged path");
        let merged = fs::read_to_string(merged_path).expect("read merged");
        assert!(merged.contains("文件路径: src/lib.rs"));
        assert!(!merged.contains("文件路径: docs/guide.md"));
    }

    #[test]
    fn full_run_can_write_content_without_any_metadata() {
        let dir = tempdir().expect("tempdir");
        let source_path = dir.path().join("plain.txt");
        fs::write(&source_path, "exact content").expect("write source");

        let result = run_request(ProcessRequest {
            selected_folder: None,
            selected_files: vec![source_path],
            folder_blacklist: Vec::new(),
            ext_blacklist: Vec::new(),
            excluded_files: Vec::new(),
            folder_whitelist: Vec::new(),
            ext_whitelist: Vec::new(),
            whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            options: ProcessingOptions {
                output_metadata: OutputMetadataOptions {
                    directory_structure: false,
                    file_path: false,
                    char_token_counts: false,
                    separator: false,
                },
                ..ProcessingOptions::default()
            },
            language: Language::Zh,
        });

        let merged_path = result.merged_content_path.expect("merged path");
        assert_eq!(
            fs::read_to_string(merged_path).expect("read merged"),
            "exact content"
        );
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
