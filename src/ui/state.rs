use std::collections::BTreeSet;
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProcessUiStatus {
    #[default]
    Idle,
    Preflight,
    Running,
    Completed,
    Cancelled,
    Error,
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

    pub fn to_config(&self) -> AppConfigV1 {
        AppConfigV1 {
            language: self.settings.language,
            options: self.settings.options.clone(),
            folder_blacklist: self.settings.folder_blacklist.clone(),
            ext_blacklist: self.settings.ext_blacklist.clone(),
        }
    }

    pub fn clear_inputs(&mut self, status_ready: String) {
        self.selection.selected_folder = None;
        self.selection.selected_files.clear();
        self.selection.gitignore_file = None;
        self.result.result = None;
        self.result.preview_rows.clear();
        self.process.ui_status = ProcessUiStatus::Idle;
        self.process.last_error = None;
        self.process.processing_current_file = status_ready;
        self.workspace.reset_tree();
        self.workspace.reset_preview();
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
    pub ui_status: ProcessUiStatus,
    pub processing_records: Vec<ProcessRecord>,
    pub processing_scanned: usize,
    pub processing_candidates: usize,
    pub processing_skipped: usize,
    pub processing_current_file: String,
    pub processing_started_at: Option<Instant>,
    pub last_error: Option<String>,
}

impl ProcessState {
    pub fn reset_for_run(&mut self, scanning_label: String) {
        self.ui_status = ProcessUiStatus::Running;
        self.last_error = None;
        self.processing_records.clear();
        self.processing_scanned = 0;
        self.processing_candidates = 0;
        self.processing_skipped = 0;
        self.processing_current_file = scanning_label;
        self.processing_started_at = Some(Instant::now());
    }

    pub fn finish_run(&mut self) {
        self.process_handle = None;
        self.processing_started_at = None;
    }
}

#[derive(Default)]
pub struct ResultState {
    pub result: Option<ProcessResult>,
    pub active_tab: ResultTab,
    pub preview_rows: Vec<PreviewRowViewModel>,
}

pub struct WorkspaceState {
    pub selected_tree_node_id: Option<String>,
    pub tree_expanded_ids: BTreeSet<String>,
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
            selected_tree_node_id: None,
            tree_expanded_ids: BTreeSet::new(),
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

impl WorkspaceState {
    pub fn reset_tree(&mut self) {
        self.selected_tree_node_id = None;
        self.tree_expanded_ids.clear();
    }

    pub fn reset_preview(&mut self) {
        self.selected_preview_file_id = None;
        self.preview_rx = None;
        self.preview_requested_range = None;
        self.preview_document = None;
        self.preview_loaded_range = 0..0;
        self.preview_visible_range = 0..0;
        self.preview_loaded_lines.clear();
        self.preview_sizes = Rc::new(vec![size(px(10.), preview_line_height())]);
    }
}
