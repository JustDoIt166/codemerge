use std::rc::Rc;

use gpui::{
    Context, IntoElement, ListSizingBehavior, ParentElement, Styled, Window, div, px, uniform_list,
};
use gpui_component::{
    ActiveTheme as _, Disableable, Icon, IconName, Sizable, Size, StyledExt as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::Input,
    list::ListItem,
    resizable::h_resizable,
    resizable::resizable_panel,
    scroll::ScrollableElement,
    tab::Tab,
    tab::TabBar,
    table::Table,
    tree::tree,
    v_flex, v_virtual_list,
};

use super::view::{
    activity_row, card, empty_box, format_duration, format_tree_summary, panel_frame,
    panel_viewport, pill_label, process_status_message, process_status_title,
    render_blacklist_section, render_blacklist_tag, render_info_block, render_kv, render_tree_row,
    section_caption, section_title, selected_file_row, stat_tile, status_banner, tab_icon_badge,
};
use super::{
    NarrowContentTab, PendingConfirmation, SidePanelTab, Workspace, fixed_list_sizes,
    preview_line_height, workspace_panel_min_height,
};
use crate::domain::{ProcessStatus, ResultTab};
use crate::utils::i18n::tr;

impl Workspace {
    fn render_process_actions(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let has_inputs = self.has_inputs();
        let is_processing = self.is_processing();

        h_flex()
            .gap_2()
            .child(
                Button::new("start-process")
                    .primary()
                    .icon(IconName::ArrowRight)
                    .label(tr(language, "start"))
                    .disabled(!has_inputs || is_processing)
                    .on_click(cx.listener(Self::start_process)),
            )
            .child(
                Button::new("cancel-process")
                    .outline()
                    .icon(IconName::Close)
                    .label(tr(language, "cancel"))
                    .disabled(!is_processing)
                    .on_click(cx.listener(Self::cancel_process)),
            )
    }

    pub(super) fn render_header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        h_flex()
            .justify_between()
            .items_center()
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .w(px(44.))
                            .h(px(44.))
                            .rounded(px(14.))
                            .bg(cx.theme().primary)
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::GalleryVerticalEnd)
                                    .text_color(cx.theme().primary_foreground)
                                    .with_size(Size::Medium),
                            ),
                    )
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
                    ),
            )
            .child(
                Button::new("toggle-language")
                    .outline()
                    .icon(IconName::Globe)
                    .label(match language {
                        crate::domain::Language::Zh => "EN",
                        crate::domain::Language::En => "中文",
                    })
                    .on_click(cx.listener(Self::toggle_language)),
            )
    }

    pub(super) fn render_input_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let has_inputs = self.has_inputs();
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
            .unwrap_or_else(|| {
                if self.state.settings.options.use_gitignore {
                    tr(language, "gitignore_auto_hint").to_string()
                } else {
                    match self.state.settings.language {
                        crate::domain::Language::Zh => "自动 .gitignore 已停用".to_string(),
                        crate::domain::Language::En => "Auto .gitignore disabled".to_string(),
                    }
                }
            });

        card(cx).size_full().child(
            v_flex()
                .gap_4()
                .size_full()
                .child(section_title(
                    tr(language, "panel_inputs"),
                    IconName::PanelLeft,
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("select-folder")
                                .primary()
                                .icon(IconName::FolderOpen)
                                .label(tr(language, "select_folder"))
                                .on_click(cx.listener(Self::select_folder)),
                        )
                        .child(
                            Button::new("select-files")
                                .outline()
                                .icon(IconName::File)
                                .label(tr(language, "select_files"))
                                .on_click(cx.listener(Self::select_files)),
                        ),
                )
                .child(render_info_block(
                    tr(language, "folder"),
                    folder_label,
                    has_inputs,
                    IconName::FolderOpen,
                    cx,
                ))
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(section_caption(
                                    tr(language, "selected_files_title"),
                                    IconName::File,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            self.state.selection.selected_files.len().to_string(),
                                        ),
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
                .child(section_title(
                    tr(language, "panel_gitignore"),
                    IconName::BookOpen,
                    cx,
                ))
                .child(render_info_block(
                    tr(language, "gitignore"),
                    gitignore_label,
                    self.state.selection.gitignore_file.is_some(),
                    IconName::BookOpen,
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("select-gitignore")
                                .outline()
                                .icon(IconName::BookOpen)
                                .label(tr(language, "select_gitignore"))
                                .on_click(cx.listener(Self::select_gitignore)),
                        )
                        .child(
                            Button::new("apply-gitignore")
                                .outline()
                                .icon(IconName::Check)
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
                .child(section_title(
                    tr(language, "section_options"),
                    IconName::Settings2,
                    cx,
                ))
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
                    div()
                        .pt_2()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(
                            v_flex()
                                .gap_2()
                                .child(section_caption(
                                    tr(language, "danger_zone"),
                                    IconName::TriangleAlert,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(tr(language, "danger_zone_hint")),
                                )
                                .child(
                                    Button::new("clear-inputs")
                                        .danger()
                                        .icon(IconName::Delete)
                                        .label(
                                            if self.pending_confirmation
                                                == Some(PendingConfirmation::ClearInputs)
                                            {
                                                tr(language, "confirm_clear_inputs")
                                            } else {
                                                tr(language, "clear")
                                            },
                                        )
                                        .on_click(cx.listener(Self::clear_inputs)),
                                ),
                        ),
                ),
        )
    }

    pub(super) fn render_status_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        card(cx).child(self.render_status_panel_body(cx))
    }

    fn render_status_panel_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let result_stats = self
            .state
            .result
            .result
            .as_ref()
            .map(|result| result.stats.clone());
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

        v_flex()
            .gap_4()
            .size_full()
            .min_h(px(0.))
            .child(section_title(
                tr(language, "panel_status"),
                IconName::LayoutDashboard,
                cx,
            ))
            .child(self.render_process_actions(cx))
            .child(
                h_flex()
                    .gap_2()
                    .child(stat_tile(
                        tr(language, "total"),
                        self.state.process.preflight.total_files.to_string(),
                        cx,
                    ))
                    .child(stat_tile(
                        tr(language, "process"),
                        self.state.process.preflight.to_process_files.to_string(),
                        cx,
                    ))
                    .child(stat_tile(
                        tr(language, "skip"),
                        self.state.process.preflight.skipped_files.to_string(),
                        cx,
                    )),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(stat_tile(
                        tr(language, "chars"),
                        result_stats
                            .as_ref()
                            .map(|stats| stats.total_chars.to_string())
                            .unwrap_or_else(|| "--".to_string()),
                        cx,
                    ))
                    .child(stat_tile(
                        tr(language, "tokens"),
                        result_stats
                            .as_ref()
                            .map(|stats| stats.total_tokens.to_string())
                            .unwrap_or_else(|| "--".to_string()),
                        cx,
                    ))
                    .child(stat_tile(
                        tr(language, "failed_count"),
                        failed_count.to_string(),
                        cx,
                    )),
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
                            .child(section_caption(
                                tr(language, "progress_overview"),
                                IconName::ChartPie,
                                cx,
                            ))
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
                    .child(render_kv(
                        tr(language, "processing"),
                        self.state.process.processing_current_file.clone(),
                        cx,
                    )),
            )
            .child(
                v_flex()
                    .gap_2()
                    .flex_1()
                    .min_h(px(0.))
                    .child(section_caption(
                        tr(language, "recent_activity"),
                        IconName::SquareTerminal,
                        cx,
                    ))
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
                            .min_h(px(0.))
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
            )
    }

    pub(super) fn render_right_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        card(cx).child(self.render_right_panel_body(cx))
    }

    fn render_right_panel_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let selected_index = if self.side_panel_tab == SidePanelTab::Results {
            0
        } else {
            1
        };

        v_flex()
            .gap_3()
            .size_full()
            .min_h(px(0.))
            .child(
                TabBar::new("side-panel-tabs")
                    .selected_index(selected_index)
                    .on_click(cx.listener(Self::set_side_panel_tab))
                    .child(
                        Tab::new()
                            .prefix(tab_icon_badge(IconName::LayoutDashboard, false, cx))
                            .label(tr(language, "panel_results")),
                    )
                    .child(
                        Tab::new()
                            .prefix(tab_icon_badge(IconName::Settings2, true, cx))
                            .label(tr(language, "panel_rules")),
                    ),
            )
            .child(div().flex_1().min_h(px(0.)).overflow_hidden().child(
                match self.side_panel_tab {
                    SidePanelTab::Results => self.render_results_panel(cx).into_any_element(),
                    SidePanelTab::Rules => self.render_rules_panel(cx).into_any_element(),
                },
            ))
    }

    pub(super) fn render_results_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let has_content_result = self.state.has_content_result();
        let selected_tab = if self.state.result.active_tab == ResultTab::Tree || !has_content_result
        {
            0
        } else {
            1
        };

        v_flex()
            .gap_3()
            .size_full()
            .min_h(px(0.))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        TabBar::new("result-tabs")
                            .selected_index(selected_tab)
                            .on_click(cx.listener(Self::set_tab))
                            .child(
                                Tab::new()
                                    .prefix(tab_icon_badge(IconName::FolderOpen, false, cx))
                                    .label(tr(language, "tab_tree_preview")),
                            )
                            .child(
                                Tab::new()
                                    .prefix(tab_icon_badge(IconName::SquareTerminal, true, cx))
                                    .disabled(!has_content_result)
                                    .label(tr(language, "tab_merged_content")),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("copy-active")
                                    .outline()
                                    .icon(IconName::Copy)
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
                                    .icon(IconName::ArrowDown)
                                    .label(tr(language, "download"))
                                    .disabled(!has_content_result)
                                    .on_click(cx.listener(Self::download_result)),
                            ),
                    ),
            )
            .child(div().flex_1().min_h(px(0.)).overflow_hidden().child(
                match self.state.result.active_tab {
                    ResultTab::Tree => self.render_tree_panel(cx).into_any_element(),
                    ResultTab::Content => self.render_content_panel(cx).into_any_element(),
                },
            ))
    }

    pub(super) fn render_rules_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let filter = self
            .blacklist_filter_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let blacklist_sections = super::model::build_blacklist_sections(
            &self.state.settings.folder_blacklist,
            &self.state.settings.ext_blacklist,
            filter.as_str(),
            language,
        );

        v_flex()
            .gap_3()
            .size_full()
            .min_h(px(0.))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(language, "rules_secondary_hint")),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(Input::new(&self.blacklist_add_input).prefix(IconName::Plus))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("add-folder-blacklist")
                                    .outline()
                                    .icon(IconName::Folder)
                                    .label(tr(language, "add_folder"))
                                    .on_click(cx.listener(Self::add_folder_blacklist)),
                            )
                            .child(
                                Button::new("add-ext-blacklist")
                                    .outline()
                                    .icon(IconName::File)
                                    .label(tr(language, "add_ext"))
                                    .on_click(cx.listener(Self::add_ext_blacklist)),
                            ),
                    ),
            )
            .child(
                Input::new(&self.blacklist_filter_input)
                    .prefix(IconName::Search)
                    .cleanable(true),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("import-blacklist")
                            .outline()
                            .icon(IconName::ArrowDown)
                            .label(tr(language, "blacklist_import_append"))
                            .on_click(cx.listener(Self::import_blacklist)),
                    )
                    .child(
                        Button::new("export-blacklist")
                            .outline()
                            .icon(IconName::ArrowUp)
                            .label(tr(language, "blacklist_export"))
                            .on_click(cx.listener(Self::export_blacklist)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(px(12.))
                    .bg(cx.theme().secondary.opacity(0.22))
                    .child(if blacklist_sections.is_empty() {
                        empty_box(
                            tr(language, "blacklist_empty_title"),
                            tr(language, "blacklist_empty_hint"),
                            IconName::Folder,
                            cx,
                        )
                        .into_any_element()
                    } else {
                        div()
                            .size_full()
                            .min_h(px(0.))
                            .overflow_x_hidden()
                            .overflow_y_scrollbar()
                            .p_2()
                            .child(v_flex().gap_3().children(
                                blacklist_sections.iter().enumerate().map(
                                    |(section_ix, section)| {
                                        let tags = section
                                            .items
                                            .iter()
                                            .enumerate()
                                            .map(|(ix, item)| {
                                                let kind = item.kind;
                                                let value = item.value.clone();
                                                render_blacklist_tag(
                                                    item,
                                                    Button::new((
                                                        "remove-blacklist",
                                                        section_ix * 1000 + ix,
                                                    ))
                                                    .ghost()
                                                    .compact()
                                                    .with_size(Size::Small)
                                                    .icon(IconName::Delete)
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
                                                    ))
                                                    .into_any_element(),
                                                    cx,
                                                )
                                            })
                                            .collect::<Vec<_>>();
                                        render_blacklist_section(section, tags, cx)
                                    },
                                ),
                            ))
                            .into_any_element()
                    }),
            )
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
                                    .icon(IconName::Undo2)
                                    .label(
                                        if self.pending_confirmation
                                            == Some(PendingConfirmation::ResetBlacklist)
                                        {
                                            tr(language, "confirm_reset_blacklist")
                                        } else {
                                            tr(language, "blacklist_reset_default")
                                        },
                                    )
                                    .on_click(cx.listener(Self::reset_blacklist)),
                            )
                            .child(
                                Button::new("clear-blacklist")
                                    .danger()
                                    .icon(IconName::Delete)
                                    .label(
                                        if self.pending_confirmation
                                            == Some(PendingConfirmation::ClearBlacklist)
                                        {
                                            tr(language, "confirm_clear_blacklist")
                                        } else {
                                            tr(language, "blacklist_clear_all")
                                        },
                                    )
                                    .on_click(cx.listener(Self::clear_blacklist)),
                            ),
                    ),
            )
    }

    pub(super) fn render_tree_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let tree_filter = self
            .tree_panel
            .filter_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let filter_active = !tree_filter.is_empty();
        let has_result = self.state.result.result.is_some();
        let has_visible_nodes = !self.tree_panel.render_state.rows.is_empty();
        let rows_by_id = Rc::new(self.tree_panel.render_state.rows_by_id.clone());
        let tree_view = tree(&self.tree_panel.state, move |ix, entry, selected, _, cx| {
            let Some(mut row) = rows_by_id.get(entry.item().id.as_ref()).cloned() else {
                return ListItem::new(ix).child(entry.item().label.clone());
            };
            row.is_expanded = entry.is_expanded();
            if row.is_folder {
                row.icon_kind = if entry.is_expanded() {
                    super::model::TreeIconKind::FolderOpen
                } else {
                    super::model::TreeIconKind::FolderClosed
                };
            }
            render_tree_row(ix, &row, selected, language, cx)
        })
        .h_full();

        v_flex()
            .gap_3()
            .size_full()
            .min_h(px(0.))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Input::new(&self.tree_panel.filter_input)
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
                            .child(format_tree_summary(
                                self.tree_panel.render_state.visible_summary,
                                self.tree_panel.render_state.total_summary,
                                language,
                            )),
                    )
                    .child(pill_label(
                        if filter_active {
                            tr(language, "tree_filter_active")
                        } else {
                            tr(language, "tree_filter_idle")
                        },
                        cx,
                    )),
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

    pub(super) fn render_content_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let has_result = self.state.result.result.is_some();
        let has_rows = !self.state.result.preview_rows.is_empty();
        let tree_only = self.state.is_tree_only_result();

        if tree_only {
            return v_flex()
                .gap_3()
                .size_full()
                .min_h(px(0.))
                .child(empty_box(
                    tr(language, "mode_tree_only"),
                    tr(language, "mode_tree_only_desc"),
                    IconName::FolderOpen,
                    cx,
                ))
                .into_any_element();
        }

        v_flex()
            .gap_3()
            .size_full()
            .min_h(px(0.))
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
            .into_any_element()
    }

    pub(super) fn render_preview(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let selected_preview = self
            .state
            .workspace
            .preview_panel
            .selected_preview_file_id
            .and_then(|id| {
                self.state
                    .result
                    .preview_rows
                    .iter()
                    .find(|row| row.id == id)
            });

        if let Some(error) = self.state.workspace.preview_panel.preview_error.as_ref() {
            let preview_failure_title = selected_preview
                .map(|row| row.display_path.clone())
                .unwrap_or_else(|| tr(language, "preview_unknown_path").to_string());
            let title = format!(
                "{}: {}",
                tr(language, "status_error"),
                preview_failure_title
            );
            return empty_box(title, error.to_string(), IconName::TriangleAlert, cx);
        }

        let Some(document) = &self.state.workspace.preview_panel.preview_document else {
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
        let line_count = document.line_count();
        let view = cx.entity();

        v_flex()
            .gap_2()
            .flex_1()
            .min_h(px(0.))
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
                        uniform_list("preview-lines", line_count, move |visible_range, _, app| {
                            let workspace = view.read(app);
                            visible_range
                                .filter(|ix| *ix < line_count)
                                .map(|ix| {
                                    let line = workspace
                                        .state
                                        .workspace
                                        .preview_panel
                                        .line_at(ix)
                                        .unwrap_or_default();
                                    h_flex()
                                        .gap_3()
                                        .px_3()
                                        .h(preview_line_height())
                                        .font_family(app.theme().mono_font_family.clone())
                                        .child(
                                            div()
                                                .w(px(64.))
                                                .text_right()
                                                .text_color(app.theme().muted_foreground)
                                                .child((ix + 1).to_string()),
                                        )
                                        .child(div().flex_1().child(line))
                                })
                                .collect()
                        })
                        .track_scroll(self.preview_scroll_handle.clone())
                        .with_sizing_behavior(ListSizingBehavior::Auto)
                        .p_2(),
                    ),
            )
    }

    pub(super) fn render_compact_content_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected_index = if self.narrow_content_tab == NarrowContentTab::Status {
            0
        } else {
            1
        };

        card(cx).size_full().child(
            v_flex()
                .gap_3()
                .size_full()
                .min_h(px(0.))
                .child(
                    TabBar::new("compact-content-tabs")
                        .selected_index(selected_index)
                        .on_click(cx.listener(Self::set_narrow_content_tab))
                        .child(
                            Tab::new()
                                .prefix(tab_icon_badge(IconName::LayoutDashboard, false, cx))
                                .label(tr(self.state.settings.language, "panel_status")),
                        )
                        .child(
                            Tab::new()
                                .prefix(tab_icon_badge(IconName::PanelRight, true, cx))
                                .label(tr(self.state.settings.language, "panel_results")),
                        ),
                )
                .child(div().flex_1().min_h(px(0.)).overflow_hidden().child(
                    match self.narrow_content_tab {
                        NarrowContentTab::Status => {
                            self.render_status_panel_body(cx).into_any_element()
                        }
                        NarrowContentTab::Results => {
                            self.render_right_panel_body(cx).into_any_element()
                        }
                    },
                )),
        )
    }

    pub(super) fn render_main_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_narrow = window.bounds().size.width < px(1320.);
        let panel_min_height = workspace_panel_min_height(is_narrow);

        if is_narrow {
            h_resizable("codemerge-layout-compact")
                .child(
                    resizable_panel()
                        .size(px(340.))
                        .size_range(px(280.)..px(420.))
                        .child(panel_viewport(
                            self.render_input_panel(cx).into_any_element(),
                            panel_min_height,
                        )),
                )
                .child(resizable_panel().child(panel_viewport(
                    self.render_compact_content_panel(cx).into_any_element(),
                    panel_min_height,
                )))
        } else {
            h_resizable("codemerge-layout")
                .child(
                    resizable_panel()
                        .size(px(340.))
                        .size_range(px(280.)..px(460.))
                        .child(panel_viewport(
                            self.render_input_panel(cx).into_any_element(),
                            panel_min_height,
                        )),
                )
                .child(
                    resizable_panel()
                        .size(px(360.))
                        .size_range(px(300.)..px(520.))
                        .child(panel_viewport(
                            self.render_status_panel(cx).into_any_element(),
                            panel_min_height,
                        )),
                )
                .child(resizable_panel().child(panel_frame(
                    self.render_right_panel(cx).into_any_element(),
                    panel_min_height,
                )))
        }
    }
}
