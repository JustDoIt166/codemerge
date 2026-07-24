mod actions;
mod background;
mod chrome;
mod design_tokens;
mod model;
mod panels;
mod sheets;
mod tree_palette;
mod view;

gpui::actions!(
    workspace,
    [
        StartProcess,
        CancelProcess,
        FocusSearch,
        CopyActiveResult,
        ExportResult,
        OpenRules,
        CloseWorkspaceSurface
    ]
);

use std::cell::Cell;
use std::ops::{Deref, DerefMut, Range};
use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, Entity, EntityId, FocusHandle, Focusable,
    InteractiveElement, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Timer, UniformListScrollHandle,
    Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable, Size, VirtualListScrollHandle, WindowExt as _,
    h_flex,
    input::InputState,
    notification::NotificationType,
    table::{Column, TableDelegate, TableState},
    tag::{Tag, TagVariant},
    tree::TreeState,
};

use crate::application::coordinator::WorkspaceCoordinator;
use crate::application::store::{
    ChangeSet, NavigationAction, Transition, WorkspaceAction, WorkspaceEffect, WorkspaceStore,
};
use crate::domain::Language;
use crate::services::settings::{self, ConfigLoadIssue};
use crate::ui::perf;
use crate::ui::state::{ProcessUiStatus, TreePanelState, WorkspaceUiState};
use crate::ui::view_model::PreviewRowViewModel;
use crate::utils::i18n::tr;

const MERGED_CONTENT_PREVIEW_FILE_ID: u32 = u32::MAX;
const MERGED_CONTENT_AUTO_PREVIEW_MAX_BYTES: u64 = 4 * 1024 * 1024;
const TREE_EXPAND_ALL_FOLDER_LIMIT: usize = 50_000;
const INPUT_DEPENDENCIES: ChangeSet = ChangeSet::DRAFT_SELECTION
    .union(ChangeSet::DRAFT_RULES)
    .union(ChangeSet::DRAFT_SETTINGS)
    .union(ChangeSet::NAVIGATION);
const STATUS_DEPENDENCIES: ChangeSet = ChangeSet::EXECUTION
    .union(ChangeSet::EXECUTION_RESULT)
    .union(ChangeSet::DRAFT_SELECTION)
    .union(ChangeSet::DRAFT_SETTINGS);
const RULES_DEPENDENCIES: ChangeSet = ChangeSet::DRAFT_RULES
    .union(ChangeSet::DRAFT_SETTINGS)
    .union(ChangeSet::NAVIGATION);
const RESULTS_DEPENDENCIES: ChangeSet = ChangeSet::EXECUTION_RESULT
    .union(ChangeSet::DRAFT_SETTINGS)
    .union(ChangeSet::NAVIGATION);
const TREE_DEPENDENCIES: ChangeSet = ChangeSet::EXECUTION_RESULT
    .union(ChangeSet::DRAFT_SETTINGS)
    .union(ChangeSet::NAVIGATION_TREE);
const PREVIEW_DEPENDENCIES: ChangeSet = ChangeSet::PREVIEW
    .union(ChangeSet::EXECUTION_RESULT)
    .union(ChangeSet::DRAFT_SETTINGS);

pub(super) fn preview_line_height() -> Pixels {
    px(22.)
}

pub(crate) use crate::application::store::BlacklistItemKind;

pub(super) struct PreviewTableDelegate {
    language: Language,
    columns: Vec<Column>,
    rows: Rc<[PreviewRowViewModel]>,
    sort: model::PreviewTableSort,
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
            rows: Rc::from([]),
            sort: model::PreviewTableSort::default(),
        }
    }

    fn set_language(&mut self, language: Language) {
        self.language = language;
        self.columns[0].name = tr(language, "table_path").into();
        self.columns[1].name = tr(language, "table_chars").into();
        self.columns[2].name = tr(language, "table_tokens").into();
    }

    fn toggle_chars_sort(&mut self) {
        self.sort = self.sort.toggle_chars();
        let mut rows = self.rows.to_vec();
        model::sort_preview_rows(&mut rows, self.sort);
        self.rows = rows.into();
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
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        if col_ix == 1 {
            let icon_name = match self.sort {
                model::PreviewTableSort::None => IconName::ChevronsUpDown,
                model::PreviewTableSort::CharsDesc => IconName::SortDescending,
                model::PreviewTableSort::CharsAsc => IconName::SortAscending,
            };
            return h_flex()
                .id(("preview-table-sort", col_ix))
                .gap_1()
                .items_center()
                .child(self.columns[col_ix].name.clone())
                .child(
                    Icon::new(icon_name)
                        .size_3()
                        .text_color(cx.theme().secondary_foreground),
                )
                .on_click(cx.listener(move |table, _, _, cx| {
                    cx.stop_propagation();
                    let selected_id = table
                        .selected_row()
                        .and_then(|row_ix| table.delegate().rows.get(row_ix))
                        .map(|row| row.id);
                    table.delegate_mut().toggle_chars_sort();
                    if let Some(selected_id) = selected_id {
                        if let Some(next_row_ix) = table
                            .delegate()
                            .rows
                            .iter()
                            .position(|row| row.id == selected_id)
                        {
                            if table.selected_row() != Some(next_row_ix) {
                                table.set_selected_row(next_row_ix, cx);
                            }
                        } else if table.selected_row().is_some() {
                            table.clear_selection(cx);
                        }
                    }
                    cx.notify();
                }))
                .into_any_element();
        }

        self.columns[col_ix].name.clone().into_any_element()
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
    input_exclusion_enabled: bool,
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
}

struct WorkspacePanelView {
    workspace: Entity<Workspace>,
    kind: WorkspacePanelKind,
    _subscriptions: Vec<Subscription>,
}

struct StatusPanelView {
    workspace: Entity<Workspace>,
    _subscriptions: Vec<Subscription>,
}

struct ActivityPanelView {
    workspace: Entity<Workspace>,
    scroll_handle: VirtualListScrollHandle,
    _subscriptions: Vec<Subscription>,
}

struct InputPanelView {
    workspace: Entity<Workspace>,
    _subscriptions: Vec<Subscription>,
}

struct RulesPanelView {
    workspace: Entity<Workspace>,
    _subscriptions: Vec<Subscription>,
}

struct ResultsPanelView {
    workspace: Entity<Workspace>,
    _subscriptions: Vec<Subscription>,
}

struct TreePaneView {
    workspace: Entity<Workspace>,
    view_mode: TreeViewMode,
    plain_text_cache_key: Option<(u64, u64, u64, String)>,
    plain_text_cache: Option<model::TreePaneBodyViewModel>,
    _subscriptions: Vec<Subscription>,
}

struct PreviewPaneView {
    entity_id: EntityId,
    workspace: Entity<Workspace>,
    store: Entity<WorkspaceStore>,
    scroll_handle: UniformListScrollHandle,
    last_requested_load_range: Range<usize>,
    render_cache_range: Range<usize>,
    render_cache_revision: u64,
    render_cache: Vec<crate::ui::preview_model::PreviewRenderLine>,
    pending_visible_range: Option<Range<usize>>,
    last_synced_visible_range: Option<Range<usize>>,
    scheduled_visible_sync: bool,
    last_scroll_anchor: usize,
    _subscriptions: Vec<Subscription>,
}

#[derive(Default)]
struct PreviewTableCache {
    filter: String,
    result_key: u64,
    current_selected_id: Option<u32>,
    sort: model::PreviewTableSort,
    model: Option<model::PreviewTableModel>,
}

#[derive(Default)]
struct InputPanelCache {
    selection_revision: Option<u64>,
    settings_revision: Option<u64>,
    selection: Option<Rc<crate::ui::state::SelectionState>>,
    selected_files: Option<Rc<Vec<crate::domain::FileEntry>>>,
    settings: Option<Rc<crate::ui::state::SettingsState>>,
}

struct ActivityPanelCache {
    execution_revision: Option<u64>,
    filter: crate::ui::state::ActivityFilter,
    expanded: Option<usize>,
    records: Rc<Vec<(usize, crate::domain::ProcessRecord)>>,
    row_sizes: Rc<Vec<gpui::Size<Pixels>>>,
    failed_diagnostics: Rc<str>,
}

impl Default for ActivityPanelCache {
    fn default() -> Self {
        Self {
            execution_revision: None,
            filter: crate::ui::state::ActivityFilter::default(),
            expanded: None,
            records: Rc::default(),
            row_sizes: Rc::default(),
            failed_diagnostics: Rc::from(""),
        }
    }
}

#[derive(Clone, Default)]
struct ResultArtifacts {
    process_dir: Option<std::path::PathBuf>,
    merged_content_path: Option<std::path::PathBuf>,
    preview_blob_dir: Option<std::path::PathBuf>,
}

#[derive(Clone)]
struct ConfigAlert {
    title: SharedString,
    detail: SharedString,
    action_label: SharedString,
    action: ConfigAlertAction,
    is_error: bool,
}

#[derive(Clone, Copy)]
enum ConfigAlertAction {
    ResetDefaults,
    RetrySave,
}

pub struct Workspace {
    focus_handle: FocusHandle,
    store: Entity<WorkspaceStore>,
    coordinator: Entity<WorkspaceCoordinator>,
    views: WorkspaceViews,
    #[allow(dead_code)] // Subscriptions are retained for their lifetime side effect.
    subscriptions: Vec<Subscription>,
}

pub struct WorkspaceViews {
    result_artifacts: ResultArtifacts,
    config_alert: Option<ConfigAlert>,
    tree_panel: TreePanelController,
    preview_table: Entity<TableState<PreviewTableDelegate>>,
    preview_filter_input: Entity<InputState>,
    preview_filter_revision: u64,
    preview_filter_task: Option<Task<()>>,
    tree_filter_revision: u64,
    tree_filter_task: Option<Task<()>>,
    preview_table_cache: PreviewTableCache,
    input_panel_cache: InputPanelCache,
    activity_panel_cache: ActivityPanelCache,
    suppress_tree_interaction_sync: Rc<Cell<bool>>,
    suppress_preview_table_events: bool,
    blacklist_filter_input: Entity<InputState>,
    blacklist_add_input: Entity<InputState>,
    temp_blacklist_add_input: Entity<InputState>,
    temp_whitelist_add_input: Entity<InputState>,
    rules_panel: RulesPanelController,
    input_panel_view: Entity<InputPanelView>,
    #[allow(dead_code)]
    status_panel_view: Entity<StatusPanelView>,
    activity_panel_view: Entity<ActivityPanelView>,
    rules_panel_view: Entity<RulesPanelView>,
    results_panel_view: Entity<ResultsPanelView>,
    tree_pane_view: Entity<TreePaneView>,
    preview_pane_view: Entity<PreviewPaneView>,
    right_panel_view: Entity<WorkspacePanelView>,
}

impl Deref for Workspace {
    type Target = WorkspaceViews;

    fn deref(&self) -> &Self::Target {
        &self.views
    }
}

impl DerefMut for Workspace {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.views
    }
}

impl Workspace {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config_report = settings::load_report();
        let config_alert =
            Self::config_alert_from_report(&config_report, config_report.config.language);
        let cfg = config_report.config.clone();
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
        let temp_blacklist_add_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(cfg.language, "temporary_rules_unified_hint"))
        });
        let temp_whitelist_add_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(cfg.language, "temporary_whitelist_unified_hint"))
        });
        let tree_state = cx.new(|cx| TreeState::new(cx));
        let preview_table = cx.new(|cx| {
            TableState::new(PreviewTableDelegate::new(cfg.language), window, cx)
                .col_selectable(false)
                .sortable(false)
        });
        let suppress_tree_interaction_sync = Rc::new(Cell::new(false));
        let store = cx.new(|_| {
            WorkspaceStore::new(cfg.clone(), tr(cfg.language, "status_ready").to_string())
        });
        let coordinator = cx.new(|_| WorkspaceCoordinator::new());
        let workspace_entity = cx.entity();
        let input_panel_view =
            cx.new(|cx| InputPanelView::new(workspace_entity.clone(), store.clone(), cx));
        let status_panel_view =
            cx.new(|cx| StatusPanelView::new(workspace_entity.clone(), store.clone(), cx));
        let activity_panel_view =
            cx.new(|cx| ActivityPanelView::new(workspace_entity.clone(), store.clone(), cx));
        let rules_panel_view = cx.new(|cx| {
            RulesPanelView::new(
                workspace_entity.clone(),
                store.clone(),
                blacklist_filter_input.clone(),
                cx,
            )
        });
        let results_panel_view = cx.new(|cx| {
            ResultsPanelView::new(
                workspace_entity.clone(),
                store.clone(),
                preview_filter_input.clone(),
                cx,
            )
        });
        let tree_pane_view = cx.new(|cx| {
            TreePaneView::new(
                workspace_entity.clone(),
                store.clone(),
                tree_state.clone(),
                tree_filter_input.clone(),
                suppress_tree_interaction_sync.clone(),
                cx,
            )
        });
        let preview_pane_view =
            cx.new(|cx| PreviewPaneView::new(workspace_entity.clone(), store.clone(), cx));
        let right_panel_view = cx.new(|cx| {
            WorkspacePanelView::new(
                workspace_entity.clone(),
                store.clone(),
                WorkspacePanelKind::Right,
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
            store,
            coordinator,
            views: WorkspaceViews {
                result_artifacts: ResultArtifacts::default(),
                config_alert,
                tree_panel: TreePanelController {
                    state: tree_state,
                    filter_input: tree_filter_input,
                    data: None,
                    projection: model::TreeProjectionState::default(),
                    render_state: model::TreeRenderState::default(),
                    total_summary: model::TreeCountSummary::default(),
                    last_filter: String::new(),
                    last_interaction: None,
                    input_exclusion_enabled: false,
                },
                preview_table,
                preview_filter_input,
                preview_filter_revision: 0,
                preview_filter_task: None,
                tree_filter_revision: 0,
                tree_filter_task: None,
                preview_table_cache: PreviewTableCache::default(),
                input_panel_cache: InputPanelCache::default(),
                activity_panel_cache: ActivityPanelCache::default(),
                suppress_tree_interaction_sync,
                suppress_preview_table_events: false,
                blacklist_filter_input,
                blacklist_add_input,
                temp_blacklist_add_input,
                temp_whitelist_add_input,
                rules_panel: RulesPanelController {
                    revision: 0,
                    cache: RulesPanelCache::default(),
                },
                input_panel_view,
                status_panel_view,
                activity_panel_view,
                rules_panel_view,
                results_panel_view,
                tree_pane_view,
                preview_pane_view,
                right_panel_view,
            },
            subscriptions,
        };
        if this.config_alert.is_some() {
            window.push_notification(
                (
                    NotificationType::Warning,
                    SharedString::from(tr(cfg.language, "config_fallback_defaults")),
                ),
                cx,
            );
        }
        this.refresh_preflight(cx);
        this.schedule_maintenance_cleanup(cx);
        this
    }

    fn schedule_maintenance_cleanup(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            gpui::Timer::after(std::time::Duration::ZERO).await;
            let stream = this.update(cx, |workspace, cx| {
                workspace.coordinator.read(cx).start_maintenance_cleanup()
            });
            let Ok(Ok(mut stream)) = stream else {
                return;
            };
            while let Some(envelope) = stream.events.recv().await {
                if matches!(
                    envelope.payload,
                    crate::application::task::TaskPayload::Finished
                ) {
                    break;
                }
            }
        })
        .detach();
    }

    fn has_inputs(&self, cx: &App) -> bool {
        self.store.read(cx).has_inputs()
    }

    fn is_processing(&self, cx: &App) -> bool {
        self.store.read(cx).is_processing()
    }

    fn dispatch(&mut self, action: WorkspaceAction, cx: &mut Context<Self>) -> Transition {
        let transition = self.store.update(cx, |store, store_cx| {
            let transition = store.dispatch(action);
            if !transition.changes.is_empty() {
                perf::record_store_dispatch();
                store_cx.emit(crate::application::store::WorkspaceEvent(
                    transition.changes,
                ));
            }
            transition
        });
        self.execute_effects(&transition.effects, cx);
        transition
    }

    fn tree_state(&self, cx: &App) -> TreePanelState {
        self.store.read(cx).tree().clone()
    }

    fn set_tree_state(&mut self, state: TreePanelState, cx: &mut Context<Self>) -> bool {
        matches!(
            self.dispatch(
                WorkspaceAction::Navigation(NavigationAction::SetTreeState(state)),
                cx,
            )
            .output,
            crate::application::store::ActionOutput::Changed(true)
        )
    }

    fn reset_tree_state(&mut self, cx: &mut Context<Self>) -> bool {
        matches!(
            self.dispatch(WorkspaceAction::Navigation(NavigationAction::ResetTree), cx,)
                .output,
            crate::application::store::ActionOutput::Changed(true)
        )
    }

    fn action_start_process(
        &mut self,
        _: &StartProcess,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_process(&ClickEvent::default(), window, cx);
    }

    fn action_cancel_process(
        &mut self,
        _: &CancelProcess,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_process(&ClickEvent::default(), window, cx);
    }

    fn action_focus_search(
        &mut self,
        _: &FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = if self.store.read(cx).result().active_tab
            == crate::ui::view_model::ResultTab::Content
        {
            self.preview_filter_input.clone()
        } else {
            self.tree_panel.filter_input.clone()
        };
        input.focus_handle(cx).focus(window);
    }

    fn action_copy_active_result(
        &mut self,
        _: &CopyActiveResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.read(cx).result().active_tab == crate::ui::view_model::ResultTab::Content {
            self.copy_preview(&ClickEvent::default(), window, cx);
        } else {
            self.copy_tree(&ClickEvent::default(), window, cx);
        }
    }

    fn action_export_result(
        &mut self,
        _: &ExportResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.download_result(&ClickEvent::default(), window, cx);
    }

    fn action_open_rules(&mut self, _: &OpenRules, window: &mut Window, cx: &mut Context<Self>) {
        self.open_rules_sheet(&ClickEvent::default(), window, cx);
    }

    fn action_close_workspace_surface(
        &mut self,
        _: &CloseWorkspaceSurface,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.close_workspace_surface(window, cx);
    }

    fn execute_effects(&mut self, effects: &[WorkspaceEffect], cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                WorkspaceEffect::RestartPreflight {
                    preserve_completed_status,
                } => self.refresh_preflight_internal(*preserve_completed_status, cx),
                WorkspaceEffect::CancelAllTasks => {
                    self.coordinator.read(cx).cancel_all();
                }
                WorkspaceEffect::CleanupResultArtifacts => {
                    self.cleanup_current_result_artifacts();
                }
                WorkspaceEffect::StartPreview(request) => {
                    self.start_preview_request(request.clone(), cx);
                }
            }
        }
    }

    fn clear_pending_confirmation(&mut self, _: &mut Context<Self>) -> bool {
        false
    }

    pub(super) fn ui_state(&self, cx: &App) -> WorkspaceUiState {
        self.store.read(cx).navigation()
    }

    pub(super) fn selection_snapshot(&self, cx: &App) -> crate::ui::state::SelectionState {
        self.store.read(cx).selection_snapshot()
    }

    pub(super) fn settings_snapshot(&self, cx: &App) -> crate::ui::state::SettingsState {
        self.store.read(cx).settings()
    }

    fn cached_input_snapshots(
        &mut self,
        cx: &App,
    ) -> (
        Rc<crate::ui::state::SelectionState>,
        Rc<Vec<crate::domain::FileEntry>>,
        Rc<crate::ui::state::SettingsState>,
    ) {
        let revisions = self.store.read(cx).revisions();
        if self.input_panel_cache.selection_revision != Some(revisions.selection)
            || self.input_panel_cache.selection.is_none()
        {
            let selection = Rc::new(self.store.read(cx).selection_snapshot());
            self.input_panel_cache.selected_files = Some(Rc::new(selection.selected_files.clone()));
            self.input_panel_cache.selection = Some(selection);
            self.input_panel_cache.selection_revision = Some(revisions.selection);
            perf::record_input_cache_rebuild();
        }
        if self.input_panel_cache.settings_revision != Some(revisions.settings)
            || self.input_panel_cache.settings.is_none()
        {
            self.input_panel_cache.settings = Some(Rc::new(self.store.read(cx).settings()));
            self.input_panel_cache.settings_revision = Some(revisions.settings);
            perf::record_input_cache_rebuild();
        }
        (
            self.input_panel_cache
                .selection
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            self.input_panel_cache
                .selected_files
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            self.input_panel_cache
                .settings
                .as_ref()
                .cloned()
                .unwrap_or_default(),
        )
    }

    pub(super) fn language(&self, cx: &App) -> Language {
        self.store.read(cx).language()
    }

    pub(super) fn effective_filters(&self, cx: &App) -> crate::ui::models::EffectiveMergeFilters {
        self.store.read(cx).effective_filters()
    }

    pub(super) fn result_has_content(&self, cx: &App) -> bool {
        self.store.read(cx).result_has_content()
    }

    pub(super) fn result_is_tree_only(&self, cx: &App) -> bool {
        self.store.read(cx).result_is_tree_only()
    }

    pub(super) fn invalidate_rules_panel_cache(&mut self) {
        self.rules_panel.revision = self.rules_panel.revision.wrapping_add(1);
    }

    pub(super) fn refresh_rules_panel_cache(&mut self, cx: &Context<Self>) {
        let settings = self.store.read(cx).settings();
        let language = settings.language;
        let filter = self
            .blacklist_filter_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let revision = self.rules_panel.revision;
        let cache = &mut self.rules_panel.cache;
        if cache.revision == revision && cache.language == language && cache.filter == filter {
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
        cache.revision = revision;
    }

    fn cleanup_result_artifacts(artifacts: &ResultArtifacts) {
        if let Some(dir) = &artifacts.process_dir {
            let _ = crate::utils::temp_file::cleanup_temp_dir(dir);
            return;
        }
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
        let language = self.store.read(cx).language();
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
        self.temp_blacklist_add_input.update(cx, |state, cx| {
            state.set_placeholder(tr(language, "temporary_rules_unified_hint"), window, cx)
        });
        self.temp_whitelist_add_input.update(cx, |state, cx| {
            state.set_placeholder(tr(language, "temporary_whitelist_unified_hint"), window, cx)
        });
        self.preview_table.update(cx, |table, cx| {
            table.delegate_mut().set_language(language);
            cx.notify();
        });
        if !self.is_processing(cx)
            && self.store.read(cx).process().ui_status == ProcessUiStatus::Idle
        {
            let _ = self.dispatch(
                WorkspaceAction::Execution(
                    crate::application::store::ExecutionAction::SetIdleLabel(
                        tr(language, "status_ready").to_string(),
                    ),
                ),
                cx,
            );
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

    fn schedule_tree_filter_sync(&mut self, cx: &mut Context<Self>) {
        self.tree_filter_revision = self.tree_filter_revision.wrapping_add(1);
        let revision = self.tree_filter_revision;
        self.tree_filter_task = Some(cx.spawn(async move |this, cx| {
            Timer::after(std::time::Duration::from_millis(75)).await;
            let _ = this.update(cx, |workspace, cx| {
                let _ = workspace.apply_scheduled_tree_filter(revision, cx);
            });
        }));
    }

    fn apply_scheduled_tree_filter(&mut self, revision: u64, cx: &mut Context<Self>) -> bool {
        if self.tree_filter_revision != revision {
            return false;
        }
        self.tree_filter_task = None;
        self.sync_tree(cx);
        true
    }

    fn config_alert_from_report(
        report: &settings::ConfigLoadReport,
        language: Language,
    ) -> Option<ConfigAlert> {
        let path = report
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| tr(language, "config_path_unknown").to_string());
        match report.issue.as_ref() {
            Some(ConfigLoadIssue::ParseFailed(err)) => Some(ConfigAlert {
                title: SharedString::from(tr(language, "config_load_failed")),
                detail: SharedString::from(format!(
                    "{} | {}: {path} | {}: {err}",
                    tr(language, "config_fallback_defaults"),
                    tr(language, "config_path_label"),
                    tr(language, "config_error_label")
                )),
                action_label: SharedString::from(tr(language, "config_reset_button")),
                action: ConfigAlertAction::ResetDefaults,
                is_error: true,
            }),
            Some(ConfigLoadIssue::ReadFailed(err)) => Some(ConfigAlert {
                title: SharedString::from(tr(language, "config_load_failed")),
                detail: SharedString::from(format!(
                    "{} | {}: {path} | {}: {err}",
                    tr(language, "config_fallback_defaults"),
                    tr(language, "config_path_label"),
                    tr(language, "config_error_label")
                )),
                action_label: SharedString::from(tr(language, "config_reset_button")),
                action: ConfigAlertAction::ResetDefaults,
                is_error: true,
            }),
            Some(ConfigLoadIssue::ConfigDirUnavailable) => Some(ConfigAlert {
                title: SharedString::from(tr(language, "config_save_failed")),
                detail: SharedString::from(tr(language, "config_dir_unavailable_detail")),
                action_label: SharedString::from(tr(language, "config_reset_button")),
                action: ConfigAlertAction::ResetDefaults,
                is_error: true,
            }),
            Some(ConfigLoadIssue::MissingFile) | None => None,
        }
    }

    fn set_config_save_error(&mut self, error: String, cx: &mut Context<Self>) {
        let language = self.language(cx);
        self.config_alert = Some(ConfigAlert {
            title: SharedString::from(tr(language, "config_save_failed")),
            detail: SharedString::from(format!(
                "{} | {}: {error}",
                tr(language, "config_save_failed_detail"),
                tr(language, "config_error_label")
            )),
            action_label: SharedString::from(tr(language, "config_retry_button")),
            action: ConfigAlertAction::RetrySave,
            is_error: true,
        });
        cx.notify();
    }

    fn clear_config_alert(&mut self, cx: &mut Context<Self>) {
        if self.config_alert.take().is_some() {
            cx.notify();
        }
    }
}

impl WorkspacePanelView {
    fn new(
        workspace: Entity<Workspace>,
        store: Entity<WorkspaceStore>,
        kind: WorkspacePanelKind,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![cx.subscribe(&store, |_, _, event, cx| {
            if event.0.intersects(
                ChangeSet::NAVIGATION
                    .union(ChangeSet::DRAFT_SETTINGS)
                    .union(ChangeSet::EXECUTION_RESULT),
            ) {
                perf::record_workspace_view_notify();
                cx.notify();
            }
        })];
        Self {
            workspace,
            kind,
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
            })
    }
}

impl InputPanelView {
    fn new(
        workspace: Entity<Workspace>,
        store: Entity<WorkspaceStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![cx.subscribe(&store, |_, _, event, cx| {
            if event.0.intersects(INPUT_DEPENDENCIES) {
                perf::record_workspace_view_notify();
                cx.notify();
            }
        })];
        Self {
            workspace,
            _subscriptions: subscriptions,
        }
    }
}

impl StatusPanelView {
    fn new(
        workspace: Entity<Workspace>,
        store: Entity<WorkspaceStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![cx.subscribe(&store, |_, _, event, cx| {
            if event.0.intersects(STATUS_DEPENDENCIES) {
                perf::record_workspace_view_notify();
                cx.notify();
            }
        })];
        Self {
            workspace,
            _subscriptions: subscriptions,
        }
    }
}

impl ActivityPanelView {
    fn new(
        workspace: Entity<Workspace>,
        store: Entity<WorkspaceStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![cx.subscribe(&store, |_, _, event, cx| {
            if event
                .0
                .intersects(ChangeSet::EXECUTION.union(ChangeSet::NAVIGATION))
            {
                perf::record_workspace_view_notify();
                cx.notify();
            }
        })];
        Self {
            workspace,
            scroll_handle: VirtualListScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }
}

impl RulesPanelView {
    fn new(
        workspace: Entity<Workspace>,
        store: Entity<WorkspaceStore>,
        blacklist_filter_input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.subscribe(&store, |_, _, event, cx| {
                if event.0.intersects(RULES_DEPENDENCIES) {
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&blacklist_filter_input, |_, _, cx| {
                perf::record_workspace_view_notify();
                cx.notify();
            }),
        ];
        Self {
            workspace,
            _subscriptions: subscriptions,
        }
    }
}

impl ResultsPanelView {
    fn new(
        workspace: Entity<Workspace>,
        store: Entity<WorkspaceStore>,
        preview_filter_input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.subscribe(&store, |_, _, event, cx| {
                if event.0.intersects(RESULTS_DEPENDENCIES) {
                    perf::record_workspace_view_notify();
                    cx.notify();
                }
            }),
            cx.observe(&preview_filter_input, |_, _, cx| {
                perf::record_workspace_view_notify();
                cx.notify();
            }),
        ];
        Self {
            workspace,
            _subscriptions: subscriptions,
        }
    }
}

impl TreePaneView {
    fn new(
        workspace: Entity<Workspace>,
        store: Entity<WorkspaceStore>,
        tree_state: Entity<TreeState>,
        tree_filter_input: Entity<InputState>,
        suppress_tree_interaction_sync: Rc<Cell<bool>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let tree_workspace = workspace.clone();
        let tree_interaction_guard = suppress_tree_interaction_sync.clone();
        let subscriptions = vec![
            cx.subscribe(&store, |_, _, event, cx| {
                if event.0.intersects(TREE_DEPENDENCIES) {
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
            cx.observe(&tree_filter_input, |_, _, cx| {
                perf::record_workspace_view_notify();
                cx.notify();
            }),
        ];
        Self {
            workspace,
            view_mode: TreeViewMode::Tree,
            plain_text_cache_key: None,
            plain_text_cache: None,
            _subscriptions: subscriptions,
        }
    }
}

impl PreviewPaneView {
    fn new(
        workspace: Entity<Workspace>,
        store: Entity<WorkspaceStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let entity_id = cx.entity().entity_id();
        let subscriptions = vec![cx.subscribe(&store, move |_, _, event, cx| {
            if event.0.intersects(PREVIEW_DEPENDENCIES) {
                perf::record_workspace_view_notify();
                cx.defer(move |cx| cx.notify(entity_id));
            }
        })];
        Self {
            entity_id: cx.entity().entity_id(),
            workspace,
            store,
            scroll_handle: UniformListScrollHandle::new(),
            last_requested_load_range: 0..0,
            render_cache_range: 0..0,
            render_cache_revision: 0,
            render_cache: Vec::new(),
            pending_visible_range: None,
            last_synced_visible_range: None,
            scheduled_visible_sync: false,
            last_scroll_anchor: 0,
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

impl Render for ActivityPanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scroll_handle = self.scroll_handle.clone();
        self.workspace.update(cx, |workspace, workspace_cx| {
            workspace.render_activity_sheet_panel(scroll_handle, workspace_cx)
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stacked_toolbar = window.bounds().size.width < px(1280.);
        self.workspace.update(cx, |workspace, workspace_cx| {
            workspace
                .render_results_panel(stacked_toolbar, workspace_cx)
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
        let sheet_overlay = self.render_workspace_sheet_overlay(cx);
        gpui_component::v_flex()
            .id("codemerge-root")
            .debug_selector(|| "codemerge-root".to_string())
            .key_context("Workspace")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::action_start_process))
            .on_action(cx.listener(Self::action_cancel_process))
            .on_action(cx.listener(Self::action_focus_search))
            .on_action(cx.listener(Self::action_copy_active_result))
            .on_action(cx.listener(Self::action_export_result))
            .on_action(cx.listener(Self::action_open_rules))
            .on_action(cx.listener(Self::action_close_workspace_surface))
            .relative()
            .size_full()
            .child(self.render_window_chrome(window, cx))
            .child(
                gpui::div()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.render_main_content(window, cx)),
            )
            .when_some(sheet_overlay, |root, overlay| root.child(overlay))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        Self::cleanup_result_artifacts(&self.result_artifacts);
    }
}

#[cfg(test)]
impl Workspace {
    fn test_selection(
        &self,
    ) -> crate::application::store::StoreSlice<crate::ui::selection_model::SelectionModel> {
        WorkspaceStore::selection_slice(self.store.clone())
    }

    fn test_process(
        &self,
    ) -> crate::application::store::StoreSlice<crate::ui::models::ProcessModel> {
        WorkspaceStore::process_slice(self.store.clone())
    }

    fn test_result(
        &self,
    ) -> crate::application::store::StoreSlice<crate::ui::result_model::ResultModel> {
        WorkspaceStore::result_slice(self.store.clone())
    }

    fn test_preview(
        &self,
    ) -> crate::application::store::StoreSlice<crate::ui::preview_model::PreviewModel> {
        WorkspaceStore::preview_slice(self.store.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use gpui::Focusable as _;

    use super::{
        PREVIEW_DEPENDENCIES, RULES_DEPENDENCIES, STATUS_DEPENDENCIES, TREE_DEPENDENCIES, Workspace,
    };
    use crate::application::store::{
        ChangeSet, DraftAction, ExecutionAction, StoreSlice, WorkspaceAction,
    };
    use crate::domain::{
        AppConfigV1, FileEntry, Language, PreviewFileEntry, ProcessRecord, ProcessResult,
        ProcessStatus, TreeNode,
    };
    use crate::processor::stats::ProcessingStats;
    use crate::services::preview::{
        EXCERPT_PREVIEW_BYTES, MAX_PREVIEW_LINE_BYTES, PreviewEvent, PreviewRequest,
        index_document, load_range,
    };
    use crate::services::process::ProcessEvent;
    use crate::ui::view_model::ResultTab;
    use crate::ui::{perf, preview_model::PreviewModel, state::ProcessUiStatus};
    use gpui::{
        AppContext as _, Context, ScrollDelta, ScrollWheelEvent, TestAppContext,
        VisualContext as _, point,
    };
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

    #[test]
    fn pane_dependencies_are_isolated_by_change_set() {
        assert!(RULES_DEPENDENCIES.intersects(ChangeSet::DRAFT_RULES));
        assert!(!PREVIEW_DEPENDENCIES.intersects(ChangeSet::DRAFT_RULES));
        assert!(STATUS_DEPENDENCIES.intersects(ChangeSet::EXECUTION));
        assert!(STATUS_DEPENDENCIES.intersects(ChangeSet::DRAFT_SELECTION));
        assert!(!RULES_DEPENDENCIES.intersects(ChangeSet::EXECUTION));
        assert!(TREE_DEPENDENCIES.intersects(ChangeSet::NAVIGATION_TREE));
        assert!(!STATUS_DEPENDENCIES.intersects(ChangeSet::NAVIGATION_TREE));
    }

    #[gpui::test]
    fn refresh_selected_folder_gitignore_rules_reads_current_file(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let root = std::env::temp_dir().join(format!(
            "codemerge_gitignore_refresh_tests_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(root.join(".gitignore"), "dist\n").expect("write gitignore");

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace
                .test_selection()
                .update(cx, |selection, selection_cx| {
                    selection.set_selected_folder(root.clone(), vec!["target".into()]);
                    selection_cx.notify();
                });

            workspace.refresh_selected_folder_gitignore_rules(cx);

            assert_eq!(
                workspace.selection_snapshot(cx).gitignore_rules,
                vec!["dist".to_string()]
            );
        });

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    fn successful_process_completion_clears_temporary_filters_and_restarts_preflight(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace
                .test_selection()
                .update(cx, |selection, selection_cx| {
                    selection.set_gitignore_file(Some(PathBuf::from("manual.gitignore")));
                    selection.append_temporary_gitignore_rules(vec!["node_modules".into()]);
                    selection.add_temporary_blacklist_tokens(&["tmp".into()], true);
                    selection.add_temporary_whitelist_tokens(&["src".into()], false);
                    selection.add_temporary_whitelist_tokens(&["rs".into()], true);
                    let _ = selection.set_temporary_whitelist_mode(
                        crate::domain::TemporaryWhitelistMode::WhitelistOnly,
                    );
                    selection_cx.notify();
                });
            let preflight_revision = workspace.test_process().read(cx).state().preflight_revision;
            start_test_process(workspace, 1, cx);
            workspace.handle_process_event(ProcessEvent::completed(1, sample_result()), cx);

            let selection = workspace.selection_snapshot(cx);
            assert!(selection.gitignore_file.is_none());
            assert!(selection.temp_folder_blacklist.is_empty());
            assert!(selection.temp_ext_blacklist.is_empty());
            assert!(selection.temp_folder_whitelist.is_empty());
            assert!(selection.temp_ext_whitelist.is_empty());
            assert_eq!(
                selection.temp_whitelist_mode,
                crate::domain::TemporaryWhitelistMode::WhitelistThenBlacklist
            );
            assert!(
                workspace.test_process().read(cx).state().preflight_revision > preflight_revision
            );
        });
    }

    #[gpui::test]
    fn successful_process_completion_keeps_completed_status_after_followup_preflight(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace
                .test_selection()
                .update(cx, |selection, selection_cx| {
                    selection.set_gitignore_file(Some(PathBuf::from("manual.gitignore")));
                    selection.append_temporary_gitignore_rules(vec!["node_modules".into()]);
                    selection.add_temporary_blacklist_tokens(&["tmp".into()], true);
                    selection.add_temporary_whitelist_tokens(&["src".into()], false);
                    let _ = selection.set_temporary_whitelist_mode(
                        crate::domain::TemporaryWhitelistMode::WhitelistOnly,
                    );
                    selection_cx.notify();
                });
            start_test_process(workspace, 1, cx);
            workspace.handle_process_event(ProcessEvent::completed(1, sample_result()), cx);

            let process = workspace.test_process().read(cx).state();
            assert_eq!(process.ui_status, ProcessUiStatus::Completed);
        });
    }

    #[gpui::test]
    fn cancelled_process_keeps_temporary_filters(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace
                .test_selection()
                .update(cx, |selection, selection_cx| {
                    selection.set_gitignore_file(Some(PathBuf::from("manual.gitignore")));
                    selection.append_temporary_gitignore_rules(vec!["node_modules".into()]);
                    selection.add_temporary_blacklist_tokens(&["tmp".into()], true);
                    selection.add_temporary_whitelist_tokens(&["src".into()], false);
                    selection.add_temporary_whitelist_tokens(&["rs".into()], true);
                    let _ = selection.set_temporary_whitelist_mode(
                        crate::domain::TemporaryWhitelistMode::WhitelistOnly,
                    );
                    selection_cx.notify();
                });
            start_test_process(workspace, 1, cx);
            workspace.handle_process_event(ProcessEvent::cancelled(1), cx);

            let selection = workspace.selection_snapshot(cx);
            assert_eq!(
                selection.gitignore_file.as_deref(),
                Some(std::path::Path::new("manual.gitignore"))
            );
            assert_eq!(
                selection.temp_folder_blacklist,
                vec!["node_modules".to_string()]
            );
            assert_eq!(selection.temp_ext_blacklist, vec![".tmp".to_string()]);
            assert_eq!(selection.temp_folder_whitelist, vec!["src".to_string()]);
            assert_eq!(selection.temp_ext_whitelist, vec![".rs".to_string()]);
            assert_eq!(
                selection.temp_whitelist_mode,
                crate::domain::TemporaryWhitelistMode::WhitelistOnly
            );
        });
    }

    #[gpui::test]
    fn clearing_inputs_detaches_cancelled_process_events(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            start_test_process(workspace, 1, cx);

            workspace.cancel_and_detach_background_work(cx);
            let _ = workspace.apply_process_event(ProcessEvent::completed(1, sample_result()), cx);

            assert!(workspace.test_result().read(cx).state().result.is_none());
            assert!(
                workspace
                    .test_process()
                    .read(cx)
                    .state()
                    .current_run_id
                    .is_none()
            );
        });
    }

    #[test]
    fn cleanup_result_artifacts_removes_owned_process_dir() {
        let process_dir = std::env::temp_dir().join(format!(
            "codemerge_workspace_artifacts_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        fs::create_dir_all(&process_dir).expect("create process dir");
        fs::write(process_dir.join("preview_0.txt"), "preview").expect("write preview");

        Workspace::cleanup_result_artifacts(&super::ResultArtifacts {
            process_dir: Some(process_dir.clone()),
            merged_content_path: Some(process_dir.join("merged.txt")),
            preview_blob_dir: Some(process_dir.clone()),
        });

        assert!(!process_dir.exists());
    }

    #[gpui::test]
    fn failed_process_keeps_temporary_filters(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace
                .test_selection()
                .update(cx, |selection, selection_cx| {
                    selection.set_gitignore_file(Some(PathBuf::from("manual.gitignore")));
                    selection.append_temporary_gitignore_rules(vec!["node_modules".into()]);
                    selection.add_temporary_blacklist_tokens(&["tmp".into()], true);
                    selection.add_temporary_whitelist_tokens(&["src".into()], false);
                    selection.add_temporary_whitelist_tokens(&["rs".into()], true);
                    let _ = selection.set_temporary_whitelist_mode(
                        crate::domain::TemporaryWhitelistMode::WhitelistOnly,
                    );
                    selection_cx.notify();
                });
            start_test_process(workspace, 1, cx);
            workspace.handle_process_event(
                ProcessEvent::failed(1, crate::error::AppError::new("boom")),
                cx,
            );

            let selection = workspace.selection_snapshot(cx);
            assert_eq!(
                selection.gitignore_file.as_deref(),
                Some(std::path::Path::new("manual.gitignore"))
            );
            assert_eq!(
                selection.temp_folder_blacklist,
                vec!["node_modules".to_string()]
            );
            assert_eq!(selection.temp_ext_blacklist, vec![".tmp".to_string()]);
            assert_eq!(selection.temp_folder_whitelist, vec!["src".to_string()]);
            assert_eq!(selection.temp_ext_whitelist, vec![".rs".to_string()]);
            assert_eq!(
                selection.temp_whitelist_mode,
                crate::domain::TemporaryWhitelistMode::WhitelistOnly
            );
        });
    }

    #[gpui::test]
    fn failed_task_event_becomes_diagnostic_error(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            start_test_process(workspace, 7, cx);
            workspace.handle_process_event(
                ProcessEvent::failed(
                    7,
                    crate::error::AppError::with_code(
                        crate::error::ErrorCode::Processing,
                        "process-channel",
                        "后台任务意外中断，未收到完成状态",
                    ),
                ),
                cx,
            );

            let process = workspace.test_process().read(cx).state();
            assert_eq!(process.ui_status, ProcessUiStatus::Error);
            assert_eq!(
                process.last_error.as_deref(),
                Some("后台任务意外中断，未收到完成状态")
            );
            assert!(process.processing_elapsed.is_some());
            let ui_state = workspace.ui_state(cx);
            assert_eq!(
                ui_state.activity_filter,
                crate::ui::state::ActivityFilter::Failed
            );
            assert_eq!(ui_state.active_sheet, None);
        });
    }

    #[gpui::test]
    fn preview_requested_range_changes_do_not_invalidate_preview_pane(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("preview_notify", 512);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_path(&path), cx);
            seed_preview_model(&workspace.test_preview(), &path, cx, 0..128, 1);
            workspace.test_preview().update(cx, |preview, _| {
                let _ = preview.load_preview_range_request(
                    256..320,
                    crate::ui::preview_model::PreviewScrollDirection::Down,
                );
            });
            perf::reset();

            workspace.test_preview().update(cx, |preview, preview_cx| {
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
            seed_preview_model(&workspace.test_preview(), &path, cx, 0..256, 1);

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
                assert_eq!(view.last_synced_visible_range, Some(128..160));
            });
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
            seed_preview_model(&workspace.test_preview(), &path, cx, 0..256, 1);

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
    fn preview_subrange_inside_last_visible_window_does_not_reschedule_notify(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("preview_measure_visible", 1_024);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_path(&path), cx);
            seed_preview_model(&workspace.test_preview(), &path, cx, 0..256, 1);

            workspace.preview_pane_view.update(cx, |view, cx| {
                view.queue_visible_range_sync(0..24, cx);
                view.flush_pending_visible_range(cx);
                assert_eq!(view.last_synced_visible_range, Some(0..24));
                assert!(!view.scheduled_visible_sync);

                view.queue_visible_range_sync(0..1, cx);
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
            seed_preview_model(&workspace.test_preview(), &path, cx, 0..128, 1);
            let initial_cache = workspace.preview_pane_view.update(cx, |view, cx| {
                view.refresh_render_cache(0..64, cx);
                (view.render_cache_range.clone(), view.render_cache.clone())
            });

            workspace.test_preview().update(cx, |preview, preview_cx| {
                let revision = preview.state().preview_revision;
                let _ = preview.apply_event(PreviewEvent::Loaded {
                    revision,
                    file_id: 1,
                    loaded_range: 384..512,
                    lines: (384..512).map(|ix| format!("line-{ix}")).collect(),
                });
                preview_cx.notify();
            });

            let current_cache = workspace.preview_pane_view.update(cx, |view, cx| {
                view.refresh_render_cache(view.render_cache_range.clone(), cx);
                (view.render_cache_range.clone(), view.render_cache.clone())
            });

            assert_eq!(current_cache, initial_cache);
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
            seed_preview_model(&workspace.test_preview(), &path, cx, 0..128, 1);

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
            seed_preview_model(&workspace.test_preview(), &path, cx, 0..3_072, 1);
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
    fn preview_deep_scroll_draw_settles_without_extra_sync(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_dense_preview_fixture("preview_deep_scroll_settle", 20_000);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_merged_content_path(&path), cx);
            seed_preview_model(
                &workspace.test_preview(),
                &path,
                cx,
                0..1_536,
                super::MERGED_CONTENT_PREVIEW_FILE_ID,
            );
            workspace.test_result().update(cx, |result, result_cx| {
                result.set_active_tab(ResultTab::Content);
                result_cx.notify();
            });
        });

        cx.update(|window, app| {
            window.draw(app).clear();
        });

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.preview_pane_view.update(cx, |view, _| {
                view.scroll_handle
                    .scroll_to_item_strict(512, gpui::ScrollStrategy::Top);
            });
        });

        cx.update(|window, app| {
            window.draw(app).clear();
        });
        cx.update(|window, app| {
            window.draw(app).clear();
        });
        cx.update(|window, app| {
            window.draw(app).clear();
        });

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.preview_pane_view.update(cx, |view, _| {
                assert!(
                    view.last_synced_visible_range
                        .as_ref()
                        .is_some_and(|range| range.start >= 512),
                    "deep scroll should settle on a deep visible range"
                );
                assert!(
                    !view.scheduled_visible_sync,
                    "stable deep scroll should not keep scheduling follow-up sync work"
                );
                assert!(
                    view.pending_visible_range.is_none(),
                    "stable deep scroll should not leave pending visible sync work behind"
                );
            });
        });

        let _ = fs::remove_dir_all(path.parent().expect("fixture dir"));
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
        for _ in 0..100 {
            cx.run_until_parked();
            if workspace.update(cx, |workspace, cx| {
                workspace
                    .test_preview()
                    .read(cx)
                    .preview_document()
                    .is_some()
            }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        workspace.update(cx, |workspace, cx| {
            let preview = workspace.test_preview().read(cx);
            assert_eq!(
                preview.selected_preview_file_id(),
                Some(super::MERGED_CONTENT_PREVIEW_FILE_ID)
            );
            assert_eq!(
                preview.preview_document().expect("preview document").path(),
                path.as_path()
            );
        });

        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn content_tab_defers_large_merged_result_preview_until_explicit_load(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_large_preview_fixture(
            "merged_content_deferred",
            super::MERGED_CONTENT_AUTO_PREVIEW_MAX_BYTES as usize + 2_048,
        );

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_merged_content_path(&path), cx);
            workspace.load_merged_content_preview(cx);

            let preview = workspace.test_preview().read(cx);
            assert_eq!(
                preview.selected_preview_file_id(),
                Some(super::MERGED_CONTENT_PREVIEW_FILE_ID)
            );
            assert!(preview.preview_document().is_none());
            let deferred = preview.deferred_preview().expect("deferred merged preview");
            assert_eq!(deferred.source_path, path);
            assert_eq!(deferred.excerpt_byte_len, EXCERPT_PREVIEW_BYTES);
        });

        let _ = fs::remove_dir_all(path.parent().expect("fixture dir"));
    }

    #[gpui::test]
    fn load_1mb_preview_opens_excerpt_for_large_merged_result(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_large_preview_fixture(
            "merged_content_excerpt",
            super::MERGED_CONTENT_AUTO_PREVIEW_MAX_BYTES as usize + 4_096,
        );

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_merged_content_path(&path), cx);
            workspace.load_merged_content_preview(cx);
            workspace.load_deferred_merged_content_excerpt(cx);
        });

        let mut excerpt_path = None;
        for _ in 0..100 {
            cx.run_until_parked();
            excerpt_path = workspace.update(cx, |workspace: &mut Workspace, cx| {
                workspace
                    .test_preview()
                    .read(cx)
                    .deferred_preview()
                    .and_then(|state| state.excerpt_path.clone())
            });
            if excerpt_path.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let excerpt_path = excerpt_path.expect("excerpt preview path");
        for _ in 0..100 {
            cx.run_until_parked();
            if workspace.update(cx, |workspace, cx| {
                workspace
                    .test_preview()
                    .read(cx)
                    .preview_document()
                    .is_some()
            }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        workspace.update(cx, |workspace, cx| {
            let preview = workspace.test_preview().read(cx);
            assert_eq!(
                preview.selected_preview_file_id(),
                Some(super::MERGED_CONTENT_PREVIEW_FILE_ID)
            );
            let document = preview.preview_document().expect("preview document");
            assert_eq!(document.path(), excerpt_path.as_path());
            assert_ne!(document.path(), path.as_path());
        });

        assert!(
            fs::metadata(&excerpt_path).expect("excerpt metadata").len() <= EXCERPT_PREVIEW_BYTES
        );
        let _ = fs::remove_dir_all(path.parent().expect("fixture dir"));
    }

    #[gpui::test]
    fn content_tab_switch_stays_responsive_with_large_result_metadata(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("merged_content_large_meta", 256);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            let mut result = sample_result();
            result.merged_content_path = Some(path.clone());
            result.preview_files = (0..20_000)
                .map(|ix| PreviewFileEntry {
                    id: (ix + 1) as u32,
                    display_path: format!("src/file_{ix}.rs"),
                    chars: ix + 1,
                    tokens: (ix % 17) + 1,
                    preview_blob_path: path.clone(),
                    byte_len: 128,
                    archive: None,
                })
                .collect();
            workspace.set_result(result, cx);
        });

        let start = Instant::now();
        cx.update_window_entity(&workspace, |workspace: &mut Workspace, window, cx| {
            workspace.set_tab(&1, window, cx);
        });

        assert!(start.elapsed() < Duration::from_millis(400));
        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn content_tab_draw_stays_responsive_with_large_result_metadata(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("merged_content_large_draw", 256);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            let mut result = sample_result();
            result.merged_content_path = Some(path.clone());
            result.preview_files = (0..120_000)
                .map(|ix| PreviewFileEntry {
                    id: (ix + 1) as u32,
                    display_path: format!("src/file_{ix}.rs"),
                    chars: ix + 1,
                    tokens: (ix % 17) + 1,
                    preview_blob_path: path.clone(),
                    byte_len: 128,
                    archive: None,
                })
                .collect();
            workspace.set_result(result, cx);
            seed_preview_model(
                &workspace.test_preview(),
                &path,
                cx,
                0..128,
                super::MERGED_CONTENT_PREVIEW_FILE_ID,
            );
        });

        cx.update_window_entity(&workspace, |workspace: &mut Workspace, window, cx| {
            workspace.set_tab(&1, window, cx);
        });

        let start = Instant::now();
        cx.update(|window, app| {
            window.draw(app).clear();
        });

        assert!(start.elapsed() < Duration::from_millis(500));
        let _ = fs::remove_file(path);
    }

    #[gpui::test]
    fn content_tab_draw_stays_responsive_with_long_single_line_preview(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_single_line_preview_fixture(
            "merged_content_long_line",
            MAX_PREVIEW_LINE_BYTES * 64,
        );

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_merged_content_path(&path), cx);
            workspace.test_result().update(cx, |result, result_cx| {
                result.set_active_tab(ResultTab::Content);
                result_cx.notify();
            });
            workspace.test_preview().update(cx, |preview, preview_cx| {
                let request =
                    preview.open_preview(super::MERGED_CONTENT_PREVIEW_FILE_ID, path.clone());
                let revision = match request {
                    PreviewRequest::Open { revision, .. } => revision,
                    _ => unreachable!(),
                };
                let document = index_document(&path).expect("index document");
                let lines = load_range(&document, 0..1).expect("load range");
                let _ = preview.apply_event(PreviewEvent::Opened {
                    revision,
                    file_id: super::MERGED_CONTENT_PREVIEW_FILE_ID,
                    document,
                    loaded_range: 0..1,
                    lines,
                });
                preview_cx.notify();
            });
        });

        let rendered_line = workspace
            .update(cx, |workspace: &mut Workspace, cx| {
                workspace
                    .test_preview()
                    .read(cx)
                    .line_at(0)
                    .map(|line| line.to_string())
            })
            .expect("preview line");
        assert!(rendered_line.len() <= MAX_PREVIEW_LINE_BYTES);

        let start = Instant::now();
        cx.update(|window, app| {
            window.draw(app).clear();
        });

        assert!(start.elapsed() < Duration::from_millis(500));
        let _ = fs::remove_dir_all(path.parent().expect("fixture dir"));
    }

    #[gpui::test]
    fn content_tab_draw_stays_responsive_with_dense_preview_document(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_dense_preview_fixture("merged_content_dense_lines", 100_000);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_merged_content_path(&path), cx);
            seed_preview_model(
                &workspace.test_preview(),
                &path,
                cx,
                0..128,
                super::MERGED_CONTENT_PREVIEW_FILE_ID,
            );
            workspace.test_result().update(cx, |result, result_cx| {
                result.set_active_tab(ResultTab::Content);
                result_cx.notify();
            });
        });

        let start = Instant::now();
        cx.update(|window, app| {
            window.draw(app).clear();
        });

        assert!(start.elapsed() < Duration::from_millis(500));
        let _ = fs::remove_dir_all(path.parent().expect("fixture dir"));
    }

    #[gpui::test]
    fn merged_content_preview_uses_virtualized_render_cache(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let path = write_preview_fixture("merged_content_virtualized", 512);

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            workspace.set_result(sample_result_with_merged_content_path(&path), cx);
            seed_preview_model(
                &workspace.test_preview(),
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
                &workspace.test_preview(),
                &path,
                cx,
                0..64,
                super::MERGED_CONTENT_PREVIEW_FILE_ID,
            );

            workspace.sync_preview_table(cx);

            let preview = workspace.test_preview().read(cx);
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
            seed_preview_model(&workspace.test_preview(), &path, cx, 0..32, 2);

            workspace
                .preview_filter_input
                .update(cx, |input, input_cx| {
                    input.set_value("src", window, input_cx);
                });
            workspace.handle_preview_filter_change(cx);

            let preview = workspace.test_preview().read(cx);
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

            assert!(workspace.sync_tree_selection_for_preview_file(1, cx));
            let tree_state = workspace.tree_state(cx);
            assert_eq!(tree_state.selected_node_id.as_deref(), Some("src/main.rs"));
            assert!(tree_state.expanded_ids.contains("src"));
        });

        let _ = fs::remove_file(main_path);
        let _ = fs::remove_file(lib_path);
    }

    #[gpui::test]
    fn input_cache_is_not_rebuilt_during_repeated_draws_without_revision_changes(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        let files = (0..10_000)
            .map(|ix| FileEntry {
                path: PathBuf::from(format!("src/file-{ix}.rs")),
                name: format!("src/file-{ix}.rs"),
                size: ix,
            })
            .collect();

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            let _ = workspace.dispatch(
                WorkspaceAction::Draft(crate::application::store::DraftAction::AddSelectedFiles(
                    files,
                )),
                cx,
            );
            let first = workspace.cached_input_snapshots(cx);
            let second = workspace.cached_input_snapshots(cx);

            assert!(Rc::ptr_eq(&first.0, &second.0));
            assert!(Rc::ptr_eq(&first.1, &second.1));
            assert!(Rc::ptr_eq(&first.2, &second.2));
        });
    }

    #[gpui::test]
    fn rapid_tree_filter_changes_rebuild_only_the_last_projection(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        cx.update_window_entity(&workspace, |workspace: &mut Workspace, window, cx| {
            workspace.set_result(sample_result(), cx);
            for value in ["s", "sr", "src"] {
                workspace
                    .tree_panel
                    .filter_input
                    .update(cx, |input, input_cx| {
                        input.set_value(value, window, input_cx);
                    });
                workspace.schedule_tree_filter_sync(cx);
            }
        });

        workspace.update(cx, |workspace: &mut Workspace, cx| {
            let current = workspace.tree_filter_revision;
            assert!(!workspace.apply_scheduled_tree_filter(current - 2, cx));
            assert!(!workspace.apply_scheduled_tree_filter(current - 1, cx));
            assert!(workspace.apply_scheduled_tree_filter(current, cx));
            assert_eq!(workspace.tree_panel.last_filter, "src");
        });
    }

    #[gpui::test]
    fn desktop_three_column_layout_stays_reachable_at_supported_sizes(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, window) = cx.add_window_view(Workspace::new);
        workspace.update(window, |workspace, cx| {
            let config = AppConfigV1 {
                language: Language::En,
                ..AppConfigV1::default()
            };
            let _ =
                workspace.dispatch(WorkspaceAction::Draft(DraftAction::ApplyConfig(config)), cx);
        });

        for (width, height) in [(1440., 900.), (1366., 768.), (1280., 720.), (1180., 720.)] {
            window.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
            window.update(|window, app| window.draw(app).clear());
            let root = window.debug_bounds("codemerge-root").expect("root bounds");
            let input = window
                .debug_bounds("input-workspace-column")
                .expect("input panel bounds");
            let status = window
                .debug_bounds("status-workspace-column")
                .expect("status panel bounds");
            let results = window
                .debug_bounds("results-workspace-column")
                .expect("results panel bounds");
            let primary = window
                .debug_bounds("primary-process-action-slot")
                .expect("primary action bounds");
            let activity_filters = window
                .debug_bounds("activity-filter-tabs")
                .expect("activity filters bounds");
            let activity_copy = window
                .debug_bounds("copy-all-failures-slot")
                .expect("activity copy bounds");
            let result_tabs = window
                .debug_bounds("result-tabs-slot")
                .expect("result tabs bounds");
            let result_actions = window
                .debug_bounds("result-actions-slot")
                .expect("result actions bounds");
            let tree_search = window
                .debug_bounds("tree-search-slot")
                .expect("tree search bounds");

            for bounds in [
                input,
                status,
                results,
                primary,
                activity_filters,
                activity_copy,
                result_tabs,
                result_actions,
                tree_search,
            ] {
                assert!(
                    root.contains(&bounds.origin),
                    "{width}x{height}: {bounds:?} starts outside {root:?}"
                );
                assert!(
                    bounds.bottom() <= root.bottom(),
                    "{width}x{height}: {bounds:?} extends below {root:?}"
                );
                assert!(
                    bounds.right() <= root.right(),
                    "{width}x{height}: {bounds:?} extends right of {root:?}"
                );
            }
            assert!(input.right() <= status.origin.x);
            assert!(status.right() <= results.origin.x);
            assert!(status.contains(&primary.origin));
            assert!(primary.right() <= status.right());
            assert!(primary.bottom() <= status.bottom());
            assert!(status.contains(&activity_filters.origin));
            assert!(activity_filters.right() <= status.right());
            assert!(status.contains(&activity_copy.origin));
            assert!(activity_copy.right() <= status.right());
            assert!(results.contains(&result_tabs.origin));
            assert!(result_tabs.right() <= results.right());
            assert!(results.contains(&result_actions.origin));
            assert!(result_actions.right() <= results.right());
            assert!(results.contains(&tree_search.origin));
            assert!(tree_search.right() <= results.right());
            assert!(results.size.width >= gpui::px(560.));
        }
    }

    #[gpui::test]
    fn primary_action_switches_between_start_and_cancel_in_place(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        cx.update(|window, app| window.draw(app).clear());
        let start_slot = cx
            .debug_bounds("primary-process-action-slot")
            .expect("start slot bounds");
        assert!(cx.debug_bounds("start-process-action").is_some());

        workspace.update(cx, |workspace, cx| start_test_process(workspace, 77, cx));
        cx.update(|window, app| window.draw(app).clear());
        let cancel_slot = cx
            .debug_bounds("primary-process-action-slot")
            .expect("cancel slot bounds");
        assert_eq!(start_slot, cancel_slot);
        assert!(cx.debug_bounds("cancel-process-action").is_some());

        workspace.update(cx, |workspace, cx| {
            workspace.handle_process_event(ProcessEvent::cancelled(77), cx);
        });
        cx.update(|window, app| window.draw(app).clear());
        assert_eq!(
            cx.debug_bounds("primary-process-action-slot")
                .expect("restored start slot bounds"),
            start_slot
        );
        assert!(cx.debug_bounds("start-process-action").is_some());
    }

    #[gpui::test]
    fn desktop_ui_runs_a_real_merge_from_primary_action(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let source = tempfile::tempdir().expect("create source fixture");
        let first_path = source.path().join("alpha.rs");
        let nested = source.path().join("src");
        fs::create_dir_all(&nested).expect("create nested source directory");
        let second_path = nested.join("beta.rs");
        fs::write(&first_path, "pub fn alpha() -> u8 { 1 }\n").expect("write alpha fixture");
        fs::write(&second_path, "pub fn beta() -> u8 { 2 }\n").expect("write beta fixture");

        let workspace_slot = Rc::new(std::cell::RefCell::new(None));
        let workspace_output = Rc::clone(&workspace_slot);
        let window_handle = cx.add_window(move |window, cx| {
            let workspace = cx.new(|cx| Workspace::new(window, cx));
            workspace_output.replace(Some(workspace.clone()));
            gpui_component::Root::new(workspace, window, cx)
        });
        let workspace = workspace_slot
            .borrow_mut()
            .take()
            .expect("workspace entity");
        let mut visual_context = gpui::VisualTestContext::from_window(window_handle.into(), cx);
        let cx = &mut visual_context;

        workspace.update(cx, |workspace, cx| {
            let _ = workspace.dispatch(
                WorkspaceAction::Draft(DraftAction::ApplyConfig(AppConfigV1::default())),
                cx,
            );
            let _ = workspace.dispatch(
                WorkspaceAction::Draft(DraftAction::AddTemporaryBlacklist {
                    tokens: vec!["never-match".to_string()],
                    as_ext: false,
                }),
                cx,
            );
            let files = [&first_path, &second_path]
                .into_iter()
                .map(|path| FileEntry {
                    path: path.clone(),
                    name: path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    size: fs::metadata(path).expect("fixture metadata").len(),
                })
                .collect();
            let _ = workspace.dispatch(
                WorkspaceAction::Draft(DraftAction::AddSelectedFiles(files)),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, app| window.draw(app).clear());

        let primary = cx
            .debug_bounds("primary-process-action-slot")
            .expect("primary action bounds");
        cx.simulate_click(primary.center(), gpui::Modifiers::default());
        assert!(workspace.update(cx, |workspace, cx| {
            workspace.store.read(cx).draft_locked()
        }));

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            cx.run_until_parked();
            let completed = workspace.update(cx, |workspace, cx| {
                let store = workspace.store.read(cx);
                store.process().ui_status == ProcessUiStatus::Completed
                    && store.result().result.is_some()
            });
            if completed {
                break;
            }
            assert!(Instant::now() < deadline, "real UI merge timed out");
            std::thread::sleep(Duration::from_millis(10));
        }

        let (merged_path, process_dir) = workspace.update(cx, |workspace, cx| {
            let store = workspace.store.read(cx);
            let result = store.result().result.as_ref().expect("merged result");
            assert_eq!(result.stats.processed_files, 2);
            assert!(result.merged_content_bytes > 0);
            assert!(store.selection().temp_folder_blacklist.is_empty());
            assert!(!store.draft_locked());
            (
                result
                    .merged_content_path
                    .clone()
                    .expect("merged content path"),
                result.process_dir.clone().expect("process directory"),
            )
        });
        let merged = fs::read_to_string(&merged_path).expect("read merged content");
        assert!(merged.contains("pub fn alpha"));
        assert!(merged.contains("pub fn beta"));
        cx.update(|window, app| window.draw(app).clear());
        assert!(cx.debug_bounds("start-process-action").is_some());
        assert!(cx.debug_bounds("results-panel-content").is_some());
        assert!(cx.debug_bounds("right-workspace-tabs").is_some());

        workspace.update(cx, |workspace, cx| {
            let _ = workspace.dispatch(
                WorkspaceAction::ClearInputs {
                    ready_label: "ready".to_string(),
                },
                cx,
            );
        });
        assert!(!process_dir.exists());
    }

    #[gpui::test]
    fn activity_virtual_list_scrolls_through_one_thousand_rows_responsively(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        cx.simulate_resize(gpui::size(gpui::px(1440.), gpui::px(900.)));
        workspace.update(cx, |workspace, cx| {
            start_test_process(workspace, 901, cx);
            workspace.handle_process_events(
                (0..1_000)
                    .map(|ix| {
                        ProcessEvent::record(
                            901,
                            ProcessRecord {
                                file_name: format!("src/file-{ix:04}.rs"),
                                status: if ix % 20 == 0 {
                                    ProcessStatus::Failed
                                } else {
                                    ProcessStatus::Success
                                },
                                chars: Some(ix),
                                tokens: Some(ix / 3),
                                error: (ix % 20 == 0).then(|| format!("diagnostic for file {ix}")),
                            },
                        )
                    })
                    .collect(),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, app| window.draw(app).clear());
        let viewport = cx
            .debug_bounds("activity-list-viewport")
            .expect("activity viewport bounds");
        assert!(viewport.size.height > gpui::px(0.));
        let cached_records = workspace.update(cx, |workspace, _| {
            workspace.activity_panel_cache.records.clone()
        });

        let start = Instant::now();
        for _ in 0..30 {
            cx.simulate_event(ScrollWheelEvent {
                position: viewport.center(),
                delta: ScrollDelta::Pixels(point(gpui::px(0.), gpui::px(-1_700.))),
                ..Default::default()
            });
            cx.update(|window, app| window.draw(app).clear());
        }
        let elapsed = start.elapsed();
        println!(
            "activity_scroll_perf rows=1000 frames=30 elapsed_ms={}",
            elapsed.as_millis()
        );
        assert!(
            elapsed < Duration::from_secs(6),
            "30 virtual-list scroll frames took {:?}",
            elapsed
        );
        workspace.update(cx, |workspace, _| {
            assert!(Rc::ptr_eq(
                &cached_records,
                &workspace.activity_panel_cache.records
            ));
        });
        workspace.update(cx, |workspace, cx| {
            workspace.activity_panel_view.update(cx, |view, _| {
                view.scroll_handle.scroll_to_bottom();
            });
        });
        cx.update(|window, app| window.draw(app).clear());
        assert!(
            cx.debug_bounds("activity-record-999").is_some(),
            "the final virtualized activity row should become reachable"
        );
    }

    #[gpui::test]
    fn repeated_desktop_resizes_keep_input_cache_and_tree_projection_stable(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (workspace, cx) = cx.add_window_view(Workspace::new);
        workspace.update(cx, |workspace, cx| {
            let _ = workspace.dispatch(
                WorkspaceAction::Draft(DraftAction::AddSelectedFiles(
                    (0..10_000)
                        .map(|ix| FileEntry {
                            path: PathBuf::from(format!("src/file-{ix}.rs")),
                            name: format!("file-{ix}.rs"),
                            size: ix,
                        })
                        .collect(),
                )),
                cx,
            );
            workspace.set_result(sample_result(), cx);
        });
        cx.update(|window, app| window.draw(app).clear());
        let (before_selection, before_settings, before_projection) =
            workspace.update(cx, |workspace, cx| {
                let snapshots = workspace.cached_input_snapshots(cx);
                (
                    snapshots.0,
                    snapshots.2,
                    (
                        workspace.tree_panel.projection.roots.as_ptr(),
                        workspace.tree_panel.projection.roots.len(),
                        workspace.tree_panel.projection.total_summary,
                    ),
                )
            });

        let start = Instant::now();
        for (width, height) in [(1180., 720.), (1280., 720.), (1366., 768.), (1440., 900.)]
            .into_iter()
            .cycle()
            .take(20)
        {
            cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
            cx.update(|window, app| window.draw(app).clear());
        }
        let elapsed = start.elapsed();
        println!(
            "desktop_resize_perf files=10000 frames=20 elapsed_ms={}",
            elapsed.as_millis()
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "20 desktop resize frames took {:?}",
            elapsed
        );
        workspace.update(cx, |workspace, cx| {
            let after = workspace.cached_input_snapshots(cx);
            assert!(Rc::ptr_eq(&before_selection, &after.0));
            assert!(Rc::ptr_eq(&before_settings, &after.2));
            assert_eq!(
                (
                    workspace.tree_panel.projection.roots.as_ptr(),
                    workspace.tree_panel.projection.roots.len(),
                    workspace.tree_panel.projection.total_summary,
                ),
                before_projection
            );
        });
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
            process_dir: None,
            merged_content_path: None,
            merged_content_bytes: 0,
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

    fn start_test_process(workspace: &mut Workspace, run_id: u64, cx: &mut Context<Workspace>) {
        let _ = workspace.dispatch(
            WorkspaceAction::Execution(ExecutionAction::StartRun {
                run_id,
                scanning_label: "scanning".into(),
            }),
            cx,
        );
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
        result.merged_content_bytes = fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        result.preview_files.clear();
        result
    }

    fn seed_preview_model(
        preview: &StoreSlice<PreviewModel>,
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

    #[gpui::test]
    fn rules_tab_is_mouse_and_keyboard_reachable(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            cx.bind_keys(crate::ui::workspace_key_bindings());
        });
        let workspace_slot = Rc::new(std::cell::RefCell::new(None));
        let workspace_output = Rc::clone(&workspace_slot);
        let window_handle = cx.add_window(move |window, cx| {
            let workspace = cx.new(|cx| Workspace::new(window, cx));
            workspace_output.replace(Some(workspace.clone()));
            gpui_component::Root::new(workspace, window, cx)
        });
        let workspace = workspace_slot
            .borrow_mut()
            .take()
            .expect("workspace entity");
        let mut visual_context = gpui::VisualTestContext::from_window(window_handle.into(), cx);
        let cx = &mut visual_context;
        cx.run_until_parked();
        cx.update_window_entity(&workspace, |workspace: &mut Workspace, window, cx| {
            let _ = cx;
            workspace.focus_handle.focus(window);
        });
        cx.update(|window, app| window.draw(app).clear());

        let rules_tab = cx
            .debug_bounds("rules-tab-target")
            .expect("rules tab bounds");
        cx.simulate_click(rules_tab.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(
            workspace.update(cx, |workspace, cx| workspace.ui_state(cx).right_tab),
            crate::ui::state::WorkspaceRightTab::Rules
        );
        cx.update(|window, app| window.draw(app).clear());
        assert!(cx.debug_bounds("rules-workspace-panel").is_some());

        let results_tab = cx
            .debug_bounds("results-tab-target")
            .expect("results tab bounds");
        cx.simulate_click(results_tab.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(
            workspace.update(cx, |workspace, cx| workspace.ui_state(cx).right_tab),
            crate::ui::state::WorkspaceRightTab::Results
        );

        cx.simulate_keystrokes("ctrl-,");
        cx.run_until_parked();
        assert_eq!(
            workspace.update(cx, |workspace, cx| workspace.ui_state(cx).right_tab),
            crate::ui::state::WorkspaceRightTab::Rules
        );

        cx.simulate_keystrokes("ctrl-f");
        cx.run_until_parked();
        assert!(
            cx.update_window_entity(&workspace, |workspace, window, cx| {
                workspace
                    .tree_panel
                    .filter_input
                    .focus_handle(cx)
                    .is_focused(window)
            })
        );
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

    fn write_large_preview_fixture(prefix: &str, min_byte_len: usize) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{prefix}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create large preview fixture dir");
        let path = root.join("merged.txt");
        let line = "line-0123456789abcdef\n";
        let repeat_count = min_byte_len / line.len() + 2;
        fs::write(&path, line.repeat(repeat_count)).expect("write large preview fixture");
        path
    }

    fn write_single_line_preview_fixture(prefix: &str, line_len: usize) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{prefix}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create single line preview fixture dir");
        let path = root.join("merged.txt");
        fs::write(&path, format!("{}\n", "a".repeat(line_len)))
            .expect("write single line preview fixture");
        path
    }

    fn write_dense_preview_fixture(prefix: &str, line_count: usize) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{prefix}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create dense preview fixture dir");
        let path = root.join("merged.txt");
        fs::write(&path, "a\n".repeat(line_count)).expect("write dense preview fixture");
        path
    }
}
