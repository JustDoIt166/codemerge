use std::ops::Range;
use std::rc::Rc;
use std::sync::mpsc::TryRecvError;

use gpui::{Context, SharedString, px, size};

use super::view::TreeExpansionMode;
use super::{Workspace, model, preview_line_height};
use crate::domain::{ProcessResult, ProcessStatus, ResultTab};
use crate::services::preflight::{PreflightEvent, PreflightRequest};
use crate::services::preview::{PreviewEvent, PreviewRequest, start as start_preview};
use crate::services::process::ProcessEvent;
use crate::ui::state::ProcessUiStatus;
use crate::utils::i18n::tr;

impl Workspace {
    pub(super) fn poll_background(&mut self, cx: &mut Context<Self>) {
        let mut dirty = false;

        if let Some(rx) = self.state.process.preflight_rx.take() {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        self.apply_preflight_event(event);
                        dirty = true;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        keep = false;
                        break;
                    }
                }
            }
            if keep {
                self.state.process.preflight_rx = Some(rx);
            }
        }

        let mut finish_processing = false;
        if let Some(handle) = self.state.process.process_handle.as_mut() {
            let mut events = Vec::new();
            loop {
                match handle.receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finish_processing = true;
                        break;
                    }
                }
            }
            for event in events {
                dirty = true;
                finish_processing = self.apply_process_event(event, cx) || finish_processing;
            }
        }
        if finish_processing {
            self.state.process.finish_run();
            dirty = true;
        }

        if let Some(rx) = self.state.workspace.preview_rx.take() {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        dirty = self.apply_preview_event(event) || dirty;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        keep = false;
                        self.state.workspace.preview_requested_range = None;
                        break;
                    }
                }
            }
            if keep {
                self.state.workspace.preview_rx = Some(rx);
            }
        }

        dirty = self.sync_tree_selection(cx) || dirty;
        dirty = self.refresh_preview_window() || dirty;
        if dirty {
            cx.notify();
        }
    }

    fn apply_preflight_event(&mut self, event: PreflightEvent) {
        let ready_label = tr(self.state.settings.language, "status_ready");
        let is_processing = self.is_processing();
        model::apply_preflight_event(&mut self.state.process, event, is_processing, ready_label);
    }

    fn apply_process_event(&mut self, event: ProcessEvent, cx: &mut Context<Self>) -> bool {
        match event {
            ProcessEvent::Scanning {
                scanned,
                candidates,
                skipped,
            } => {
                self.state.process.processing_scanned = scanned;
                self.state.process.processing_candidates = candidates;
                self.state.process.processing_skipped = skipped;
                self.state.process.ui_status = ProcessUiStatus::Running;
                self.state.process.processing_current_file = format!(
                    "{} {}",
                    tr(self.state.settings.language, "scanning_files"),
                    scanned
                );
                false
            }
            ProcessEvent::Record(record) => {
                self.state.process.ui_status = ProcessUiStatus::Running;
                self.state.process.processing_current_file = record.file_name.clone();
                if !matches!(record.status, ProcessStatus::Success) {
                    self.state.process.processing_skipped += 1;
                }
                self.state.process.processing_records.push(record);
                false
            }
            ProcessEvent::Completed(result) => {
                self.state.process.ui_status = ProcessUiStatus::Completed;
                self.state.process.last_error = None;
                self.state.process.processing_current_file =
                    tr(self.state.settings.language, "status_completed_hint").to_string();
                self.set_result(result, cx);
                true
            }
            ProcessEvent::Cancelled => {
                self.state.process.ui_status = ProcessUiStatus::Cancelled;
                self.state.process.processing_current_file =
                    tr(self.state.settings.language, "status_cancelled_hint").to_string();
                true
            }
            ProcessEvent::Failed(err) => {
                self.state.process.ui_status = ProcessUiStatus::Error;
                self.state.process.last_error = Some(err.to_string());
                self.state.process.processing_current_file = err.to_string();
                true
            }
        }
    }

    fn apply_preview_event(&mut self, event: PreviewEvent) -> bool {
        match event {
            PreviewEvent::Opened {
                revision,
                file_id,
                document,
                loaded_range,
                lines,
            } => {
                if revision != self.state.workspace.preview_revision
                    || self.state.workspace.selected_preview_file_id != Some(file_id)
                {
                    return false;
                }
                let line_count = document.line_count().max(1);
                self.state.workspace.preview_document = Some(document);
                self.state.workspace.preview_loaded_range = loaded_range;
                self.state.workspace.preview_loaded_lines =
                    lines.into_iter().map(SharedString::from).collect();
                self.state.workspace.preview_requested_range = None;
                self.state.workspace.preview_sizes = Rc::new(
                    (0..line_count)
                        .map(|_| size(px(100.), preview_line_height()))
                        .collect::<Vec<_>>(),
                );
                self.preview_scroll_handle.scroll_to_top_of_item(0);
                true
            }
            PreviewEvent::Loaded {
                revision,
                file_id,
                loaded_range,
                lines,
            } => {
                if revision != self.state.workspace.preview_revision
                    || self.state.workspace.selected_preview_file_id != Some(file_id)
                {
                    return false;
                }
                self.state.workspace.preview_loaded_range = loaded_range;
                self.state.workspace.preview_loaded_lines =
                    lines.into_iter().map(SharedString::from).collect();
                self.state.workspace.preview_requested_range = None;
                true
            }
            PreviewEvent::Failed {
                revision, file_id, ..
            } => {
                if revision != self.state.workspace.preview_revision
                    || self.state.workspace.selected_preview_file_id != Some(file_id)
                {
                    return false;
                }
                self.state.workspace.preview_requested_range = None;
                self.state.workspace.preview_loaded_range = 0..0;
                self.state.workspace.preview_loaded_lines.clear();
                true
            }
        }
    }

    fn set_result(&mut self, result: ProcessResult, cx: &mut Context<Self>) {
        if let Some(prev_dir) = self
            .state
            .result
            .result
            .as_ref()
            .and_then(|result| result.preview_blob_dir.as_ref())
        {
            let _ = crate::utils::temp_file::cleanup_preview_dir(prev_dir);
        }
        self.state.result.result = Some(result);
        self.state.result.active_tab = ResultTab::Tree;
        self.side_panel_tab = super::SidePanelTab::Results;
        let expanded_ids = self
            .state
            .result
            .result
            .as_ref()
            .map(|result| model::default_expanded_ids(&result.tree_nodes))
            .unwrap_or_default();
        self.state.workspace.reset_tree();
        self.state.workspace.tree_expanded_ids = expanded_ids;
        self.sync_tree(cx);
        self.sync_preview_table(cx);
    }

    pub(super) fn sync_tree(&mut self, cx: &mut Context<Self>) {
        self.sync_tree_with_mode(TreeExpansionMode::Default, cx);
    }

    pub(super) fn sync_tree_with_mode(&mut self, mode: TreeExpansionMode, cx: &mut Context<Self>) {
        if let Some(result) = self.state.result.result.as_ref() {
            match mode {
                TreeExpansionMode::Default => {}
                TreeExpansionMode::ExpandAll => {
                    self.state.workspace.tree_expanded_ids =
                        model::collect_folder_ids(&result.tree_nodes);
                }
                TreeExpansionMode::CollapseAll => {
                    self.state.workspace.tree_expanded_ids.clear();
                }
            }
        }

        let filter = self
            .tree_filter_input
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        let tree_model = model::build_tree_panel_model(
            self.state.result.result.as_ref(),
            filter.as_str(),
            &self.state.workspace.tree_expanded_ids,
            self.state.workspace.selected_tree_node_id.as_deref(),
        );
        let tree_row_map = tree_model
            .rows
            .into_iter()
            .map(|row| (row.node_id.to_string(), row))
            .collect();
        let tree_total_summary = tree_model.total_summary;
        let tree_visible_summary = tree_model.visible_summary;
        self.tree_state.update(cx, |state, tree_cx| {
            state.set_items(tree_model.items, tree_cx);
            state.set_selected_index(tree_model.selected_row_ix, tree_cx);
        });
        self.tree_row_map = tree_row_map;
        self.tree_total_summary = tree_total_summary;
        self.tree_visible_summary = tree_visible_summary;
    }

    pub(super) fn sync_preview_table(&mut self, cx: &mut Context<Self>) {
        let filter = self
            .preview_filter_input
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        let table_model = model::build_preview_table_model(
            self.state.result.result.as_ref(),
            filter.as_str(),
            self.state.workspace.selected_preview_file_id,
        );
        self.state.result.preview_rows = table_model.rows.clone();
        self.preview_table.update(cx, |table, cx| {
            table.delegate_mut().rows = table_model.rows;
            if let Some(row_ix) =
                table_model
                    .selected_row_ix
                    .or(if table_model.next_selected_file_id.is_some() {
                        Some(0)
                    } else {
                        None
                    })
            {
                table.set_selected_row(row_ix, cx);
            } else {
                table.clear_selection(cx);
            }
            cx.notify();
        });

        match table_model.next_selected_file_id {
            Some(file_id) => self.load_preview(file_id, cx),
            None => self.clear_preview_state(),
        }
    }

    pub(super) fn refresh_preflight(&mut self) {
        self.state.process.preflight_revision += 1;
        if !self.is_processing() {
            self.state.process.ui_status = ProcessUiStatus::Preflight;
            self.state.process.last_error = None;
        }
        self.state.process.preflight_rx =
            Some(crate::services::preflight::start(PreflightRequest {
                revision: self.state.process.preflight_revision,
                selected_folder: self.state.selection.selected_folder.clone(),
                selected_files: self
                    .state
                    .selection
                    .selected_files
                    .iter()
                    .map(|f| f.path.clone())
                    .collect(),
                folder_blacklist: self.state.settings.folder_blacklist.clone(),
                ext_blacklist: self.state.settings.ext_blacklist.clone(),
            }));
    }

    pub(super) fn clear_preview_state(&mut self) {
        self.state.workspace.reset_preview();
    }

    fn sync_tree_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let selected_entry = self.tree_state.read(cx).selected_entry().cloned();
        let selected_entry_state = selected_entry
            .as_ref()
            .map(|entry| (entry.is_folder(), entry.is_expanded()));
        let selected_row = selected_entry
            .as_ref()
            .and_then(|entry| self.tree_row_map.get(entry.item().id.as_ref()))
            .cloned();
        let selected_node_id = selected_row.as_ref().map(|row| row.node_id.to_string());

        if self.state.workspace.selected_tree_node_id == selected_node_id {
            if let Some(row) = selected_row
                && row.is_folder
            {
                let is_expanded = selected_entry_state
                    .map(|(_, expanded)| expanded)
                    .unwrap_or(row.is_expanded);
                if is_expanded {
                    self.state
                        .workspace
                        .tree_expanded_ids
                        .insert(row.node_id.to_string());
                } else {
                    self.state
                        .workspace
                        .tree_expanded_ids
                        .remove(row.node_id.as_ref());
                }
            }
            return false;
        }

        self.state.workspace.selected_tree_node_id = selected_node_id;
        let Some(row) = selected_row else {
            return true;
        };

        if row.is_folder {
            let is_expanded = selected_entry_state
                .map(|(_, expanded)| expanded)
                .unwrap_or(row.is_expanded);
            if is_expanded {
                self.state
                    .workspace
                    .tree_expanded_ids
                    .insert(row.node_id.to_string());
            } else {
                self.state
                    .workspace
                    .tree_expanded_ids
                    .remove(row.node_id.as_ref());
            }
            self.sync_tree(cx);
            return true;
        }

        if let Some(file_id) = row.preview_file_id {
            self.state.result.active_tab = ResultTab::Content;
            self.load_preview(file_id, cx);
        }
        true
    }

    fn padded_preview_range(&self, range: Range<usize>, line_count: usize) -> Range<usize> {
        if line_count == 0 {
            return 0..0;
        }
        let start = range.start.min(line_count.saturating_sub(1));
        let end = range.end.max(start + 1).min(line_count);
        start.saturating_sub(50)..(end + 100).min(line_count)
    }

    fn request_preview_range(&mut self, range: Range<usize>) -> bool {
        let Some(document) = &self.state.workspace.preview_document else {
            return false;
        };
        let Some(file_id) = self.state.workspace.selected_preview_file_id else {
            return false;
        };

        let padded = self.padded_preview_range(range, document.line_count());
        if padded.start >= padded.end {
            return false;
        }
        if self.state.workspace.preview_loaded_range.start <= padded.start
            && self.state.workspace.preview_loaded_range.end >= padded.end
        {
            return false;
        }
        if self.state.workspace.preview_requested_range.as_ref() == Some(&padded)
            || self.state.workspace.preview_rx.is_some()
        {
            return false;
        }

        self.state.workspace.preview_requested_range = Some(padded.clone());
        self.state.workspace.preview_rx = Some(start_preview(PreviewRequest::LoadRange {
            revision: self.state.workspace.preview_revision,
            file_id,
            document: document.clone(),
            range: padded,
        }));
        true
    }

    fn refresh_preview_window(&mut self) -> bool {
        let Some(document) = &self.state.workspace.preview_document else {
            return false;
        };
        let line_count = document.line_count();
        if line_count == 0 {
            return false;
        }

        let visible = if self.state.workspace.preview_visible_range.start
            < self.state.workspace.preview_visible_range.end
        {
            self.state.workspace.preview_visible_range.clone()
        } else {
            0..line_count.min(1)
        };

        if self.state.workspace.preview_loaded_range.start <= visible.start
            && self.state.workspace.preview_loaded_range.end >= visible.end
        {
            return false;
        }

        self.request_preview_range(visible)
    }

    pub(super) fn load_preview(&mut self, file_id: u32, cx: &mut Context<Self>) {
        if self.state.workspace.selected_preview_file_id == Some(file_id)
            && (self.state.workspace.preview_document.is_some()
                || self.state.workspace.preview_rx.is_some())
        {
            return;
        }
        let Some(entry) = self.state.result.result.as_ref().and_then(|result| {
            result
                .preview_files
                .iter()
                .find(|entry| entry.id == file_id)
        }) else {
            return;
        };
        self.state.workspace.preview_revision += 1;
        self.state.workspace.selected_preview_file_id = Some(file_id);
        self.state.workspace.preview_rx = Some(start_preview(PreviewRequest::Open {
            revision: self.state.workspace.preview_revision,
            file_id,
            path: entry.preview_blob_path.clone(),
            initial_range: 0..200,
        }));
        self.state.workspace.preview_requested_range = Some(0..200);
        self.state.workspace.preview_document = None;
        self.state.workspace.preview_loaded_range = 0..0;
        self.state.workspace.preview_loaded_lines.clear();
        self.state.workspace.preview_visible_range = 0..0;
        self.state.workspace.preview_sizes = Rc::new(vec![size(px(100.), preview_line_height())]);
        self.preview_scroll_handle.scroll_to_top_of_item(0);
        cx.notify();
    }
}
