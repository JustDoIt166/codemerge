use std::path::PathBuf;

use chrono::Local;

pub fn make_temp_result_path() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("codemerge");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create temp dir failed: {e}"))?;
    let ts = Local::now().format("%Y%m%d_%H%M%S");
    Ok(dir.join(format!("merged_{ts}.txt")))
}
