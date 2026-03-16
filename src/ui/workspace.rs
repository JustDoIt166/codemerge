mod actions;
mod background;

use std::rc::Rc;
use std::time::Duration;

use arboard::Clipboard;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, SharedString, Styled, Subscription, Task, Timer,
    Window, div, prelude::FluentBuilder as _, px, size,
};
use gpui_component::{
    ActiveTheme as _, Disableable, IconName, Sizable, Size, StyledExt as _,
    VirtualListScrollHandle, WindowExt as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    list::ListItem,
    notification::NotificationType,
    resizable::{h_resizable, resizable_panel},
    tab::{Tab, TabBar},
    table::{Column, Table, TableDelegate, TableState},
    tree::{TreeItem, TreeState, tree},
    v_flex, v_virtual_list,
};

use crate::domain::{FileEntry, Language, PreviewRowViewModel, ProcessStatus, ResultTab, TreeNode};
use crate::services::settings;
use crate::ui::state::{AppState, ProcessUiStatus};
use crate::utils::i18n::tr;

fn preview_line_height() -> Pixels {
    px(22.)
}

fn fixed_list_sizes(len: usize, height: Pixels) -> Rc<Vec<gpui::Size<Pixels>>> {
    Rc::new((0..len).map(|_| size(px(100.), height)).collect::<Vec<_>>())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlacklistItemKind {
    Folder,
    Ext,
}

#[derive(Clone)]
struct BlacklistItemViewModel {
    kind: BlacklistItemKind,
    value: String,
    display_label: SharedString,
    deletable: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidePanelTab {
    Results,
    Rules,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NarrowContentTab {
    Status,
    Results,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingConfirmation {
    ClearInputs,
    ResetBlacklist,
    ClearBlacklist,
}

struct PreviewTableDelegate {
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

pub struct Workspace {
    focus_handle: FocusHandle,
    state: AppState,
    preview_scroll_handle: VirtualListScrollHandle,
    tree_state: Entity<TreeState>,
    tree_filter_input: Entity<InputState>,
    preview_table: Entity<TableState<PreviewTableDelegate>>,
    preview_filter_input: Entity<InputState>,
    blacklist_filter_input: Entity<InputState>,
    blacklist_add_input: Entity<InputState>,
    side_panel_tab: SidePanelTab,
    narrow_content_tab: NarrowContentTab,
    pending_confirmation: Option<PendingConfirmation>,
    _poll_task: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let cfg = settings::load();
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
        ];
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(33)).await;
                let _ = this.update(cx, |this, cx| this.poll_background(cx));
            }
        });
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            state: AppState::from_config(cfg.clone(), tr(cfg.language, "status_ready").to_string()),
            preview_scroll_handle: VirtualListScrollHandle::new(),
            tree_state,
            tree_filter_input,
            preview_table,
            preview_filter_input,
            blacklist_filter_input,
            blacklist_add_input,
            side_panel_tab: SidePanelTab::Results,
            narrow_content_tab: NarrowContentTab::Status,
            pending_confirmation: None,
            _poll_task: poll_task,
            _subscriptions: subscriptions,
        };
        this.refresh_preflight();
        this
    }

    fn has_inputs(&self) -> bool {
        self.state.selection.selected_folder.is_some() || !self.state.selection.selected_files.is_empty()
    }

    fn is_processing(&self) -> bool {
        self.state.process.process_handle.is_some()
    }

    fn clear_pending_confirmation(&mut self) {
        self.pending_confirmation = None;
    }

    fn sync_localized_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.state.settings.language;
        self.tree_filter_input
            .update(cx, |state, cx| state.set_placeholder(tr(language, "tree_filter"), window, cx));
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

    fn build_blacklist_rows(&self, cx: &Context<Self>) -> Rc<Vec<BlacklistItemViewModel>> {
        let filter = self
            .blacklist_filter_input
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        Rc::new(
            self.state
                .settings
                .folder_blacklist
                .iter()
                .map(|item| BlacklistItemViewModel {
                    kind: BlacklistItemKind::Folder,
                    value: item.clone(),
                    display_label: SharedString::from(format!("folder: {item}")),
                    deletable: true,
                })
                .chain(self.state.settings.ext_blacklist.iter().map(|item| BlacklistItemViewModel {
                    kind: BlacklistItemKind::Ext,
                    value: item.clone(),
                    display_label: SharedString::from(format!("ext: {item}")),
                    deletable: true,
                }))
                .filter(|item| {
                    filter.is_empty() || item.display_label.to_ascii_lowercase().contains(filter.as_str())
                })
                .collect::<Vec<_>>(),
        )
    }

    fn render_header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        h_flex()
            .justify_between()
            .items_end()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xl().font_semibold().child("CodeMerge"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(language, "app_subtitle")),
                    ),
            )
            .child(
                Button::new("toggle-language")
                    .outline()
                    .label(match language {
                        Language::Zh => "EN",
                        Language::En => "中文",
                    })
                    .on_click(cx.listener(Self::toggle_language)),
            )
    }

    fn render_input_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let has_inputs = self.has_inputs();
        let is_processing = self.is_processing();
        let selected_files = Rc::new(self.state.selection.selected_files.clone());
        let folder_label = self
            .state
            .selection
            .selected_folder
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| tr(language, "input_folder_empty").to_string());
        let gitignore_label = self
            .state
            .selection
            .gitignore_file
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| tr(language, "gitignore_auto_hint").to_string());

        card(cx).size_full().child(
            v_flex()
                .gap_4()
                .size_full()
                .child(section_title(tr(language, "panel_inputs"), cx))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("select-folder")
                                .primary()
                                .label(tr(language, "select_folder"))
                                .on_click(cx.listener(Self::select_folder)),
                        )
                        .child(
                            Button::new("select-files")
                                .outline()
                                .label(tr(language, "select_files"))
                                .on_click(cx.listener(Self::select_files)),
                        ),
                )
                .child(render_info_block(
                    tr(language, "folder"),
                    folder_label,
                    has_inputs,
                    cx,
                ))
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(section_caption(tr(language, "selected_files_title"), cx))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(self.state.selection.selected_files.len().to_string()),
                                ),
                        )
                        .child(if selected_files.is_empty() {
                            empty_box(
                                tr(language, "selected_files_empty"),
                                tr(language, "selected_files_hint"),
                                IconName::File,
                                cx,
                            )
                            .into_any_element()
                        } else {
                            let rows = selected_files.clone();
                            div()
                                .h(px(180.))
                                .border_1()
                                .border_color(cx.theme().border)
                                .rounded(px(12.))
                                .bg(cx.theme().secondary.opacity(0.22))
                                .child(
                                    v_virtual_list(
                                        cx.entity().clone(),
                                        "selected-files",
                                        fixed_list_sizes(rows.len(), px(52.)),
                                        move |_, visible_range, _, cx| {
                                            visible_range
                                                .filter_map(|ix| rows.get(ix))
                                                .map(|entry| selected_file_row(entry, cx))
                                                .collect::<Vec<_>>()
                                        },
                                    )
                                    .p_1(),
                                )
                                .into_any_element()
                        }),
                )
                .child(section_title(tr(language, "panel_gitignore"), cx))
                .child(render_info_block(
                    tr(language, "gitignore"),
                    gitignore_label,
                    self.state.selection.gitignore_file.is_some(),
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("select-gitignore")
                                .outline()
                                .label(tr(language, "select_gitignore"))
                                .on_click(cx.listener(Self::select_gitignore)),
                        )
                        .child(
                            Button::new("apply-gitignore")
                                .outline()
                                .label(tr(language, "apply_gitignore"))
                                .disabled(self.state.selection.gitignore_file.is_none())
                                .on_click(cx.listener(Self::apply_gitignore)),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(language, "gitignore_apply_hint")),
                )
                .child(section_title(tr(language, "section_options"), cx))
                .child(
                    Checkbox::new("compress")
                        .checked(self.state.settings.options.compress)
                        .label(tr(language, "compress"))
                        .on_click(cx.listener(Self::toggle_compress)),
                )
                .child(
                    Checkbox::new("use-gitignore")
                        .checked(self.state.settings.options.use_gitignore)
                        .label(tr(language, "use_gitignore"))
                        .on_click(cx.listener(Self::toggle_use_gitignore)),
                )
                .child(
                    Checkbox::new("ignore-git")
                        .checked(self.state.settings.options.ignore_git)
                        .label(tr(language, "ignore_git"))
                        .on_click(cx.listener(Self::toggle_ignore_git)),
                )
                .child(
                    Checkbox::new("dedupe")
                        .checked(self.state.selection.dedupe_exact_path)
                        .label(tr(language, "dedupe_exact_path"))
                        .on_click(cx.listener(Self::toggle_dedupe)),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("start-process")
                                .primary()
                                .label(tr(language, "start"))
                                .disabled(!has_inputs || is_processing)
                                .on_click(cx.listener(Self::start_process)),
                        )
                        .child(
                            Button::new("cancel-process")
                                .outline()
                                .label(tr(language, "cancel"))
                                .disabled(!is_processing)
                                .on_click(cx.listener(Self::cancel_process)),
                        ),
                )
                .child(
                    div()
                        .pt_2()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(
                            v_flex()
                                .gap_2()
                                .child(section_caption(tr(language, "danger_zone"), cx))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(tr(language, "danger_zone_hint")),
                                )
                                .child(
                                    Button::new("clear-inputs")
                                        .danger()
                                        .label(if self.pending_confirmation
                                            == Some(PendingConfirmation::ClearInputs)
                                        {
                                            tr(language, "confirm_clear_inputs")
                                        } else {
                                            tr(language, "clear")
                                        })
                                        .on_click(cx.listener(Self::clear_inputs)),
                                ),
                        ),
                ),
        )
    }

    fn render_status_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let result_stats = self.state.result.result.as_ref().map(|result| &result.stats);
        let processed_count = self.state.process.processing_records.len();
        let failed_count = self
            .state
            .process
            .processing_records
            .iter()
            .filter(|record| matches!(record.status, ProcessStatus::Failed))
            .count();
        let activity_rows = Rc::new(
            self.state
                .process
                .processing_records
                .iter()
                .rev()
                .take(16)
                .cloned()
                .collect::<Vec<_>>(),
        );
        let progress_total = self
            .state
            .process
            .processing_candidates
            .max(self.state.process.preflight.to_process_files)
            .max(1);
        let progress_value = processed_count.min(progress_total);
        let progress_ratio = progress_value as f32 / progress_total as f32;
        let bar_fill = px((progress_ratio * 240.0).round());
        let elapsed = self
            .state
            .process
            .processing_started_at
            .map(|start| format_duration(start.elapsed()))
            .unwrap_or_else(|| "--:--".to_string());

        card(cx).size_full().child(
            v_flex()
                .gap_4()
                .size_full()
                .child(section_title(tr(language, "panel_status"), cx))
                .child(
                    h_flex()
                        .gap_2()
                        .child(stat_tile(tr(language, "total"), self.state.process.preflight.total_files.to_string(), cx))
                        .child(stat_tile(tr(language, "process"), self.state.process.preflight.to_process_files.to_string(), cx))
                        .child(stat_tile(tr(language, "skip"), self.state.process.preflight.skipped_files.to_string(), cx)),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(stat_tile(tr(language, "chars"), result_stats.map(|stats| stats.total_chars.to_string()).unwrap_or_else(|| "--".to_string()), cx))
                        .child(stat_tile(tr(language, "tokens"), result_stats.map(|stats| stats.total_tokens.to_string()).unwrap_or_else(|| "--".to_string()), cx))
                        .child(stat_tile(tr(language, "failed_count"), failed_count.to_string(), cx)),
                )
                .child(status_banner(
                    process_status_title(self.state.process.ui_status, language),
                    process_status_message(self, language),
                    self.state.process.ui_status,
                    cx,
                ))
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(section_caption(tr(language, "progress_overview"), cx))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!("{progress_value}/{progress_total}")),
                                ),
                        )
                        .child(
                            div()
                                .w(px(240.))
                                .h(px(10.))
                                .rounded(px(999.))
                                .bg(cx.theme().secondary)
                                .child(
                                    div()
                                        .h_full()
                                        .w(bar_fill)
                                        .rounded(px(999.))
                                        .bg(cx.theme().primary),
                                ),
                        )
                        .child(render_kv(tr(language, "elapsed"), elapsed, cx))
                        .child(render_kv(tr(language, "processing"), self.state.process.processing_current_file.clone(), cx)),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .flex_1()
                        .child(section_caption(tr(language, "recent_activity"), cx))
                        .child(if activity_rows.is_empty() {
                            empty_box(
                                tr(language, "activity_empty"),
                                tr(language, "activity_empty_hint"),
                                IconName::File,
                                cx,
                            )
                            .into_any_element()
                        } else {
                            let rows = activity_rows.clone();
                            div()
                                .flex_1()
                                .border_1()
                                .border_color(cx.theme().border)
                                .rounded(px(12.))
                                .bg(cx.theme().secondary.opacity(0.22))
                                .child(
                                    v_virtual_list(
                                        cx.entity().clone(),
                                        "activity-rows",
                                        fixed_list_sizes(rows.len(), px(38.)),
                                        move |_, visible_range, _, cx| {
                                            visible_range
                                                .filter_map(|ix| rows.get(ix))
                                                .map(|record| activity_row(record, cx))
                                                .collect::<Vec<_>>()
                                        },
                                    )
                                    .p_1(),
                                )
                                .into_any_element()
                        }),
                ),
        )
    }

    fn render_right_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let selected_index = if self.side_panel_tab == SidePanelTab::Results {
            0
        } else {
            1
        };

        card(cx).size_full().child(
            v_flex()
                .gap_3()
                .size_full()
                .child(
                    TabBar::new("side-panel-tabs")
                        .selected_index(selected_index)
                        .on_click(cx.listener(Self::set_side_panel_tab))
                        .child(Tab::new().label(tr(language, "panel_results")))
                        .child(Tab::new().label(tr(language, "panel_rules"))),
                )
                .child(match self.side_panel_tab {
                    SidePanelTab::Results => self.render_results_panel(cx).into_any_element(),
                    SidePanelTab::Rules => self.render_rules_panel(cx).into_any_element(),
                }),
        )
    }

    fn render_results_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let selected_tab = if self.state.result.active_tab == ResultTab::Tree {
            0
        } else {
            1
        };

        v_flex()
            .gap_3()
            .size_full()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        TabBar::new("result-tabs")
                            .selected_index(selected_tab)
                            .on_click(cx.listener(Self::set_tab))
                            .child(Tab::new().label(tr(language, "tab_tree_preview")))
                            .child(Tab::new().label(tr(language, "tab_merged_content"))),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("copy-active")
                                    .outline()
                                    .label(if self.state.result.active_tab == ResultTab::Tree {
                                        tr(language, "copy_tree")
                                    } else {
                                        tr(language, "copy_current_page")
                                    })
                                    .on_click(cx.listener(
                                        if self.state.result.active_tab == ResultTab::Tree {
                                            Self::copy_tree
                                        } else {
                                            Self::copy_preview
                                        },
                                    )),
                            )
                            .child(
                                Button::new("download-result")
                                    .outline()
                                    .label(tr(language, "download"))
                                    .disabled(self.state.result.active_tab == ResultTab::Tree)
                                    .on_click(cx.listener(Self::download_result)),
                            ),
                    ),
            )
            .child(match self.state.result.active_tab {
                ResultTab::Tree => self.render_tree_panel(cx).into_any_element(),
                ResultTab::Content => self.render_content_panel(cx).into_any_element(),
            })
    }

    fn render_rules_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let blacklist_rows = self.build_blacklist_rows(cx);

        v_flex()
            .gap_3()
            .size_full()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(language, "rules_secondary_hint")),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(Input::new(&self.blacklist_add_input))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("add-folder-blacklist")
                                    .outline()
                                    .label(tr(language, "add_folder"))
                                    .on_click(cx.listener(Self::add_folder_blacklist)),
                            )
                            .child(
                                Button::new("add-ext-blacklist")
                                    .outline()
                                    .label(tr(language, "add_ext"))
                                    .on_click(cx.listener(Self::add_ext_blacklist)),
                            ),
                    ),
            )
            .child(Input::new(&self.blacklist_filter_input).cleanable(true))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("import-blacklist")
                            .outline()
                            .label(tr(language, "blacklist_import_append"))
                            .on_click(cx.listener(Self::import_blacklist)),
                    )
                    .child(
                        Button::new("export-blacklist")
                            .outline()
                            .label(tr(language, "blacklist_export"))
                            .on_click(cx.listener(Self::export_blacklist)),
                    ),
            )
            .child(if blacklist_rows.is_empty() {
                empty_box(
                    tr(language, "blacklist_empty_title"),
                    tr(language, "blacklist_empty_hint"),
                    IconName::Folder,
                    cx,
                )
                .into_any_element()
            } else {
                let rows = blacklist_rows.clone();
                div()
                    .flex_1()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(px(12.))
                    .bg(cx.theme().secondary.opacity(0.22))
                    .child(
                        v_virtual_list(
                            cx.entity().clone(),
                            "blacklist-items",
                            fixed_list_sizes(rows.len(), px(44.)),
                            move |_, visible_range, _, cx| {
                                visible_range
                                    .filter_map(|ix| rows.get(ix).cloned().map(|item| (ix, item)))
                                    .map(|(ix, item)| {
                                        let kind = item.kind;
                                        let value = item.value.clone();
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .px_3()
                                            .h(px(44.))
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        pill_label(
                                                            match item.kind {
                                                                BlacklistItemKind::Folder => {
                                                                    tr(language, "folder")
                                                                }
                                                                BlacklistItemKind::Ext => {
                                                                    tr(language, "extension")
                                                                }
                                                            },
                                                            cx,
                                                        ),
                                                    )
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .child(item.display_label.clone()),
                                                    ),
                                            )
                                            .child(
                                                Button::new(("remove-blacklist", ix))
                                                    .outline()
                                                    .label(tr(language, "remove_tag"))
                                                    .disabled(!item.deletable)
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.remove_blacklist_item(
                                                                kind,
                                                                value.clone(),
                                                                window,
                                                                cx,
                                                            );
                                                        },
                                                    )),
                                            )
                                            .into_any_element()
                                    })
                                    .collect::<Vec<_>>()
                            },
                        )
                        .p_1(),
                    )
                    .into_any_element()
            })
            .child(
                div()
                    .pt_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("reset-blacklist")
                                    .outline()
                                    .label(if self.pending_confirmation
                                        == Some(PendingConfirmation::ResetBlacklist)
                                    {
                                        tr(language, "confirm_reset_blacklist")
                                    } else {
                                        tr(language, "blacklist_reset_default")
                                    })
                                    .on_click(cx.listener(Self::reset_blacklist)),
                            )
                            .child(
                                Button::new("clear-blacklist")
                                    .danger()
                                    .label(if self.pending_confirmation
                                        == Some(PendingConfirmation::ClearBlacklist)
                                    {
                                        tr(language, "confirm_clear_blacklist")
                                    } else {
                                        tr(language, "blacklist_clear_all")
                                    })
                                    .on_click(cx.listener(Self::clear_blacklist)),
                            ),
                    ),
            )
    }

    fn render_tree_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let tree_filter = self.tree_filter_input.read(cx).value().trim().to_string();
        let filter_active = !tree_filter.is_empty();
        let visible_summary = self
            .state
            .result
            .result
            .as_ref()
            .map(|result| summarize_visible_tree(&result.tree_nodes, tree_filter.as_str()))
            .unwrap_or_default();
        let has_result = self.state.result.result.is_some();
        let has_visible_nodes = visible_summary.total() > 0;
        let view = cx.entity();
        let tree_view =
            tree(&self.tree_state, move |ix, entry, selected, _, cx| {
                view.update(cx, |_, cx| {
                    let item = entry.item();
                    let chevron = if entry.is_folder() {
                        if entry.is_expanded() {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        }
                    } else {
                        IconName::Dash
                    };
                    let icon = if !entry.is_folder() {
                        IconName::File
                    } else if entry.is_expanded() {
                        IconName::FolderOpen
                    } else {
                        IconName::Folder
                    };
                    let guide_color = if selected {
                        cx.theme().primary.opacity(0.35)
                    } else {
                        cx.theme().border.opacity(0.65)
                    };
                    let icon_color = if selected {
                        cx.theme().primary_foreground
                    } else if entry.is_folder() && entry.is_expanded() {
                        cx.theme().primary
                    } else if entry.is_folder() {
                        cx.theme().foreground
                    } else {
                        cx.theme().muted_foreground
                    };
                    ListItem::new(ix)
                        .w_full()
                        .h(px(32.))
                        .rounded(px(10.))
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .gap_1()
                                .children((0..entry.depth()).map(|_| {
                                    div()
                                        .w(px(12.))
                                        .h(px(22.))
                                        .items_center()
                                        .justify_center()
                                        .child(div().w(px(1.)).h_full().bg(guide_color))
                                        .into_any_element()
                                }))
                                .child(
                                    div()
                                        .w(px(14.))
                                        .items_center()
                                        .justify_center()
                                        .text_color(if entry.is_folder() {
                                            icon_color
                                        } else {
                                            cx.theme().muted_foreground.opacity(0.35)
                                        })
                                        .child(chevron),
                                )
                                .child(div().w(px(3.)).h(px(18.)).rounded(px(999.)).bg(
                                    if selected {
                                        cx.theme().primary
                                    } else if entry.is_folder() && entry.is_expanded() {
                                        cx.theme().border
                                    } else {
                                        cx.theme().transparent
                                    },
                                ))
                                .child(
                                    div()
                                        .w(px(22.))
                                        .h(px(22.))
                                        .rounded(px(6.))
                                        .items_center()
                                        .justify_center()
                                        .bg(if entry.is_folder() {
                                            if selected {
                                                cx.theme().primary_foreground.opacity(0.15)
                                            } else {
                                                cx.theme().secondary
                                            }
                                        } else {
                                            cx.theme().transparent
                                        })
                                        .text_color(icon_color)
                                        .child(icon),
                                )
                                .child(
                                    div().min_w(px(0.)).flex_1().overflow_hidden().child(
                                        div()
                                            .truncate()
                                            .whitespace_nowrap()
                                            .text_color(if selected {
                                                cx.theme().primary_foreground
                                            } else {
                                                cx.theme().foreground
                                            })
                                            .when(entry.is_folder(), |this| this.font_semibold())
                                            .child(item.label.clone()),
                                    ),
                                ),
                        )
                })
            })
            .h_full();

        v_flex()
            .gap_3()
            .size_full()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Input::new(&self.tree_filter_input)
                            .prefix(IconName::Search)
                            .cleanable(true),
                    )
                    .child(
                        Button::new("tree-expand")
                            .outline()
                            .icon(IconName::ChevronDown)
                            .label(tr(language, "tree_expand_all"))
                            .disabled(!has_result || filter_active)
                            .on_click(cx.listener(Self::expand_tree)),
                    )
                    .child(
                        Button::new("tree-collapse")
                            .outline()
                            .icon(IconName::ChevronRight)
                            .label(tr(language, "tree_collapse_all"))
                            .disabled(!has_result || filter_active)
                            .on_click(cx.listener(Self::collapse_tree)),
                    ),
            )
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .px_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} {} · {} {}",
                                visible_summary.folders,
                                tr(language, "folders"),
                                visible_summary.files,
                                tr(language, "files")
                            )),
                    )
                    .child(
                        pill_label(
                            if filter_active {
                                tr(language, "tree_filter_active")
                            } else {
                                tr(language, "tree_filter_idle")
                            },
                            cx,
                        ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(px(14.))
                    .bg(cx.theme().secondary.opacity(0.35))
                    .p_2()
                    .child(if has_visible_nodes {
                        tree_view.into_any_element()
                    } else {
                        empty_box(
                            if has_result {
                                tr(language, "tree_no_match")
                            } else {
                                tr(language, "tree_empty")
                            },
                            if has_result && filter_active {
                                tr(language, "tree_no_match_hint")
                            } else {
                                tr(language, "tree_empty_hint")
                            },
                            IconName::FolderOpen,
                            cx,
                        )
                        .into_any_element()
                    }),
            )
    }

    fn render_content_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let has_result = self.state.result.result.is_some();
        let has_rows = !self.state.result.preview_rows.is_empty();

        v_flex()
            .gap_3()
            .size_full()
            .child(Input::new(&self.preview_filter_input).cleanable(true))
            .child(if has_result && has_rows {
                div()
                    .h(px(220.))
                    .child(
                        Table::new(&self.preview_table)
                            .with_size(Size::Small)
                            .stripe(true),
                    )
                    .into_any_element()
            } else {
                empty_box(
                    if has_result {
                        tr(language, "content_no_match")
                    } else {
                        tr(language, "content_empty")
                    },
                    if has_result {
                        tr(language, "content_no_match_hint")
                    } else {
                        tr(language, "content_empty_hint")
                    },
                    IconName::File,
                    cx,
                )
                .into_any_element()
            })
            .child(self.render_preview(cx))
    }

    fn render_preview(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let selected_preview = self
            .state
            .workspace
            .selected_preview_file_id
            .and_then(|id| self.state.result.preview_rows.iter().find(|row| row.id == id));

        let Some(document) = &self.state.workspace.preview_document else {
            return empty_box(
                tr(language, "preview_empty"),
                tr(language, "preview_empty_hint"),
                IconName::File,
                cx,
            );
        };

        let file_path = selected_preview
            .map(|row| row.display_path.clone())
            .unwrap_or_else(|| tr(language, "preview_unknown_path").to_string());

        v_flex()
            .gap_2()
            .flex_1()
            .child(render_kv(tr(language, "table_path"), file_path, cx))
            .child(
                h_flex()
                    .gap_3()
                    .child(render_kv(
                        tr(language, "line_count"),
                        document.line_count().to_string(),
                        cx,
                    ))
                    .child(render_kv(
                        tr(language, "byte_size"),
                        document.byte_len().to_string(),
                        cx,
                    )),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(cx.theme().radius)
                    .child(
                        v_virtual_list(
                            cx.entity().clone(),
                            "preview-lines",
                            self.state.workspace.preview_sizes.clone(),
                            move |view, visible_range, _, cx| {
                                view.state.workspace.preview_visible_range = visible_range.clone();
                                let Some(document) = &view.state.workspace.preview_document else {
                                    return Vec::new();
                                };
                                let line_count = document.line_count();
                                visible_range
                                    .filter(|ix| *ix < line_count)
                                    .map(|ix| {
                                        let line = if ix
                                            >= view.state.workspace.preview_loaded_range.start
                                            && ix < view.state.workspace.preview_loaded_range.end
                                        {
                                            view.state
                                                .workspace
                                                .preview_loaded_lines
                                                .get(
                                                    ix - view
                                                        .state
                                                        .workspace
                                                        .preview_loaded_range
                                                        .start,
                                                )
                                                .cloned()
                                                .unwrap_or_default()
                                        } else {
                                            SharedString::from("")
                                        };
                                        h_flex()
                                            .gap_3()
                                            .px_3()
                                            .h(preview_line_height())
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .child(
                                                div()
                                                    .w(px(64.))
                                                    .text_right()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child((ix + 1).to_string()),
                                            )
                                            .child(div().flex_1().child(line))
                                    })
                                    .collect()
                            },
                        )
                        .track_scroll(&self.preview_scroll_handle)
                        .p_2(),
                    ),
            )
    }

    fn render_main_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_narrow = window.bounds().size.width < px(1320.);

        if is_narrow {
            let selected_index = if self.narrow_content_tab == NarrowContentTab::Status {
                0
            } else {
                1
            };

            h_resizable("codemerge-layout-compact")
                .child(
                    resizable_panel()
                        .size(px(340.))
                        .size_range(px(280.)..px(420.))
                        .child(self.render_input_panel(cx)),
                )
                .child(
                    resizable_panel().child(
                        card(cx).size_full().child(
                            v_flex()
                                .gap_3()
                                .size_full()
                                .child(
                                    TabBar::new("compact-content-tabs")
                                        .selected_index(selected_index)
                                        .on_click(cx.listener(Self::set_narrow_content_tab))
                                        .child(Tab::new().label(tr(
                                            self.state.settings.language,
                                            "panel_status",
                                        )))
                                        .child(Tab::new().label(tr(
                                            self.state.settings.language,
                                            "panel_results",
                                        ))),
                                )
                                .child(match self.narrow_content_tab {
                                    NarrowContentTab::Status => {
                                        self.render_status_panel(cx).into_any_element()
                                    }
                                    NarrowContentTab::Results => {
                                        self.render_right_panel(cx).into_any_element()
                                    }
                                }),
                        ),
                    ),
                )
        } else {
            h_resizable("codemerge-layout")
                .child(
                    resizable_panel()
                        .size(px(340.))
                        .size_range(px(280.)..px(460.))
                        .child(self.render_input_panel(cx)),
                )
                .child(
                    resizable_panel()
                        .size(px(360.))
                        .size_range(px(300.)..px(520.))
                        .child(self.render_status_panel(cx)),
                )
                .child(resizable_panel().child(self.render_right_panel(cx)))
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
        v_flex()
            .id("codemerge-root")
            .track_focus(&self.focus_handle)
            .size_full()
            .p_4()
            .gap_4()
            .child(self.render_header(cx))
            .child(div().flex_1().child(self.render_main_content(window, cx)))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if let Some(dir) = self
            .state
            .result
            .result
            .as_ref()
            .and_then(|result| result.preview_blob_dir.as_ref())
        {
            let _ = crate::utils::temp_file::cleanup_preview_dir(dir);
        }
    }
}

#[derive(Clone, Copy)]
enum TreeExpansionMode {
    Default,
    ExpandAll,
    CollapseAll,
}

#[derive(Default, Clone, Copy)]
struct TreeSummary {
    folders: usize,
    files: usize,
}

impl TreeSummary {
    fn total(self) -> usize {
        self.folders + self.files
    }

    fn merge(&mut self, other: Self) {
        self.folders += other.folders;
        self.files += other.files;
    }
}

fn build_tree_items(nodes: &[TreeNode], filter: &str, mode: TreeExpansionMode) -> Vec<TreeItem> {
    nodes
        .iter()
        .filter_map(|node| build_tree_item(node, filter, mode, 0))
        .collect()
}

fn build_tree_item(
    node: &TreeNode,
    filter: &str,
    mode: TreeExpansionMode,
    depth: usize,
) -> Option<TreeItem> {
    let children = node
        .children
        .iter()
        .filter_map(|child| build_tree_item(child, filter, mode, depth + 1))
        .collect::<Vec<_>>();
    let matches = tree_matches_filter(node, filter);
    if !matches && children.is_empty() {
        return None;
    }

    let mut item = TreeItem::new(node.id.clone(), node.label.clone());
    if node.is_folder {
        let expanded = if !filter.is_empty() {
            true
        } else {
            match mode {
                TreeExpansionMode::Default => depth < 2,
                TreeExpansionMode::ExpandAll => true,
                TreeExpansionMode::CollapseAll => false,
            }
        };
        item = item.expanded(expanded).children(children);
    }

    Some(item)
}

fn summarize_visible_tree(nodes: &[TreeNode], filter: &str) -> TreeSummary {
    nodes
        .iter()
        .filter_map(|node| summarize_visible_tree_node(node, filter))
        .fold(TreeSummary::default(), |mut summary, node_summary| {
            summary.merge(node_summary);
            summary
        })
}

fn summarize_visible_tree_node(node: &TreeNode, filter: &str) -> Option<TreeSummary> {
    let child_summary = summarize_visible_tree(&node.children, filter);
    let matches = tree_matches_filter(node, filter);
    if !matches && child_summary.total() == 0 {
        return None;
    }

    let mut summary = child_summary;
    if node.is_folder {
        summary.folders += 1;
    } else {
        summary.files += 1;
    }
    Some(summary)
}

fn tree_matches_filter(node: &TreeNode, filter: &str) -> bool {
    filter.is_empty()
        || node.label.to_ascii_lowercase().contains(filter)
        || node.relative_path.to_ascii_lowercase().contains(filter)
}

fn card(cx: &App) -> gpui::Div {
    div()
        .p_4()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .rounded(cx.theme().radius)
}

fn section_title(title: &str, cx: &App) -> AnyElement {
    div()
        .font_semibold()
        .text_color(cx.theme().foreground)
        .child(title.to_string())
        .into_any_element()
}

fn section_caption(title: &str, cx: &App) -> AnyElement {
    div()
        .text_sm()
        .font_semibold()
        .text_color(cx.theme().foreground)
        .child(title.to_string())
        .into_any_element()
}

fn render_info_block(label: &str, value: String, emphasized: bool, cx: &App) -> AnyElement {
    v_flex()
        .gap_1()
        .p_3()
        .rounded(px(12.))
        .border_1()
        .border_color(cx.theme().border)
        .bg(if emphasized {
            cx.theme().secondary.opacity(0.25)
        } else {
            cx.theme().background
        })
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(div().text_sm().child(value))
        .into_any_element()
}

fn render_kv(label: &str, value: String, cx: &App) -> AnyElement {
    h_flex()
        .justify_between()
        .gap_3()
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(div().text_sm().child(value))
        .into_any_element()
}

fn stat_tile(label: &str, value: String, cx: &App) -> AnyElement {
    div()
        .flex_1()
        .p_3()
        .rounded(cx.theme().radius)
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(label.to_string()),
                )
                .child(div().text_lg().font_semibold().child(value)),
        )
        .into_any_element()
}

fn status_banner(
    title: &str,
    message: String,
    status: ProcessUiStatus,
    cx: &App,
) -> AnyElement {
    let tone = match status {
        ProcessUiStatus::Completed => cx.theme().primary.opacity(0.18),
        ProcessUiStatus::Cancelled => cx.theme().warning.opacity(0.22),
        ProcessUiStatus::Error => cx.theme().danger.opacity(0.18),
        ProcessUiStatus::Running | ProcessUiStatus::Preflight => cx.theme().accent.opacity(0.18),
        ProcessUiStatus::Idle => cx.theme().secondary,
    };

    v_flex()
        .gap_1()
        .p_3()
        .rounded(px(12.))
        .bg(tone)
        .child(div().font_semibold().child(title.to_string()))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(message),
        )
        .into_any_element()
}

fn empty_box(title: &str, hint: &str, icon: IconName, cx: &App) -> gpui::Div {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .rounded(px(12.))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary.opacity(0.18))
        .child(
            div()
                .w(px(40.))
                .h(px(40.))
                .rounded(px(12.))
                .bg(cx.theme().accent)
                .text_color(cx.theme().accent_foreground)
                .items_center()
                .justify_center()
                .child(icon),
        )
        .child(div().font_semibold().child(title.to_string()))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(hint.to_string()),
        )
}

fn pill_label(label: &str, cx: &App) -> AnyElement {
    div()
        .text_xs()
        .px_2()
        .py_1()
        .rounded(px(999.))
        .bg(cx.theme().secondary)
        .text_color(cx.theme().muted_foreground)
        .child(label.to_string())
        .into_any_element()
}

fn selected_file_row(entry: &FileEntry, cx: &App) -> AnyElement {
    v_flex()
        .gap_1()
        .px_3()
        .py_2()
        .h(px(52.))
        .child(
            h_flex()
                .justify_between()
                .child(div().font_semibold().truncate().child(entry.name.clone()))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format_size(entry.size)),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .truncate()
                .child(entry.path.display().to_string()),
        )
        .into_any_element()
}

fn activity_row(record: &crate::domain::ProcessRecord, cx: &App) -> AnyElement {
    let (icon, accent, status_label) = match record.status {
        ProcessStatus::Success => (IconName::Check, cx.theme().primary, "OK"),
        ProcessStatus::Skipped => (IconName::ArrowRight, cx.theme().warning, "Skip"),
        ProcessStatus::Failed => (IconName::Close, cx.theme().danger, "Error"),
    };

    h_flex()
        .justify_between()
        .items_center()
        .px_3()
        .h(px(38.))
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .rounded(px(999.))
                        .bg(accent.opacity(0.15))
                        .text_color(accent)
                        .items_center()
                        .justify_center()
                        .child(icon),
                )
                .child(div().truncate().child(record.file_name.clone())),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(status_label),
        )
        .into_any_element()
}

fn process_status_title(status: ProcessUiStatus, language: Language) -> &'static str {
    match status {
        ProcessUiStatus::Idle => tr(language, "status_idle"),
        ProcessUiStatus::Preflight => tr(language, "status_preflight"),
        ProcessUiStatus::Running => tr(language, "status_running"),
        ProcessUiStatus::Completed => tr(language, "status_completed"),
        ProcessUiStatus::Cancelled => tr(language, "status_cancelled"),
        ProcessUiStatus::Error => tr(language, "status_error"),
    }
}

fn process_status_message(workspace: &Workspace, language: Language) -> String {
    match workspace.state.process.ui_status {
        ProcessUiStatus::Idle => tr(language, "status_idle_hint").to_string(),
        ProcessUiStatus::Preflight => format!(
            "{} {}",
            tr(language, "status_preflight_hint"),
            workspace.state.process.preflight.scanned_entries
        ),
        ProcessUiStatus::Running => workspace.state.process.processing_current_file.clone(),
        ProcessUiStatus::Completed => tr(language, "status_completed_hint").to_string(),
        ProcessUiStatus::Cancelled => tr(language, "status_cancelled_hint").to_string(),
        ProcessUiStatus::Error => workspace
            .state
            .process
            .last_error
            .clone()
            .unwrap_or_else(|| tr(language, "status_error_hint").to_string()),
    }
}

fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", size as f64 / 1024.0 / 1024.0)
    }
}

fn copy_to_clipboard(content: &str, language: Language, window: &mut Window, cx: &mut App) {
    match Clipboard::new().and_then(|mut clip| clip.set_text(content.to_string())) {
        Ok(_) => window.push_notification((NotificationType::Success, tr(language, "copied")), cx),
        Err(err) => window.push_notification(
            (
                NotificationType::Error,
                SharedString::from(format!("{}{}", tr(language, "copy_failed"), err)),
            ),
            cx,
        ),
    }
}

fn format_duration(duration: Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}
