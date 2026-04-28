use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::processor::stats::ProcessingStats;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Language {
    Zh,
    En,
}

impl Language {
    pub fn toggle(self) -> Self {
        match self {
            Self::Zh => Self::En,
            Self::En => Self::Zh,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OutputFormat {
    Default,
    Xml,
    PlainText,
    Markdown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProcessingMode {
    Full,
    TreeOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ProcessingOptions {
    pub compress: bool,
    pub use_gitignore: bool,
    pub ignore_git: bool,
    pub output_format: OutputFormat,
    pub mode: ProcessingMode,
}

impl Default for ProcessingOptions {
    fn default() -> Self {
        Self {
            compress: false,
            use_gitignore: true,
            ignore_git: true,
            output_format: OutputFormat::Default,
            mode: ProcessingMode::Full,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigV1 {
    pub language: Language,
    pub options: ProcessingOptions,
    pub folder_blacklist: Vec<String>,
    pub ext_blacklist: Vec<String>,
}

impl Default for AppConfigV1 {
    fn default() -> Self {
        Self {
            language: Language::Zh,
            options: ProcessingOptions::default(),
            folder_blacklist: default_folder_blacklist(),
            ext_blacklist: default_ext_blacklist(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub enum ProcessStatus {
    Success,
    Skipped,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ProcessRecord {
    pub file_name: String,
    pub status: ProcessStatus,
    pub chars: Option<usize>,
    pub tokens: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileDetail {
    pub path: String,
    pub chars: usize,
    pub tokens: usize,
}

#[derive(Debug, Clone)]
pub struct PreviewFileEntry {
    pub id: u32,
    pub display_path: String,
    pub chars: usize,
    pub tokens: usize,
    pub preview_blob_path: PathBuf,
    pub byte_len: u64,
    pub archive: Option<ArchiveEntrySource>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ArchiveEntrySource {
    pub archive_path: String,
    pub entry_path: String,
}

#[derive(Debug, Clone)]
pub struct TreeNodeViewModel {
    pub id: String,
    pub label: String,
    pub relative_path: String,
    pub is_folder: bool,
    pub depth: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct PreviewRowViewModel {
    pub id: u32,
    pub display_path: String,
    pub chars: usize,
    pub tokens: usize,
    pub archive: Option<ArchiveEntrySource>,
}

#[derive(Debug, Clone, Default)]
pub struct PreviewViewport {
    pub visible_range: std::ops::Range<usize>,
    pub loaded_range: std::ops::Range<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct ProgressRowViewModel {
    pub file_name: String,
    pub status_label: String,
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub relative_path: String,
    pub is_folder: bool,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub stats: ProcessingStats,
    pub tree_string: String,
    pub tree_nodes: Vec<TreeNode>,
    pub process_dir: Option<PathBuf>,
    pub merged_content_path: Option<PathBuf>,
    pub suggested_result_name: String,
    pub file_details: Vec<FileDetail>,
    pub preview_files: Vec<PreviewFileEntry>,
    pub preview_blob_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct PreflightStats {
    pub total_files: usize,
    pub skipped_files: usize,
    pub to_process_files: usize,
    pub scanned_entries: usize,
    pub is_scanning: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ResultTab {
    #[default]
    Tree,
    Content,
}

#[derive(Debug, Clone)]
pub enum SettingsCommand {
    Save(AppConfigV1),
    ResetToDefault,
}

pub fn default_folder_blacklist() -> Vec<String> {
    [
        "node_modules",
        "dist",
        "build",
        "target",
        "bin",
        "obj",
        "vendor",
        ".git",
        ".idea",
        ".vscode",
        "__pycache__",
        "venv",
        "env",
        ".env",
        "coverage",
        "tmp",
        "temp",
    ]
    .iter()
    .map(|v| (*v).to_string())
    .collect()
}

pub fn default_ext_blacklist() -> Vec<String> {
    [
        ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".svg", ".ico", ".mp3", ".wav", ".ogg",
        ".flac", ".m4a", ".mp4", ".mov", ".avi", ".mkv", ".webm", ".pdf", ".rar", ".7z", ".tar",
        ".gz", ".xz", ".exe", ".dll", ".so", ".dylib", ".class", ".jar", ".ttf", ".woff", ".woff2",
        ".eot", ".otf", ".db", ".sqlite",
    ]
    .iter()
    .map(|v| (*v).to_string())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{AppConfigV1, default_ext_blacklist};

    #[test]
    fn default_ext_blacklist_keeps_zip_available() {
        let blacklist = default_ext_blacklist();
        assert!(!blacklist.iter().any(|ext| ext == ".zip"));
        assert!(
            !AppConfigV1::default()
                .ext_blacklist
                .iter()
                .any(|ext| ext == ".zip")
        );
    }
}
