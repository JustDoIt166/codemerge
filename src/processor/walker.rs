use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ignore::{DirEntry, WalkBuilder, WalkState};

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WalkerOptions {
    pub use_gitignore: bool,
    pub ignore_git: bool,
}

pub fn collect_candidates(
    selected_folder: Option<&PathBuf>,
    selected_files: &[PathBuf],
    folder_blacklist: &[String],
    ext_blacklist: &[String],
    options: WalkerOptions,
) -> WalkerOutput {
    collect_candidates_with_progress(
        selected_folder,
        selected_files,
        folder_blacklist,
        ext_blacklist,
        options,
        |_scanned, _candidates, _skipped| {},
    )
}

pub fn collect_candidates_with_progress<F>(
    selected_folder: Option<&PathBuf>,
    selected_files: &[PathBuf],
    folder_blacklist: &[String],
    ext_blacklist: &[String],
    options: WalkerOptions,
    on_progress: F,
) -> WalkerOutput
where
    F: Fn(usize, usize, usize) + Send + Sync + 'static,
{
    let folder_blacklist_set: HashSet<String> =
        folder_blacklist.iter().map(|v| v.to_lowercase()).collect();
    let ext_blacklist_set: HashSet<String> =
        ext_blacklist.iter().map(|v| v.to_lowercase()).collect();

    let on_progress: Arc<dyn Fn(usize, usize, usize) + Send + Sync> = Arc::new(on_progress);
    let mut candidates = Vec::new();
    let mut skipped = 0usize;
    let mut scanned_total = 0usize;

    if let Some(root) = selected_folder {
        let candidates_acc = Arc::new(Mutex::new(Vec::new()));
        let skipped_acc = Arc::new(AtomicUsize::new(0));
        let scanned_acc = Arc::new(AtomicUsize::new(0));
        let folder_blacklist_set = Arc::new(folder_blacklist_set.clone());
        let ext_blacklist_set = Arc::new(ext_blacklist_set.clone());
        let on_progress = Arc::clone(&on_progress);
        let root = root.clone();

        let mut builder = WalkBuilder::new(&root);
        builder
            .hidden(false)
            .ignore(options.use_gitignore)
            .git_ignore(options.use_gitignore)
            .git_global(options.use_gitignore)
            .git_exclude(options.use_gitignore)
            .require_git(false)
            .parents(true);
        if let Ok(parallelism) = std::thread::available_parallelism() {
            builder.threads(parallelism.get());
        }

        builder.build_parallel().run(|| {
            let candidates_acc = Arc::clone(&candidates_acc);
            let skipped_acc = Arc::clone(&skipped_acc);
            let scanned_acc = Arc::clone(&scanned_acc);
            let folder_blacklist_set = Arc::clone(&folder_blacklist_set);
            let ext_blacklist_set = Arc::clone(&ext_blacklist_set);
            let on_progress = Arc::clone(&on_progress);
            let root = root.clone();

            Box::new(move |entry| match entry {
                Ok(entry) => {
                    if process_entry(
                        &entry,
                        &root,
                        &folder_blacklist_set,
                        &ext_blacklist_set,
                        options.ignore_git,
                        &candidates_acc,
                        &skipped_acc,
                    ) {
                        let scanned = scanned_acc.fetch_add(1, Ordering::Relaxed) + 1;
                        if scanned.is_multiple_of(200) {
                            let current_skipped = skipped_acc.load(Ordering::Relaxed);
                            let current_candidates =
                                candidates_acc.lock().map(|v| v.len()).unwrap_or_default();
                            on_progress(scanned, current_candidates, current_skipped);
                        }
                    }
                    WalkState::Continue
                }
                Err(_) => WalkState::Continue,
            })
        });

        scanned_total += scanned_acc.load(Ordering::Relaxed);
        skipped += skipped_acc.load(Ordering::Relaxed);
        candidates = match Arc::try_unwrap(candidates_acc) {
            Ok(mutex) => mutex.into_inner().unwrap_or_default(),
            Err(arc) => arc.lock().map(|v| v.clone()).unwrap_or_default(),
        };
        on_progress(scanned_total, candidates.len(), skipped);
    }

    for path in selected_files {
        scanned_total += 1;
        if !path.is_file() {
            skipped += 1;
            on_progress(scanned_total, candidates.len(), skipped);
            continue;
        }

        let rel = path
            .file_name()
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        if should_skip(
            &rel,
            &folder_blacklist_set,
            &ext_blacklist_set,
            options.ignore_git,
        ) {
            skipped += 1;
            on_progress(scanned_total, candidates.len(), skipped);
            continue;
        }

        candidates.push(CandidateFile {
            absolute: path.clone(),
            relative: rel,
        });
        on_progress(scanned_total, candidates.len(), skipped);
    }
    candidates.sort_by(|a, b| a.relative.cmp(&b.relative));

    let tree = build_tree(selected_folder, &candidates);

    WalkerOutput {
        candidates,
        skipped,
        tree,
    }
}

fn process_entry(
    entry: &DirEntry,
    root: &Path,
    folder_blacklist: &HashSet<String>,
    ext_blacklist: &HashSet<String>,
    ignore_git: bool,
    candidates: &Arc<Mutex<Vec<CandidateFile>>>,
    skipped: &Arc<AtomicUsize>,
) -> bool {
    if !entry.file_type().is_some_and(|ft| ft.is_file()) {
        return false;
    }

    let path = entry.path().to_path_buf();
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path.as_path())
        .to_string_lossy()
        .replace('\\', "/");

    if should_skip(&rel, folder_blacklist, ext_blacklist, ignore_git) {
        skipped.fetch_add(1, Ordering::Relaxed);
        return true;
    }

    if let Ok(mut locked) = candidates.lock() {
        locked.push(CandidateFile {
            absolute: path,
            relative: rel,
        });
    }
    true
}

fn should_skip(
    path: &str,
    folder_blacklist: &HashSet<String>,
    ext_blacklist: &HashSet<String>,
    ignore_git: bool,
) -> bool {
    let lower = path.to_lowercase();

    if (ignore_git || folder_blacklist.contains(".git"))
        && lower.split('/').any(|segment| segment == ".git")
    {
        return true;
    }

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
