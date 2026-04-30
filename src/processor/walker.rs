use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ignore::{DirEntry, WalkBuilder, WalkState};

use crate::domain::TemporaryWhitelistMode;
use crate::processor::archive::{is_zip_path, list_zip_file_entries};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateFile {
    pub absolute: PathBuf,
    pub relative: String,
    pub archive_entry: Option<String>,
    pub archive_path: Option<String>,
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

#[derive(Clone, Copy)]
pub struct WalkerFilterRules<'a> {
    pub folder_blacklist: &'a [String],
    pub ext_blacklist: &'a [String],
    pub folder_whitelist: &'a [String],
    pub ext_whitelist: &'a [String],
    pub whitelist_mode: TemporaryWhitelistMode,
}

struct ResolvedFilterRules {
    folder_blacklist: HashSet<String>,
    ext_blacklist: HashSet<String>,
    folder_whitelist: HashSet<String>,
    ext_whitelist: HashSet<String>,
    whitelist_mode: TemporaryWhitelistMode,
}

pub fn collect_candidates(
    selected_folder: Option<&PathBuf>,
    selected_files: &[PathBuf],
    filters: WalkerFilterRules<'_>,
    options: WalkerOptions,
) -> WalkerOutput {
    collect_candidates_with_progress(
        selected_folder,
        selected_files,
        filters,
        options,
        |_scanned, _candidates, _skipped| {},
    )
}

pub fn collect_candidates_with_progress<F>(
    selected_folder: Option<&PathBuf>,
    selected_files: &[PathBuf],
    filters: WalkerFilterRules<'_>,
    options: WalkerOptions,
    on_progress: F,
) -> WalkerOutput
where
    F: Fn(usize, usize, usize) + Send + Sync + 'static,
{
    let resolved_filters = Arc::new(ResolvedFilterRules {
        folder_blacklist: filters
            .folder_blacklist
            .iter()
            .map(|v| v.to_lowercase())
            .collect(),
        ext_blacklist: filters
            .ext_blacklist
            .iter()
            .map(|v| v.to_lowercase())
            .collect(),
        folder_whitelist: filters
            .folder_whitelist
            .iter()
            .map(|v| v.to_lowercase())
            .collect(),
        ext_whitelist: filters
            .ext_whitelist
            .iter()
            .map(|v| v.to_lowercase())
            .collect(),
        whitelist_mode: filters.whitelist_mode,
    });

    let on_progress: Arc<dyn Fn(usize, usize, usize) + Send + Sync> = Arc::new(on_progress);
    let mut candidates = Vec::new();
    let mut skipped = 0usize;
    let mut scanned_total = 0usize;

    if let Some(root) = selected_folder {
        let candidates_acc = Arc::new(Mutex::new(Vec::new()));
        let skipped_acc = Arc::new(AtomicUsize::new(0));
        let scanned_acc = Arc::new(AtomicUsize::new(0));
        let resolved_filters = Arc::clone(&resolved_filters);
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
            let resolved_filters = Arc::clone(&resolved_filters);
            let on_progress = Arc::clone(&on_progress);
            let root = root.clone();

            Box::new(move |entry| match entry {
                Ok(entry) => {
                    let scanned_delta = process_entry(
                        &entry,
                        &root,
                        &resolved_filters,
                        options.ignore_git,
                        &candidates_acc,
                        &skipped_acc,
                    );
                    if scanned_delta > 0 {
                        let scanned_before =
                            scanned_acc.fetch_add(scanned_delta, Ordering::Relaxed);
                        let scanned = scanned_before + scanned_delta;
                        if scanned / 200 > scanned_before / 200 {
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
        if !path.is_file() {
            scanned_total += 1;
            skipped += 1;
            on_progress(scanned_total, candidates.len(), skipped);
            continue;
        }

        let rel = path
            .file_name()
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        // Explicitly selected file paths bypass root-level blacklist matching.
        let result =
            collect_path_candidates(path, rel, &resolved_filters, options.ignore_git, true);
        scanned_total += result.scanned;
        skipped += result.skipped;
        candidates.extend(result.candidates);
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
    filters: &ResolvedFilterRules,
    ignore_git: bool,
    candidates: &Arc<Mutex<Vec<CandidateFile>>>,
    skipped: &Arc<AtomicUsize>,
) -> usize {
    if !entry.file_type().is_some_and(|ft| ft.is_file()) {
        return 0;
    }

    let path = entry.path().to_path_buf();
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path.as_path())
        .to_string_lossy()
        .replace('\\', "/");

    let result = collect_path_candidates(&path, rel, filters, ignore_git, false);
    if result.skipped > 0 {
        skipped.fetch_add(result.skipped, Ordering::Relaxed);
    }
    if !result.candidates.is_empty()
        && let Ok(mut locked) = candidates.lock()
    {
        locked.extend(result.candidates);
    }
    result.scanned
}

fn matches_folder_rule(path: &str, folder_rules: &HashSet<String>) -> bool {
    if folder_rules.is_empty() {
        return false;
    }

    let lower = path.to_lowercase();
    folder_rules.iter().any(|rule| {
        if rule.contains('/') {
            lower.contains(rule)
        } else {
            lower.split('/').any(|segment| segment == rule)
        }
    })
}

fn matches_ext_rule(path: &str, ext_rules: &HashSet<String>) -> bool {
    let lower = path.to_lowercase();
    ext_rules.iter().any(|ext| lower.ends_with(ext))
}

fn should_skip_blacklist(
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

    if matches_folder_rule(&lower, folder_blacklist) {
        return true;
    }

    matches_ext_rule(&lower, ext_blacklist)
}

fn should_keep_whitelist(
    path: &str,
    folder_whitelist: &HashSet<String>,
    ext_whitelist: &HashSet<String>,
    whitelist_mode: TemporaryWhitelistMode,
) -> bool {
    let folder_match = matches_folder_rule(path, folder_whitelist);
    let ext_match = matches_ext_rule(path, ext_whitelist);
    let any_whitelist = !folder_whitelist.is_empty() || !ext_whitelist.is_empty();

    match whitelist_mode {
        TemporaryWhitelistMode::WhitelistThenBlacklist => {
            !any_whitelist || folder_match || ext_match
        }
        TemporaryWhitelistMode::WhitelistOnly => folder_match || ext_match,
    }
}

fn should_skip(
    path: &str,
    filters: &ResolvedFilterRules,
    ignore_git: bool,
    bypass_blacklist: bool,
) -> bool {
    if !should_keep_whitelist(
        path,
        &filters.folder_whitelist,
        &filters.ext_whitelist,
        filters.whitelist_mode,
    ) {
        return true;
    }

    if matches!(
        filters.whitelist_mode,
        TemporaryWhitelistMode::WhitelistOnly
    ) {
        return false;
    }

    if bypass_blacklist {
        return false;
    }

    should_skip_blacklist(
        path,
        &filters.folder_blacklist,
        &filters.ext_blacklist,
        ignore_git,
    )
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

#[derive(Default)]
struct CandidateCollection {
    candidates: Vec<CandidateFile>,
    skipped: usize,
    scanned: usize,
}

fn collect_path_candidates(
    path: &Path,
    relative: String,
    filters: &ResolvedFilterRules,
    ignore_git: bool,
    bypass_blacklist: bool,
) -> CandidateCollection {
    if !is_zip_path(path) {
        if should_skip(&relative, filters, ignore_git, bypass_blacklist) {
            return CandidateCollection {
                skipped: 1,
                scanned: 1,
                ..CandidateCollection::default()
            };
        }

        return CandidateCollection {
            candidates: vec![CandidateFile {
                absolute: path.to_path_buf(),
                relative,
                archive_entry: None,
                archive_path: None,
            }],
            scanned: 1,
            skipped: 0,
        };
    }

    if !bypass_blacklist
        && should_skip_blacklist(
            &relative,
            &filters.folder_blacklist,
            &filters.ext_blacklist,
            ignore_git,
        )
    {
        return CandidateCollection {
            skipped: 1,
            scanned: 1,
            ..CandidateCollection::default()
        };
    }

    let entries = match list_zip_file_entries(path) {
        Ok(entries) => entries,
        Err(_) => {
            return CandidateCollection {
                skipped: 1,
                scanned: 1,
                ..CandidateCollection::default()
            };
        }
    };
    if entries.is_empty() {
        return CandidateCollection {
            skipped: 1,
            scanned: 1,
            ..CandidateCollection::default()
        };
    }

    let mut result = CandidateCollection::default();
    for entry in entries {
        result.scanned += 1;
        let combined_relative = format!("{relative}/{}", entry.display_name);
        if should_skip(&combined_relative, filters, ignore_git, false) {
            result.skipped += 1;
            continue;
        }
        result.candidates.push(CandidateFile {
            absolute: path.to_path_buf(),
            relative: combined_relative,
            archive_entry: Some(entry.archive_name),
            archive_path: Some(relative.clone()),
        });
    }

    result
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

pub fn load_gitignore_rules_for_root(root: &Path) -> Vec<String> {
    std::fs::read_to_string(auto_gitignore_path(root))
        .ok()
        .map(|content| parse_gitignore_rules(&content))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{WalkerFilterRules, WalkerOptions, collect_candidates, normalize_ext};
    use crate::domain::TemporaryWhitelistMode;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    #[test]
    fn whitelist_then_blacklist_keeps_only_whitelisted_matches() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        fs::write(dir.path().join("src/lib.rs"), "lib").expect("write");
        fs::write(dir.path().join("notes.md"), "notes").expect("write");

        let out = collect_candidates(
            Some(&dir.path().to_path_buf()),
            &[],
            WalkerFilterRules {
                folder_blacklist: &[],
                ext_blacklist: &[],
                folder_whitelist: &["src".into()],
                ext_whitelist: &[],
                whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            },
            WalkerOptions::default(),
        );

        assert_eq!(
            out.candidates
                .iter()
                .map(|item| item.relative.as_str())
                .collect::<Vec<_>>(),
            vec!["src/lib.rs"]
        );
    }

    #[test]
    fn whitelist_then_blacklist_respects_blacklist_after_whitelist_match() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        fs::write(dir.path().join("src/lib.rs"), "lib").expect("write");

        let out = collect_candidates(
            Some(&dir.path().to_path_buf()),
            &[],
            WalkerFilterRules {
                folder_blacklist: &[],
                ext_blacklist: &[normalize_ext("rs")],
                folder_whitelist: &["src".into()],
                ext_whitelist: &[],
                whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            },
            WalkerOptions::default(),
        );

        assert!(out.candidates.is_empty());
        assert_eq!(out.skipped, 1);
    }

    #[test]
    fn whitelist_only_ignores_blacklist_after_whitelist_match() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        fs::write(dir.path().join("src/lib.rs"), "lib").expect("write");

        let out = collect_candidates(
            Some(&dir.path().to_path_buf()),
            &[],
            WalkerFilterRules {
                folder_blacklist: &[],
                ext_blacklist: &[normalize_ext("rs")],
                folder_whitelist: &["src".into()],
                ext_whitelist: &[],
                whitelist_mode: TemporaryWhitelistMode::WhitelistOnly,
            },
            WalkerOptions::default(),
        );

        assert_eq!(out.candidates.len(), 1);
        assert_eq!(out.candidates[0].relative, "src/lib.rs");
    }

    #[test]
    fn whitelist_only_without_rules_keeps_nothing() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("lib.rs"), "lib").expect("write");

        let out = collect_candidates(
            Some(&dir.path().to_path_buf()),
            &[],
            WalkerFilterRules {
                folder_blacklist: &[],
                ext_blacklist: &[],
                folder_whitelist: &[],
                ext_whitelist: &[],
                whitelist_mode: TemporaryWhitelistMode::WhitelistOnly,
            },
            WalkerOptions::default(),
        );

        assert!(out.candidates.is_empty());
        assert_eq!(out.skipped, 1);
    }

    #[test]
    fn whitelist_then_blacklist_allows_folder_or_extension_matches() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        fs::create_dir_all(dir.path().join("docs")).expect("mkdir");
        fs::write(dir.path().join("src/lib.rs"), "lib").expect("write");
        fs::write(dir.path().join("src/tmp.log"), "log").expect("write");
        fs::write(dir.path().join("docs/guide.md"), "guide").expect("write");

        let out = collect_candidates(
            Some(&dir.path().to_path_buf()),
            &[],
            WalkerFilterRules {
                folder_blacklist: &[],
                ext_blacklist: &[normalize_ext("log")],
                folder_whitelist: &["src".into()],
                ext_whitelist: &[normalize_ext("md")],
                whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            },
            WalkerOptions::default(),
        );

        assert_eq!(
            out.candidates
                .iter()
                .map(|item| item.relative.as_str())
                .collect::<Vec<_>>(),
            vec!["docs/guide.md", "src/lib.rs"]
        );
    }

    #[test]
    fn selected_file_still_respects_whitelist_when_blacklist_is_bypassed() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("notes.md");
        fs::write(&file_path, "notes").expect("write");

        let out = collect_candidates(
            None,
            std::slice::from_ref(&file_path),
            WalkerFilterRules {
                folder_blacklist: &["notes.md".into()],
                ext_blacklist: &[normalize_ext("md")],
                folder_whitelist: &["src".into()],
                ext_whitelist: &[],
                whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            },
            WalkerOptions::default(),
        );

        assert!(out.candidates.is_empty());
        assert_eq!(out.skipped, 1);
    }

    #[test]
    fn zip_entries_follow_whitelist_rules() {
        let dir = tempdir().expect("tempdir");
        let zip_path = dir.path().join("bundle.zip");
        let file = fs::File::create(&zip_path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("src/lib.rs", options).expect("start file");
        zip.write_all(b"lib").expect("write");
        zip.start_file("README.md", options).expect("start file");
        zip.write_all(b"readme").expect("write");
        zip.finish().expect("finish");

        let out = collect_candidates(
            None,
            std::slice::from_ref(&zip_path),
            WalkerFilterRules {
                folder_blacklist: &[],
                ext_blacklist: &[],
                folder_whitelist: &["src".into()],
                ext_whitelist: &[],
                whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            },
            WalkerOptions::default(),
        );

        assert_eq!(out.candidates.len(), 1);
        assert_eq!(out.candidates[0].relative, "bundle.zip/src/lib.rs");
    }
}
