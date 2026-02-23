use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::processor::stats::ProcessingStats;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputFormat {
    Default,
    Xml,
    PlainText,
    Markdown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProcessingMode {
    Full,
    TreeOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone)]
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
pub struct ProcessResult {
    pub stats: ProcessingStats,
    pub tree_string: Option<String>,
    pub merged_content_path: Option<PathBuf>,
    pub file_details: Vec<FileDetail>,
}

#[derive(Debug, Clone)]
pub enum ProcessingState {
    Idle,
    InProgress {
        total: usize,
        processed: usize,
        skipped: usize,
        current_file: String,
        records: Vec<ProcessRecord>,
    },
    Completed {
        processed: usize,
        skipped: usize,
    },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastStyle {
    Success,
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub style: ToastStyle,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsDetailType {
    Files,
    Chars,
    Tokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTab {
    Tree,
    MergedContent,
}

#[derive(Debug, Clone)]
pub struct UIState {
    pub folder_blacklist_input: String,
    pub ext_blacklist_input: String,
    pub blacklist_filter_input: String,
    pub show_guide: bool,
    pub show_reset_confirmation: bool,
    pub show_cancel_confirmation: bool,
    pub expanded_stats: Option<StatsDetailType>,
    pub toast: Option<Toast>,
    pub toast_elapsed_ms: u64,
    pub processing_elapsed_ms: u64,
    pub toast_last_key: String,
    pub preview_content: String,
    pub preview_loaded_all: bool,
    pub show_load_all_confirm: bool,
    pub pulse_phase: f32,
    pub active_output_tab: OutputTab,
    pub config_expanded: bool,
    pub blacklist_expanded: bool,
    pub blacklist_selected_all: bool,
    pub blacklist_selected: HashSet<String>,
}

impl Default for UIState {
    fn default() -> Self {
        Self {
            folder_blacklist_input: String::new(),
            ext_blacklist_input: String::new(),
            blacklist_filter_input: String::new(),
            show_guide: true,
            show_reset_confirmation: false,
            show_cancel_confirmation: false,
            expanded_stats: None,
            toast: None,
            toast_elapsed_ms: 0,
            processing_elapsed_ms: 0,
            toast_last_key: String::new(),
            preview_content: String::new(),
            preview_loaded_all: false,
            show_load_all_confirm: false,
            pulse_phase: 0.0,
            active_output_tab: OutputTab::Tree,
            config_expanded: true,
            blacklist_expanded: true,
            blacklist_selected_all: false,
            blacklist_selected: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PreflightStats {
    pub total_files: usize,
    pub skipped_files: usize,
    pub to_process_files: usize,
    pub scanned_entries: usize,
    pub is_scanning: bool,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub selected_folder: Option<PathBuf>,
    pub selected_files: Vec<FileEntry>,
    pub gitignore_file: Option<PathBuf>,
    pub options: ProcessingOptions,
    pub folder_blacklist: Vec<String>,
    pub ext_blacklist: Vec<String>,
    pub processing_state: ProcessingState,
    pub result: Option<ProcessResult>,
    pub ui: UIState,
    pub language: Language,
    pub window_size: (f32, f32),
    pub dedupe_exact_path: bool,
    pub cancel_token: Option<CancellationToken>,
    pub preflight: PreflightStats,
    pub preflight_revision: u64,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            selected_folder: None,
            selected_files: Vec::new(),
            gitignore_file: None,
            options: ProcessingOptions::default(),
            folder_blacklist: default_folder_blacklist(),
            ext_blacklist: default_ext_blacklist(),
            processing_state: ProcessingState::Idle,
            result: None,
            ui: UIState::default(),
            language: Language::Zh,
            window_size: (1200.0, 820.0),
            dedupe_exact_path: true,
            cancel_token: None,
            preflight: PreflightStats::default(),
            preflight_revision: 0,
        }
    }
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
        ".flac", ".m4a", ".mp4", ".mov", ".avi", ".mkv", ".webm", ".pdf", ".zip", ".rar", ".7z",
        ".tar", ".gz", ".xz", ".exe", ".dll", ".so", ".dylib", ".class", ".jar", ".ttf", ".woff",
        ".woff2", ".eot", ".otf", ".db", ".sqlite",
    ]
    .iter()
    .map(|v| (*v).to_string())
    .collect()
}
