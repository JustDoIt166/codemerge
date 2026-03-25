use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::error::{AppError, AppResult};

pub const EXCERPT_PREVIEW_BYTES: u64 = 1024 * 1024;
pub const MAX_PREVIEW_LINE_BYTES: usize = 8 * 1024;
const TRUNCATED_PREVIEW_SUFFIX: &str = "...";

#[derive(Debug, Clone)]
pub enum PreviewRequest {
    Open {
        revision: u64,
        file_id: u32,
        path: PathBuf,
        initial_range: Range<usize>,
    },
    LoadRange {
        revision: u64,
        file_id: u32,
        document: PreviewDocument,
        range: Range<usize>,
    },
}

#[derive(Debug, Clone)]
pub enum PreviewEvent {
    Opened {
        revision: u64,
        file_id: u32,
        document: PreviewDocument,
        loaded_range: Range<usize>,
        lines: Vec<String>,
    },
    Loaded {
        revision: u64,
        file_id: u32,
        loaded_range: Range<usize>,
        lines: Vec<String>,
    },
    Failed {
        revision: u64,
        file_id: u32,
        error: AppError,
    },
}

#[derive(Debug, Clone)]
pub struct PreviewDocument {
    path: PathBuf,
    byte_len: u64,
    line_offsets: Arc<[u64]>,
}

impl PreviewDocument {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn line_count(&self) -> usize {
        self.line_offsets.len()
    }

    pub fn byte_len(&self) -> u64 {
        self.byte_len
    }
}

pub fn start(request: PreviewRequest) -> Receiver<PreviewEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || match request {
        PreviewRequest::Open {
            revision,
            file_id,
            path,
            initial_range,
        } => {
            let result = (|| {
                let document = index_document(&path)?;
                let loaded_range = clamp_range(&document, initial_range);
                let lines = load_range(&document, loaded_range.clone())?;
                Ok::<_, AppError>((document, loaded_range, lines))
            })();

            match result {
                Ok((document, loaded_range, lines)) => {
                    let _ = tx.send(PreviewEvent::Opened {
                        revision,
                        file_id,
                        document,
                        loaded_range,
                        lines,
                    });
                }
                Err(error) => {
                    let _ = tx.send(PreviewEvent::Failed {
                        revision,
                        file_id,
                        error,
                    });
                }
            }
        }
        PreviewRequest::LoadRange {
            revision,
            file_id,
            document,
            range,
        } => match load_range(&document, clamp_range(&document, range.clone())) {
            Ok(lines) => {
                let _ = tx.send(PreviewEvent::Loaded {
                    revision,
                    file_id,
                    loaded_range: clamp_range(&document, range),
                    lines,
                });
            }
            Err(error) => {
                let _ = tx.send(PreviewEvent::Failed {
                    revision,
                    file_id,
                    error,
                });
            }
        },
    });
    rx
}

pub fn index_document(path: &Path) -> AppResult<PreviewDocument> {
    let file = std::fs::File::open(path)
        .map_err(|e| AppError::new(format!("open preview file failed: {e}")))?;
    let byte_len = file
        .metadata()
        .map_err(|e| AppError::new(format!("read preview metadata failed: {e}")))?
        .len();

    let mut reader = BufReader::new(file);
    let mut line_offsets = vec![0];
    let mut buffer = Vec::with_capacity(4096);
    let mut offset = 0_u64;

    loop {
        buffer.clear();
        let read = reader
            .read_until(b'\n', &mut buffer)
            .map_err(|e| AppError::new(format!("index preview file failed: {e}")))?;
        if read == 0 {
            break;
        }
        offset += read as u64;
        if offset < byte_len {
            line_offsets.push(offset);
        }
    }

    Ok(PreviewDocument {
        path: path.to_path_buf(),
        byte_len,
        line_offsets: line_offsets.into(),
    })
}

pub fn load_range(document: &PreviewDocument, range: Range<usize>) -> AppResult<Vec<String>> {
    if range.start >= document.line_count() || range.start >= range.end {
        return Ok(Vec::new());
    }

    let clamped_end = range.end.min(document.line_count());
    let start_offset = document.line_offsets[range.start];
    let mut file = std::fs::File::open(&document.path)
        .map_err(|e| AppError::new(format!("open preview file failed: {e}")))?;
    file.seek(SeekFrom::Start(start_offset))
        .map_err(|e| AppError::new(format!("seek preview file failed: {e}")))?;

    let mut reader = BufReader::new(file);
    let mut lines = Vec::with_capacity(clamped_end - range.start);
    for _ in range.start..clamped_end {
        let mut line = String::new();
        let _ = reader
            .read_line(&mut line)
            .map_err(|e| AppError::new(format!("read preview file failed: {e}")))?;
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        truncate_preview_line_for_display(&mut line);
        lines.push(line);
    }

    if lines.is_empty() && document.line_count() == 1 && document.byte_len == 0 {
        lines.push(String::new());
    }

    Ok(lines)
}

pub fn load_text(document: &PreviewDocument) -> AppResult<String> {
    std::fs::read_to_string(&document.path)
        .map_err(|e| AppError::new(format!("read preview file failed: {e}")))
}

pub fn create_excerpt_preview(path: &Path, max_bytes: u64) -> AppResult<PathBuf> {
    let excerpt_path = build_excerpt_path(path)?;
    if let Err(error) = write_excerpt_preview(path, &excerpt_path, max_bytes) {
        let _ = std::fs::remove_file(&excerpt_path);
        return Err(error);
    }
    Ok(excerpt_path)
}

fn clamp_range(document: &PreviewDocument, range: Range<usize>) -> Range<usize> {
    let line_count = document.line_count();
    if line_count == 0 || range.start >= line_count || range.start >= range.end {
        return 0..line_count.min(1);
    }

    let start = range.start.min(line_count.saturating_sub(1));
    let end = range.end.max(start + 1).min(line_count);
    start..end
}

fn truncate_preview_line_for_display(line: &mut String) {
    if line.len() <= MAX_PREVIEW_LINE_BYTES {
        return;
    }

    let target = MAX_PREVIEW_LINE_BYTES.saturating_sub(TRUNCATED_PREVIEW_SUFFIX.len());
    let mut truncate_at = target.min(line.len());
    while truncate_at > 0 && !line.is_char_boundary(truncate_at) {
        truncate_at -= 1;
    }
    line.truncate(truncate_at);
    line.push_str(TRUNCATED_PREVIEW_SUFFIX);
}

fn build_excerpt_path(path: &Path) -> AppResult<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("resolve preview temp directory failed"))?;
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("preview");
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty());
    let unique = format!(
        "{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AppError::new(format!("system clock drifted before unix epoch: {e}")))?
            .as_nanos()
    );
    let file_name = match ext {
        Some(ext) => format!("{stem}_excerpt_{unique}.{ext}"),
        None => format!("{stem}_excerpt_{unique}"),
    };
    Ok(parent.join(file_name))
}

fn write_excerpt_preview(source: &Path, target: &Path, max_bytes: u64) -> AppResult<u64> {
    let input = std::fs::File::open(source)
        .map_err(|e| AppError::new(format!("open preview file failed: {e}")))?;
    let mut reader = BufReader::new(input);
    let mut output = std::fs::File::create(target)
        .map_err(|e| AppError::new(format!("create preview excerpt failed: {e}")))?;
    let mut written = 0_u64;
    let mut buffer = Vec::with_capacity(4096);

    loop {
        buffer.clear();
        let read = reader
            .read_until(b'\n', &mut buffer)
            .map_err(|e| AppError::new(format!("read preview file failed: {e}")))?;
        if read == 0 {
            break;
        }

        let next_written = written + read as u64;
        if written > 0 && next_written > max_bytes {
            break;
        }

        output
            .write_all(&buffer)
            .map_err(|e| AppError::new(format!("write preview excerpt failed: {e}")))?;
        written = next_written;

        if written >= max_bytes {
            break;
        }
    }

    output
        .flush()
        .map_err(|e| AppError::new(format!("flush preview excerpt failed: {e}")))?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_PREVIEW_LINE_BYTES, PreviewEvent, PreviewRequest, TRUNCATED_PREVIEW_SUFFIX,
        create_excerpt_preview, index_document, load_range, load_text, start,
    };

    fn with_preview_file(name: &str, content: &str, test: impl FnOnce(&std::path::Path)) {
        let root = std::env::temp_dir().join(format!(
            "codemerge_preview_tests_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create temp dir");
        let path = root.join(name);
        std::fs::write(&path, content).expect("write temp preview");
        test(&path);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn indexes_and_reads_preview_ranges() {
        with_preview_file("preview.txt", "alpha\nbeta\ngamma", |path| {
            let document = index_document(path).expect("index document");
            assert_eq!(document.line_count(), 3);
            assert_eq!(
                load_range(&document, 1..3).expect("load range"),
                vec!["beta".to_string(), "gamma".to_string()]
            );
        });
    }

    #[test]
    fn preview_document_clone_shares_line_offsets() {
        with_preview_file("clone.txt", "alpha\nbeta\ngamma", |path| {
            let document = index_document(path).expect("index document");
            let clone = document.clone();

            assert!(std::sync::Arc::ptr_eq(
                &document.line_offsets,
                &clone.line_offsets
            ));
        });
    }

    #[test]
    fn empty_file_keeps_a_single_blank_line() {
        with_preview_file("empty.txt", "", |path| {
            let document = index_document(path).expect("index document");
            assert_eq!(document.line_count(), 1);
            assert_eq!(
                load_range(&document, 0..1).expect("load range"),
                vec![String::new()]
            );
            assert_eq!(load_text(&document).expect("load text"), "");
        });
    }

    #[test]
    fn trailing_newline_does_not_create_fake_rows() {
        with_preview_file("trailing.txt", "a\nb\n", |path| {
            let document = index_document(path).expect("index document");
            assert_eq!(document.line_count(), 2);
            assert_eq!(
                load_range(&document, 0..2).expect("load range"),
                vec!["a".to_string(), "b".to_string()]
            );
        });
    }

    #[test]
    fn open_request_indexes_and_primes_initial_window() {
        with_preview_file("open.txt", "zero\none\ntwo\nthree", |path| {
            let rx = start(PreviewRequest::Open {
                revision: 7,
                file_id: 42,
                path: path.to_path_buf(),
                initial_range: 1..3,
            });

            match rx.recv().expect("preview event") {
                PreviewEvent::Opened {
                    revision,
                    file_id,
                    document,
                    loaded_range,
                    lines,
                } => {
                    assert_eq!(revision, 7);
                    assert_eq!(file_id, 42);
                    assert_eq!(document.line_count(), 4);
                    assert_eq!(loaded_range, 1..3);
                    assert_eq!(lines, vec!["one".to_string(), "two".to_string()]);
                }
                other => panic!("unexpected event: {other:?}"),
            }
        });
    }

    #[test]
    fn excerpt_preview_keeps_complete_lines_within_budget() {
        with_preview_file("excerpt.txt", "alpha\nbeta\ngamma\n", |path| {
            let excerpt = create_excerpt_preview(path, 8).expect("create excerpt");
            assert_eq!(excerpt.parent(), path.parent());
            assert_eq!(
                std::fs::read_to_string(&excerpt).expect("read excerpt"),
                "alpha\n"
            );
        });
    }

    #[test]
    fn excerpt_preview_keeps_first_line_when_it_alone_exceeds_budget() {
        with_preview_file(
            "excerpt_long.txt",
            "supercalifragilisticexpialidocious\nsecond\n",
            |path| {
                let excerpt = create_excerpt_preview(path, 4).expect("create excerpt");
                assert_eq!(
                    std::fs::read_to_string(&excerpt).expect("read excerpt"),
                    "supercalifragilisticexpialidocious\n"
                );
            },
        );
    }

    #[test]
    fn load_range_truncates_very_long_lines_for_display() {
        let long_line = "a".repeat(MAX_PREVIEW_LINE_BYTES + 512);
        with_preview_file("long_line.txt", &format!("{long_line}\nsecond\n"), |path| {
            let document = index_document(path).expect("index document");
            let lines = load_range(&document, 0..2).expect("load range");

            assert_eq!(lines.len(), 2);
            assert_eq!(lines[1], "second");
            assert!(lines[0].len() <= MAX_PREVIEW_LINE_BYTES);
            assert!(lines[0].ends_with(TRUNCATED_PREVIEW_SUFFIX));
        });
    }
}
