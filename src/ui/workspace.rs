mod actions;
mod background;
mod model;
mod panels;
mod tree_palette;
mod view;

use std::rc::Rc;

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, ParentElement,
    Pixels, Render, SharedString, Styled, Subscription, Task, Timer, UniformListScrollHandle,
    Window, px, size,
};
use gpui_component::{
    WindowExt as _,
    input::InputState,
    notification::NotificationType,
    table::{Column, TableDelegate, TableState},
    tree::TreeState,
};

use crate::domain::{Language, PreviewRowViewModel, ProcessResult};
use crate::services::settings::{self, ConfigLoadIssue};
use crate::ui::state::{AppState, ProcessUiStatus};
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
pub(super) enum BlacklistItemKind {
    Folder,
    Ext,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SidePanelTab {
    Results,
    Rules,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NarrowContentTab {
    Status,
    Results,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingConfirmation {
    ClearInputs,
    ResetBlacklist,
    ClearBlacklist,
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
    render_state: model::TreeRenderState,
    last_interaction: Option<model::TreeInteractionSnapshot>,
}

pub struct Workspace {
    focus_handle: FocusHandle,
    state: AppState,
    preview_scroll_handle: UniformListScrollHandle,
    tree_panel: TreePanelController,
    preview_table: Entity<TableState<PreviewTableDelegate>>,
    preview_filter_input: Entity<InputState>,
    blacklist_filter_input: Entity<InputState>,
    blacklist_add_input: Entity<InputState>,
    side_panel_tab: SidePanelTab,
    narrow_content_tab: NarrowContentTab,
    pending_confirmation: Option<PendingConfirmation>,
    poll_task: Option<Task<()>>,
    poll_task_running: bool,
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
        let subscriptions = vec![
            cx.subscribe_in(&tree_filter_input, window, Self::on_tree_filter_event),
            cx.subscribe_in(&preview_filter_input, window, Self::on_preview_filter_event),
            cx.subscribe_in(
                &blacklist_filter_input,
                window,
                Self::on_blacklist_filter_event,
            ),
            cx.subscribe_in(&preview_table, window, Self::on_preview_table_event),
            cx.observe(&tree_state, |this, _, cx| {
                let dirty = this.sync_tree_interaction(cx);
                if dirty {
                    cx.notify();
                }
            }),
        ];
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            state: AppState::from_config(cfg.clone(), tr(cfg.language, "status_ready").to_string()),
            preview_scroll_handle: UniformListScrollHandle::new(),
            tree_panel: TreePanelController {
                state: tree_state,
                filter_input: tree_filter_input,
                data: None,
                render_state: model::TreeRenderState::default(),
                last_interaction: None,
            },
            preview_table,
            preview_filter_input,
            blacklist_filter_input,
            blacklist_add_input,
            side_panel_tab: SidePanelTab::Results,
            narrow_content_tab: NarrowContentTab::Status,
            pending_confirmation: None,
            poll_task: None,
            poll_task_running: false,
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
        if self.poll_task_running || !self.needs_background_polling() {
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

    fn needs_background_polling(&self) -> bool {
        self.state.process.preflight_rx.is_some()
            || self.state.process.process_handle.is_some()
            || self.state.workspace.preview_panel.preview_rx.is_some()
            || (self.state.result.active_tab == crate::domain::ResultTab::Content
                && self
                    .state
                    .workspace
                    .preview_panel
                    .preview_document
                    .is_some())
    }

    fn has_inputs(&self) -> bool {
        self.state.selection.selected_folder.is_some()
            || !self.state.selection.selected_files.is_empty()
    }

    fn is_processing(&self) -> bool {
        self.state.process.process_handle.is_some()
    }

    fn clear_pending_confirmation(&mut self) {
        self.pending_confirmation = None;
    }

    pub(super) fn cleanup_result_artifacts(result: &ProcessResult) {
        if let Some(path) = &result.merged_content_path {
            let _ = std::fs::remove_file(path);
        }
        if let Some(dir) = &result.preview_blob_dir {
            let _ = crate::utils::temp_file::cleanup_preview_dir(dir);
        }
    }

    pub(super) fn cleanup_current_result_artifacts(&self) {
        if let Some(result) = self.state.result.result.as_ref() {
            Self::cleanup_result_artifacts(result);
        }
    }

    fn sync_localized_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.state.settings.language;
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
        if !self.is_processing() && self.state.process.ui_status == ProcessUiStatus::Idle {
            self.state.process.processing_current_file = tr(language, "status_ready").to_string();
        }
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
        if let Some(result) = self.state.result.result.as_ref() {
            Self::cleanup_result_artifacts(result);
        }
    }
}
