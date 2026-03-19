use std::ops::Range;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use gpui::{Context, ScrollStrategy, SharedString};

use super::view::TreeExpansionMode;
use super::{Workspace, model};
use crate::domain::{ProcessResult, ProcessStatus, ResultTab};
use crate::services::preflight::{PreflightEvent, PreflightRequest};
use crate::services::preview::{PreviewEvent, PreviewRequest, start as start_preview};
use crate::services::process::ProcessEvent;
use crate::ui::state::ProcessUiStatus;
use crate::utils::i18n::tr;

impl Workspace {
    pub(super) fn poll_background(&mut self, cx: &mut Context<Self>) -> Option<Duration> {
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

        if let Some(rx) = self.state.workspace.preview_panel.preview_rx.take() {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        dirty = self.apply_preview_event(event) || dirty;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        keep = false;
                        self.state.workspace.preview_panel.preview_requested_range = None;
                        break;
                    }
                }
            }
            if keep {
                self.state.workspace.preview_panel.preview_rx = Some(rx);
            }
        }

        if dirty {
            cx.notify();
        }

        let next_delay = if self.state.process.preflight_rx.is_some()
            || self.state.process.process_handle.is_some()
            || self.state.workspace.preview_panel.preview_rx.is_some()
        {
            Some(Duration::from_millis(16))
        } else {
            None
        };

        if next_delay.is_none() {
            self.poll_task_running = false;
            self.poll_task = None;
        }

        next_delay
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
                if revision != self.state.workspace.preview_panel.preview_revision
                    || self.state.workspace.preview_panel.selected_preview_file_id != Some(file_id)
                {
                    return false;
                }
                self.state.workspace.preview_panel.preview_document = Some(document);
                self.state.workspace.preview_panel.preview_error = None;
                self.state.workspace.preview_panel.preview_requested_range = None;
                self.state.workspace.preview_panel.clear_loaded_chunks();
                self.state.workspace.preview_panel.store_chunk(
                    loaded_range,
                    lines.into_iter().map(SharedString::from).collect(),
                );
                self.preview_scroll_handle
                    .scroll_to_item_strict(0, ScrollStrategy::Top);
                true
            }
            PreviewEvent::Loaded {
                revision,
                file_id,
                loaded_range,
                lines,
            } => {
                if revision != self.state.workspace.preview_panel.preview_revision
                    || self.state.workspace.preview_panel.selected_preview_file_id != Some(file_id)
                {
                    return false;
                }
                self.state.workspace.preview_panel.preview_error = None;
                self.state.workspace.preview_panel.preview_requested_range = None;
                self.state.workspace.preview_panel.store_chunk(
                    loaded_range,
                    lines.into_iter().map(SharedString::from).collect(),
                );
                true
            }
            PreviewEvent::Failed {
                revision,
                file_id,
                error,
            } => {
                if revision != self.state.workspace.preview_panel.preview_revision
                    || self.state.workspace.preview_panel.selected_preview_file_id != Some(file_id)
                {
                    return false;
                }
                self.state.workspace.preview_panel.preview_error = Some(error.to_string());
                self.state.workspace.preview_panel.preview_requested_range = None;
                if self
                    .state
                    .workspace
                    .preview_panel
                    .preview_document
                    .is_none()
                {
                    self.state.workspace.preview_panel.clear_loaded_chunks();
                }
                true
            }
        }
    }

    fn set_result(&mut self, result: ProcessResult, cx: &mut Context<Self>) {
        self.cleanup_current_result_artifacts();
        self.state.result.result = Some(result);
        self.state.result.active_tab = ResultTab::Tree;
        self.tree_panel.data = model::build_tree_panel_data(self.state.result.result.as_ref());
        self.tree_panel.last_interaction = None;
        self.tree_panel.render_state = model::TreeRenderState::default();
        self.state.workspace.reset_tree();
        if let Some(data) = self.tree_panel.data.as_ref() {
            self.state.workspace.tree_panel.expanded_ids = data.index.default_expanded_ids.clone();
        }
        self.sync_tree(cx);
        self.sync_preview_table(cx);
    }

    pub(super) fn sync_tree(&mut self, cx: &mut Context<Self>) {
        self.sync_tree_with_mode(TreeExpansionMode::Default, cx);
    }

    pub(super) fn sync_tree_with_mode(&mut self, mode: TreeExpansionMode, cx: &mut Context<Self>) {
        if let Some(data) = self.tree_panel.data.as_ref() {
            match mode {
                TreeExpansionMode::Default => {}
                TreeExpansionMode::ExpandAll => {
                    self.state.workspace.tree_panel.expanded_ids = data.index.folder_ids.clone();
                }
                TreeExpansionMode::CollapseAll => {
                    self.state.workspace.tree_panel.expanded_ids.clear();
                }
            }
        }

        let filter = self
            .tree_panel
            .filter_input
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        let render_state = model::project_tree_panel(
            self.tree_panel.data.as_ref(),
            filter.as_str(),
            &self.state.workspace.tree_panel.expanded_ids,
            self.state.workspace.tree_panel.selected_node_id.as_deref(),
        );
        self.tree_panel.state.update(cx, |state, tree_cx| {
            state.set_items(render_state.items.clone(), tree_cx);
            state.set_selected_index(render_state.selected_row_ix, tree_cx);
        });
        self.tree_panel.render_state = render_state;
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
            self.state.workspace.preview_panel.selected_preview_file_id,
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
            Some(file_id) => {
                let _ =
                    self.apply_tree_panel_effect(model::TreePanelEffect::OpenPreview(file_id), cx);
            }
            None => self.clear_preview_state(),
        }
    }

    pub(super) fn refresh_preflight(&mut self, cx: &mut Context<Self>) {
        self.state.process.preflight_revision += 1;
        if !self.is_processing() {
            self.state.process.ui_status = ProcessUiStatus::Preflight;
            self.state.process.last_error = None;
        }
        self.state.process.preflight_rx = Some(crate::services::preflight::start_with_options(
            PreflightRequest {
                revision: self.state.process.preflight_revision,
                selected_folder: self.state.selection.selected_folder.clone(),
                selected_files: self
                    .state
                    .selection
                    .selected_files
                    .iter()
                    .map(|f| f.path.clone())
                    .collect(),
                folder_blacklist: self.state.effective_folder_blacklist(),
                ext_blacklist: self.state.settings.ext_blacklist.clone(),
            },
            crate::processor::walker::WalkerOptions {
                use_gitignore: self.state.settings.options.use_gitignore,
                ignore_git: self.state.settings.options.ignore_git,
            },
        ));
        self.ensure_background_polling(cx);
    }

    pub(super) fn clear_preview_state(&mut self) {
        self.state.workspace.reset_preview();
    }

    pub(super) fn sync_tree_interaction(&mut self, cx: &mut Context<Self>) -> bool {
        let next = self.current_tree_interaction_snapshot(cx);
        let effect = model::apply_tree_interaction(
            &mut self.state.workspace.tree_panel,
            self.tree_panel.last_interaction.as_ref(),
            next.clone(),
        );
        self.tree_panel.last_interaction = next;
        self.apply_tree_panel_effect(effect, cx)
    }

    fn current_tree_interaction_snapshot(
        &self,
        cx: &Context<Self>,
    ) -> Option<model::TreeInteractionSnapshot> {
        let selected_entry = self.tree_panel.state.read(cx).selected_entry().cloned()?;
        let selected_row = self
            .tree_panel
            .render_state
            .rows_by_id
            .get(selected_entry.item().id.as_ref());

        Some(model::TreeInteractionSnapshot {
            node_id: Some(selected_entry.item().id.as_ref().to_string()),
            is_folder: selected_entry.is_folder(),
            is_expanded: selected_entry.is_expanded(),
            preview_file_id: selected_row.and_then(|row| row.preview_file_id),
        })
    }

    fn apply_tree_panel_effect(
        &mut self,
        effect: model::TreePanelEffect,
        cx: &mut Context<Self>,
    ) -> bool {
        match effect {
            model::TreePanelEffect::None => false,
            model::TreePanelEffect::RefreshTree => {
                self.sync_tree(cx);
                true
            }
            model::TreePanelEffect::OpenPreview(file_id) => {
                self.load_preview(file_id, cx);
                true
            }
            model::TreePanelEffect::SwitchToContentAndOpen(file_id) => {
                self.state.result.active_tab = ResultTab::Content;
                self.load_preview(file_id, cx);
                true
            }
        }
    }

    fn padded_preview_range(&self, range: Range<usize>, line_count: usize) -> Range<usize> {
        if line_count == 0 {
            return 0..0;
        }
        let start = range.start.min(line_count.saturating_sub(1));
        let end = range.end.max(start + 1).min(line_count);
        let visible_len = end.saturating_sub(start).max(1);
        let preload_before = visible_len.max(64);
        let preload_after = visible_len.saturating_mul(2).max(128);

        start.saturating_sub(preload_before)..(end + preload_after).min(line_count)
    }

    fn request_preview_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> bool {
        let Some(document) = &self.state.workspace.preview_panel.preview_document else {
            return false;
        };
        let Some(file_id) = self.state.workspace.preview_panel.selected_preview_file_id else {
            return false;
        };

        let padded = self.padded_preview_range(range, document.line_count());
        if padded.start >= padded.end {
            return false;
        }
        if self.state.workspace.preview_panel.has_loaded_range(&padded) {
            return false;
        }
        if self
            .state
            .workspace
            .preview_panel
            .preview_requested_range
            .as_ref()
            == Some(&padded)
        {
            return false;
        }

        self.state.workspace.preview_panel.preview_requested_range = Some(padded.clone());
        self.state.workspace.preview_panel.preview_rx =
            Some(start_preview(PreviewRequest::LoadRange {
                revision: self.state.workspace.preview_panel.preview_revision,
                file_id,
                document: document.clone(),
                range: padded,
            }));
        self.ensure_background_polling(cx);
        true
    }

    pub(super) fn sync_preview_visible_range(
        &mut self,
        visible: Range<usize>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(document) = &self.state.workspace.preview_panel.preview_document else {
            return false;
        };
        let line_count = document.line_count();
        if line_count == 0 {
            return false;
        }

        let start = visible.start.min(line_count.saturating_sub(1));
        let end = visible.end.max(start + 1).min(line_count);
        let visible = start..end;
        let changed = self
            .state
            .workspace
            .preview_panel
            .update_visible_range(visible.clone());

        if !changed
            || self
                .state
                .workspace
                .preview_panel
                .has_loaded_range(&visible)
        {
            return false;
        }

        self.request_preview_range(visible, cx)
    }

    pub(super) fn load_preview(&mut self, file_id: u32, cx: &mut Context<Self>) {
        if self.state.workspace.preview_panel.selected_preview_file_id == Some(file_id)
            && (self
                .state
                .workspace
                .preview_panel
                .preview_document
                .is_some()
                || self.state.workspace.preview_panel.preview_rx.is_some())
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
        self.state.workspace.preview_panel.preview_revision += 1;
        self.state.workspace.preview_panel.selected_preview_file_id = Some(file_id);
        self.state.workspace.preview_panel.preview_error = None;
        self.state.workspace.preview_panel.preview_rx = Some(start_preview(PreviewRequest::Open {
            revision: self.state.workspace.preview_panel.preview_revision,
            file_id,
            path: entry.preview_blob_path.clone(),
            initial_range: 0..200,
        }));
        self.state.workspace.preview_panel.preview_requested_range = Some(0..200);
        self.state.workspace.preview_panel.preview_document = None;
        self.state
            .workspace
            .preview_panel
            .preview_last_visible_range = 0..0;
        self.state.workspace.preview_panel.clear_loaded_chunks();
        self.preview_scroll_handle
            .scroll_to_item_strict(0, ScrollStrategy::Top);
        self.ensure_background_polling(cx);
        cx.notify();
    }
}
