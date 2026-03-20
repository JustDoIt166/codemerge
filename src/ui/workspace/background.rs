use std::ops::Range;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use gpui::{Context, ScrollStrategy};

use super::view::TreeExpansionMode;
use super::{Workspace, model};
use crate::domain::{ProcessResult, ResultTab};
use crate::services::preflight::{PreflightEvent, PreflightRequest};
use crate::services::preview::{PreviewEvent, start as start_preview};
use crate::ui::models::ProcessEventEffect;
use crate::ui::preview_model::PreviewEventEffect;

impl Workspace {
    pub(super) fn poll_background(&mut self, cx: &mut Context<Self>) -> Option<Duration> {
        let mut workspace_dirty = false;
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
            let (event_workspace_dirty, event_finished) = self.apply_process_event(event, cx);
            workspace_dirty = event_workspace_dirty || workspace_dirty;
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
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        received_events = true;
                        workspace_dirty = self.apply_preview_event(event, cx) || workspace_dirty;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        keep = false;
                        self.preview.update(cx, |preview, preview_cx| {
                            preview.clear_request();
                            preview_cx.notify();
                        });
                        break;
                    }
                }
            }
            if keep {
                self.preview
                    .update(cx, |preview, _| preview.set_preview_rx(Some(rx)));
            }
        }

        if workspace_dirty {
            cx.notify();
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
            process.apply_preflight_event(event, language);
            process_cx.notify();
        });
    }

    fn apply_process_event(
        &mut self,
        event: crate::services::process::ProcessEvent,
        cx: &mut Context<Self>,
    ) -> (bool, bool) {
        let language = self.language(cx);
        match self.process.update(cx, |process, process_cx| {
            process_cx.notify();
            process.apply_process_event(event, language)
        }) {
            ProcessEventEffect::Continue => (false, false),
            ProcessEventEffect::Completed(result) => {
                self.set_result(*result, cx);
                (true, true)
            }
            ProcessEventEffect::Finish => (false, true),
        }
    }

    fn apply_preview_event(&mut self, event: PreviewEvent, cx: &mut Context<Self>) -> bool {
        let effect = self.preview.update(cx, |preview, preview_cx| {
            let effect = preview.apply_event(event);
            preview_cx.notify();
            effect
        });
        if matches!(effect, PreviewEventEffect::ScrollTop) {
            self.preview_scroll_handle
                .scroll_to_item_strict(0, ScrollStrategy::Top);
        }
        !matches!(effect, PreviewEventEffect::Ignored)
    }

    fn set_result(&mut self, result: ProcessResult, cx: &mut Context<Self>) {
        self.cleanup_current_result_artifacts();
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
            self.result.read(cx).state().result.as_ref(),
            filter.as_str(),
            self.preview.read(cx).selected_preview_file_id(),
        );
        let preview_rows = table_model.rows.clone();
        self.result.update(cx, |result, result_cx| {
            result.set_preview_rows(preview_rows);
            result_cx.notify();
        });
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
            None => self.clear_preview_state(cx),
        }
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
            preview.clear();
            preview_cx.notify();
        });
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
                self.result.update(cx, |result, result_cx| {
                    result.set_active_tab(ResultTab::Content);
                    result_cx.notify();
                });
                self.load_preview(file_id, cx);
                true
            }
        }
    }

    fn preview_request_range(&self, range: Range<usize>, line_count: usize) -> Range<usize> {
        if line_count == 0 {
            return 0..0;
        }
        let bucket = crate::ui::state::PreviewPanelState::VISIBLE_BUCKET_LINES;
        let start = range.start.min(line_count.saturating_sub(1));
        let end = range.end.max(start + 1).min(line_count);
        let bucket_start = (start / bucket) * bucket;
        let bucket_end = end
            .saturating_sub(1)
            .checked_div(bucket)
            .map(|ix| (ix + 1) * bucket)
            .unwrap_or(bucket)
            .min(line_count);

        bucket_start.saturating_sub(bucket)..(bucket_end + bucket * 2).min(line_count)
    }

    fn request_preview_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> bool {
        if self.preview.read(cx).preview_document().is_none() {
            return false;
        }
        if self.preview.read(cx).selected_preview_file_id().is_none() {
            return false;
        }

        let Some(request) = self.preview.update(cx, |preview, preview_cx| {
            let request = preview.load_preview_range_request(range);
            if request.is_some() {
                preview_cx.notify();
            }
            request
        }) else {
            return false;
        };
        self.preview.update(cx, |preview, _| {
            preview.set_preview_rx(Some(start_preview(request)));
        });
        self.ensure_background_polling(cx);
        true
    }

    pub(super) fn sync_preview_visible_range(
        &mut self,
        visible: Range<usize>,
        cx: &mut Context<Self>,
    ) -> bool {
        let preview_state = self.preview.read(cx).state();
        let Some(document) = &preview_state.preview_document else {
            return false;
        };
        let line_count = document.line_count();
        if line_count == 0 {
            return false;
        }

        let start = visible.start.min(line_count.saturating_sub(1));
        let end = visible.end.max(start + 1).min(line_count);
        let visible = start..end;
        let request_range = self.preview_request_range(visible.clone(), line_count);
        let already_loaded = preview_state.has_loaded_range(&request_range);
        let changed = self.preview.update(cx, |preview, _| {
            preview
                .state_mut()
                .update_visible_range(visible.clone(), line_count)
        });

        if !changed || already_loaded {
            return false;
        }

        self.request_preview_range(visible, cx)
    }

    pub(super) fn load_preview(&mut self, file_id: u32, cx: &mut Context<Self>) {
        let preview_state = self.preview.read(cx).state();
        if preview_state.selected_preview_file_id == Some(file_id)
            && (preview_state.preview_document.is_some() || preview_state.preview_rx.is_some())
        {
            return;
        }
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
        let preview_blob_path = entry.preview_blob_path.clone();
        let request = self.preview.update(cx, |preview, preview_cx| {
            let request = preview.open_preview(file_id, preview_blob_path);
            preview_cx.notify();
            request
        });
        self.preview.update(cx, |preview, _| {
            preview.set_preview_rx(Some(start_preview(request)));
        });
        self.preview_scroll_handle
            .scroll_to_item_strict(0, ScrollStrategy::Top);
        self.ensure_background_polling(cx);
        cx.notify();
    }
}
