use std::ops::Range;
use std::rc::Rc;

use gpui::Context;
use gpui_component::notification::NotificationType;

use super::view::TreeExpansionMode;
use super::{Workspace, model};
use crate::application::task::TaskPayload;
use crate::domain::ProcessResult;
use crate::services::preflight::{PreflightEvent, PreflightRequest};
use crate::services::preview::{
    EXCERPT_PREVIEW_BYTES, PreviewEvent, PreviewRequest, create_excerpt_preview,
};
use crate::ui::models::ProcessEventEffect;
use crate::ui::perf;
use crate::ui::preview_model::PreviewEventEffect;
use crate::ui::view_model::ResultTab;

impl Workspace {
    pub(super) fn sync_tree_selection_for_preview_file(
        &mut self,
        file_id: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(node_id) =
            model::preview_file_node_id(self.store.read(cx).result().result.as_ref(), file_id)
        else {
            return false;
        };

        let mut tree_state = self.tree_state(cx);
        let selected_changed = tree_state.selected_node_id.as_deref() != Some(node_id.as_str());
        tree_state.selected_node_id = Some(node_id.clone());

        let mut expansion_changed = false;
        for ancestor in model::ancestor_node_ids(&node_id) {
            expansion_changed |= tree_state.expanded_ids.insert(ancestor);
        }

        if selected_changed || expansion_changed {
            self.set_tree_state(tree_state, cx);
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
        let preview_state = self.store.read(cx).preview();
        if preview_state.selected_preview_file_id == Some(file_id)
            && (preview_state.preview_document.is_some()
                || preview_state.pending_request_type.is_some())
        {
            return;
        }

        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Preview(
                crate::application::store::PreviewAction::Open {
                    file_id,
                    path: preview_path,
                },
            ),
            cx,
        );
    }

    pub(super) fn start_preview_request(
        &mut self,
        request: PreviewRequest,
        cx: &mut Context<Self>,
    ) {
        let mut stream = match self.coordinator.read(cx).start_preview(request) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = self.dispatch(
                    crate::application::store::WorkspaceAction::Preview(
                        crate::application::store::PreviewAction::SetError(error.to_string()),
                    ),
                    cx,
                );
                return;
            }
        };
        let preview_pane = self.preview_pane_view.clone();
        cx.defer(move |cx| {
            preview_pane.update(cx, |view, _| view.scroll_to_top());
        });
        cx.spawn(async move |this, cx| {
            while let Some(envelope) = stream.events.recv().await {
                match envelope.payload {
                    TaskPayload::Event(event) => {
                        let _ = this.update(cx, |workspace, cx| {
                            workspace.apply_preview_events(vec![event], cx);
                        });
                    }
                    TaskPayload::Failed(error) => {
                        let _ = this.update(cx, |workspace, cx| {
                            let _ = workspace.dispatch(
                                crate::application::store::WorkspaceAction::Preview(
                                    crate::application::store::PreviewAction::SetError(
                                        error.to_string(),
                                    ),
                                ),
                                cx,
                            );
                        });
                    }
                    TaskPayload::Finished => break,
                }
            }
        })
        .detach();
    }

    pub(super) fn load_merged_content_preview(&mut self, cx: &mut Context<Self>) {
        let merged_content_path = self
            .store
            .read(cx)
            .result()
            .result
            .as_ref()
            .and_then(|result| result.merged_content_path.clone());
        let Some(merged_content_path) = merged_content_path else {
            return;
        };

        let preview_state = self.store.read(cx).preview();
        if preview_state.selected_preview_file_id == Some(super::MERGED_CONTENT_PREVIEW_FILE_ID)
            && (preview_state.preview_document.is_some()
                || preview_state.pending_request_type.is_some()
                || preview_state
                    .deferred_preview
                    .as_ref()
                    .is_some_and(|state| state.source_path == merged_content_path))
        {
            return;
        }

        match std::fs::metadata(&merged_content_path) {
            Ok(metadata) if metadata.len() > super::MERGED_CONTENT_AUTO_PREVIEW_MAX_BYTES => {
                let _ = self.dispatch(
                    crate::application::store::WorkspaceAction::Preview(
                        crate::application::store::PreviewAction::Defer {
                            file_id: super::MERGED_CONTENT_PREVIEW_FILE_ID,
                            source_path: merged_content_path.clone(),
                            source_byte_len: metadata.len(),
                            excerpt_byte_len: EXCERPT_PREVIEW_BYTES,
                        },
                    ),
                    cx,
                );
                let preview_pane = self.preview_pane_view.clone();
                cx.defer(move |cx| {
                    preview_pane.update(cx, |view, _| view.scroll_to_top());
                });
                return;
            }
            Err(error) => {
                let language = self.language(cx);
                let _ = self.dispatch(
                    crate::application::store::WorkspaceAction::Preview(
                        crate::application::store::PreviewAction::SetError(format!(
                            "{}: {error}",
                            crate::utils::i18n::tr(language, "merged_content_unavailable")
                        )),
                    ),
                    cx,
                );
                return;
            }
            Ok(_) => {}
        }

        self.load_preview_path(
            super::MERGED_CONTENT_PREVIEW_FILE_ID,
            merged_content_path,
            cx,
        );
    }

    pub(super) fn load_deferred_merged_content_excerpt(&mut self, cx: &mut Context<Self>) {
        let Some((source_path, source_byte_len)) = ({
            let preview = self.store.read(cx).preview_model();
            preview
                .state()
                .pending_request_type
                .is_none()
                .then(|| preview.deferred_preview())
                .flatten()
                .filter(|state| state.excerpt_path.is_none())
                .map(|state| (state.source_path.clone(), state.source_byte_len))
        }) else {
            return;
        };

        match create_excerpt_preview(&source_path, EXCERPT_PREVIEW_BYTES) {
            Ok(excerpt_path) => {
                let _ = self.dispatch(
                    crate::application::store::WorkspaceAction::Preview(
                        crate::application::store::PreviewAction::OpenDeferredExcerpt {
                            file_id: super::MERGED_CONTENT_PREVIEW_FILE_ID,
                            source_path: source_path.clone(),
                            source_byte_len,
                            excerpt_byte_len: EXCERPT_PREVIEW_BYTES,
                            excerpt_path,
                        },
                    ),
                    cx,
                );
            }
            Err(error) => {
                let _ = self.dispatch(
                    crate::application::store::WorkspaceAction::Preview(
                        crate::application::store::PreviewAction::SetError(error.to_string()),
                    ),
                    cx,
                );
            }
        }
    }

    pub(super) fn load_deferred_merged_content_full(&mut self, cx: &mut Context<Self>) {
        let Some(source_path) = ({
            let preview = self.store.read(cx).preview_model();
            preview
                .state()
                .pending_request_type
                .is_none()
                .then(|| preview.deferred_preview())
                .flatten()
                .map(|state| state.source_path.clone())
        }) else {
            return;
        };

        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Preview(
                crate::application::store::PreviewAction::OpenDeferredFull {
                    file_id: super::MERGED_CONTENT_PREVIEW_FILE_ID,
                    path: source_path,
                },
            ),
            cx,
        );
    }

    fn apply_preflight_event(&mut self, event: PreflightEvent, cx: &mut Context<Self>) {
        let current_revision = self.store.read(cx).process().preflight_revision;
        let completed_files = match &event {
            PreflightEvent::Completed {
                revision, files, ..
            } if *revision == current_revision
                && self.selection_snapshot(cx).selected_folder.is_some() =>
            {
                Some(files.clone())
            }
            _ => None,
        };
        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Execution(
                crate::application::store::ExecutionAction::Preflight(event),
            ),
            cx,
        );
        if let Some(files) = completed_files {
            let data = model::build_preflight_tree_panel_data(
                files.as_ref(),
                self.store.read(cx).result().result.as_ref(),
            );
            let initialize_expansion = !self.tree_panel.input_exclusion_enabled;
            self.tree_panel.data = Some(data);
            self.tree_panel.input_exclusion_enabled = true;
            self.tree_panel.projection = model::TreeProjectionState::default();
            self.tree_panel.render_state = model::TreeRenderState::default();
            self.tree_panel.total_summary = model::TreeCountSummary::default();
            self.tree_panel.last_filter.clear();
            if initialize_expansion {
                let mut tree_state = crate::ui::state::TreePanelState::default();
                if let Some(data) = self.tree_panel.data.as_ref() {
                    tree_state.expanded_ids = data.index.default_expanded_ids.clone();
                }
                self.set_tree_state(tree_state, cx);
            }
            self.tree_pane_view.update(cx, |view, _| {
                view.view_mode = super::TreeViewMode::Tree;
            });
            self.sync_tree(cx);
        }
    }

    pub(super) fn apply_process_event(
        &mut self,
        event: crate::services::process::ProcessEvent,
        cx: &mut Context<Self>,
    ) -> (bool, bool) {
        self.apply_process_events(vec![event], cx)
    }

    pub(super) fn apply_process_events(
        &mut self,
        events: Vec<crate::services::process::ProcessEvent>,
        cx: &mut Context<Self>,
    ) -> (bool, bool) {
        if events.is_empty() {
            return (false, false);
        }
        perf::record_progress_batch(events.len());
        let language = self.language(cx);
        let transition = self.dispatch(
            crate::application::store::WorkspaceAction::Execution(
                crate::application::store::ExecutionAction::ProcessBatch(events),
            ),
            cx,
        );
        let crate::application::store::ActionOutput::Process(effect) = transition.output else {
            return (false, false);
        };
        match effect {
            ProcessEventEffect::Ignored | ProcessEventEffect::Continue => (false, false),
            ProcessEventEffect::Completed(result) => {
                self.install_result_views(result.as_ref(), cx);
                let process = self.store.read(cx).process();
                let completed = process.processing_completed;
                let succeeded = process.processing_succeeded;
                let failed = completed.saturating_sub(succeeded);
                if failed == 0 {
                    Self::notify_active_window(
                        cx,
                        NotificationType::Success,
                        crate::utils::i18n::tr(language, "process_completed_notice"),
                    );
                } else {
                    Self::notify_active_window(
                        cx,
                        NotificationType::Warning,
                        format!(
                            "{} {failed}",
                            crate::utils::i18n::tr(language, "process_completed_with_failures")
                        ),
                    );
                }
                (true, true)
            }
            ProcessEventEffect::Cancelled => {
                Self::notify_active_window(
                    cx,
                    NotificationType::Warning,
                    crate::utils::i18n::tr(language, "cancelled"),
                );
                (false, true)
            }
            ProcessEventEffect::Failed => {
                let error = self
                    .store
                    .read(cx)
                    .process()
                    .last_error
                    .clone()
                    .unwrap_or_else(|| {
                        crate::utils::i18n::tr(language, "status_error_hint").to_string()
                    });
                Self::notify_active_window(cx, NotificationType::Error, error);
                (false, true)
            }
        }
    }

    pub(super) fn handle_process_event(
        &mut self,
        event: crate::services::process::ProcessEvent,
        cx: &mut Context<Self>,
    ) {
        let _ = self.apply_process_event(event, cx);
    }

    pub(super) fn handle_process_events(
        &mut self,
        events: Vec<crate::services::process::ProcessEvent>,
        cx: &mut Context<Self>,
    ) {
        let _ = self.apply_process_events(events, cx);
    }

    fn apply_preview_events(&mut self, events: Vec<PreviewEvent>, cx: &mut Context<Self>) {
        let transition = self.dispatch(
            crate::application::store::WorkspaceAction::Preview(
                crate::application::store::PreviewAction::ApplyMany(events),
            ),
            cx,
        );
        let crate::application::store::ActionOutput::Preview(effect) = transition.output else {
            return;
        };
        if matches!(effect, PreviewEventEffect::ScrollTop) {
            self.preview_pane_view.update(cx, |view, _| {
                view.scroll_to_top();
            });
        }
        self.request_queued_preview_range(cx);
    }

    #[cfg(test)]
    pub(super) fn set_result(&mut self, result: ProcessResult, cx: &mut Context<Self>) {
        let view_result = result.clone();
        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Execution(
                crate::application::store::ExecutionAction::SetResult(result),
            ),
            cx,
        );
        self.install_result_views(&view_result, cx);
    }

    fn install_result_views(&mut self, result: &ProcessResult, cx: &mut Context<Self>) {
        self.cleanup_current_result_artifacts();
        self.preview_table_cache = super::PreviewTableCache::default();
        self.result_artifacts = super::ResultArtifacts {
            process_dir: result.process_dir.clone(),
            merged_content_path: result.merged_content_path.clone(),
            preview_blob_dir: result.preview_blob_dir.clone(),
        };
        self.tree_panel.data = model::build_tree_panel_data(Some(result));
        self.tree_panel.input_exclusion_enabled = false;
        self.tree_panel.projection = model::TreeProjectionState::default();
        self.tree_panel.last_interaction = None;
        self.tree_panel.render_state = model::TreeRenderState::default();
        self.tree_panel.total_summary = model::TreeCountSummary::default();
        self.tree_panel.last_filter.clear();
        let mut tree_state = crate::ui::state::TreePanelState::default();
        if let Some(data) = self.tree_panel.data.as_ref() {
            tree_state.expanded_ids = data.index.default_expanded_ids.clone();
        }
        self.set_tree_state(tree_state, cx);
        self.sync_tree(cx);
        self.sync_preview_table(cx);
    }

    pub(super) fn sync_tree(&mut self, cx: &mut Context<Self>) {
        self.sync_tree_with_mode(TreeExpansionMode::Default, cx);
    }

    pub(super) fn sync_tree_with_mode(&mut self, mode: TreeExpansionMode, cx: &mut Context<Self>) {
        perf::record_tree_sync();
        let mut tree_state = self.tree_state(cx);
        if let Some(data) = self.tree_panel.data.as_ref() {
            match mode {
                TreeExpansionMode::Default => {}
                TreeExpansionMode::ExpandAll => {
                    tree_state.expanded_ids = data.index.folder_ids.clone();
                }
                TreeExpansionMode::CollapseAll => {
                    tree_state.expanded_ids.clear();
                }
            }
        }
        self.set_tree_state(tree_state.clone(), cx);

        let filter = self
            .tree_panel
            .filter_input
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        let filter_changed = self.tree_panel.last_filter != filter;
        if filter_changed || self.tree_panel.projection.roots.is_empty() {
            self.tree_panel.projection = if self.tree_panel.input_exclusion_enabled {
                let excluded_files = self.selection_snapshot(cx).excluded_folder_files;
                model::build_tree_projection_with_exclusions(
                    self.tree_panel.data.as_ref(),
                    filter.as_str(),
                    &excluded_files,
                )
            } else {
                model::build_tree_projection(self.tree_panel.data.as_ref(), filter.as_str())
            };
            self.tree_panel.total_summary = self.tree_panel.projection.total_summary;
            self.tree_panel.last_filter = filter.clone();
        }
        let render_state = model::build_tree_render_state(
            &self.tree_panel.projection,
            !filter.is_empty(),
            &tree_state.expanded_ids,
            tree_state.selected_node_id.as_deref(),
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
        let current_selected_id = self
            .store
            .read(cx)
            .preview_model()
            .selected_preview_file_id();
        let sort = self.preview_table.read(cx).delegate().sort;
        let has_merged_content = self
            .store
            .read(cx)
            .result()
            .result
            .as_ref()
            .is_some_and(|result| result.merged_content_path.is_some());
        let result_key = self.store.read(cx).result().result_revision;
        let table_model = if self.preview_table_cache.filter == filter
            && self.preview_table_cache.result_key == result_key
            && self.preview_table_cache.current_selected_id == current_selected_id
            && self.preview_table_cache.sort == sort
        {
            self.preview_table_cache.model.clone().unwrap_or_else(|| {
                model::build_preview_table_model(
                    self.store.read(cx).result().result.as_ref(),
                    filter.as_str(),
                    current_selected_id,
                    sort,
                )
            })
        } else {
            let model = model::build_preview_table_model(
                self.store.read(cx).result().result.as_ref(),
                filter.as_str(),
                current_selected_id,
                sort,
            );
            self.preview_table_cache.filter = filter.clone();
            self.preview_table_cache.result_key = result_key;
            self.preview_table_cache.current_selected_id = current_selected_id;
            self.preview_table_cache.sort = sort;
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

        let preview_row_count = table_model.rows.len();
        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Execution(
                crate::application::store::ExecutionAction::SetPreviewRowCount(preview_row_count),
            ),
            cx,
        );
        self.suppress_preview_table_events = true;
        self.preview_table.update(cx, |table, cx| {
            let prev_rows = table.delegate().rows.clone();
            table.delegate_mut().rows = table_model.rows.clone();
            if let Some(row_ix) = target_row_ix {
                if table.selected_row() != Some(row_ix) {
                    table.set_selected_row(row_ix, cx);
                }
            } else if table.selected_row().is_some() {
                table.clear_selection(cx);
            }
            if !Rc::ptr_eq(&prev_rows, &table.delegate().rows) {
                cx.notify();
            }
        });
        self.suppress_preview_table_events = false;
    }

    pub(super) fn refresh_preflight(&mut self, cx: &mut Context<Self>) {
        self.refresh_preflight_internal(false, cx);
    }

    pub(super) fn refresh_preflight_internal(
        &mut self,
        preserve_completed_status: bool,
        cx: &mut Context<Self>,
    ) {
        self.refresh_selected_folder_gitignore_rules(cx);
        let settings = self.settings_snapshot(cx);
        let selection = self.selection_snapshot(cx);
        let effective_filters = self.effective_filters(cx);
        let transition = self.dispatch(
            crate::application::store::WorkspaceAction::Execution(
                crate::application::store::ExecutionAction::BeginPreflight {
                    preserve_completed_status,
                },
            ),
            cx,
        );
        let crate::application::store::ActionOutput::PreflightRevision(revision) =
            transition.output
        else {
            return;
        };
        let request = PreflightRequest {
            revision,
            selected_folder: selection.selected_folder.clone(),
            selected_files: selection
                .selected_files
                .iter()
                .map(|f| f.path.clone())
                .collect(),
            folder_blacklist: effective_filters.folder_blacklist,
            ext_blacklist: effective_filters.ext_blacklist,
            excluded_files: effective_filters.excluded_files,
            folder_whitelist: effective_filters.folder_whitelist,
            ext_whitelist: effective_filters.ext_whitelist,
            whitelist_mode: effective_filters.whitelist_mode,
        };
        let options = crate::processor::walker::WalkerOptions {
            use_gitignore: settings.options.use_gitignore,
            ignore_git: settings.options.ignore_git,
        };
        let mut stream = match self.coordinator.read(cx).start_preflight(request, options) {
            Ok(stream) => stream,
            Err(error) => {
                self.apply_preflight_event(PreflightEvent::Failed { revision, error }, cx);
                return;
            }
        };
        cx.spawn(async move |this, cx| {
            while let Some(envelope) = stream.events.recv().await {
                match envelope.payload {
                    TaskPayload::Event(event) => {
                        let _ = this.update(cx, |workspace, cx| {
                            workspace.apply_preflight_event(event, cx);
                        });
                    }
                    TaskPayload::Failed(error) => {
                        let _ = this.update(cx, |workspace, cx| {
                            workspace.apply_preflight_event(
                                PreflightEvent::Failed { revision, error },
                                cx,
                            );
                        });
                    }
                    TaskPayload::Finished => break,
                }
            }
        })
        .detach();
    }

    pub(super) fn clear_preview_state(&mut self, cx: &mut Context<Self>) {
        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Preview(
                crate::application::store::PreviewAction::Clear,
            ),
            cx,
        );
    }

    pub(super) fn sync_tree_interaction(&mut self, cx: &mut Context<Self>) -> bool {
        if self.suppress_tree_interaction_sync.get() {
            return false;
        }
        let next = self.current_tree_interaction_snapshot(cx);
        let mut tree_state = self.tree_state(cx);
        let effect = model::apply_tree_interaction(
            &mut tree_state,
            self.tree_panel.last_interaction.as_ref(),
            next.clone(),
        );
        self.set_tree_state(tree_state, cx);
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
                let _ = self.dispatch(
                    crate::application::store::WorkspaceAction::Execution(
                        crate::application::store::ExecutionAction::SetResultTab(
                            ResultTab::Content,
                        ),
                    ),
                    cx,
                );
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
        if self
            .store
            .read(cx)
            .preview_model()
            .preview_document()
            .is_none()
        {
            return false;
        }
        if self
            .store
            .read(cx)
            .preview_model()
            .selected_preview_file_id()
            .is_none()
        {
            return false;
        }

        let transition = self.dispatch(
            crate::application::store::WorkspaceAction::Preview(
                crate::application::store::PreviewAction::RequestRange { range, direction },
            ),
            cx,
        );
        let crate::application::store::ActionOutput::PreviewRequest(Some(_)) = transition.output
        else {
            return false;
        };
        perf::record_preview_range_request();
        true
    }

    pub(super) fn load_preview(&mut self, file_id: u32, cx: &mut Context<Self>) {
        let Some(entry) = self
            .store
            .read(cx)
            .result()
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
        let transition = self.dispatch(
            crate::application::store::WorkspaceAction::Preview(
                crate::application::store::PreviewAction::RequestQueuedRange {
                    direction: crate::ui::preview_model::PreviewScrollDirection::Down,
                },
            ),
            cx,
        );
        if matches!(
            transition.output,
            crate::application::store::ActionOutput::PreviewRequest(Some(_))
        ) {
            perf::record_preview_range_request();
        }
    }
}
