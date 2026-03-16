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

use crate::domain::{
    Language, PreviewRowViewModel, ProcessStatus, ProgressRowViewModel, ResultTab, TreeNode,
};
use crate::services::settings;
use crate::ui::state::AppState;
use crate::utils::i18n::tr;

fn preview_line_height() -> Pixels {
    px(22.)
}

fn fixed_list_sizes(len: usize, height: Pixels) -> Rc<Vec<gpui::Size<Pixels>>> {
    Rc::new((0..len).map(|_| size(px(100.), height)).collect::<Vec<_>>())
}

struct PreviewTableDelegate {
    columns: Vec<Column>,
    rows: Vec<PreviewRowViewModel>,
}

impl PreviewTableDelegate {
    fn new() -> Self {
        Self {
            columns: vec![
                Column::new("path", "Path").width(420.),
                Column::new("chars", "Chars").width(100.).text_right(),
                Column::new("tokens", "Tokens").width(100.).text_right(),
            ],
            rows: Vec::new(),
        }
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
        let preview_table = cx.new(|cx| TableState::new(PreviewTableDelegate::new(), window, cx));
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
            _poll_task: poll_task,
            _subscriptions: subscriptions,
        };
        this.refresh_preflight();
        this
    }

    fn render_header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .justify_between()
            .items_center()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xl().font_semibold().child("CodeMerge"))
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child("GPUI Component Workspace"),
                    ),
            )
            .child(
                Button::new("toggle-language")
                    .outline()
                    .label(match self.state.settings.language {
                        Language::Zh => "EN",
                        Language::En => "中文",
                    })
                    .on_click(cx.listener(Self::toggle_language)),
            )
    }

    fn render_left_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let blacklist_filter = self
            .blacklist_filter_input
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        let blacklist_items = self
            .state
            .settings
            .folder_blacklist
            .iter()
            .map(|item| format!("folder: {item}"))
            .chain(
                self.state
                    .settings
                    .ext_blacklist
                    .iter()
                    .map(|item| format!("ext: {item}")),
            )
            .filter(|item| {
                blacklist_filter.is_empty() || item.to_ascii_lowercase().contains(&blacklist_filter)
            })
            .collect::<Vec<_>>();
        let blacklist_rows = Rc::new(
            blacklist_items
                .iter()
                .cloned()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        );

        card(cx).size_full().child(
            v_flex()
                .gap_4()
                .child(section_title(
                    tr(self.state.settings.language, "section_files"),
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("select-folder")
                                .primary()
                                .label(tr(self.state.settings.language, "select_folder"))
                                .on_click(cx.listener(Self::select_folder)),
                        )
                        .child(
                            Button::new("select-files")
                                .outline()
                                .label(tr(self.state.settings.language, "select_files"))
                                .on_click(cx.listener(Self::select_files)),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("select-gitignore")
                                .outline()
                                .label(tr(self.state.settings.language, "select_gitignore"))
                                .on_click(cx.listener(Self::select_gitignore)),
                        )
                        .child(
                            Button::new("apply-gitignore")
                                .outline()
                                .label(tr(self.state.settings.language, "apply_gitignore"))
                                .on_click(cx.listener(Self::apply_gitignore)),
                        ),
                )
                .child(render_kv(
                    tr(self.state.settings.language, "folder"),
                    self.state
                        .selection
                        .selected_folder
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| tr(self.state.settings.language, "none").to_string()),
                    cx,
                ))
                .child(render_kv(
                    tr(self.state.settings.language, "files"),
                    self.state.selection.selected_files.len().to_string(),
                    cx,
                ))
                .child(
                    v_flex().gap_1().children(
                        self.state
                            .selection
                            .selected_files
                            .iter()
                            .take(6)
                            .map(|entry| {
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} ({:.1} KB)",
                                        entry.name,
                                        entry.size as f64 / 1024.0
                                    ))
                                    .into_any_element()
                            }),
                    ),
                )
                .child(
                    Button::new("clear-inputs")
                        .danger()
                        .label(tr(self.state.settings.language, "clear"))
                        .on_click(cx.listener(Self::clear_inputs)),
                )
                .child(section_title(
                    tr(self.state.settings.language, "section_options"),
                    cx,
                ))
                .child(
                    Checkbox::new("compress")
                        .checked(self.state.settings.options.compress)
                        .label(tr(self.state.settings.language, "compress"))
                        .on_click(cx.listener(Self::toggle_compress)),
                )
                .child(
                    Checkbox::new("use-gitignore")
                        .checked(self.state.settings.options.use_gitignore)
                        .label(tr(self.state.settings.language, "use_gitignore"))
                        .on_click(cx.listener(Self::toggle_use_gitignore)),
                )
                .child(
                    Checkbox::new("ignore-git")
                        .checked(self.state.settings.options.ignore_git)
                        .label(tr(self.state.settings.language, "ignore_git"))
                        .on_click(cx.listener(Self::toggle_ignore_git)),
                )
                .child(
                    Checkbox::new("dedupe")
                        .checked(self.state.selection.dedupe_exact_path)
                        .label(tr(self.state.settings.language, "dedupe_exact_path"))
                        .on_click(cx.listener(Self::toggle_dedupe)),
                )
                .child(section_title(
                    tr(self.state.settings.language, "section_blacklist"),
                    cx,
                ))
                .child(Input::new(&self.blacklist_filter_input).cleanable(true))
                .child(Input::new(&self.blacklist_add_input))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("add-folder-blacklist")
                                .outline()
                                .label(tr(self.state.settings.language, "add_folder"))
                                .on_click(cx.listener(Self::add_folder_blacklist)),
                        )
                        .child(
                            Button::new("add-ext-blacklist")
                                .outline()
                                .label(tr(self.state.settings.language, "add_ext"))
                                .on_click(cx.listener(Self::add_ext_blacklist)),
                        ),
                )
                .child(if blacklist_rows.is_empty() {
                    div()
                        .max_h(px(220.))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(self.state.settings.language, "none"))
                        .into_any_element()
                } else {
                    let rows = blacklist_rows.clone();
                    div()
                        .h(px(220.))
                        .child(
                            v_virtual_list(
                                cx.entity().clone(),
                                "blacklist-items",
                                fixed_list_sizes(rows.len(), px(34.)),
                                move |_, visible_range, _, cx| {
                                    visible_range
                                        .filter_map(|ix| rows.get(ix).cloned())
                                        .map(|item| {
                                            div()
                                                .text_sm()
                                                .p_2()
                                                .h(px(34.))
                                                .rounded(cx.theme().radius)
                                                .bg(cx.theme().secondary)
                                                .child(item)
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
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("import-blacklist")
                                .outline()
                                .label(tr(self.state.settings.language, "blacklist_import_append"))
                                .on_click(cx.listener(Self::import_blacklist)),
                        )
                        .child(
                            Button::new("export-blacklist")
                                .outline()
                                .label(tr(self.state.settings.language, "blacklist_export"))
                                .on_click(cx.listener(Self::export_blacklist)),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("reset-blacklist")
                                .outline()
                                .label(tr(self.state.settings.language, "blacklist_reset_default"))
                                .on_click(cx.listener(Self::reset_blacklist)),
                        )
                        .child(
                            Button::new("clear-blacklist")
                                .danger()
                                .label(tr(self.state.settings.language, "blacklist_clear_all"))
                                .on_click(cx.listener(Self::clear_blacklist)),
                        ),
                )
                .child(
                    Button::new("save-settings")
                        .primary()
                        .label(tr(self.state.settings.language, "save_settings"))
                        .on_click(cx.listener(Self::save_blacklists)),
                ),
        )
    }

    fn render_center_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let progress_rows = Rc::new(
            self.state
                .process
                .processing_records
                .iter()
                .rev()
                .take(2000)
                .map(|record| ProgressRowViewModel {
                    file_name: record.file_name.clone(),
                    status_label: match record.status {
                        ProcessStatus::Success => "[OK]".to_string(),
                        ProcessStatus::Skipped => "[SKIP]".to_string(),
                        ProcessStatus::Failed => "[ERR]".to_string(),
                    },
                })
                .collect::<Vec<_>>(),
        );
        let processed = self
            .state
            .process
            .processing_records
            .iter()
            .filter(|record| matches!(record.status, ProcessStatus::Success))
            .count();
        let elapsed = self
            .state
            .process
            .processing_started_at
            .map(|start| format_duration(start.elapsed()))
            .unwrap_or_else(|| "--:--".to_string());
        card(cx).size_full().child(
            v_flex()
                .gap_4()
                .child(section_title(
                    tr(self.state.settings.language, "section_summary"),
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .child(stat_tile(
                            tr(self.state.settings.language, "total"),
                            self.state.process.preflight.total_files.to_string(),
                            cx,
                        ))
                        .child(stat_tile(
                            tr(self.state.settings.language, "skip"),
                            self.state.process.preflight.skipped_files.to_string(),
                            cx,
                        ))
                        .child(stat_tile(
                            tr(self.state.settings.language, "process"),
                            self.state.process.preflight.to_process_files.to_string(),
                            cx,
                        )),
                )
                .child(if let Some(result) = &self.state.result.result {
                    h_flex()
                        .gap_2()
                        .child(stat_tile(
                            tr(self.state.settings.language, "chars"),
                            result.stats.total_chars.to_string(),
                            cx,
                        ))
                        .child(stat_tile(
                            tr(self.state.settings.language, "tokens"),
                            result.stats.total_tokens.to_string(),
                            cx,
                        ))
                        .into_any_element()
                } else {
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(self.state.settings.language, "no_stats"))
                        .into_any_element()
                })
                .child(section_title(
                    tr(self.state.settings.language, "section_progress"),
                    cx,
                ))
                .child(render_kv(
                    tr(self.state.settings.language, "progress_count"),
                    format!(
                        "{processed}/{}",
                        self.state.process.processing_candidates.max(1)
                    ),
                    cx,
                ))
                .child(render_kv(
                    tr(self.state.settings.language, "elapsed"),
                    elapsed,
                    cx,
                ))
                .child(render_kv(
                    tr(self.state.settings.language, "processing"),
                    self.state.process.processing_current_file.clone(),
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("start-process")
                                .primary()
                                .label(tr(self.state.settings.language, "start"))
                                .on_click(cx.listener(Self::start_process)),
                        )
                        .child(
                            Button::new("cancel-process")
                                .danger()
                                .label(tr(self.state.settings.language, "cancel"))
                                .on_click(cx.listener(Self::cancel_process)),
                        ),
                )
                .child(if progress_rows.is_empty() {
                    div()
                        .max_h(px(460.))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(self.state.settings.language, "status_ready"))
                        .into_any_element()
                } else {
                    let rows = progress_rows.clone();
                    div()
                        .h(px(460.))
                        .child(
                            v_virtual_list(
                                cx.entity().clone(),
                                "progress-rows",
                                fixed_list_sizes(rows.len(), px(26.)),
                                move |_, visible_range, _, cx| {
                                    visible_range
                                        .filter_map(|ix| rows.get(ix))
                                        .map(|row| {
                                            div()
                                                .text_sm()
                                                .h(px(26.))
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .child(format!(
                                                    "{} {}",
                                                    row.status_label, row.file_name
                                                ))
                                                .into_any_element()
                                        })
                                        .collect::<Vec<_>>()
                                },
                            )
                            .p_1(),
                        )
                        .into_any_element()
                }),
        )
    }

    fn render_right_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_tab = if self.state.result.active_tab == ResultTab::Tree {
            0
        } else {
            1
        };
        card(cx).size_full().child(
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
                                .child(
                                    Tab::new().label(tr(
                                        self.state.settings.language,
                                        "tab_tree_preview",
                                    )),
                                )
                                .child(
                                    Tab::new().label(tr(
                                        self.state.settings.language,
                                        "tab_merged_content",
                                    )),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new("copy-active")
                                        .outline()
                                        .label(if self.state.result.active_tab == ResultTab::Tree {
                                            tr(self.state.settings.language, "copy_tree")
                                        } else {
                                            tr(self.state.settings.language, "copy_current_page")
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
                                        .label(tr(self.state.settings.language, "download"))
                                        .disabled(self.state.result.active_tab == ResultTab::Tree)
                                        .on_click(cx.listener(Self::download_result)),
                                ),
                        ),
                )
                .child(match self.state.result.active_tab {
                    ResultTab::Tree => self.render_tree_panel(cx).into_any_element(),
                    ResultTab::Content => self.render_content_panel(cx).into_any_element(),
                }),
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
                        div()
                            .text_xs()
                            .px_2()
                            .py_1()
                            .rounded(px(999.))
                            .bg(if filter_active {
                                cx.theme().accent
                            } else {
                                cx.theme().secondary
                            })
                            .text_color(if filter_active {
                                cx.theme().accent_foreground
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(if filter_active {
                                tr(language, "tree_filter_active")
                            } else {
                                tr(language, "tree_filter_idle")
                            }),
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
                        v_flex()
                            .size_full()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(40.))
                                    .h(px(40.))
                                    .rounded(px(12.))
                                    .bg(cx.theme().accent)
                                    .text_color(cx.theme().accent_foreground)
                                    .items_center()
                                    .justify_center()
                                    .child(IconName::FolderOpen),
                            )
                            .child(div().font_semibold().child(if has_result {
                                tr(language, "tree_no_match")
                            } else {
                                tr(language, "tree_empty")
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if has_result && filter_active {
                                        tr(language, "tree_no_match_hint")
                                    } else {
                                        tr(language, "tree_empty_hint")
                                    }),
                            )
                            .into_any_element()
                    }),
            )
    }

    fn render_content_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .size_full()
            .child(Input::new(&self.preview_filter_input).cleanable(true))
            .child(
                div().h(px(220.)).child(
                    Table::new(&self.preview_table)
                        .with_size(Size::Small)
                        .stripe(true),
                ),
            )
            .child(self.render_preview(cx))
    }

    fn render_preview(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(document) = &self.state.workspace.preview_document else {
            return div()
                .flex_1()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .items_center()
                .justify_center()
                .child(tr(self.state.settings.language, "no_content"));
        };
        let line_count = document.line_count();
        v_flex()
            .gap_2()
            .flex_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} lines | {} bytes | cached {:?} | visible {:?}",
                        line_count,
                        document.byte_len(),
                        self.state.workspace.preview_loaded_range,
                        self.state.workspace.preview_visible_range
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
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("codemerge-root")
            .track_focus(&self.focus_handle)
            .size_full()
            .p_4()
            .gap_4()
            .child(self.render_header(cx))
            .child(
                div().flex_1().child(
                    h_resizable("codemerge-layout")
                        .child(
                            resizable_panel()
                                .size(px(340.))
                                .size_range(px(280.)..px(460.))
                                .child(self.render_left_panel(cx)),
                        )
                        .child(
                            resizable_panel()
                                .size(px(360.))
                                .size_range(px(280.)..px(520.))
                                .child(self.render_center_panel(cx)),
                        )
                        .child(resizable_panel().child(self.render_right_panel(cx))),
                ),
            )
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
