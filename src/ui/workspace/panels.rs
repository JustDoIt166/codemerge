use std::rc::Rc;

use gpui::{
    App, Context, IntoElement, ListSizingBehavior, ParentElement, Styled, Window, div,
    prelude::FluentBuilder as _, px, uniform_list,
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
    panel_viewport, process_status_message, process_status_title, render_blacklist_section,
    render_blacklist_tag, render_info_block, render_kv, render_tree_row, section_caption,
    section_title, selected_file_row, stat_tile, status_banner, tab_icon_badge,
};
use super::{
    PreviewPaneView, TreePaneView, TreeViewMode, Workspace, fixed_list_sizes, preview_line_height,
    workspace_panel_min_height,
};
use crate::domain::{ProcessStatus, ResultTab};
use crate::ui::perf;
use crate::ui::preview_model::PreviewScrollDirection;
use crate::ui::state::{NarrowContentTab, PendingConfirmation, SidePanelTab};
use crate::utils::i18n::tr;

impl Workspace {
    fn render_process_actions(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language(cx);
        let has_inputs = self.has_inputs(cx);
        let is_processing = self.is_processing(cx);

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
        let language = self.language(cx);
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
        let settings = self.settings_snapshot(cx);
        let selection = self.selection_snapshot(cx);
        let ui_state = self.ui_state(cx);
        let language = settings.language;
        let has_inputs = self.has_inputs(cx);
        let selected_files = Rc::new(selection.selected_files.clone());
        let folder_label = self
            .selection_snapshot(cx)
            .selected_folder
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| tr(language, "input_folder_empty").to_string());
        let gitignore_label = self
            .selection_snapshot(cx)
            .gitignore_file
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| {
                if settings.options.use_gitignore {
                    tr(language, "gitignore_auto_hint").to_string()
                } else {
                    match settings.language {
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
                                        .child(selection.selected_files.len().to_string()),
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
                    selection.gitignore_file.is_some(),
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
                                .disabled(selection.gitignore_file.is_none())
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
                        .checked(settings.options.compress)
                        .label(tr(language, "compress"))
                        .on_click(cx.listener(Self::toggle_compress)),
                )
                .child(
                    Checkbox::new("use-gitignore")
                        .checked(settings.options.use_gitignore)
                        .label(tr(language, "use_gitignore"))
                        .on_click(cx.listener(Self::toggle_use_gitignore)),
                )
                .child(
                    Checkbox::new("ignore-git")
                        .checked(settings.options.ignore_git)
                        .label(tr(language, "ignore_git"))
                        .on_click(cx.listener(Self::toggle_ignore_git)),
                )
                .child(
                    Checkbox::new("dedupe")
                        .checked(selection.dedupe_exact_path)
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
                                            if ui_state.pending_confirmation
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
        let language = self.language(cx);
        let process_actions = self.render_process_actions(cx).into_any_element();
        let process = self.process.read(cx);
        let process = process.state();
        let result = self.result.read(cx);
        let result = result.state().result.as_ref();
        let result_stats = result.map(|result| result.stats.clone());
        let merged_file_size_hint = result
            .and_then(|result| result.merged_content_path.as_ref())
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|metadata| super::view::format_size(metadata.len()));
        let processed_count = process.processing_records.len();
        let failed_count = process
            .processing_records
            .iter()
            .filter(|record| matches!(record.status, ProcessStatus::Failed))
            .count();
        let activity_rows = Rc::new(
            process
                .processing_records
                .iter()
                .rev()
                .take(16)
                .cloned()
                .collect::<Vec<_>>(),
        );
        let progress_total = process
            .processing_candidates
            .max(process.preflight.to_process_files)
            .max(1);
        let progress_value = processed_count.min(progress_total);
        let progress_ratio = progress_value as f32 / progress_total as f32;
        let bar_fill = px((progress_ratio * 240.0).round());
        let elapsed = process
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
            .child(process_actions)
            .child(
                h_flex()
                    .gap_2()
                    .child(stat_tile(
                        tr(language, "total"),
                        process.preflight.total_files.to_string(),
                        cx,
                    ))
                    .child(stat_tile(
                        tr(language, "process"),
                        process.preflight.to_process_files.to_string(),
                        cx,
                    ))
                    .child(stat_tile(
                        tr(language, "skip"),
                        process.preflight.skipped_files.to_string(),
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
                process_status_title(process.ui_status, language),
                process_status_message(process, language, merged_file_size_hint),
                process.ui_status,
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
                        process.processing_current_file.clone(),
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
        let language = self.language(cx);
        let ui_state = self.ui_state(cx);
        let selected_index = if ui_state.side_panel_tab == SidePanelTab::Results {
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
                match ui_state.side_panel_tab {
                    SidePanelTab::Results => self.results_panel_view.clone().into_any_element(),
                    SidePanelTab::Rules => self.rules_panel_view.clone().into_any_element(),
                },
            ))
    }

    pub(super) fn render_results_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language(cx);
        let result_state = self.result.read(cx).state().clone();
        let has_content_result = self.result_has_content(cx);
        let selected_tab = if result_state.active_tab == ResultTab::Tree || !has_content_result {
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
                                    .label(if result_state.active_tab == ResultTab::Tree {
                                        tr(language, "copy_tree")
                                    } else {
                                        tr(language, "copy_current_page")
                                    })
                                    .on_click(cx.listener(
                                        if result_state.active_tab == ResultTab::Tree {
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
                match result_state.active_tab {
                    ResultTab::Tree => self.tree_pane_view.clone().into_any_element(),
                    ResultTab::Content => self.render_content_panel(cx).into_any_element(),
                },
            ))
    }

    pub(super) fn render_rules_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language(cx);
        let ui_state = self.ui_state(cx);
        self.refresh_rules_panel_cache(cx);
        let blacklist_sections = self.rules_panel.cache.sections.clone();

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
                                        if ui_state.pending_confirmation
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
                                        if ui_state.pending_confirmation
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

    pub(super) fn render_content_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language(cx);
        let result_state = self.result.read(cx).state().clone();
        let has_result = result_state.result.is_some();
        let has_rows = !result_state.preview_rows.is_empty();
        let tree_only = self.result_is_tree_only(cx);

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
            .child(self.preview_pane_view.clone())
            .into_any_element()
    }

    pub(super) fn render_compact_content_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ui_state = self.ui_state(cx);
        let selected_index = if ui_state.narrow_content_tab == NarrowContentTab::Status {
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
                                .label(tr(self.language(cx), "panel_status")),
                        )
                        .child(
                            Tab::new()
                                .prefix(tab_icon_badge(IconName::PanelRight, true, cx))
                                .label(tr(self.language(cx), "panel_results")),
                        ),
                )
                .child(div().flex_1().min_h(px(0.)).overflow_hidden().child(
                    match ui_state.narrow_content_tab {
                        NarrowContentTab::Status => {
                            self.status_panel_view.clone().into_any_element()
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
        _cx: &mut Context<Self>,
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
                            self.input_panel_view.clone().into_any_element(),
                            panel_min_height,
                        )),
                )
                .child(resizable_panel().child(panel_viewport(
                    self.compact_content_view.clone().into_any_element(),
                    panel_min_height,
                )))
        } else {
            h_resizable("codemerge-layout")
                .child(
                    resizable_panel()
                        .size(px(340.))
                        .size_range(px(280.)..px(460.))
                        .child(panel_viewport(
                            self.input_panel_view.clone().into_any_element(),
                            panel_min_height,
                        )),
                )
                .child(
                    resizable_panel()
                        .size(px(360.))
                        .size_range(px(300.)..px(520.))
                        .child(panel_viewport(
                            self.status_panel_view.clone().into_any_element(),
                            panel_min_height,
                        )),
                )
                .child(resizable_panel().child(panel_frame(
                    self.right_panel_view.clone().into_any_element(),
                    panel_min_height,
                )))
        }
    }
}

impl TreePaneView {
    pub(super) fn render_tree_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.clone();
        let (
            language,
            filter_input,
            tree_filter,
            has_visible_nodes,
            visible_summary,
            total_summary,
            tree_state,
        ) = {
            let workspace_ref = workspace.read(cx);
            let language = workspace_ref.language(cx);
            let filter_input = workspace_ref.tree_panel.filter_input.clone();
            let tree_filter = filter_input.read(cx).value().trim().to_string();
            let has_visible_nodes = !workspace_ref.tree_panel.render_state.rows.is_empty();
            let visible_summary = workspace_ref.tree_panel.render_state.visible_summary;
            let total_summary = workspace_ref.tree_panel.total_summary;
            let tree_state = workspace_ref.tree_panel.state.clone();
            (
                language,
                filter_input,
                tree_filter,
                has_visible_nodes,
                visible_summary,
                total_summary,
                tree_state,
            )
        };
        let has_result = self.result_has_tree(cx);
        let filter_active = !tree_filter.is_empty();
        let is_plain_text_mode = matches!(self.view_mode, TreeViewMode::PlainText);
        let view_mode_label = if is_plain_text_mode {
            tr(language, "tree_view_tree")
        } else {
            tr(language, "tree_view_text")
        };
        let plain_text = if is_plain_text_mode {
            workspace
                .read(cx)
                .result
                .read(cx)
                .state()
                .result
                .as_ref()
                .map(|result| result.tree_string.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let plain_text_lines = if is_plain_text_mode {
            plain_text
                .split('\n')
                .map(|line| line.trim_end_matches('\r').replace(' ', "\u{00A0}"))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let row_workspace = workspace.clone();
        let expand_workspace = workspace.clone();
        let collapse_workspace = workspace.clone();
        let tree_view = tree(&tree_state, move |ix, entry, selected, _, cx| {
            let workspace = row_workspace.read(cx);
            let Some(mut row) = workspace.tree_panel.render_state.rows.get(ix).cloned() else {
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
                        Input::new(&filter_input)
                            .prefix(IconName::Search)
                            .cleanable(true),
                    )
                    .child(
                        Button::new("tree-expand")
                            .outline()
                            .icon(IconName::ChevronDown)
                            .label(tr(language, "tree_expand_all"))
                            .disabled(!has_result || filter_active || is_plain_text_mode)
                            .on_click(cx.listener(move |_, event, window, cx| {
                                expand_workspace.update(cx, |workspace, cx| {
                                    workspace.expand_tree(event, window, cx);
                                });
                            })),
                    )
                    .child(
                        Button::new("tree-collapse")
                            .outline()
                            .icon(IconName::ChevronRight)
                            .label(tr(language, "tree_collapse_all"))
                            .disabled(!has_result || filter_active || is_plain_text_mode)
                            .on_click(cx.listener(move |_, event, window, cx| {
                                collapse_workspace.update(cx, |workspace, cx| {
                                    workspace.collapse_tree(event, window, cx);
                                });
                            })),
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
                                visible_summary,
                                total_summary,
                                language,
                            )),
                    )
                    .child(
                        Button::new("tree-view-mode")
                            .outline()
                            .label(view_mode_label)
                            .on_click(cx.listener(Self::toggle_view_mode)),
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
                    .child(if is_plain_text_mode {
                        if plain_text.is_empty() {
                            empty_box(
                                tr(language, "tree_empty"),
                                tr(language, "tree_empty_hint"),
                                IconName::FolderOpen,
                                cx,
                            )
                            .into_any_element()
                        } else {
                            div()
                                .size_full()
                                .min_h(px(0.))
                                .overflow_y_scrollbar()
                                .p_2()
                                .child(v_flex().children(plain_text_lines.into_iter().map(
                                    |line| {
                                        div()
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .text_sm()
                                            .whitespace_nowrap()
                                            .child(line)
                                    },
                                )))
                                .into_any_element()
                        }
                    } else if has_visible_nodes {
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

    fn result_has_tree(&self, cx: &App) -> bool {
        self.workspace
            .read(cx)
            .result
            .read(cx)
            .state()
            .result
            .is_some()
    }
}

impl PreviewPaneView {
    pub(super) fn scroll_to_top(&mut self) {
        self.scroll_handle
            .scroll_to_item_strict(0, gpui::ScrollStrategy::Top);
        self.last_requested_load_range = 0..0;
        self.render_cache_range = 0..0;
        self.pending_visible_range = Some(0..0);
        self.last_scroll_anchor = 0;
    }

    pub(super) fn render_preview_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.settings.read(cx).language();
        let result_state = self.result.read(cx).state().clone();
        let (preview_error, preview_document, selected_preview_id) = {
            let preview = self.preview.read(cx);
            (
                preview.state().preview_error.clone(),
                preview.preview_document().cloned(),
                preview.selected_preview_file_id(),
            )
        };
        let selected_preview = selected_preview_id
            .and_then(|id| result_state.preview_rows.iter().find(|row| row.id == id));

        if let Some(error) = preview_error.as_ref() {
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

        let Some(document) = preview_document.as_ref() else {
            self.last_requested_load_range = 0..0;
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
        self.flush_pending_visible_range(cx);
        if self.render_cache_range.is_empty() && line_count > 0 {
            let initial =
                0..line_count.min(crate::ui::state::PreviewPanelState::RENDER_WINDOW_LINES);
            self.refresh_render_cache(initial, cx);
        } else {
            self.refresh_render_cache(self.render_cache_range.clone(), cx);
        }

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
                        uniform_list(
                            "preview-lines",
                            line_count,
                            cx.processor(
                                move |view, visible_range: std::ops::Range<usize>, _, app_cx| {
                                    view.queue_visible_range_sync(visible_range.clone(), app_cx);
                                    let muted = app_cx.theme().muted_foreground;
                                    let mono = app_cx.theme().mono_font_family.clone();
                                    let rows = view.render_lines_for(visible_range);

                                    rows.into_iter()
                                        .map(|row| {
                                            h_flex()
                                                .gap_3()
                                                .px_3()
                                                .h(preview_line_height())
                                                .font_family(mono.clone())
                                                .child(
                                                    div()
                                                        .w(px(64.))
                                                        .text_right()
                                                        .text_color(muted)
                                                        .child(row.line_number),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .when(row.missing, |this| {
                                                            this.text_color(muted.opacity(0.75))
                                                        })
                                                        .child(row.text),
                                                )
                                        })
                                        .collect()
                                },
                            ),
                        )
                        .track_scroll(self.scroll_handle.clone())
                        .with_sizing_behavior(ListSizingBehavior::Auto)
                        .p_2(),
                    ),
            )
    }

    pub(super) fn queue_visible_range_sync(
        &mut self,
        visible: std::ops::Range<usize>,
        cx: &mut App,
    ) {
        self.pending_visible_range = Some(visible);
        if self.scheduled_visible_sync {
            return;
        }
        self.scheduled_visible_sync = true;
        let entity_id = self.entity_id;
        cx.defer(move |cx| {
            cx.notify(entity_id);
        });
    }

    pub(super) fn flush_pending_visible_range(&mut self, cx: &mut App) {
        self.scheduled_visible_sync = false;
        let Some(visible) = self.pending_visible_range.take() else {
            return;
        };
        self.sync_visible_range(visible, cx);
    }

    pub(super) fn sync_visible_range(&mut self, visible: std::ops::Range<usize>, cx: &mut App) {
        perf::record_preview_visible_sync();
        let (line_count, already_loaded, direction) = {
            let preview = self.preview.read(cx);
            let preview_state = preview.state();
            let Some(document) = &preview_state.preview_document else {
                self.last_requested_load_range = 0..0;
                return;
            };
            let line_count = document.line_count();
            let load_window = bucket_visible_range(
                visible.clone(),
                crate::ui::state::PreviewPanelState::VISIBLE_BUCKET_LINES,
                line_count,
            );
            let already_loaded = preview_state.has_loaded_range(&load_window);
            let anchor = visible.start.min(line_count.saturating_sub(1));
            let direction = if anchor >= self.last_scroll_anchor {
                PreviewScrollDirection::Down
            } else {
                PreviewScrollDirection::Up
            };
            (line_count, already_loaded, direction)
        };
        if line_count == 0 {
            self.last_requested_load_range = 0..0;
            return;
        }

        let anchor = visible.start.min(line_count.saturating_sub(1));
        self.last_scroll_anchor = anchor;
        let render_window = bucket_visible_range(
            visible.clone(),
            crate::ui::state::PreviewPanelState::RENDER_WINDOW_LINES,
            line_count,
        );
        self.refresh_render_cache(render_window, cx);
        let load_window = bucket_visible_range(
            visible,
            crate::ui::state::PreviewPanelState::VISIBLE_BUCKET_LINES,
            line_count,
        );
        if self.last_requested_load_range == load_window {
            return;
        }
        self.last_requested_load_range = load_window.clone();
        if already_loaded {
            return;
        }

        self.workspace.update(cx, |workspace, cx| {
            workspace.request_preview_range(load_window, direction, cx);
        });
    }

    pub(super) fn refresh_render_cache(&mut self, visible: std::ops::Range<usize>, cx: &mut App) {
        if visible.is_empty() {
            self.render_cache.clear();
            self.render_cache_range = visible;
            self.render_cache_revision = self.preview.read(cx).render_revision();
            return;
        }
        let render_revision = self.preview.read(cx).render_revision();
        if self.render_cache_revision == render_revision && self.render_cache_range == visible {
            return;
        }
        let overlaps = self.render_cache_range.start < visible.end
            && visible.start < self.render_cache_range.end
            && !self.render_cache.is_empty();
        if self.render_cache_range != visible {
            if overlaps && self.render_cache_revision == render_revision {
                self.patch_render_cache_range(visible.clone(), cx);
                perf::record_preview_render_cache_partial_update();
            } else {
                self.render_cache = self.preview.read(cx).build_render_lines(visible.clone());
                perf::record_preview_render_cache_rebuild();
            }
        } else {
            self.patch_render_cache_contents(cx);
            perf::record_preview_render_cache_partial_update();
        }
        self.render_cache_range = visible;
        self.render_cache_revision = render_revision;
    }

    fn patch_render_cache_range(&mut self, visible: std::ops::Range<usize>, cx: &mut App) {
        let overlap_start = self.render_cache_range.start.max(visible.start);
        let overlap_end = self.render_cache_range.end.min(visible.end);
        let mut next_cache = Vec::with_capacity(visible.end.saturating_sub(visible.start));
        let preview = self.preview.read(cx);

        if visible.start < overlap_start {
            next_cache.extend(preview.build_render_lines_partial(visible.start..overlap_start));
        }
        if overlap_start < overlap_end {
            let start_ix = overlap_start.saturating_sub(self.render_cache_range.start);
            let end_ix = overlap_end.saturating_sub(self.render_cache_range.start);
            next_cache.extend(self.render_cache[start_ix..end_ix].iter().cloned());
        }
        if overlap_end < visible.end {
            next_cache.extend(preview.build_render_lines_partial(overlap_end..visible.end));
        }

        self.render_cache = next_cache;
    }

    fn patch_render_cache_contents(&mut self, cx: &mut App) {
        let preview = self.preview.read(cx);
        for (offset, line) in self.render_cache.iter_mut().enumerate() {
            let ix = self.render_cache_range.start + offset;
            let next = preview.build_render_line(ix);
            if *line != next {
                *line = next;
            }
        }
    }

    fn render_lines_for(
        &self,
        visible_range: std::ops::Range<usize>,
    ) -> Vec<crate::ui::preview_model::PreviewRenderLine> {
        visible_range
            .filter_map(|ix| {
                if ix < self.render_cache_range.start || ix >= self.render_cache_range.end {
                    return None;
                }
                self.render_cache
                    .get(ix.saturating_sub(self.render_cache_range.start))
                    .cloned()
            })
            .collect()
    }
}

fn bucket_visible_range(
    visible: std::ops::Range<usize>,
    bucket_lines: usize,
    line_count: usize,
) -> std::ops::Range<usize> {
    if line_count == 0 || bucket_lines == 0 {
        return 0..0;
    }
    let start = visible.start.min(line_count.saturating_sub(1));
    let end = visible.end.max(start + 1).min(line_count);
    let bucket_start = (start / bucket_lines) * bucket_lines;
    let bucket_end = end
        .saturating_sub(1)
        .checked_div(bucket_lines)
        .map(|bucket| (bucket + 1) * bucket_lines)
        .unwrap_or(bucket_lines)
        .min(line_count);
    bucket_start..bucket_end.max(bucket_start + 1).min(line_count)
}
