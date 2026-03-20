use std::collections::BTreeSet;
use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

use gpui::SharedString;

use crate::domain::{AppConfigV1, FileEntry, PreflightStats, ProcessRecord};
use crate::services::preflight::PreflightEvent;
use crate::services::preview::{PreviewDocument, PreviewEvent};
use crate::services::process::ProcessHandle;

#[derive(Default)]
pub struct AppState {
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

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SidePanelTab {
    #[default]
    Results,
    Rules,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum NarrowContentTab {
    #[default]
    Status,
    Results,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PendingConfirmation {
    ClearInputs,
    ResetBlacklist,
    ClearBlacklist,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceUiState {
    pub side_panel_tab: SidePanelTab,
    pub narrow_content_tab: NarrowContentTab,
    pub pending_confirmation: Option<PendingConfirmation>,
}

impl AppState {
    pub fn from_config(_config: AppConfigV1, _status_ready: String) -> Self {
        Self {
            workspace: WorkspaceState::default(),
        }
    }

    pub fn clear_inputs(&mut self) {
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

#[derive(Clone)]
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
pub struct TreePanelState {
    pub selected_node_id: Option<String>,
    pub expanded_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewChunk {
    pub range: Range<usize>,
    pub lines: Vec<SharedString>,
}

pub struct PreviewPanelState {
    pub selected_preview_file_id: Option<u32>,
    pub preview_revision: u64,
    pub preview_rx: Option<std::sync::mpsc::Receiver<PreviewEvent>>,
    pub preview_requested_range: Option<Range<usize>>,
    pub preview_document: Option<PreviewDocument>,
    pub preview_error: Option<String>,
    pub preview_last_visible_range: Range<usize>,
    pub preview_chunks: Vec<PreviewChunk>,
}

impl PreviewPanelState {
    const MAX_CHUNKS: usize = 3;
    pub const VISIBLE_BUCKET_LINES: usize = 192;

    pub fn clear_loaded_chunks(&mut self) {
        self.preview_chunks.clear();
    }

    pub fn update_visible_range(&mut self, range: Range<usize>, line_count: usize) -> bool {
        let bucketed = bucketize_range(range, Self::VISIBLE_BUCKET_LINES, line_count);
        if self.preview_last_visible_range == bucketed {
            return false;
        }

        self.preview_last_visible_range = bucketed;
        true
    }

    pub fn store_chunk(&mut self, range: Range<usize>, lines: Vec<SharedString>) {
        if range.start >= range.end || lines.is_empty() {
            return;
        }

        self.preview_chunks
            .retain(|chunk| chunk.range.end <= range.start || chunk.range.start >= range.end);
        self.preview_chunks.push(PreviewChunk {
            range: range.clone(),
            lines,
        });
        self.preview_chunks.sort_by_key(|chunk| chunk.range.start);

        let focus = if self.preview_last_visible_range.start < self.preview_last_visible_range.end {
            self.preview_last_visible_range.clone()
        } else {
            range
        };
        while self.preview_chunks.len() > Self::MAX_CHUNKS {
            let focus_center = range_center(&focus);
            let prune_ix = self
                .preview_chunks
                .iter()
                .enumerate()
                .max_by_key(|(_, chunk)| range_center(&chunk.range).abs_diff(focus_center))
                .map(|(ix, _)| ix)
                .unwrap_or(0);
            self.preview_chunks.remove(prune_ix);
        }
    }

    pub fn has_loaded_range(&self, range: &Range<usize>) -> bool {
        if range.start >= range.end {
            return true;
        }

        let mut covered_until = range.start;
        for chunk in &self.preview_chunks {
            if chunk.range.end <= covered_until {
                continue;
            }
            if chunk.range.start > covered_until {
                return false;
            }
            covered_until = covered_until.max(chunk.range.end);
            if covered_until >= range.end {
                return true;
            }
        }

        false
    }

    pub fn line_at(&self, ix: usize) -> Option<SharedString> {
        self.preview_chunks.iter().find_map(|chunk| {
            if ix < chunk.range.start || ix >= chunk.range.end {
                return None;
            }

            chunk.lines.get(ix - chunk.range.start).cloned()
        })
    }
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
            preview_last_visible_range: 0..0,
            preview_chunks: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct WorkspaceState {
    pub tree_panel: TreePanelState,
}

impl WorkspaceState {
    pub fn reset_tree(&mut self) {
        self.tree_panel = TreePanelState::default();
    }
}

fn range_center(range: &Range<usize>) -> usize {
    range.start + (range.end.saturating_sub(range.start) / 2)
}

fn bucketize_range(range: Range<usize>, bucket_lines: usize, line_count: usize) -> Range<usize> {
    if line_count == 0 || bucket_lines == 0 {
        return 0..0;
    }

    let start = range.start.min(line_count.saturating_sub(1));
    let end = range.end.max(start + 1).min(line_count);
    let bucket_start = (start / bucket_lines) * bucket_lines;
    let bucket_end = end
        .saturating_sub(1)
        .checked_div(bucket_lines)
        .map(|bucket| (bucket + 1) * bucket_lines)
        .unwrap_or(bucket_lines)
        .min(line_count);
    bucket_start..bucket_end.max(bucket_start + 1).min(line_count)
}

#[cfg(test)]
mod tests {
    use super::{AppState, PreviewPanelState};
    use crate::domain::{AppConfigV1, Language, OutputFormat, ProcessingMode, ProcessingOptions};

    #[test]
    fn clear_inputs_resets_workspace_runtime_state() {
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
        state.workspace.tree_panel.selected_node_id = Some("file.rs".into());

        state.clear_inputs();

        assert!(state.workspace.tree_panel.selected_node_id.is_none());
    }

    #[test]
    fn preview_panel_tracks_covered_ranges_across_adjacent_chunks() {
        let mut state = PreviewPanelState::default();
        state.store_chunk(
            0..50,
            (0..50).map(|ix| format!("line-{ix}").into()).collect(),
        );
        state.store_chunk(
            50..120,
            (50..120).map(|ix| format!("line-{ix}").into()).collect(),
        );

        assert!(state.has_loaded_range(&(10..90)));
        assert!(state.has_loaded_range(&(0..120)));
        assert!(!state.has_loaded_range(&(0..121)));
        assert_eq!(
            state.line_at(65).map(|line| line.to_string()),
            Some("line-65".into())
        );
    }

    #[test]
    fn preview_panel_prunes_far_chunks_and_keeps_nearby_data() {
        let mut state = PreviewPanelState::default();
        assert!(state.update_visible_range(220..260, 400));
        state.store_chunk(0..50, (0..50).map(|ix| format!("a-{ix}").into()).collect());
        state.store_chunk(
            100..150,
            (100..150).map(|ix| format!("b-{ix}").into()).collect(),
        );
        state.store_chunk(
            200..250,
            (200..250).map(|ix| format!("c-{ix}").into()).collect(),
        );
        state.store_chunk(
            250..300,
            (250..300).map(|ix| format!("d-{ix}").into()).collect(),
        );

        assert_eq!(state.preview_chunks.len(), 3);
        assert!(
            !state
                .preview_chunks
                .iter()
                .any(|chunk| chunk.range == (0..50))
        );
        assert!(
            state
                .preview_chunks
                .iter()
                .any(|chunk| chunk.range == (200..250))
        );
        assert!(
            state
                .preview_chunks
                .iter()
                .any(|chunk| chunk.range == (250..300))
        );
    }

    #[test]
    fn preview_panel_ignores_duplicate_visible_ranges() {
        let mut state = PreviewPanelState::default();

        assert!(state.update_visible_range(10..20, 500));
        assert!(!state.update_visible_range(10..20, 500));
        assert!(!state.update_visible_range(11..21, 500));
        assert!(state.update_visible_range(220..260, 500));
    }
}
