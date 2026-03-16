use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use gpui::{Pixels, SharedString, px, size};

use crate::domain::{
    AppConfigV1, FileEntry, PreflightStats, PreviewRowViewModel, ProcessRecord, ProcessResult,
    ResultTab,
};
use crate::services::preflight::PreflightEvent;
use crate::services::preview::{PreviewDocument, PreviewEvent};
use crate::services::process::ProcessHandle;

fn preview_line_height() -> Pixels {
    px(22.)
}

#[derive(Default)]
pub struct AppState {
    pub selection: SelectionState,
    pub settings: SettingsState,
    pub process: ProcessState,
    pub result: ResultState,
    pub workspace: WorkspaceState,
}

impl AppState {
    pub fn from_config(config: AppConfigV1, status_ready: String) -> Self {
        Self {
            selection: SelectionState {
                dedupe_exact_path: true,
                ..SelectionState::default()
            },
            settings: SettingsState {
                language: config.language,
                options: config.options,
                folder_blacklist: config.folder_blacklist,
                ext_blacklist: config.ext_blacklist,
            },
            process: ProcessState {
                processing_current_file: status_ready,
                ..ProcessState::default()
            },
            result: ResultState::default(),
            workspace: WorkspaceState::default(),
        }
    }
}

#[derive(Default)]
pub struct SelectionState {
    pub dedupe_exact_path: bool,
    pub selected_folder: Option<PathBuf>,
    pub selected_files: Vec<FileEntry>,
    pub gitignore_file: Option<PathBuf>,
}

pub struct SettingsState {
    pub language: crate::domain::Language,
    pub options: crate::domain::ProcessingOptions,
    pub folder_blacklist: Vec<String>,
    pub ext_blacklist: Vec<String>,
}

impl Default for SettingsState {
    fn default() -> Self {
        let config = AppConfigV1::default();
        Self {
            language: config.language,
            options: config.options,
            folder_blacklist: config.folder_blacklist,
            ext_blacklist: config.ext_blacklist,
        }
    }
}

#[derive(Default)]
pub struct ProcessState {
    pub preflight: PreflightStats,
    pub preflight_revision: u64,
    pub preflight_rx: Option<std::sync::mpsc::Receiver<PreflightEvent>>,
    pub process_handle: Option<ProcessHandle>,
    pub processing_records: Vec<ProcessRecord>,
    pub processing_scanned: usize,
    pub processing_candidates: usize,
    pub processing_skipped: usize,
    pub processing_current_file: String,
    pub processing_started_at: Option<Instant>,
}

#[derive(Default)]
pub struct ResultState {
    pub result: Option<ProcessResult>,
    pub active_tab: ResultTab,
    pub preview_rows: Vec<PreviewRowViewModel>,
}

pub struct WorkspaceState {
    pub selected_preview_file_id: Option<u32>,
    pub preview_revision: u64,
    pub preview_rx: Option<std::sync::mpsc::Receiver<PreviewEvent>>,
    pub preview_requested_range: Option<Range<usize>>,
    pub preview_document: Option<PreviewDocument>,
    pub preview_loaded_range: Range<usize>,
    pub preview_visible_range: Range<usize>,
    pub preview_loaded_lines: Vec<SharedString>,
    pub preview_sizes: Rc<Vec<gpui::Size<Pixels>>>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            selected_preview_file_id: None,
            preview_revision: 0,
            preview_rx: None,
            preview_requested_range: None,
            preview_document: None,
            preview_loaded_range: 0..0,
            preview_visible_range: 0..0,
            preview_loaded_lines: Vec::new(),
            preview_sizes: Rc::new(vec![size(px(10.), preview_line_height())]),
        }
    }
}
