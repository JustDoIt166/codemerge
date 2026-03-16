use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

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

pub fn cleanup_preview_dir(path: &std::path::Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|e| format!("remove preview dir failed: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{cleanup_preview_dir, make_temp_preview_dir, make_temp_result_path};

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
}
