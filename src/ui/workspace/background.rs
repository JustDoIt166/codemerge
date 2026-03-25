use std::ops::Range;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use gpui::Context;

use super::view::TreeExpansionMode;
use super::{Workspace, model};
use crate::domain::{ProcessResult, ResultTab};
use crate::services::preflight::{PreflightEvent, PreflightRequest};
use crate::services::preview::{
    EXCERPT_PREVIEW_BYTES, PreviewEvent, PreviewRequest, create_excerpt_preview,
    start as start_preview,
};
use crate::ui::models::ProcessEventEffect;
use crate::ui::perf;
use crate::ui::preview_model::PreviewEventEffect;

impl Workspace {
    pub(super) fn sync_tree_selection_for_preview_file(
        &mut self,
        file_id: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(node_id) =
            model::preview_file_node_id(self.result.read(cx).state().result.as_ref(), file_id)
        else {
            return false;
        };

        let selected_changed =
            self.state.workspace.tree_panel.selected_node_id.as_deref() != Some(node_id.as_str());
        self.state.workspace.tree_panel.selected_node_id = Some(node_id.clone());

        let mut expansion_changed = false;
        for ancestor in model::ancestor_node_ids(&node_id) {
            expansion_changed |= self
                .state
                .workspace
                .tree_panel
                .expanded_ids
                .insert(ancestor);
        }

        if selected_changed || expansion_changed {
            self.sync_tree(cx);
        }

        selected_changed || expansion_changed
    }

    pub(super) fn open_preview_file_from_results(
        &mut self,
        file_id: u32,
        sync_tree_selection: bool,
        cx: &mut Context<Self>,
    ) {
        self.load_preview(file_id, cx);
        if sync_tree_selection {
            let _ = self.sync_tree_selection_for_preview_file(file_id, cx);
        }
    }

    fn load_preview_path(
        &mut self,
        file_id: u32,
        preview_path: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        let preview_state = self.preview.read(cx).state();
        if preview_state.selected_preview_file_id == Some(file_id)
            && (preview_state.preview_document.is_some() || preview_state.preview_rx.is_some())
        {
            return;
        }

        let request = self.preview.update(cx, |preview, preview_cx| {
            let request = preview.open_preview(file_id, preview_path);
            preview_cx.notify();
            request
        });
        self.start_preview_request(request, cx);
    }

    fn start_preview_request(&mut self, request: PreviewRequest, cx: &mut Context<Self>) {
        self.preview.update(cx, |preview, _| {
            preview.set_preview_rx(Some(start_preview(request)));
        });
        self.preview_pane_view.update(cx, |view, _| {
            view.scroll_to_top();
        });
        self.ensure_background_polling(cx);
    }

    pub(super) fn load_merged_content_preview(&mut self, cx: &mut Context<Self>) {
        let merged_content_path = self
            .result
            .read(cx)
            .state()
            .result
            .as_ref()
            .and_then(|result| result.merged_content_path.clone());
        let Some(merged_content_path) = merged_content_path else {
            return;
        };

        let preview_state = self.preview.read(cx).state();
        if preview_state.selected_preview_file_id == Some(super::MERGED_CONTENT_PREVIEW_FILE_ID)
            && (preview_state.preview_document.is_some()
                || preview_state.preview_rx.is_some()
                || preview_state
                    .deferred_preview
                    .as_ref()
                    .is_some_and(|state| state.source_path == merged_content_path))
        {
            return;
        }

        if let Ok(metadata) = std::fs::metadata(&merged_content_path)
            && metadata.len() > super::MERGED_CONTENT_AUTO_PREVIEW_MAX_BYTES
        {
            self.preview.update(cx, |preview, preview_cx| {
                preview.defer_preview(
                    super::MERGED_CONTENT_PREVIEW_FILE_ID,
                    merged_content_path.clone(),
                    metadata.len(),
                    EXCERPT_PREVIEW_BYTES,
                );
                preview_cx.notify();
            });
            self.preview_pane_view.update(cx, |view, _| {
                view.scroll_to_top();
            });
            return;
        }

        self.load_preview_path(
            super::MERGED_CONTENT_PREVIEW_FILE_ID,
            merged_content_path,
            cx,
        );
    }

    pub(super) fn load_deferred_merged_content_excerpt(&mut self, cx: &mut Context<Self>) {
        let Some((source_path, source_byte_len)) = self
            .preview
            .read(cx)
            .deferred_preview()
            .filter(|state| state.excerpt_path.is_none())
            .map(|state| (state.source_path.clone(), state.source_byte_len))
        else {
            return;
        };

        match create_excerpt_preview(&source_path, EXCERPT_PREVIEW_BYTES) {
            Ok(excerpt_path) => {
                let request = self.preview.update(cx, |preview, preview_cx| {
                    let request = preview.open_deferred_excerpt_preview(
                        super::MERGED_CONTENT_PREVIEW_FILE_ID,
                        source_path.clone(),
                        source_byte_len,
                        EXCERPT_PREVIEW_BYTES,
                        excerpt_path,
                    );
                    preview_cx.notify();
                    request
                });
                self.start_preview_request(request, cx);
            }
            Err(error) => {
                self.preview.update(cx, |preview, preview_cx| {
                    preview.set_preview_error_message(error.to_string());
                    preview_cx.notify();
                });
            }
        }
    }

    pub(super) fn load_deferred_merged_content_full(&mut self, cx: &mut Context<Self>) {
        let Some(source_path) = self
            .preview
            .read(cx)
            .deferred_preview()
            .map(|state| state.source_path.clone())
        else {
            return;
        };

        let request = self.preview.update(cx, |preview, preview_cx| {
            let request =
                preview.open_preview(super::MERGED_CONTENT_PREVIEW_FILE_ID, source_path.clone());
            preview_cx.notify();
            request
        });
        self.start_preview_request(request, cx);
    }

    pub(super) fn poll_background(&mut self, cx: &mut Context<Self>) -> Option<Duration> {
        let mut received_events = false;

        if let Some(rx) = self
            .process
            .update(cx, |process, _| process.state_mut().preflight_rx.take())
        {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        self.apply_preflight_event(event, cx);
                        received_events = true;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        keep = false;
                        break;
                    }
                }
            }
            if keep {
                self.process
                    .update(cx, |process, _| process.state_mut().preflight_rx = Some(rx));
            }
        }

        let (events, disconnected) = self.process.update(cx, |process, _| {
            let mut events = Vec::new();
            let mut disconnected = false;
            if let Some(handle) = process.state_mut().process_handle.as_mut() {
                loop {
                    match handle.receiver.try_recv() {
                        Ok(event) => events.push(event),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }
            }
            (events, disconnected)
        });
        let mut finish_processing = disconnected;
        for event in events {
            received_events = true;
            let (_, event_finished) = self.apply_process_event(event, cx);
            finish_processing = event_finished || finish_processing;
        }
        if finish_processing {
            self.process
                .update(cx, |process, _| process.state_mut().finish_run());
        }

        if let Some(rx) = self
            .preview
            .update(cx, |preview, _| preview.take_preview_rx())
        {
            let mut keep = true;
            let mut events = Vec::new();
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        received_events = true;
                        events.push(event);
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        keep = false;
                        break;
                    }
                }
            }
            if !events.is_empty() {
                self.apply_preview_events(events, cx);
            }
            if !keep {
                self.preview.update(cx, |preview, _| {
                    preview.clear_request();
                });
            }
            if keep {
                self.preview
                    .update(cx, |preview, _| preview.set_preview_rx(Some(rx)));
            }
        }

        let active = self.needs_background_polling(cx);
        if active {
            self.poll_idle_streak = if received_events {
                0
            } else {
                self.poll_idle_streak.saturating_add(1)
            };
        }
        let next_delay = if active {
            Some(match self.poll_idle_streak {
                0..=1 => Duration::from_millis(16),
                2..=5 => Duration::from_millis(33),
                _ => Duration::from_millis(66),
            })
        } else {
            None
        };

        if next_delay.is_none() {
            self.poll_task_running = false;
            self.poll_task = None;
            self.poll_idle_streak = 0;
        }

        next_delay
    }

    fn apply_preflight_event(&mut self, event: PreflightEvent, cx: &mut Context<Self>) {
        let language = self.language(cx);
        self.process.update(cx, |process, process_cx| {
            let before = (
                process.state().ui_status,
                process.state().preflight.total_files,
                process.state().preflight.skipped_files,
                process.state().preflight.to_process_files,
                process.state().preflight.scanned_entries,
                process.state().preflight.is_scanning,
                process.state().processing_current_file.clone(),
                process.state().last_error.clone(),
            );
            process.apply_preflight_event(event, language);
            let after = (
                process.state().ui_status,
                process.state().preflight.total_files,
                process.state().preflight.skipped_files,
                process.state().preflight.to_process_files,
                process.state().preflight.scanned_entries,
                process.state().preflight.is_scanning,
                process.state().processing_current_file.clone(),
                process.state().last_error.clone(),
            );
            if before != after {
                process_cx.notify();
            }
        });
    }

    fn apply_process_event(
        &mut self,
        event: crate::services::process::ProcessEvent,
        cx: &mut Context<Self>,
    ) -> (bool, bool) {
        let language = self.language(cx);
        match self.process.update(cx, |process, process_cx| {
            let before = (
                process.state().ui_status,
                process.state().processing_records.len(),
                process.state().processing_scanned,
                process.state().processing_candidates,
                process.state().processing_skipped,
                process.state().processing_current_file.clone(),
                process.state().last_error.clone(),
            );
            let effect = process.apply_process_event(event, language);
            let after = (
                process.state().ui_status,
                process.state().processing_records.len(),
                process.state().processing_scanned,
                process.state().processing_candidates,
                process.state().processing_skipped,
                process.state().processing_current_file.clone(),
                process.state().last_error.clone(),
            );
            if before != after {
                process_cx.notify();
            }
            effect
        }) {
            ProcessEventEffect::Continue => (false, false),
            ProcessEventEffect::Completed(result) => {
                self.set_result(*result, cx);
                (true, true)
            }
            ProcessEventEffect::Finish => (false, true),
        }
    }

    fn apply_preview_events(&mut self, events: Vec<PreviewEvent>, cx: &mut Context<Self>) {
        let effect = self.preview.update(cx, |preview, preview_cx| {
            let before = (
                preview.state().selected_preview_file_id,
                preview.state().preview_error.clone(),
                preview.render_revision(),
            );
            let effect = preview.apply_events(events);
            let after = (
                preview.state().selected_preview_file_id,
                preview.state().preview_error.clone(),
                preview.render_revision(),
            );
            if before != after {
                preview_cx.notify();
            }
            effect
        });
        if matches!(effect, PreviewEventEffect::ScrollTop) {
            self.preview_pane_view.update(cx, |view, _| {
                view.scroll_to_top();
            });
        }
        self.request_queued_preview_range(cx);
    }

    pub(super) fn set_result(&mut self, result: ProcessResult, cx: &mut Context<Self>) {
        self.cleanup_current_result_artifacts();
        self.preview_table_cache = super::PreviewTableCache::default();
        self.result_artifacts = super::ResultArtifacts {
            merged_content_path: result.merged_content_path.clone(),
            preview_blob_dir: result.preview_blob_dir.clone(),
        };
        self.result.update(cx, |result_model, result_cx| {
            result_model.set_result(result);
            result_cx.notify();
        });
        self.preview.update(cx, |preview, preview_cx| {
            preview.clear();
            preview_cx.notify();
        });
        let result = self.result.read(cx);
        self.tree_panel.data = model::build_tree_panel_data(result.state().result.as_ref());
        self.tree_panel.projection = model::TreeProjectionState::default();
        self.tree_panel.last_interaction = None;
        self.tree_panel.render_state = model::TreeRenderState::default();
        self.tree_panel.total_summary = model::TreeCountSummary::default();
        self.tree_panel.last_filter.clear();
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
        perf::record_tree_sync();
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
        let filter_changed = self.tree_panel.last_filter != filter;
        if filter_changed || self.tree_panel.projection.roots.is_empty() {
            self.tree_panel.projection =
                model::build_tree_projection(self.tree_panel.data.as_ref(), filter.as_str());
            self.tree_panel.total_summary = self.tree_panel.projection.total_summary;
            self.tree_panel.last_filter = filter.clone();
        }
        let render_state = model::build_tree_render_state(
            &self.tree_panel.projection,
            !filter.is_empty(),
            &self.state.workspace.tree_panel.expanded_ids,
            self.state.workspace.tree_panel.selected_node_id.as_deref(),
        );
        let replace_items =
            self.tree_panel.render_state.structure_signature != render_state.structure_signature;
        let selected_changed =
            self.tree_panel.render_state.selected_row_ix != render_state.selected_row_ix;
        self.suppress_tree_interaction_sync.set(true);
        self.tree_panel.state.update(cx, |state, tree_cx| {
            if replace_items {
                perf::record_tree_set_items();
                state.set_items(render_state.items.clone(), tree_cx);
            }
            if replace_items || selected_changed {
                state.set_selected_index(render_state.selected_row_ix, tree_cx);
            }
        });
        let tree_interaction_guard = self.suppress_tree_interaction_sync.clone();
        cx.defer(move |_| {
            tree_interaction_guard.set(false);
        });
        self.tree_panel.render_state = render_state;
    }

    pub(super) fn sync_preview_table(&mut self, cx: &mut Context<Self>) {
        perf::record_preview_table_sync();
        let filter = self
            .preview_filter_input
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        let current_selected_id = self.preview.read(cx).selected_preview_file_id();
        let has_merged_content = self
            .result
            .read(cx)
            .state()
            .result
            .as_ref()
            .is_some_and(|result| result.merged_content_path.is_some());
        let result_key = self
            .result
            .read(cx)
            .state()
            .result
            .as_ref()
            .map_or(0, |result| {
                result.preview_files.len()
                    ^ result.tree_nodes.len()
                    ^ usize::from(result.merged_content_path.is_some())
            }) as u64;
        let table_model = if self.preview_table_cache.filter == filter
            && self.preview_table_cache.result_key == result_key
            && self.preview_table_cache.current_selected_id == current_selected_id
        {
            self.preview_table_cache.model.clone().unwrap_or_else(|| {
                model::build_preview_table_model(
                    self.result.read(cx).state().result.as_ref(),
                    filter.as_str(),
                    current_selected_id,
                )
            })
        } else {
            let model = model::build_preview_table_model(
                self.result.read(cx).state().result.as_ref(),
                filter.as_str(),
                current_selected_id,
            );
            self.preview_table_cache.filter = filter.clone();
            self.preview_table_cache.result_key = result_key;
            self.preview_table_cache.current_selected_id = current_selected_id;
            self.preview_table_cache.model = Some(model.clone());
            model
        };
        let preserve_merged_preview = has_merged_content
            && current_selected_id == Some(super::MERGED_CONTENT_PREVIEW_FILE_ID);
        let show_merged_preview = preserve_merged_preview
            || (has_merged_content && table_model.next_selected_file_id.is_none());
        let target_row_ix = if show_merged_preview {
            None
        } else {
            table_model
                .selected_row_ix
                .or(if table_model.next_selected_file_id.is_some() {
                    Some(0)
                } else {
                    None
                })
        };
        let should_sync_tree_selection = !show_merged_preview
            && table_model.next_selected_file_id.is_some_and(|file_id| {
                current_selected_id != Some(file_id)
                    && (current_selected_id.is_some() || !filter.is_empty())
            });

        if show_merged_preview {
            self.load_merged_content_preview(cx);
        } else if let Some(file_id) = table_model.next_selected_file_id {
            self.open_preview_file_from_results(file_id, should_sync_tree_selection, cx);
        } else {
            self.clear_preview_state(cx);
        }

        let preview_rows = table_model.rows.clone();
        self.result.update(cx, |result, result_cx| {
            if result.state().preview_rows != preview_rows {
                result.set_preview_rows(preview_rows);
                result_cx.notify();
            }
        });
        self.suppress_preview_table_events = true;
        self.preview_table.update(cx, |table, cx| {
            let prev_rows = table.delegate().rows.clone();
            table.delegate_mut().rows = table_model.rows;
            if let Some(row_ix) = target_row_ix {
                if table.selected_row() != Some(row_ix) {
                    table.set_selected_row(row_ix, cx);
                }
            } else if table.selected_row().is_some() {
                table.clear_selection(cx);
            }
            if prev_rows != table.delegate().rows {
                cx.notify();
            }
        });
        self.suppress_preview_table_events = false;
    }

    pub(super) fn refresh_preflight(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings_snapshot(cx);
        let selection = self.selection_snapshot(cx);
        let revision = self.process.update(cx, |process, process_cx| {
            let is_processing = process.is_processing();
            let state = process.state_mut();
            state.preflight_revision += 1;
            if !is_processing {
                state.ui_status = crate::ui::state::ProcessUiStatus::Preflight;
                state.last_error = None;
            }
            process_cx.notify();
            state.preflight_revision
        });
        let rx = crate::services::preflight::start_with_options(
            PreflightRequest {
                revision,
                selected_folder: selection.selected_folder.clone(),
                selected_files: selection
                    .selected_files
                    .iter()
                    .map(|f| f.path.clone())
                    .collect(),
                folder_blacklist: self.effective_folder_blacklist(cx),
                ext_blacklist: settings.ext_blacklist.clone(),
            },
            crate::processor::walker::WalkerOptions {
                use_gitignore: settings.options.use_gitignore,
                ignore_git: settings.options.ignore_git,
            },
        );
        self.process.update(cx, |process, process_cx| {
            process.state_mut().preflight_rx = Some(rx);
            process_cx.notify();
        });
        self.ensure_background_polling(cx);
    }

    pub(super) fn clear_preview_state(&mut self, cx: &mut Context<Self>) {
        self.preview.update(cx, |preview, preview_cx| {
            let before = preview.render_revision();
            preview.clear();
            if before != preview.render_revision() {
                preview_cx.notify();
            }
        });
    }

    pub(super) fn sync_tree_interaction(&mut self, cx: &mut Context<Self>) -> bool {
        if self.suppress_tree_interaction_sync.get() {
            return false;
        }
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
            model::TreePanelEffect::RefreshVisibleTree => {
                self.sync_tree(cx);
                true
            }
            model::TreePanelEffect::SwitchToContentAndOpen(file_id) => {
                self.result.update(cx, |result, result_cx| {
                    result.set_active_tab(ResultTab::Content);
                    result_cx.notify();
                });
                self.load_preview(file_id, cx);
                true
            }
        }
    }

    pub(super) fn request_preview_range(
        &mut self,
        range: Range<usize>,
        direction: crate::ui::preview_model::PreviewScrollDirection,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.preview.read(cx).preview_document().is_none() {
            return false;
        }
        if self.preview.read(cx).selected_preview_file_id().is_none() {
            return false;
        }

        let Some(request) = self.preview.update(cx, |preview, _| {
            preview.load_preview_range_request(range, direction)
        }) else {
            return false;
        };
        perf::record_preview_range_request();
        self.preview.update(cx, |preview, _| {
            preview.set_preview_rx(Some(start_preview(request)));
        });
        self.ensure_background_polling(cx);
        true
    }

    pub(super) fn load_preview(&mut self, file_id: u32, cx: &mut Context<Self>) {
        let Some(entry) = self
            .result
            .read(cx)
            .state()
            .result
            .as_ref()
            .and_then(|result| {
                result
                    .preview_files
                    .iter()
                    .find(|entry| entry.id == file_id)
            })
        else {
            return;
        };
        self.load_preview_path(file_id, entry.preview_blob_path.clone(), cx);
    }

    fn request_queued_preview_range(&mut self, cx: &mut Context<Self>) {
        let queued = self.preview.update(cx, |preview, _| {
            if preview.state().preview_requested_range.is_some() {
                return None;
            }
            preview.take_queued_preview_range()
        });
        if let Some(range) = queued {
            let _ = self.request_preview_range(
                range,
                crate::ui::preview_model::PreviewScrollDirection::Down,
                cx,
            );
        }
    }
}
