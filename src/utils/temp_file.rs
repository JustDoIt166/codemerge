use std::path::PathBuf;

use chrono::Local;

fn codemerge_temp_root() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("codemerge");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create temp dir failed: {e}"))?;
    Ok(dir)
}

pub fn make_temp_result_path() -> Result<PathBuf, String> {
    let dir = codemerge_temp_root()?;
    let ts = Local::now().format("%Y%m%d_%H%M%S");
    Ok(dir.join(format!("merged_{ts}.txt")))
}

pub fn make_temp_preview_dir() -> Result<PathBuf, String> {
    let root = codemerge_temp_root()?;
    let ts = Local::now().format("%Y%m%d_%H%M%S_%3f");
    let dir = root.join(format!("preview_{ts}"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create preview dir failed: {e}"))?;
    Ok(dir)
}

pub fn cleanup_preview_dir(path: &std::path::Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|e| format!("remove preview dir failed: {e}"))?;
    }
    Ok(())
}
