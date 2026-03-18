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

    pub fn effective_folder_blacklist(&self) -> Vec<String> {
        let mut rules = self.settings.folder_blacklist.clone();
        if self.settings.options.use_gitignore {
            for rule in &self.selection.gitignore_rules {
                if !rules.contains(rule) {
                    rules.push(rule.clone());
                }
            }
        }
        rules
    }

    pub fn has_content_result(&self) -> bool {
        self.result.result.as_ref().is_some_and(|result| {
            result.merged_content_path.is_some() || !result.preview_files.is_empty()
        })
    }

    pub fn is_tree_only_result(&self) -> bool {
        self.result.result.as_ref().is_some_and(|result| {
            result.merged_content_path.is_none() && result.preview_files.is_empty()
        })
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
        self.selection = SelectionState {
            dedupe_exact_path: self.selection.dedupe_exact_path,
            ..SelectionState::default()
        };
        self.result = ResultState::default();
        self.process = ProcessState {
            processing_current_file: status_ready,
            ..ProcessState::default()
        };
        self.workspace = WorkspaceState::default();
    }
}

#[derive(Default)]
pub struct SelectionState {
    pub dedupe_exact_path: bool,
    pub selected_folder: Option<PathBuf>,
    pub selected_files: Vec<FileEntry>,
    pub gitignore_file: Option<PathBuf>,
    pub gitignore_rules: Vec<String>,
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

#[derive(Default)]
pub struct TreePanelState {
    pub selected_node_id: Option<String>,
    pub expanded_ids: BTreeSet<String>,
}

pub struct PreviewPanelState {
    pub selected_preview_file_id: Option<u32>,
    pub preview_revision: u64,
    pub preview_rx: Option<std::sync::mpsc::Receiver<PreviewEvent>>,
    pub preview_requested_range: Option<Range<usize>>,
    pub preview_document: Option<PreviewDocument>,
    pub preview_error: Option<String>,
    pub preview_loaded_range: Range<usize>,
    pub preview_visible_range: Range<usize>,
    pub preview_loaded_lines: Vec<SharedString>,
    pub preview_sizes: Rc<Vec<gpui::Size<Pixels>>>,
}

impl Default for PreviewPanelState {
    fn default() -> Self {
        Self {
            selected_preview_file_id: None,
            preview_revision: 0,
            preview_rx: None,
            preview_requested_range: None,
            preview_document: None,
            preview_error: None,
            preview_loaded_range: 0..0,
            preview_visible_range: 0..0,
            preview_loaded_lines: Vec::new(),
            preview_sizes: Rc::new(vec![size(px(10.), preview_line_height())]),
        }
    }
}

#[derive(Default)]
pub struct WorkspaceState {
    pub tree_panel: TreePanelState,
    pub preview_panel: PreviewPanelState,
}

impl WorkspaceState {
    pub fn reset_tree(&mut self) {
        self.tree_panel = TreePanelState::default();
    }

    pub fn reset_preview(&mut self) {
        self.preview_panel = PreviewPanelState::default();
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::domain::{
        AppConfigV1, FileEntry, Language, OutputFormat, PreflightStats, PreviewFileEntry,
        ProcessResult, ProcessingMode, ProcessingOptions,
    };
    use crate::processor::stats::ProcessingStats;
    use std::path::PathBuf;

    #[test]
    fn clear_inputs_resets_runtime_state_and_keeps_dedupe_setting() {
        let config = AppConfigV1 {
            language: Language::En,
            options: ProcessingOptions {
                compress: true,
                use_gitignore: true,
                ignore_git: false,
                output_format: OutputFormat::Markdown,
                mode: ProcessingMode::TreeOnly,
            },
            folder_blacklist: vec!["target".into()],
            ext_blacklist: vec![".log".into()],
        };
        let mut state = AppState::from_config(config, "ready".to_string());
        state.selection.selected_folder = Some(PathBuf::from("root"));
        state.selection.selected_files.push(FileEntry {
            path: PathBuf::from("file.rs"),
            name: "file.rs".into(),
            size: 10,
        });
        state.selection.gitignore_file = Some(PathBuf::from("root/.gitignore"));
        state.selection.gitignore_rules = vec!["node_modules".into()];
        state.process.preflight = PreflightStats {
            total_files: 3,
            skipped_files: 1,
            to_process_files: 2,
            scanned_entries: 4,
            is_scanning: true,
        };
        state
            .process
            .processing_records
            .push(crate::domain::ProcessRecord {
                file_name: "file.rs".into(),
                status: crate::domain::ProcessStatus::Success,
                chars: Some(12),
                tokens: Some(4),
                error: None,
            });
        state.process.last_error = Some("boom".into());
        state.result.result = Some(ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: Vec::new(),
            merged_content_path: Some(PathBuf::from("merged.txt")),
            file_details: Vec::new(),
            preview_files: vec![PreviewFileEntry {
                id: 1,
                display_path: "file.rs".into(),
                chars: 12,
                tokens: 4,
                preview_blob_path: PathBuf::from("preview.txt"),
                byte_len: 12,
            }],
            preview_blob_dir: Some(PathBuf::from("preview-dir")),
        });
        state
            .result
            .preview_rows
            .push(crate::domain::PreviewRowViewModel {
                id: 1,
                display_path: "file.rs".into(),
                chars: 12,
                tokens: 4,
            });
        state.workspace.tree_panel.selected_node_id = Some("file.rs".into());
        state.workspace.preview_panel.preview_error = Some("broken".into());
        state.workspace.preview_panel.selected_preview_file_id = Some(1);

        state.clear_inputs("Status: Ready".into());

        assert!(state.selection.selected_folder.is_none());
        assert!(state.selection.selected_files.is_empty());
        assert!(state.selection.gitignore_file.is_none());
        assert!(state.selection.gitignore_rules.is_empty());
        assert!(state.selection.dedupe_exact_path);
        assert!(state.result.result.is_none());
        assert!(state.result.preview_rows.is_empty());
        assert_eq!(state.process.processing_current_file, "Status: Ready");
        assert_eq!(
            state.process.ui_status,
            crate::ui::state::ProcessUiStatus::Idle
        );
        assert_eq!(state.process.preflight.total_files, 0);
        assert_eq!(state.process.preflight.skipped_files, 0);
        assert_eq!(state.process.preflight.to_process_files, 0);
        assert_eq!(state.process.preflight.scanned_entries, 0);
        assert!(!state.process.preflight.is_scanning);
        assert!(state.process.processing_records.is_empty());
        assert!(state.process.last_error.is_none());
        assert!(state.workspace.tree_panel.selected_node_id.is_none());
        assert!(state.workspace.preview_panel.preview_error.is_none());
        assert!(
            state
                .workspace
                .preview_panel
                .selected_preview_file_id
                .is_none()
        );
    }

    #[test]
    fn effective_folder_blacklist_respects_use_gitignore() {
        let config = AppConfigV1::default();
        let mut state = AppState::from_config(config, "ready".to_string());
        state.settings.folder_blacklist = vec!["target".into()];
        state.selection.gitignore_rules = vec!["node_modules".into()];

        assert_eq!(
            state.effective_folder_blacklist(),
            vec!["target".to_string(), "node_modules".to_string()]
        );

        state.settings.options.use_gitignore = false;
        assert_eq!(
            state.effective_folder_blacklist(),
            vec!["target".to_string()]
        );
    }

    #[test]
    fn tree_only_and_content_flags_track_result_shape() {
        let config = AppConfigV1::default();
        let mut state = AppState::from_config(config, "ready".to_string());

        assert!(!state.has_content_result());
        assert!(!state.is_tree_only_result());

        state.result.result = Some(ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: Vec::new(),
            merged_content_path: None,
            file_details: Vec::new(),
            preview_files: Vec::new(),
            preview_blob_dir: None,
        });

        assert!(!state.has_content_result());
        assert!(state.is_tree_only_result());
    }
}
