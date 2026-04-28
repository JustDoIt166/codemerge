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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ProcessUiStatus {
    #[default]
    Idle,
    Preflight,
    Running,
    Completed,
    Cancelled,
    Error,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SidePanelTab {
    #[default]
    Results,
    Rules,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum NarrowContentTab {
    #[default]
    Status,
    Results,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum PendingConfirmation {
    ClearInputs,
    ResetBlacklist,
    ClearBlacklist,
}

pub const DEFAULT_SELECTED_FILES_PANEL_HEIGHT: u16 = 180;
pub const MIN_SELECTED_FILES_PANEL_HEIGHT: u16 = 120;
pub const MAX_SELECTED_FILES_PANEL_HEIGHT: u16 = 560;

pub fn clamp_selected_files_panel_height(height: u16) -> u16 {
    height.clamp(
        MIN_SELECTED_FILES_PANEL_HEIGHT,
        MAX_SELECTED_FILES_PANEL_HEIGHT,
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceUiState {
    pub side_panel_tab: SidePanelTab,
    pub narrow_content_tab: NarrowContentTab,
    pub content_file_list_collapsed: bool,
    pub selected_files_panel_height: u16,
    pub pending_confirmation: Option<PendingConfirmation>,
}

impl Default for WorkspaceUiState {
    fn default() -> Self {
        Self {
            side_panel_tab: SidePanelTab::default(),
            narrow_content_tab: NarrowContentTab::default(),
            content_file_list_collapsed: false,
            selected_files_panel_height: DEFAULT_SELECTED_FILES_PANEL_HEIGHT,
            pending_confirmation: None,
        }
    }
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
    pub temp_folder_blacklist: Vec<String>,
    pub temp_ext_blacklist: Vec<String>,
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
    pub preflight_preserves_status: bool,
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
    pub fn discard_preflight_for_run(&mut self) {
        self.preflight_rx = None;
        self.preflight_revision = self.preflight_revision.wrapping_add(1);
        self.preflight_preserves_status = false;
        self.preflight = PreflightStats::default();
    }

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeferredPreviewState {
    pub source_path: PathBuf,
    pub source_byte_len: u64,
    pub excerpt_byte_len: u64,
    pub excerpt_path: Option<PathBuf>,
}

impl DeferredPreviewState {
    pub fn is_excerpt_loaded(&self) -> bool {
        self.excerpt_path.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewLoadRequestKind {
    File,
    DeferredExcerpt,
    DeferredFull,
}

#[derive(Default)]
pub struct PreviewPanelState {
    pub selected_preview_file_id: Option<u32>,
    pub preview_revision: u64,
    pub preview_rx: Option<std::sync::mpsc::Receiver<PreviewEvent>>,
    pub preview_requested_range: Option<Range<usize>>,
    pub queued_preview_range: Option<Range<usize>>,
    pub preview_document: Option<PreviewDocument>,
    pub preview_error: Option<String>,
    pub deferred_preview: Option<DeferredPreviewState>,
    pub pending_request_type: Option<PreviewLoadRequestKind>,
    pub preview_chunks: Vec<PreviewChunk>,
    pub render_revision: u64,
}

impl PreviewPanelState {
    const MAX_CHUNKS: usize = 6;
    pub const VISIBLE_BUCKET_LINES: usize = 192;
    pub const RENDER_WINDOW_LINES: usize = 64;

    pub fn clear_loaded_chunks(&mut self) {
        self.preview_chunks.clear();
        self.bump_render_revision();
    }

    pub fn store_chunk(&mut self, range: Range<usize>, lines: Vec<SharedString>) {
        self.store_chunk_with_focus(range.clone(), lines, &range);
    }

    pub fn store_chunk_with_focus(
        &mut self,
        range: Range<usize>,
        lines: Vec<SharedString>,
        focus_range: &Range<usize>,
    ) {
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
        while self.preview_chunks.len() > Self::MAX_CHUNKS {
            let focus_center = range_center(focus_range);
            let prune_ix = self
                .preview_chunks
                .iter()
                .enumerate()
                .max_by_key(|(_, chunk)| range_center(&chunk.range).abs_diff(focus_center))
                .map(|(ix, _)| ix)
                .unwrap_or(0);
            self.preview_chunks.remove(prune_ix);
        }
        self.bump_render_revision();
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
        // Chunks are sorted by range.start — use binary search.
        let pos = self
            .preview_chunks
            .binary_search_by(|chunk| {
                if ix < chunk.range.start {
                    std::cmp::Ordering::Greater
                } else if ix >= chunk.range.end {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()?;
        let chunk = &self.preview_chunks[pos];
        chunk.lines.get(ix - chunk.range.start).cloned()
    }

    pub fn queue_preview_range(&mut self, range: Range<usize>) {
        match &mut self.queued_preview_range {
            Some(queued) => {
                queued.start = queued.start.min(range.start);
                queued.end = queued.end.max(range.end);
            }
            None => self.queued_preview_range = Some(range),
        }
    }

    pub fn take_queued_preview_range(&mut self) -> Option<Range<usize>> {
        self.queued_preview_range.take()
    }

    pub fn bump_render_revision(&mut self) {
        self.render_revision = self.render_revision.wrapping_add(1);
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

#[cfg(test)]
mod tests {
    use super::{AppState, PreviewPanelState};
    use crate::domain::{AppConfigV1, Language, OutputFormat, ProcessingMode, ProcessingOptions};

    #[test]
    fn clear_inputs_resets_workspace_runtime_state() {
        let config = AppConfigV1 {
            version: crate::domain::APP_CONFIG_VERSION,
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
        state.store_chunk(
            400..450,
            (400..450).map(|ix| format!("e-{ix}").into()).collect(),
        );
        state.store_chunk(
            500..550,
            (500..550).map(|ix| format!("f-{ix}").into()).collect(),
        );
        state.store_chunk_with_focus(
            600..650,
            (600..650).map(|ix| format!("g-{ix}").into()).collect(),
            &(550..650),
        );

        assert_eq!(state.preview_chunks.len(), 6);
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
                .any(|chunk| chunk.range == (500..550))
        );
        assert!(
            state
                .preview_chunks
                .iter()
                .any(|chunk| chunk.range == (600..650))
        );
        assert!(
            state
                .preview_chunks
                .iter()
                .any(|chunk| chunk.range == (250..300))
        );
    }

    #[test]
    fn preview_panel_line_lookup_tracks_loaded_chunks_without_full_index() {
        let mut state = PreviewPanelState::default();
        state.store_chunk(
            200..250,
            (200..250).map(|ix| format!("c-{ix}").into()).collect(),
        );
        state.store_chunk(
            250..300,
            (250..300).map(|ix| format!("d-{ix}").into()).collect(),
        );

        assert_eq!(
            state.line_at(220).map(|line| line.to_string()),
            Some("c-220".into())
        );
        assert_eq!(
            state.line_at(275).map(|line| line.to_string()),
            Some("d-275".into())
        );
    }

    #[test]
    fn preview_panel_queues_ranges_by_merging_visible_buckets() {
        let mut state = PreviewPanelState::default();

        state.queue_preview_range(100..180);
        state.queue_preview_range(180..260);

        assert_eq!(state.take_queued_preview_range(), Some(100..260));
        assert_eq!(state.take_queued_preview_range(), None);
    }

    #[test]
    fn preview_panel_keeps_neighboring_chunks_around_latest_focus() {
        let mut state = PreviewPanelState::default();
        for base in [0, 100, 200, 300, 400, 500] {
            state.store_chunk_with_focus(
                base..base + 50,
                (base..base + 50)
                    .map(|ix| format!("line-{ix}").into())
                    .collect(),
                &(450..550),
            );
        }
        state.store_chunk_with_focus(
            600..650,
            (600..650).map(|ix| format!("line-{ix}").into()).collect(),
            &(550..650),
        );

        assert_eq!(state.preview_chunks.len(), 6);
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
                .any(|chunk| chunk.range == (400..450))
        );
        assert!(
            state
                .preview_chunks
                .iter()
                .any(|chunk| chunk.range == (500..550))
        );
        assert!(
            state
                .preview_chunks
                .iter()
                .any(|chunk| chunk.range == (600..650))
        );
    }
}
