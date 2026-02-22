use std::collections::HashSet;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct CandidateFile {
    pub absolute: PathBuf,
    pub relative: String,
}

#[derive(Debug, Clone, Default)]
pub struct WalkerOutput {
    pub candidates: Vec<CandidateFile>,
    pub skipped: usize,
    pub tree: String,
}

pub fn collect_candidates(
    selected_folder: Option<&PathBuf>,
    selected_files: &[PathBuf],
    folder_blacklist: &[String],
    ext_blacklist: &[String],
) -> WalkerOutput {
    let folder_blacklist_set: HashSet<String> =
        folder_blacklist.iter().map(|v| v.to_lowercase()).collect();
    let ext_blacklist_set: HashSet<String> =
        ext_blacklist.iter().map(|v| v.to_lowercase()).collect();

    let mut candidates = Vec::new();
    let mut skipped = 0usize;

    if let Some(root) = selected_folder {
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path().to_path_buf();
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");

            if should_skip(&rel, &folder_blacklist_set, &ext_blacklist_set) {
                skipped += 1;
                continue;
            }

            candidates.push(CandidateFile {
                absolute: path,
                relative: rel,
            });
        }
    }

    for path in selected_files {
        if !path.is_file() {
            skipped += 1;
            continue;
        }

        let rel = path
            .file_name()
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        if should_skip(&rel, &folder_blacklist_set, &ext_blacklist_set) {
            skipped += 1;
            continue;
        }

        candidates.push(CandidateFile {
            absolute: path.clone(),
            relative: rel,
        });
    }

    let tree = build_tree(selected_folder, &candidates);

    WalkerOutput {
        candidates,
        skipped,
        tree,
    }
}

fn should_skip(
    path: &str,
    folder_blacklist: &HashSet<String>,
    ext_blacklist: &HashSet<String>,
) -> bool {
    let lower = path.to_lowercase();

    if lower
        .split('/')
        .any(|segment| folder_blacklist.contains(segment))
    {
        return true;
    }

    ext_blacklist.iter().any(|ext| lower.ends_with(ext))
}

fn build_tree(selected_folder: Option<&PathBuf>, candidates: &[CandidateFile]) -> String {
    match selected_folder {
        Some(root) => {
            let root_name = root
                .file_name()
                .map(|v| v.to_string_lossy().to_string())
                .unwrap_or_else(|| "root".to_string());
            let mut lines = vec![format!("{root_name}/")];
            let mut rels: Vec<_> = candidates.iter().map(|c| c.relative.clone()).collect();
            rels.sort();
            for rel in rels {
                let depth = rel.matches('/').count();
                let indent = "  ".repeat(depth + 1);
                lines.push(format!("{indent}├── {rel}"));
            }
            lines.join("\n")
        }
        None => {
            let mut lines = vec!["selected_files/".to_string()];
            let mut rels: Vec<_> = candidates.iter().map(|c| c.relative.clone()).collect();
            rels.sort();
            for rel in rels {
                lines.push(format!("  ├── {rel}"));
            }
            lines.join("\n")
        }
    }
}

pub fn normalize_ext(input: &str) -> String {
    let t = input.trim().to_lowercase();
    if t.is_empty() {
        return t;
    }
    if t.starts_with('.') {
        t
    } else {
        format!(".{t}")
    }
}

pub fn parse_gitignore_rules(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with('!') {
            continue;
        }
        let normalized = t.trim_start_matches('/').trim_end_matches('/').to_string();
        if normalized.is_empty() {
            continue;
        }
        out.push(normalized);
    }
    out
}

pub fn unique_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut set = HashSet::new();
    let mut out = Vec::new();
    for p in paths {
        let key = p.to_string_lossy().to_string();
        if set.insert(key) {
            out.push(p.clone());
        }
    }
    out
}

pub fn auto_gitignore_path(root: &Path) -> PathBuf {
    root.join(".gitignore")
}
