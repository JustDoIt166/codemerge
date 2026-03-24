mod actions;
mod background;
mod chrome;
mod model;
mod panels;
mod tree_palette;
mod view;

use std::cell::Cell;
use std::rc::Rc;
use std::{hash::Hash, hash::Hasher, ops::Range};

use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, Entity, EntityId, FocusHandle, Focusable,
    InteractiveElement, ParentElement, Pixels, Render, SharedString, Styled, Subscription, Task,
    Timer, UniformListScrollHandle, Window, div, prelude::FluentBuilder as _, px, size,
};
use gpui_component::{
    ActiveTheme as _, Sizable, Size, WindowExt as _, h_flex,
    input::InputState,
    notification::NotificationType,
    table::{Column, TableDelegate, TableState},
    tag::{Tag, TagVariant},
    tree::TreeState,
};

use crate::domain::{Language, PreviewRowViewModel};
use crate::services::settings::{self, ConfigLoadIssue};
use crate::ui::models::{ProcessModel, SettingsModel, WorkspaceUiModel};
use crate::ui::perf;
use crate::ui::preview_model::PreviewModel;
use crate::ui::result_model::ResultModel;
use crate::ui::selection_model::SelectionModel;
use crate::ui::state::{AppState, ProcessUiStatus, WorkspaceUiState};
use crate::utils::i18n::tr;

const MERGED_CONTENT_PREVIEW_FILE_ID: u32 = u32::MAX;

pub(super) fn preview_line_height() -> Pixels {
    px(22.)
}

pub(super) fn workspace_panel_min_height(is_narrow: bool) -> Pixels {
    if is_narrow { px(900.) } else { px(720.) }
}

pub(super) fn fixed_list_sizes(len: usize, height: Pixels) -> Rc<Vec<gpui::Size<Pixels>>> {
    Rc::new((0..len).map(|_| size(px(100.), height)).collect::<Vec<_>>())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlacklistItemKind {
    Folder,
    Ext,
}

pub(super) struct PreviewTableDelegate {
    language: Language,
    columns: Vec<Column>,
    rows: Vec<PreviewRowViewModel>,
}

impl PreviewTableDelegate {
    fn new(language: Language) -> Self {
        Self {
            language,
            columns: vec![
                Column::new("path", tr(language, "table_path")).width(420.),
                Column::new("chars", tr(language, "table_chars"))
                    .width(100.)
                    .text_right(),
                Column::new("tokens", tr(language, "table_tokens"))
                    .width(100.)
                    .text_right(),
            ],
            rows: Vec::new(),
        }
    }

    fn set_language(&mut self, language: Language) {
        self.language = language;
        self.columns[0].name = tr(language, "table_path").into();
        self.columns[1].name = tr(language, "table_chars").into();
        self.columns[2].name = tr(language, "table_tokens").into();
    }
}

impl TableDelegate for PreviewTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> &Column {
        &self.columns[col_ix]
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        self.columns[col_ix].name.clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        match col_ix {
            0 => h_flex()
                .gap_2()
                .items_center()
                .min_w(px(0.))
                .when(row.archive.is_some(), |this| {
                    this.child(
                        Tag::new()
                            .with_variant(TagVariant::Custom {
                                color: cx.theme().primary.opacity(0.14),
                                foreground: cx.theme().foreground,
                                border: cx.theme().primary.opacity(0.32),
                            })
                            .with_size(Size::Small)
                            .rounded_full()
                            .child(tr(self.language, "archive_badge")),
                    )
                })
                .child(
                    div()
                        .min_w(px(0.))
                        .truncate()
                        .child(SharedString::from(row.display_path.clone())),
                )
                .into_any_element(),
            1 => row.chars.to_string().into_any_element(),
            _ => row.tokens.to_string().into_any_element(),
        }
    }
}

use gpui::IntoElement;

pub(super) struct TreePanelController {
    state: Entity<TreeState>,
    filter_input: Entity<InputState>,
    data: Option<model::TreePanelData>,
    projection: model::TreeProjectionState,
    render_state: model::TreeRenderState,
    total_summary: model::TreeCountSummary,
    last_filter: String,
    last_interaction: Option<model::TreeInteractionSnapshot>,
}

struct RulesPanelController {
    revision: u64,
    cache: RulesPanelCache,
}

struct RulesPanelCache {
    revision: u64,
    language: Language,
    filter: String,
    sections: Rc<Vec<model::BlacklistSectionViewModel>>,
}

impl Default for RulesPanelCache {
    fn default() -> Self {
        Self {
            revision: u64::MAX,
            language: Language::Zh,
            filter: String::new(),
            sections: Rc::default(),
        }
    }
}

impl TreePaneView {
    fn toggle_view_mode(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.view_mode = match self.view_mode {
            TreeViewMode::Tree => TreeViewMode::PlainText,
            TreeViewMode::PlainText => TreeViewMode::Tree,
        };
        cx.notify();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TreeViewMode {
    #[default]
    Tree,
    PlainText,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum WorkspacePanelKind {
    Right,
    CompactContent,
}

struct WorkspacePanelView {
    workspace: Entity<Workspace>,
    kind: WorkspacePanelKind,
    last_invalidation_key: u64,
    _subscriptions: Vec<Subscription>,
}

struct StatusPanelView {
    workspace: Entity<Workspace>,
    last_invalidation_key: u64,
    _subscriptions: Vec<Subscription>,
}

struct InputPanelView {
    workspace: Entity<Workspace>,
    last_invalidation_key: u64,
    _subscriptions: Vec<Subscription>,
}

struct RulesPanelView {
    workspace: Entity<Workspace>,
    last_invalidation_key: u64,
    _subscriptions: Vec<Subscription>,
}

struct ResultsPanelView {
    workspace: Entity<Workspace>,
    last_invalidation_key: u64,
    _subscriptions: Vec<Subscription>,
}

struct TreePaneView {
    workspace: Entity<Workspace>,
    view_mode: TreeViewMode,
    last_invalidation_key: u64,
    _subscriptions: Vec<Subscription>,
}

struct PreviewPaneView {
    entity_id: EntityId,
    workspace: Entity<Workspace>,
    preview: Entity<PreviewModel>,
    result: Entity<ResultModel>,
    settings: Entity<SettingsModel>,
    scroll_handle: UniformListScrollHandle,
    last_requested_load_range: Range<usize>,
    render_cache_range: Range<usize>,
    render_cache_revision: u64,
    render_cache: Vec<crate::ui::preview_model::PreviewRenderLine>,
    pending_visible_range: Option<Range<usize>>,
    last_synced_visible_range: Option<Range<usize>>,
    scheduled_visible_sync: bool,
    last_scroll_anchor: usize,
    last_invalidation_key: u64,
    _subscriptions: Vec<Subscription>,
}

#[derive(Default)]
struct PreviewTableCache {
    filter: String,
    result_key: u64,
    current_selected_id: Option<u32>,
    model: Option<model::PreviewTableModel>,
}

#[derive(Clone, Default)]
struct ResultArtifacts {
    merged_content_path: Option<std::path::PathBuf>,
    preview_blob_dir: Option<std::path::PathBuf>,
}

pub struct Workspace {
    focus_handle: FocusHandle,
    state: AppState,
    ui: Entity<WorkspaceUiModel>,
    selection: Entity<SelectionModel>,
    settings: Entity<SettingsModel>,
    process: Entity<ProcessModel>,
    result: Entity<ResultModel>,
    preview: Entity<PreviewModel>,
    result_artifacts: ResultArtifacts,
    tree_panel: TreePanelController,
    preview_table: Entity<TableState<PreviewTableDelegate>>,
    preview_filter_input: Entity<InputState>,
    preview_filter_revision: u64,
    preview_filter_task: Option<Task<()>>,
    preview_table_cache: PreviewTableCache,
    suppress_tree_interaction_sync: Rc<Cell<bool>>,
    suppress_preview_table_events: bool,
    blacklist_filter_input: Entity<InputState>,
    blacklist_add_input: Entity<InputState>,
    rules_panel: RulesPanelController,
    input_panel_view: Entity<InputPanelView>,
    status_panel_view: Entity<StatusPanelView>,
    rules_panel_view: Entity<RulesPanelView>,
    results_panel_view: Entity<ResultsPanelView>,
    tree_pane_view: Entity<TreePaneView>,
    preview_pane_view: Entity<PreviewPaneView>,
    right_panel_view: Entity<WorkspacePanelView>,
    compact_content_view: Entity<WorkspacePanelView>,
    poll_task: Option<Task<()>>,
    poll_task_running: bool,
    poll_idle_streak: u8,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config_report = settings::load_report();
        let cfg = config_report.config;
        let tree_filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr(cfg.language, "tree_filter")));
        let preview_filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr(cfg.language, "file_filter")));
        let blacklist_filter_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr(cfg.language, "blacklist_filter"))
        });
        let blacklist_add_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr(cfg.language, "blacklist_unified_hint"))
        });
        let tree_state = cx.new(|cx| TreeState::new(cx));
        let preview_table =
            cx.new(|cx| TableState::new(PreviewTableDelegate::new(cfg.language), window, cx));
        let suppress_tree_interaction_sync = Rc::new(Cell::new(false));
        let settings_model = cx.new(|_| SettingsModel::from_config(cfg.clone()));
        let ui_model = cx.new(|_| WorkspaceUiModel::new());
        let selection_model = cx.new(|_| SelectionModel::new());
        let process_model =
            cx.new(|_| ProcessModel::new(tr(cfg.language, "status_ready").to_string()));
        let result_model = cx.new(|_| ResultModel::new());
        let preview_model = cx.new(|_| PreviewModel::new());
        let workspace_entity = cx.entity();
        let input_panel_view = cx.new(|cx| {
            InputPanelView::new(
                workspace_entity.clone(),
                settings_model.clone(),
                ui_model.clone(),
                selection_model.clone(),
                cx,
            )
        });
        let status_panel_view = cx.new(|cx| {
            StatusPanelView::new(
                workspace_entity.clone(),
                process_model.clone(),
                result_model.clone(),
                settings_model.clone(),
                cx,
            )
        });
        let rules_panel_view = cx.new(|cx| {
            RulesPanelView::new(
                workspace_entity.clone(),
                settings_model.clone(),
                ui_model.clone(),
                blacklist_filter_input.clone(),
                cx,
            )
        });
        let results_panel_view = cx.new(|cx| {
            ResultsPanelView::new(
                workspace_entity.clone(),
                result_model.clone(),
                settings_model.clone(),
                ui_model.clone(),
                preview_filter_input.clone(),
                cx,
            )
        });
        let tree_pane_view = cx.new(|cx| {
            TreePaneView::new(
                workspace_entity.clone(),
                result_model.clone(),
                settings_model.clone(),
                tree_state.clone(),
                tree_filter_input.clone(),
                suppress_tree_interaction_sync.clone(),
                cx,
            )
        });
        let preview_pane_view = cx.new(|cx| {
            PreviewPaneView::new(
                workspace_entity.clone(),
                preview_model.clone(),
                result_model.clone(),
                settings_model.clone(),
                cx,
            )
        });
        let right_panel_view = cx.new(|cx| {
            WorkspacePanelView::new(
                workspace_entity.clone(),
                ui_model.clone(),
                settings_model.clone(),
                WorkspacePanelKind::Right,
                cx,
            )
        });
        let compact_content_view = cx.new(|cx| {
            WorkspacePanelView::new(
                workspace_entity.clone(),
                ui_model.clone(),
                settings_model.clone(),
                WorkspacePanelKind::CompactContent,
                cx,
            )
        });
        let subscriptions = vec![
            cx.subscribe_in(&tree_filter_input, window, Self::on_tree_filter_event),
            cx.subscribe_in(&preview_filter_input, window, Self::on_preview_filter_event),
            cx.subscribe_in(
                &blacklist_filter_input,
                window,
                Self::on_blacklist_filter_event,
            ),
            cx.subscribe_in(&preview_table, window, Self::on_preview_table_event),
        ];
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            state: AppState::from_config(cfg.clone(), tr(cfg.language, "status_ready").to_string()),
            ui: ui_model,
            selection: selection_model,
            settings: settings_model,
            process: process_model,
            result: result_model,
            preview: preview_model,
            result_artifacts: ResultArtifacts::default(),
            tree_panel: TreePanelController {
                state: tree_state,
                filter_input: tree_filter_input,
                data: None,
                projection: model::TreeProjectionState::default(),
                render_state: model::TreeRenderState::default(),
                total_summary: model::TreeCountSummary::default(),
                last_filter: String::new(),
                last_interaction: None,
            },
            preview_table,
            preview_filter_input,
            preview_filter_revision: 0,
            preview_filter_task: None,
            preview_table_cache: PreviewTableCache::default(),
            suppress_tree_interaction_sync,
            suppress_preview_table_events: false,
            blacklist_filter_input,
            blacklist_add_input,
            rules_panel: RulesPanelController {
                revision: 0,
                cache: RulesPanelCache::default(),
            },
            input_panel_view,
            status_panel_view,
            rules_panel_view,
            results_panel_view,
            tree_pane_view,
            preview_pane_view,
            right_panel_view,
            compact_content_view,
            poll_task: None,
            poll_task_running: false,
            poll_idle_streak: 0,
            _subscriptions: subscriptions,
        };
        if matches!(
            config_report.issue,
            Some(ConfigLoadIssue::ParseFailed(_)) | Some(ConfigLoadIssue::ReadFailed(_))
        ) {
            window.push_notification(
                (
                    NotificationType::Warning,
                    SharedString::from(tr(cfg.language, "config_fallback_defaults")),
                ),
                cx,
            );
        }
        this.refresh_preflight(cx);
        this
    }

    fn ensure_background_polling(&mut self, cx: &mut Context<Self>) {
        if self.poll_task_running || !self.needs_background_polling(cx) {
            return;
        }

        self.poll_task_running = true;
        self.poll_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let Some(delay) = this
                    .update(cx, |this, cx| this.poll_background(cx))
                    .ok()
                    .flatten()
                else {
                    break;
                };
                Timer::after(delay).await;
            }
        }));
    }

    fn needs_background_polling(&self, cx: &App) -> bool {
        let process = self.process.read(cx);
        process.state().preflight_rx.is_some()
            || process.state().process_handle.is_some()
            || self.preview.read(cx).state().preview_rx.is_some()
    }

    fn has_inputs(&self, cx: &App) -> bool {
        self.selection.read(cx).has_inputs()
    }

    fn is_processing(&self, cx: &App) -> bool {
        self.process.read(cx).is_processing()
    }

    fn clear_pending_confirmation(&mut self, cx: &mut Context<Self>) -> bool {
        self.ui.update(cx, |ui, ui_cx| {
            let changed = ui.clear_pending_confirmation();
            if changed {
                ui_cx.notify();
            }
            changed
        })
    }

    pub(super) fn ui_state(&self, cx: &App) -> WorkspaceUiState {
        self.ui.read(cx).state()
    }

    pub(super) fn selection_snapshot(&self, cx: &App) -> crate::ui::state::SelectionState {
        self.selection.read(cx).snapshot()
    }

    pub(super) fn settings_snapshot(&self, cx: &App) -> crate::ui::state::SettingsState {
        self.settings.read(cx).snapshot()
    }

    pub(super) fn language(&self, cx: &App) -> Language {
        self.settings.read(cx).language()
    }

    pub(super) fn effective_folder_blacklist(&self, cx: &App) -> Vec<String> {
        let selection = self.selection_snapshot(cx);
        self.settings
            .read(cx)
            .effective_folder_blacklist(&selection)
    }

    pub(super) fn result_has_content(&self, cx: &App) -> bool {
        self.result.read(cx).has_content_result()
    }

    pub(super) fn result_is_tree_only(&self, cx: &App) -> bool {
        self.result.read(cx).is_tree_only_result()
    }

    pub(super) fn invalidate_rules_panel_cache(&mut self) {
        self.rules_panel.revision = self.rules_panel.revision.wrapping_add(1);
    }

    pub(super) fn refresh_rules_panel_cache(&mut self, cx: &Context<Self>) {
        let settings = self.settings.read(cx).snapshot();
        let language = settings.language;
        let filter = self
            .blacklist_filter_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let cache = &mut self.rules_panel.cache;
        if cache.revision == self.rules_panel.revision
            && cache.language == language
            && cache.filter == filter
        {
            return;
        }

        cache.sections = Rc::new(model::build_blacklist_sections(
            &settings.folder_blacklist,
            &settings.ext_blacklist,
            filter.as_str(),
            language,
        ));
        cache.filter = filter;
        cache.language = language;
        cache.revision = self.rules_panel.revision;
    }

    fn cleanup_result_artifacts(artifacts: &ResultArtifacts) {
        if let Some(path) = &artifacts.merged_content_path {
            let _ = std::fs::remove_file(path);
        }
        if let Some(dir) = &artifacts.preview_blob_dir {
            let _ = crate::utils::temp_file::cleanup_preview_dir(dir);
        }
    }

    pub(super) fn cleanup_current_result_artifacts(&mut self) {
        Self::cleanup_result_artifacts(&self.result_artifacts);
        self.result_artifacts = ResultArtifacts::default();
    }

    fn sync_localized_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.settings.read(cx).language();
        self.tree_panel.filter_input.update(cx, |state, cx| {
            state.set_placeholder(tr(language, "tree_filter"), window, cx)
        });
        self.preview_filter_input.update(cx, |state, cx| {
            state.set_placeholder(tr(language, "file_filter"), window, cx)
        });
        self.blacklist_filter_input.update(cx, |state, cx| {
            state.set_placeholder(tr(language, "blacklist_filter"), window, cx)
        });
        self.blacklist_add_input.update(cx, |state, cx| {
            state.set_placeholder(tr(language, "blacklist_unified_hint"), window, cx)
        });
        self.preview_table.update(cx, |table, cx| {
            table.delegate_mut().set_language(language);
            cx.notify();
        });
        if !self.is_processing(cx)
            && self.process.read(cx).state().ui_status == ProcessUiStatus::Idle
        {
            self.process.update(cx, |process, process_cx| {
                process.state_mut().processing_current_file =
                    tr(language, "status_ready").to_string();
                process_cx.notify();
            });
        }
    }

    fn schedule_preview_table_sync(&mut self, cx: &mut Context<Self>) {
        self.preview_filter_revision = self.preview_filter_revision.wrapping_add(1);
        let revision = self.preview_filter_revision;
        self.preview_filter_task = Some(cx.spawn(async move |this, cx| {
            Timer::after(std::time::Duration::from_millis(50)).await;
            let _ = this.update(cx, |workspace, cx| {
                if workspace.preview_filter_revision != revision {
                    return;
                }
                workspace.preview_filter_task = None;
                workspace.sync_preview_table(cx);
            });
        }));
    }

    fn hash_value<T: Hash>(value: &T) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    fn input_panel_invalidation_key(&self, cx: &App) -> u64 {
        let settings = self.settings.read(cx).snapshot();
        let selection = self.selection.read(cx).snapshot();
        let ui = self.ui.read(cx).state();
        Self::hash_value(&(
            settings.language,
            settings.options,
            settings.folder_blacklist,
            settings.ext_blacklist,
            selection.dedupe_exact_path,
            selection.selected_folder,
            selection.selected_files,
            selection.gitignore_file,
            selection.gitignore_rules,
            ui.selected_files_panel_height,
            ui.pending_confirmation,
        ))
    }

    fn status_panel_invalidation_key(&self, cx: &App) -> u64 {
        let process = self.process.read(cx).state();
        let result = self.result.read(cx).state();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.settings.read(cx).language().hash(&mut hasher);
        process.ui_status.hash(&mut hasher);
        process.preflight.total_files.hash(&mut hasher);
        process.preflight.skipped_files.hash(&mut hasher);
        process.preflight.to_process_files.hash(&mut hasher);
        process.preflight.scanned_entries.hash(&mut hasher);
        process.preflight.is_scanning.hash(&mut hasher);
        process.processing_records.len().hash(&mut hasher);
        process.processing_scanned.hash(&mut hasher);
        process.processing_candidates.hash(&mut hasher);
        process.processing_skipped.hash(&mut hasher);
        process.processing_current_file.hash(&mut hasher);
        process.last_error.hash(&mut hasher);
        result
            .result
            .as_ref()
            .map(|result| {
                (
                    result.stats.total_chars,
                    result.stats.total_tokens,
                    result.preview_files.len(),
                    model::summarize_archive_entries(Some(result)),
                    result
                        .preview_files
                        .iter()
                        .map(|entry| {
                            (
                                entry.display_path.clone(),
                                entry
                                    .archive
                                    .as_ref()
                                    .map(|archive| archive.archive_path.clone()),
                                entry
                                    .archive
                                    .as_ref()
                                    .map(|archive| archive.entry_path.clone()),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .hash(&mut hasher);
        hasher.finish()
    }

    fn rules_panel_invalidation_key(&self, cx: &App) -> u64 {
        let settings = self.settings.read(cx).snapshot();
        let ui = self.ui.read(cx).state();
        let filter = self.blacklist_filter_input.read(cx).value().to_string();
        Self::hash_value(&(
            settings.language,
            settings.folder_blacklist,
            settings.ext_blacklist,
            ui.pending_confirmation,
            filter,
            self.rules_panel.revision,
        ))
    }

    fn results_panel_invalidation_key(&self, cx: &App) -> u64 {
        let result = self.result.read(cx).state();
        let filter = self.preview_filter_input.read(cx).value().to_string();
        let ui = self.ui.read(cx).state();
        Self::hash_value(&(
            self.settings.read(cx).language(),
            result.active_tab,
            result.preview_rows.clone(),
            result.result.as_ref().map(|result| {
                (
                    result.preview_files.len(),
                    result.merged_content_path.is_some(),
                )
            }),
            ui.content_file_list_collapsed,
            filter,
        ))
    }

    fn tree_pane_invalidation_key(&self, cx: &App) -> u64 {
        let result = self.result.read(cx).state();
        let tree_state = self.tree_panel.state.read(cx);
        Self::hash_value(&(
            self.settings.read(cx).language(),
            self.tree_panel.render_state.structure_signature,
            self.tree_panel.render_state.selected_row_ix,
            self.tree_panel.total_summary,
            self.tree_panel.filter_input.read(cx).value().to_string(),
            result.result.as_ref().map(|result| result.tree_nodes.len()),
            result.result.as_ref().map(|result| {
                result
                    .preview_files
                    .iter()
                    .map(|entry| {
                        (
                            entry.display_path.clone(),
                            entry.archive.is_some(),
                            entry
                                .archive
                                .as_ref()
                                .map(|archive| archive.archive_path.clone()),
                        )
                    })
                    .collect::<Vec<_>>()
            }),
            result
                .result
                .as_ref()
                .map(|result| result.tree_string.as_str()),
            tree_state.selected_index(),
        ))
    }

    fn preview_pane_invalidation_key(&self, cx: &App) -> u64 {
        let preview = self.preview.read(cx);
        let state = preview.state();
        Self::hash_value(&(
            self.settings.read(cx).language(),
            preview.render_revision(),
            state.selected_preview_file_id,
            state.preview_error.as_deref(),
        ))
    }

    fn workspace_panel_invalidation_key(&self, kind: WorkspacePanelKind, cx: &App) -> u64 {
        let ui = self.ui.read(cx).state();
        match kind {
            WorkspacePanelKind::Right => {
                Self::hash_value(&(self.settings.read(cx).language(), ui.side_panel_tab, kind))
            }
            WorkspacePanelKind::CompactContent => Self::hash_value(&(
                self.settings.read(cx).language(),
                ui.narrow_content_tab,
                kind,
            )),
        }
    }
}

impl WorkspacePanelView {
    fn new(
        workspace: Entity<Workspace>,
        ui: Entity<WorkspaceUiModel>,
        settings: Entity<SettingsModel>,
        kind: WorkspacePanelKind,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&ui, |this, _, cx| {
                let key = this
                    .workspace
                    .read(cx)
                    .workspace_panel_invalidation_key(this.kind, cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&settings, |this, _, cx| {
                let key = this
                    .workspace
                    .read(cx)
                    .workspace_panel_invalidation_key(this.kind, cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
        ];
        Self {
            workspace,
            kind,
            last_invalidation_key: 0,
            _subscriptions: subscriptions,
        }
    }

    fn render_workspace_panel(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace
            .update(cx, |workspace, workspace_cx| match self.kind {
                WorkspacePanelKind::Right => workspace
                    .render_right_panel(workspace_cx)
                    .into_any_element(),
                WorkspacePanelKind::CompactContent => workspace
                    .render_compact_content_panel(workspace_cx)
                    .into_any_element(),
            })
    }
}

impl InputPanelView {
    fn new(
        workspace: Entity<Workspace>,
        settings: Entity<SettingsModel>,
        ui: Entity<WorkspaceUiModel>,
        selection: Entity<SelectionModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&settings, |this, _, cx| {
                let key = this.workspace.read(cx).input_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&ui, |this, _, cx| {
                let key = this.workspace.read(cx).input_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&selection, |this, _, cx| {
                let key = this.workspace.read(cx).input_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
        ];
        Self {
            workspace,
            last_invalidation_key: 0,
            _subscriptions: subscriptions,
        }
    }
}

impl StatusPanelView {
    fn new(
        workspace: Entity<Workspace>,
        process: Entity<ProcessModel>,
        result: Entity<ResultModel>,
        settings: Entity<SettingsModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&process, |this, _, cx| {
                let key = this.workspace.read(cx).status_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&result, |this, _, cx| {
                let key = this.workspace.read(cx).status_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&settings, |this, _, cx| {
                let key = this.workspace.read(cx).status_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
        ];
        Self {
            workspace,
            last_invalidation_key: 0,
            _subscriptions: subscriptions,
        }
    }
}

impl RulesPanelView {
    fn new(
        workspace: Entity<Workspace>,
        settings: Entity<SettingsModel>,
        ui: Entity<WorkspaceUiModel>,
        blacklist_filter_input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&settings, |this, _, cx| {
                let key = this.workspace.read(cx).rules_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&ui, |this, _, cx| {
                let key = this.workspace.read(cx).rules_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&blacklist_filter_input, |this, _, cx| {
                let key = this.workspace.read(cx).rules_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
        ];
        Self {
            workspace,
            last_invalidation_key: 0,
            _subscriptions: subscriptions,
        }
    }
}

impl ResultsPanelView {
    fn new(
        workspace: Entity<Workspace>,
        result: Entity<ResultModel>,
        settings: Entity<SettingsModel>,
        ui: Entity<WorkspaceUiModel>,
        preview_filter_input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&result, |this, _, cx| {
                let key = this.workspace.read(cx).results_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&settings, |this, _, cx| {
                let key = this.workspace.read(cx).results_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&ui, |this, _, cx| {
                let key = this.workspace.read(cx).results_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&preview_filter_input, |this, _, cx| {
                let key = this.workspace.read(cx).results_panel_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
        ];
        Self {
            workspace,
            last_invalidation_key: 0,
            _subscriptions: subscriptions,
        }
    }
}

impl TreePaneView {
    fn new(
        workspace: Entity<Workspace>,
        result: Entity<ResultModel>,
        settings: Entity<SettingsModel>,
        tree_state: Entity<TreeState>,
        tree_filter_input: Entity<InputState>,
        suppress_tree_interaction_sync: Rc<Cell<bool>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let tree_workspace = workspace.clone();
        let tree_interaction_guard = suppress_tree_interaction_sync.clone();
        let subscriptions = vec![
            cx.observe(&result, |this, _, cx| {
                let key = this.workspace.read(cx).tree_pane_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&settings, |this, _, cx| {
                let key = this.workspace.read(cx).tree_pane_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&tree_state, move |_, _, cx| {
                if tree_interaction_guard.get() {
                    return;
                }
                tree_workspace.update(cx, |workspace, workspace_cx| {
                    let _ = workspace.sync_tree_interaction(workspace_cx);
                });
            }),
            cx.observe(&tree_filter_input, |this, _, cx| {
                let key = this.workspace.read(cx).tree_pane_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
        ];
        Self {
            workspace,
            view_mode: TreeViewMode::Tree,
            last_invalidation_key: 0,
            _subscriptions: subscriptions,
        }
    }
}

impl PreviewPaneView {
    fn new(
        workspace: Entity<Workspace>,
        preview: Entity<PreviewModel>,
        result: Entity<ResultModel>,
        settings: Entity<SettingsModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&preview, |this, _, cx| {
                let key = this.workspace.read(cx).preview_pane_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&result, |this, _, cx| {
                let key = this.workspace.read(cx).preview_pane_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&settings, |this, _, cx| {
                let key = this.workspace.read(cx).preview_pane_invalidation_key(cx);
                if this.last_invalidation_key != key {
                    this.last_invalidation_key = key;
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
        ];
        Self {
            entity_id: cx.entity().entity_id(),
            workspace,
            preview,
            result,
            settings,
            scroll_handle: UniformListScrollHandle::new(),
            last_requested_load_range: 0..0,
            render_cache_range: 0..0,
            render_cache_revision: 0,
            render_cache: Vec::new(),
            pending_visible_range: None,
            last_synced_visible_range: None,
            scheduled_visible_sync: false,
            last_scroll_anchor: 0,
            last_invalidation_key: 0,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for WorkspacePanelView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_workspace_panel(window, cx)
    }
}

impl Render for InputPanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.workspace.update(cx, |workspace, workspace_cx| {
            workspace
                .render_input_panel(workspace_cx)
                .into_any_element()
        })
    }
}

impl Render for StatusPanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.workspace.update(cx, |workspace, workspace_cx| {
            workspace
                .render_status_panel(workspace_cx)
                .into_any_element()
        })
    }
}

impl Render for RulesPanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.workspace.update(cx, |workspace, workspace_cx| {
            workspace
                .render_rules_panel(workspace_cx)
                .into_any_element()
        })
    }
}

impl Render for ResultsPanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.workspace.update(cx, |workspace, workspace_cx| {
            workspace
                .render_results_panel(workspace_cx)
                .into_any_element()
        })
    }
}

impl Render for TreePaneView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_tree_pane(cx)
    }
}

impl Render for PreviewPaneView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_preview_pane(window, cx)
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gpui_component::v_flex()
            .id("codemerge-root")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(self.render_window_chrome(window, cx))
            .child(
                gpui::div()
                    .flex_1()
                    .min_h(px(0.))
                    .p_4()
                    .child(self.render_main_content(window, cx)),
            )
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        Self::cleanup_result_artifacts(&self.result_artifacts);
    }
}

#[cfg(test)]
mod tests {
    use super::{Workspace, WorkspacePanelKind};
    use crate::domain::{PreviewFileEntry, ProcessResult, TreeNode};
    use crate::processor::stats::ProcessingStats;
    use crate::services::preview::{PreviewEvent, PreviewRequest, index_document};
    use crate::ui::{perf, preview_model::PreviewModel};
    use gpui::{AppContext as _, Entity, TestAppContext, VisualContext as _};
    use gpui_component::tree::TreeState;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    #[gpui::test]
    fn preview_render_cache_handles_large_visible_windows(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!(
            "codemerge_preview_perf_tests_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        let path = root.join("preview.txt");
        fs::write(
            &path,
            (0..10_240)
                .map(|ix| format!("line-{ix}\n"))
                .collect::<String>(),
        )
        .expect("write preview");

        let preview = cx.new(|_| PreviewModel::new());
        preview.update(cx, |preview: &mut PreviewModel, _| {
            let request = preview.open_preview(7, path.clone());
            let revision = match request {
                PreviewRequest::Open { revision, .. } => revision,
                _ => unreachable!(),
            };
            let document = index_document(&path).expect("index document");
            let _ = preview.apply_event(PreviewEvent::Opened {
                revision,
                file_id: 7,
                document,
                loaded_range: 0..512,
                lines: (0..512).map(|ix| format!("line-{ix}")).collect(),
            });
        });

        let start = Instant::now();
        preview.update(cx, |preview: &mut PreviewModel, _| {
            for _ in 0..200 {
                let lines = preview.build_render_lines(128..320);
                assert_eq!(lines.len(), 192);
            }
        });
        assert!(start.elapsed() < Duration::from_millis(200));
        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    fn tree_selection_changes_do_not_rebuild_tree_items(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result(), cx);
            perf::reset();
            workspace.sync_tree(cx);
            let baseline = perf::snapshot();

            workspace
                .tree_panel
                .state
                .update(cx, |state: &mut TreeState, tree_cx| {
                    state.set_selected_index(Some(0), tree_cx);
                });
            let _ = workspace.sync_tree_interaction(cx);

            let after = perf::snapshot();
            assert_eq!(after.tree_set_items, baseline.tree_set_items);
            assert!(after.tree_syncs >= baseline.tree_syncs);
        });
    }

    #[gpui::test]
    fn tree_pane_invalidation_key_tracks_tree_string_changes(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            let mut first_result = sample_result();
            first_result.tree_string = "src/\n  main.rs\n".to_string();
            workspace.set_result(first_result, cx);
            let first_key = workspace.tree_pane_invalidation_key(cx);

            let mut second_result = sample_result();
            second_result.tree_string = "app/\n  lib.rs\n".to_string();
            workspace.set_result(second_result, cx);
            let second_key = workspace.tree_pane_invalidation_key(cx);

            assert_ne!(first_key, second_key);
        });
    }

    #[gpui::test]
    fn results_panel_invalidation_key_tracks_content_file_list_fold(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            let first_key = workspace.results_panel_invalidation_key(cx);
            workspace.ui.update(cx, |ui, ui_cx| {
                let changed = ui.set_content_file_list_collapsed(true);
                assert!(changed);
                ui_cx.notify();
            });
            let second_key = workspace.results_panel_invalidation_key(cx);

            assert_ne!(first_key, second_key);
        });
    }

    #[gpui::test]
    fn input_panel_invalidation_key_tracks_selected_files_panel_height_changes(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            let first_key = workspace.input_panel_invalidation_key(cx);
            workspace.ui.update(cx, |ui, ui_cx| {
                let changed = ui.set_selected_files_panel_height(320);
                assert!(changed);
                ui_cx.notify();
            });
            let second_key = workspace.input_panel_invalidation_key(cx);

            assert_ne!(first_key, second_key);
        });
    }

    #[gpui::test]
    fn workspace_panel_invalidation_key_ignores_selected_files_panel_height_changes(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            let first_key =
                workspace.workspace_panel_invalidation_key(WorkspacePanelKind::Right, cx);
            workspace.ui.update(cx, |ui, ui_cx| {
                let changed = ui.set_selected_files_panel_height(320);
                assert!(changed);
                ui_cx.notify();
            });
            let second_key =
                workspace.workspace_panel_invalidation_key(WorkspacePanelKind::Right, cx);

            assert_eq!(first_key, second_key);
        });
    }

    #[gpui::test]
    fn preview_requested_range_changes_do_not_invalidate_preview_pane(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("preview_notify", 512);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_path(&path), cx);
            seed_preview_model(&workspace.preview, &path, cx, 0..128, 1);
            workspace.preview.update(cx, |preview, _| {
                let _ = preview.load_preview_range_request(
                    256..320,
                    crate::ui::preview_model::PreviewScrollDirection::Down,
                );
            });
            perf::reset();

            workspace.preview.update(cx, |preview, preview_cx| {
                preview.clear_request();
                preview_cx.notify();
            });

            let snapshot = perf::snapshot();
            assert_eq!(snapshot.workspace_view_notifies, 0);
        });

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn preview_visible_range_sync_coalesces_to_latest_window(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("preview_scroll", 1_024);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_path(&path), cx);
            seed_preview_model(&workspace.preview, &path, cx, 0..256, 1);
            perf::reset();

            workspace.preview_pane_view.update(cx, |view, cx| {
                view.queue_visible_range_sync(0..24, cx);
                view.queue_visible_range_sync(128..160, cx);
                assert_eq!(view.pending_visible_range, Some(128..160));
                assert!(view.scheduled_visible_sync);
                view.flush_pending_visible_range(cx);
                assert_eq!(
                    view.render_cache_range,
                    128..192.min(crate::ui::state::PreviewPanelState::RENDER_WINDOW_LINES + 128)
                );
            });

            let snapshot = perf::snapshot();
            assert_eq!(snapshot.preview_visible_syncs, 1);
        });

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn preview_same_visible_range_does_not_reschedule_notify(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("preview_repeat_visible", 1_024);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_path(&path), cx);
            seed_preview_model(&workspace.preview, &path, cx, 0..256, 1);

            workspace.preview_pane_view.update(cx, |view, cx| {
                view.queue_visible_range_sync(128..160, cx);
                assert!(view.scheduled_visible_sync);

                view.flush_pending_visible_range(cx);
                assert_eq!(view.last_synced_visible_range, Some(128..160));
                assert!(!view.scheduled_visible_sync);

                view.queue_visible_range_sync(128..160, cx);
                assert_eq!(view.pending_visible_range, None);
                assert!(!view.scheduled_visible_sync);
            });
        });

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn preview_updates_outside_visible_window_do_not_rebuild_render_cache(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("preview_cache", 1_024);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_path(&path), cx);
            seed_preview_model(&workspace.preview, &path, cx, 0..128, 1);
            workspace.preview_pane_view.update(cx, |view, cx| {
                view.refresh_render_cache(0..64, cx);
            });
            perf::reset();

            workspace.preview.update(cx, |preview, preview_cx| {
                let revision = preview.state().preview_revision;
                let _ = preview.apply_event(PreviewEvent::Loaded {
                    revision,
                    file_id: 1,
                    loaded_range: 384..512,
                    lines: (384..512).map(|ix| format!("line-{ix}")).collect(),
                });
                preview_cx.notify();
            });

            workspace.preview_pane_view.update(cx, |view, cx| {
                view.refresh_render_cache(view.render_cache_range.clone(), cx);
            });

            let snapshot = perf::snapshot();
            assert_eq!(snapshot.preview_render_cache_rebuilds, 0);
            assert!(snapshot.preview_render_cache_partial_updates >= 1);
        });

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn preview_lines_outside_render_cache_fall_back_to_placeholders(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("preview_placeholder_gap", 1_024);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_path(&path), cx);
            seed_preview_model(&workspace.preview, &path, cx, 0..128, 1);

            workspace.preview_pane_view.update(cx, |view, cx| {
                view.refresh_render_cache(0..64, cx);
                let rows = view.render_lines_for(320..340, cx);

                assert_eq!(rows.len(), 20);
                assert!(rows.iter().all(|row| row.missing));
                assert_eq!(rows[0].line_number.to_string(), "321");
            });
        });

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn preview_scroll_perf_reports_batched_metrics(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("preview_perf_scroll", 8_192);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_path(&path), cx);
            seed_preview_model(&workspace.preview, &path, cx, 0..3_072, 1);
            perf::reset();

            let start = Instant::now();
            workspace.preview_pane_view.update(cx, |view, cx| {
                for frame in 0..30usize {
                    for offset in 0..4usize {
                        let start_ix = frame * 16 + offset * 8;
                        view.queue_visible_range_sync(start_ix..start_ix + 24, cx);
                    }
                    view.flush_pending_visible_range(cx);
                }
            });
            let elapsed = start.elapsed();
            let snapshot = perf::snapshot();

            println!(
                "preview_scroll_perf frames=30 events=120 elapsed_ms={} visible_syncs={} range_requests={} cache_rebuilds={} cache_partial_updates={} workspace_notifies={}",
                elapsed.as_millis(),
                snapshot.preview_visible_syncs,
                snapshot.preview_range_requests,
                snapshot.preview_render_cache_rebuilds,
                snapshot.preview_render_cache_partial_updates,
                snapshot.workspace_view_notifies,
            );

            assert!(
                (30..=31).contains(&snapshot.preview_visible_syncs),
                "manual flushes should batch to roughly one visible sync per frame; a deferred trailing notify may add one extra sync"
            );
            assert!(snapshot.preview_range_requests <= snapshot.preview_visible_syncs);
            assert!(snapshot.preview_render_cache_rebuilds <= snapshot.preview_visible_syncs);
            assert!(snapshot.preview_render_cache_partial_updates > 0);
            assert!(elapsed < Duration::from_millis(500));
        });

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn content_tab_loads_merged_result_preview(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("merged_content", 256);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_merged_content_path(&path), cx);
            workspace.load_merged_content_preview(cx);
        });

        let rx = workspace
            .update(cx, |workspace: &mut Workspace, cx| {
                workspace
                    .preview
                    .update(cx, |preview, _| preview.take_preview_rx())
            })
            .expect("preview receiver");
        match rx
            .recv_timeout(Duration::from_secs(1))
            .expect("preview open event")
        {
            PreviewEvent::Opened {
                file_id, document, ..
            } => {
                assert_eq!(file_id, super::MERGED_CONTENT_PREVIEW_FILE_ID);
                assert_eq!(document.path(), path.as_path());
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn merged_content_preview_uses_virtualized_render_cache(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("merged_content_virtualized", 512);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_merged_content_path(&path), cx);
            seed_preview_model(
                &workspace.preview,
                &path,
                cx,
                0..128,
                super::MERGED_CONTENT_PREVIEW_FILE_ID,
            );

            workspace.preview_pane_view.update(cx, |view, cx| {
                view.refresh_render_cache(0..64, cx);
                let rows = view.render_lines_for(0..3, cx);

                assert_eq!(
                    rows.iter()
                        .map(|row| row.text.to_string())
                        .collect::<Vec<_>>(),
                    vec![
                        "line-0".to_string(),
                        "line-1".to_string(),
                        "line-2".to_string(),
                    ]
                );
                assert!(rows.iter().all(|row| !row.missing));
            });
        });

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn merged_content_preview_survives_preview_table_sync(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("merged_content_sync", 128);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_merged_content_path(&path), cx);
            seed_preview_model(
                &workspace.preview,
                &path,
                cx,
                0..64,
                super::MERGED_CONTENT_PREVIEW_FILE_ID,
            );

            workspace.sync_preview_table(cx);

            let preview = workspace.preview.read(cx);
            assert_eq!(
                preview.selected_preview_file_id(),
                Some(super::MERGED_CONTENT_PREVIEW_FILE_ID)
            );
            assert!(preview.preview_document().is_some());
        });

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn preview_filter_change_keeps_current_preview_until_sync(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("preview_filter_preserve", 64);

        cx.update_window_entity(&workspace, |workspace: &mut Workspace, window, cx| {
            workspace.set_result(sample_result_with_second_path(&path), cx);
            seed_preview_model(&workspace.preview, &path, cx, 0..32, 2);

            workspace
                .preview_filter_input
                .update(cx, |input, input_cx| {
                    input.set_value("src", window, input_cx);
                });
            workspace.handle_preview_filter_change(cx);

            let preview = workspace.preview.read(cx);
            assert_eq!(preview.selected_preview_file_id(), Some(2));
            assert!(preview.preview_document().is_some());
        });

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn sync_tree_selection_for_preview_file_updates_tree_state(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let main_path = write_preview_fixture("preview_filter_main", 64);
        let lib_path = write_preview_fixture("preview_filter_lib", 64);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_paths(&main_path, &lib_path), cx);
            workspace.preview.update(cx, |preview, _| {
                preview.set_preview_rx(None);
            });
            workspace.poll_task = None;
            workspace.poll_task_running = false;

            assert!(workspace.sync_tree_selection_for_preview_file(1, cx));
            assert_eq!(
                workspace
                    .state
                    .workspace
                    .tree_panel
                    .selected_node_id
                    .as_deref(),
                Some("src/main.rs")
            );
            assert!(
                workspace
                    .state
                    .workspace
                    .tree_panel
                    .expanded_ids
                    .contains("src")
            );
        });

        let _ = fs::remove_file(main_path);
        let _ = fs::remove_file(lib_path);
    }

    fn sample_result() -> ProcessResult {
        ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: vec![
                TreeNode {
                    id: "src".to_string(),
                    label: "src".to_string(),
                    relative_path: "src".to_string(),
                    is_folder: true,
                    children: vec![
                        TreeNode {
                            id: "src/main.rs".to_string(),
                            label: "main.rs".to_string(),
                            relative_path: "src/main.rs".to_string(),
                            is_folder: false,
                            children: Vec::new(),
                        },
                        TreeNode {
                            id: "src/lib.rs".to_string(),
                            label: "lib.rs".to_string(),
                            relative_path: "src/lib.rs".to_string(),
                            is_folder: false,
                            children: Vec::new(),
                        },
                    ],
                },
                TreeNode {
                    id: "README.md".to_string(),
                    label: "README.md".to_string(),
                    relative_path: "README.md".to_string(),
                    is_folder: false,
                    children: Vec::new(),
                },
            ],
            merged_content_path: None,
            suggested_result_name: "workspace-20260321.txt".to_string(),
            file_details: Vec::new(),
            preview_files: vec![
                PreviewFileEntry {
                    id: 1,
                    display_path: "src/main.rs".to_string(),
                    chars: 10,
                    tokens: 3,
                    preview_blob_path: PathBuf::from("a"),
                    byte_len: 10,
                    archive: None,
                },
                PreviewFileEntry {
                    id: 2,
                    display_path: "src/lib.rs".to_string(),
                    chars: 12,
                    tokens: 4,
                    preview_blob_path: PathBuf::from("b"),
                    byte_len: 12,
                    archive: None,
                },
            ],
            preview_blob_dir: None,
        }
    }

    fn sample_result_with_path(path: &std::path::Path) -> ProcessResult {
        let mut result = sample_result();
        result.preview_files[0].preview_blob_path = path.to_path_buf();
        result
    }

    fn sample_result_with_second_path(path: &std::path::Path) -> ProcessResult {
        let mut result = sample_result();
        result.preview_files[1].preview_blob_path = path.to_path_buf();
        result
    }

    fn sample_result_with_paths(
        main_path: &std::path::Path,
        lib_path: &std::path::Path,
    ) -> ProcessResult {
        let mut result = sample_result();
        result.preview_files[0].preview_blob_path = main_path.to_path_buf();
        result.preview_files[1].preview_blob_path = lib_path.to_path_buf();
        result
    }

    fn sample_result_with_merged_content_path(path: &std::path::Path) -> ProcessResult {
        let mut result = sample_result();
        result.merged_content_path = Some(path.to_path_buf());
        result.preview_files.clear();
        result
    }

    fn seed_preview_model(
        preview: &Entity<PreviewModel>,
        path: &std::path::Path,
        cx: &mut impl gpui::AppContext,
        loaded_range: std::ops::Range<usize>,
        file_id: u32,
    ) {
        preview.update(cx, |preview: &mut PreviewModel, _| {
            let request = preview.open_preview(file_id, path.to_path_buf());
            let revision = match request {
                PreviewRequest::Open { revision, .. } => revision,
                _ => unreachable!(),
            };
            let document = index_document(path).expect("index document");
            let _ = preview.apply_event(PreviewEvent::Opened {
                revision,
                file_id,
                document,
                loaded_range: loaded_range.clone(),
                lines: loaded_range
                    .clone()
                    .map(|ix| format!("line-{ix}"))
                    .collect(),
            });
        });
    }

    fn write_preview_fixture(prefix: &str, line_count: usize) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "{prefix}_{}_{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        fs::write(
            &path,
            (0..line_count)
                .map(|ix| format!("line-{ix}\n"))
                .collect::<String>(),
        )
        .expect("write preview fixture");
        path
    }
}
