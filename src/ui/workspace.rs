mod actions;
mod background;
mod model;
mod panels;
mod tree_palette;
mod view;

use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    ParentElement, Pixels, Render, SharedString, Styled, Subscription, Task, Timer,
    UniformListScrollHandle, Window, px, size,
};
use gpui_component::{
    WindowExt as _,
    input::InputState,
    notification::NotificationType,
    table::{Column, TableDelegate, TableState},
    tree::TreeState,
};

use crate::domain::{Language, PreviewRowViewModel};
use crate::services::settings::{self, ConfigLoadIssue};
use crate::ui::models::{ProcessModel, SettingsModel, WorkspaceUiModel};
use crate::ui::preview_model::PreviewModel;
use crate::ui::result_model::ResultModel;
use crate::ui::selection_model::SelectionModel;
use crate::ui::state::{AppState, ProcessUiStatus, WorkspaceUiState};
use crate::utils::i18n::tr;

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
    columns: Vec<Column>,
    rows: Vec<PreviewRowViewModel>,
}

impl PreviewTableDelegate {
    fn new(language: Language) -> Self {
        Self {
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
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        match col_ix {
            0 => SharedString::from(row.display_path.clone()).into_any_element(),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspacePanelKind {
    Right,
    CompactContent,
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
    _subscriptions: Vec<Subscription>,
}

struct PreviewPaneView {
    workspace: Entity<Workspace>,
    preview: Entity<PreviewModel>,
    result: Entity<ResultModel>,
    settings: Entity<SettingsModel>,
    scroll_handle: UniformListScrollHandle,
    last_visible_bucket: std::ops::Range<usize>,
    _subscriptions: Vec<Subscription>,
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
            cx.observe(&ui, |_, _, cx| cx.notify()),
            cx.observe(&settings, |_, _, cx| cx.notify()),
        ];
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
            cx.observe(&settings, |_, _, cx| cx.notify()),
            cx.observe(&ui, |_, _, cx| cx.notify()),
            cx.observe(&selection, |_, _, cx| cx.notify()),
        ];
        Self {
            workspace,
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
            cx.observe(&process, |_, _, cx| cx.notify()),
            cx.observe(&result, |_, _, cx| cx.notify()),
            cx.observe(&settings, |_, _, cx| cx.notify()),
        ];
        Self {
            workspace,
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
            cx.observe(&settings, |_, _, cx| cx.notify()),
            cx.observe(&ui, |_, _, cx| cx.notify()),
            cx.observe(&blacklist_filter_input, |_, _, cx| cx.notify()),
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
        result: Entity<ResultModel>,
        settings: Entity<SettingsModel>,
        preview_filter_input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&result, |_, _, cx| cx.notify()),
            cx.observe(&settings, |_, _, cx| cx.notify()),
            cx.observe(&preview_filter_input, |_, _, cx| cx.notify()),
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
        result: Entity<ResultModel>,
        settings: Entity<SettingsModel>,
        tree_state: Entity<TreeState>,
        tree_filter_input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let tree_workspace = workspace.clone();
        let subscriptions = vec![
            cx.observe(&result, |_, _, cx| cx.notify()),
            cx.observe(&settings, |_, _, cx| cx.notify()),
            cx.observe(&tree_state, move |_, _, cx| {
                tree_workspace.update(cx, |workspace, workspace_cx| {
                    let _ = workspace.sync_tree_interaction(workspace_cx);
                });
                cx.notify();
            }),
            cx.observe(&tree_filter_input, |_, _, cx| cx.notify()),
        ];
        Self {
            workspace,
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
            cx.observe(&preview, |_, _, cx| cx.notify()),
            cx.observe(&result, |_, _, cx| cx.notify()),
            cx.observe(&settings, |_, _, cx| cx.notify()),
        ];
        Self {
            workspace,
            preview,
            result,
            settings,
            scroll_handle: UniformListScrollHandle::new(),
            last_visible_bucket: 0..0,
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_preview_pane(cx)
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
            .p_4()
            .gap_4()
            .child(self.render_header(cx))
            .child(
                gpui::div()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.render_main_content(window, cx)),
            )
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        Self::cleanup_result_artifacts(&self.result_artifacts);
    }
}
