use std::path::{Path, PathBuf};

pub fn filename(path: &Path) -> String {
    path.file_name()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn ext(path: &PathBuf) -> String {
    path.extension()
        .map(|v| format!(".{}", v.to_string_lossy().to_lowercase()))
        .unwrap_or_default()
}
