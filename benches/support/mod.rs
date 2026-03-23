#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use codemerge::processor::merger::MergedFile;
use codemerge::processor::walker::CandidateFile;

/// File scale gradients for multi-gradient benchmarks.
pub const SCALES: &[usize] = &[100, 1_000, 10_000];

/// File extensions cycled through for realistic coverage.
const EXTENSIONS: &[&str] = &["rs", "js", "html", "css", "json"];

/// Creates a temporary directory tree with `n` files spread across
/// a realistic nested folder structure.
///
/// Files are distributed into `ceil(sqrt(n))` modules, each with
/// `ceil(sqrt(n))` sub-folders, giving a realistic depth.
///
/// Returns `(TempDir, root_path)` — `TempDir` must be kept alive.
pub fn make_file_tree(n: usize) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create temp dir");
    let root = dir.path().to_path_buf();

    let modules = (n as f64).sqrt().ceil() as usize;
    let per_module = n.div_ceil(modules);

    let mut created = 0usize;
    for m in 0..modules {
        if created >= n {
            break;
        }
        let module_dir = root.join(format!("module_{m}"));
        let subs = (per_module as f64).sqrt().ceil() as usize;
        let per_sub = per_module.div_ceil(subs);

        for s in 0..subs {
            if created >= n {
                break;
            }
            let sub_dir = module_dir.join(format!("sub_{s}"));
            fs::create_dir_all(&sub_dir).expect("create sub dir");

            for f in 0..per_sub {
                if created >= n {
                    break;
                }
                let ext = EXTENSIONS[created % EXTENSIONS.len()];
                let file_path = sub_dir.join(format!("file_{f}.{ext}"));
                let (content, _) = synthetic_content(created, 30);
                fs::write(&file_path, &content).expect("write synthetic file");
                created += 1;
            }
        }
    }

    (dir, root)
}

/// Generates synthetic source content.
///
/// `variant % 5` selects the language:
/// 0 = Rust, 1 = JavaScript, 2 = HTML, 3 = CSS, 4 = JSON
pub fn synthetic_content(variant: usize, line_count: usize) -> (String, &'static str) {
    match variant % 5 {
        0 => {
            // Rust
            let mut lines = Vec::with_capacity(line_count + 2);
            lines.push("fn main() {".to_string());
            for i in 0..line_count {
                lines.push(format!("    let var_{i} = {i} * 2 + 1;"));
            }
            lines.push("}".to_string());
            (lines.join("\n"), "rs")
        }
        1 => {
            // JavaScript
            let mut lines = Vec::with_capacity(line_count + 2);
            lines.push("function process(data) {".to_string());
            for i in 0..line_count {
                lines.push(format!("  const val_{i} = data.map(x => x + {i});"));
            }
            lines.push("}".to_string());
            (lines.join("\n"), "js")
        }
        2 => {
            // HTML
            let mut lines = Vec::with_capacity(line_count + 4);
            lines.push("<!DOCTYPE html><html><body>".to_string());
            for i in 0..line_count {
                lines.push(format!(
                    "  <div class=\"item-{i}\"><p>Content block {i}</p></div>"
                ));
            }
            lines.push("</body></html>".to_string());
            (lines.join("\n"), "html")
        }
        3 => {
            // CSS
            let mut lines = Vec::with_capacity(line_count);
            for i in 0..line_count {
                lines.push(format!(
                    ".class-{i} {{ margin: {i}px; padding: {}px; color: #333; }}",
                    i + 1
                ));
            }
            (lines.join("\n"), "css")
        }
        _ => {
            // JSON
            let mut entries = Vec::with_capacity(line_count);
            for i in 0..line_count {
                entries.push(format!("\"key_{i}\": \"value_{i}\""));
            }
            let content = format!("{{{}}}", entries.join(", "));
            (content, "json")
        }
    }
}

/// Builds `CandidateFile` entries from an existing file tree.
///
/// Walks the directory and collects up to `n` files as candidates.
pub fn candidate_files(root: &Path, n: usize) -> Vec<CandidateFile> {
    let mut candidates = Vec::new();
    collect_files_recursive(root, root, &mut candidates, n);
    candidates.sort_by(|a, b| a.relative.cmp(&b.relative));
    candidates
}

fn collect_files_recursive(
    base: &Path,
    current: &Path,
    candidates: &mut Vec<CandidateFile>,
    limit: usize,
) {
    if candidates.len() >= limit {
        return;
    }
    let mut entries: Vec<_> = fs::read_dir(current)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if candidates.len() >= limit {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(base, &path, candidates, limit);
        } else if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            candidates.push(CandidateFile {
                absolute: path,
                relative: rel,
                archive_entry: None,
                archive_path: None,
            });
        }
    }
}

/// Creates a temporary file with `n_lines` lines of text.
///
/// Returns `(TempDir, file_path)` — `TempDir` must be kept alive.
pub fn make_preview_file(n_lines: usize) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("preview_large.txt");

    let mut content = String::with_capacity(n_lines * 60);
    for i in 0..n_lines {
        content.push_str(&format!(
            "Line {i:06}: The quick brown fox jumps over the lazy dog.\n"
        ));
    }
    fs::write(&path, &content).expect("write preview file");

    (dir, path)
}

/// Generates `n` synthetic `MergedFile` structs.
pub fn merged_files(n: usize) -> Vec<MergedFile> {
    (0..n)
        .map(|i| {
            let ext = EXTENSIONS[i % EXTENSIONS.len()];
            let (content, _) = synthetic_content(i, 30);
            let chars = content.len();
            let tokens = chars / 5; // rough estimate
            MergedFile {
                path: format!(
                    "module_{}/sub_{}/file_{}.{}",
                    i / 100,
                    (i / 10) % 10,
                    i % 10,
                    ext
                ),
                chars,
                tokens,
                content,
            }
        })
        .collect()
}
