use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use chrono::Local;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn codemerge_temp_root() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("codemerge");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create temp dir failed: {e}"))?;
    Ok(dir)
}

fn unique_suffix() -> String {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}_{}_{}",
        Local::now().format("%Y%m%d_%H%M%S_%3f"),
        std::process::id(),
        sequence
    )
}

fn create_temp_child_dir(prefix: &str) -> Result<PathBuf, String> {
    let dir = codemerge_temp_root()?.join(format!("{prefix}_{}", unique_suffix()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create temp dir failed: {e}"))?;
    Ok(dir)
}

pub fn make_temp_result_path() -> Result<PathBuf, String> {
    let dir = codemerge_temp_root()?;
    Ok(dir.join(format!("merged_{}.txt", unique_suffix())))
}

pub fn make_temp_preview_dir() -> Result<PathBuf, String> {
    let root = codemerge_temp_root()?;
    let dir = root.join(format!("preview_{}", unique_suffix()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create preview dir failed: {e}"))?;
    Ok(dir)
}

pub fn make_temp_process_dir() -> Result<PathBuf, String> {
    create_temp_child_dir("process")
}

pub fn make_temp_result_path_in(process_dir: &std::path::Path) -> PathBuf {
    process_dir.join("merged.txt")
}

pub fn make_temp_preview_dir_in(process_dir: &std::path::Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(process_dir).map_err(|e| format!("create preview dir failed: {e}"))?;
    Ok(process_dir.to_path_buf())
}

pub fn cleanup_temp_dir(path: &std::path::Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|e| format!("remove temp dir failed: {e}"))?;
    }
    Ok(())
}

pub fn cleanup_preview_dir(path: &std::path::Path) -> Result<(), String> {
    cleanup_temp_dir(path)
}

pub fn cleanup_stale_temp_entries(max_age: Duration) -> Result<usize, String> {
    let root = codemerge_temp_root()?;
    cleanup_stale_temp_entries_in(&root, max_age)
}

fn cleanup_stale_temp_entries_in(
    root: &std::path::Path,
    max_age: Duration,
) -> Result<usize, String> {
    let now = SystemTime::now();
    let mut removed = 0;

    for entry in std::fs::read_dir(root).map_err(|e| format!("read temp dir failed: {e}"))? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_owned_temp_entry(name) {
            continue;
        }

        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age < max_age {
            continue;
        }

        let cleanup_result = if metadata.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if cleanup_result.is_ok() {
            removed += 1;
        }
    }

    Ok(removed)
}

fn is_owned_temp_entry(name: &str) -> bool {
    name.starts_with("process_") || name.starts_with("preview_") || name.starts_with("merged_")
}

#[cfg(test)]
mod tests {
    use super::{
        cleanup_preview_dir, cleanup_stale_temp_entries_in, cleanup_temp_dir,
        make_temp_preview_dir, make_temp_process_dir, make_temp_result_path,
        make_temp_result_path_in,
    };

    #[test]
    fn temp_paths_are_unique() {
        let first = make_temp_result_path().expect("first result path");
        let second = make_temp_result_path().expect("second result path");
        assert_ne!(first, second);
    }

    #[test]
    fn preview_dir_can_be_created_and_cleaned() {
        let dir = make_temp_preview_dir().expect("preview dir");
        assert!(dir.exists());

        cleanup_preview_dir(&dir).expect("cleanup preview dir");
        assert!(!dir.exists());
    }

    #[test]
    fn process_dir_cleanup_removes_nested_result_file() {
        let dir = make_temp_process_dir().expect("process dir");
        let result_path = make_temp_result_path_in(&dir);
        std::fs::write(&result_path, "content").expect("write result file");
        assert!(result_path.exists());

        cleanup_temp_dir(&dir).expect("cleanup temp dir");
        assert!(!dir.exists());
        assert!(!result_path.exists());
    }

    #[test]
    fn stale_cleanup_removes_only_owned_old_entries() {
        let root = tempfile::tempdir().expect("tempdir");
        let old_dir = root.path().join("process_old");
        let old_file = root.path().join("merged_old.txt");
        std::fs::create_dir_all(&old_dir).expect("create process dir");
        std::fs::write(&old_file, "content").expect("write result");
        let unrelated = root.path().join("user_file.txt");
        std::fs::write(&unrelated, "keep").expect("write unrelated");

        let removed =
            cleanup_stale_temp_entries_in(root.path(), std::time::Duration::ZERO).expect("cleanup");

        assert_eq!(removed, 2);
        assert!(!old_dir.exists());
        assert!(!old_file.exists());
        assert!(unrelated.exists());
    }
}
