use std::rc::Rc;

use gpui::{
    Context, IntoElement, ParentElement, SharedString, Styled, Window, div,
    prelude::FluentBuilder as _, px,
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
    tab::Tab,
    tab::TabBar,
    table::Table,
    tree::tree,
    v_flex, v_virtual_list,
};

use super::view::{
    activity_row, card, empty_box, format_duration, panel_viewport, pill_label,
    process_status_message, process_status_title, render_info_block, render_kv, section_caption,
    section_title, selected_file_row, stat_tile, status_banner, summarize_visible_tree,
    tab_icon_badge,
};
use super::{
    BlacklistItemKind, NarrowContentTab, PendingConfirmation, SidePanelTab, Workspace,
    fixed_list_sizes, preview_line_height, workspace_panel_min_height,
};
use crate::domain::{ProcessStatus, ResultTab};
use crate::utils::i18n::tr;

impl Workspace {
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
                            .w(px(44.))
                            .h(px(44.))
                            .rounded(px(14.))
                            .bg(cx.theme().primary)
                            .text_color(cx.theme().primary_foreground)
                            .items_center()
                            .justify_center()
                            .child(IconName::GalleryVerticalEnd),
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
        let language = self.state.settings.language;
        let result_stats = self
            .state
            .result
            .result
            .as_ref()
            .map(|result| &result.stats);
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
                .child(section_title(
                    tr(language, "panel_status"),
                    IconName::LayoutDashboard,
                    cx,
                ))
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
                                .map(|stats| stats.total_chars.to_string())
                                .unwrap_or_else(|| "--".to_string()),
                            cx,
                        ))
                        .child(stat_tile(
                            tr(language, "tokens"),
                            result_stats
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

    pub(super) fn render_right_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
                .child(match self.side_panel_tab {
                    SidePanelTab::Results => self.render_results_panel(cx).into_any_element(),
                    SidePanelTab::Rules => self.render_rules_panel(cx).into_any_element(),
                }),
        )
    }

    pub(super) fn render_results_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .child(
                                Tab::new()
                                    .prefix(tab_icon_badge(IconName::FolderOpen, false, cx))
                                    .label(tr(language, "tab_tree_preview")),
                            )
                            .child(
                                Tab::new()
                                    .prefix(tab_icon_badge(IconName::SquareTerminal, true, cx))
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

    pub(super) fn render_rules_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
                                                    .child(pill_label(
                                                        match item.kind {
                                                            BlacklistItemKind::Folder => {
                                                                tr(language, "folder")
                                                            }
                                                            BlacklistItemKind::Ext => {
                                                                tr(language, "extension")
                                                            }
                                                        },
                                                        cx,
                                                    ))
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .child(item.display_label.clone()),
                                                    ),
                                            )
                                            .child(
                                                Button::new(("remove-blacklist", ix))
                                                    .outline()
                                                    .icon(IconName::Delete)
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
                    let (icon_bg, icon_fg) = if entry.is_folder() {
                        if selected {
                            (
                                cx.theme().primary_foreground.opacity(0.18),
                                cx.theme().primary_foreground,
                            )
                        } else if entry.is_expanded() {
                            (cx.theme().primary.opacity(0.16), cx.theme().primary)
                        } else {
                            (cx.theme().warning.opacity(0.16), cx.theme().warning)
                        }
                    } else if selected {
                        (
                            cx.theme().primary_foreground.opacity(0.18),
                            cx.theme().primary_foreground,
                        )
                    } else {
                        (cx.theme().accent.opacity(0.16), cx.theme().accent)
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
                                        .w(px(24.))
                                        .h(px(24.))
                                        .rounded(px(8.))
                                        .items_center()
                                        .justify_center()
                                        .bg(icon_bg)
                                        .child(
                                            Icon::new(icon)
                                                .text_color(icon_fg)
                                                .with_size(Size::Small),
                                        ),
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

    pub(super) fn render_preview(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language;
        let selected_preview = self
            .state
            .workspace
            .selected_preview_file_id
            .and_then(|id| {
                self.state
                    .result
                    .preview_rows
                    .iter()
                    .find(|row| row.id == id)
            });

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
                .child(match self.narrow_content_tab {
                    NarrowContentTab::Status => self.render_status_panel(cx).into_any_element(),
                    NarrowContentTab::Results => self.render_right_panel(cx).into_any_element(),
                }),
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
                .child(resizable_panel().child(panel_viewport(
                    self.render_right_panel(cx).into_any_element(),
                    panel_min_height,
                )))
        }
    }
}
