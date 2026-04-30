use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, DragMoveEvent, Empty,
    InteractiveElement, IntoElement, ListSizingBehavior, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, UniformListDecoration, Window, div,
    prelude::FluentBuilder as _, px, uniform_list,
};
use gpui_component::{
    ActiveTheme as _, Disableable, IconName, Sizable, Size, StyledExt as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    list::ListItem,
    resizable::h_resizable,
    resizable::resizable_panel,
    scroll::ScrollableElement,
    tab::Tab,
    tab::TabBar,
    table::Table,
    tree::{TreeState, tree},
    v_flex, v_virtual_list,
};

use super::view::{
    activity_row, card, empty_box, flow_card, format_tree_summary, panel_frame, panel_viewport,
    render_blacklist_section, render_blacklist_tag, render_info_block, render_kv, render_tree_row,
    section_caption, section_title, selected_file_row, stat_tile, status_banner, tab_icon_badge,
};
use super::{
    PreviewPaneView, TreePaneView, TreeViewMode, Workspace, fixed_list_sizes, preview_line_height,
    workspace_panel_min_height,
};
use crate::domain::{OutputFormat, TemporaryWhitelistMode};
use crate::ui::perf;
use crate::ui::preview_model::PreviewScrollDirection;
use crate::ui::state::{PendingConfirmation, SidePanelTab};
use crate::utils::i18n::tr;

#[derive(Clone)]
struct PreviewVisibleRangeDecoration {
    preview_pane: gpui::Entity<PreviewPaneView>,
}

#[derive(Clone)]
struct SelectedFilesResizeDrag {
    start_height: u16,
    start_y: gpui::Pixels,
}

const SELECTED_FILES_RESIZE_STEP_PX: f32 = 6.0;
const PREVIEW_PENDING_RANGE_PADDING_LINES: usize = 24;

impl Render for SelectedFilesResizeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

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

    fn render_input_toolbar(
        &self,
        language: crate::domain::Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            )
            .into_any_element()
    }

    fn render_selected_files_section(
        &self,
        language: crate::domain::Language,
        selected_files: Rc<Vec<crate::domain::FileEntry>>,
        panel_height: gpui::Pixels,
        start_height: u16,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                            .child(selected_files.len().to_string()),
                    ),
            )
            .child(self.render_selected_files_body(
                language,
                selected_files,
                panel_height,
                start_height,
                cx,
            ))
            .into_any_element()
    }

    fn render_selected_files_body(
        &self,
        language: crate::domain::Language,
        selected_files: Rc<Vec<crate::domain::FileEntry>>,
        panel_height: gpui::Pixels,
        start_height: u16,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if selected_files.is_empty() {
            return empty_box(
                tr(language, "selected_files_empty"),
                tr(language, "selected_files_hint"),
                IconName::File,
                cx,
            )
            .into_any_element();
        }

        self.render_selected_files_list(selected_files, panel_height, start_height, cx)
    }

    fn render_selected_files_list(
        &self,
        rows: Rc<Vec<crate::domain::FileEntry>>,
        panel_height: gpui::Pixels,
        start_height: u16,
        cx: &App,
    ) -> AnyElement {
        v_flex()
            .gap_1()
            .child(
                div()
                    .w_full()
                    .min_h(panel_height)
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(px(12.))
                    .bg(cx.theme().secondary.opacity(0.22))
                    .child(
                        v_flex()
                            .w_full()
                            .children(rows.iter().map(|entry| selected_file_row(entry, cx)))
                            .p_1(),
                    ),
            )
            .child(self.render_selected_files_resize_handle(start_height, cx))
            .into_any_element()
    }

    fn render_selected_files_resize_handle(&self, start_height: u16, cx: &App) -> AnyElement {
        let resize_ui_for_drag = self.ui.clone();

        h_flex()
            .id("selected-files-resize-handle")
            .w_full()
            .h(px(18.))
            .justify_center()
            .items_center()
            .cursor_row_resize()
            .on_drag(
                start_height,
                move |drag_start_height: &u16,
                      position: gpui::Point<gpui::Pixels>,
                      _: &mut Window,
                      cx: &mut App| {
                    cx.stop_propagation();
                    cx.new(|_| SelectedFilesResizeDrag {
                        start_height: *drag_start_height,
                        start_y: position.y,
                    })
                },
            )
            .on_drag_move(
                move |event: &DragMoveEvent<SelectedFilesResizeDrag>,
                      _: &mut Window,
                      cx: &mut App| {
                    let drag = event.drag(cx);
                    let delta = f32::from(event.event.position.y - drag.start_y);
                    let quantized_delta = (delta / SELECTED_FILES_RESIZE_STEP_PX).round()
                        * SELECTED_FILES_RESIZE_STEP_PX;
                    let next_height = (f32::from(drag.start_height) + quantized_delta)
                        .round()
                        .max(0.0) as u16;
                    resize_ui_for_drag.update(cx, |ui, ui_cx| {
                        if ui.set_selected_files_panel_height(next_height) {
                            ui_cx.notify();
                        }
                    });
                },
            )
            .child(
                div()
                    .w(px(52.))
                    .h(px(4.))
                    .rounded(px(999.))
                    .bg(cx.theme().border.opacity(0.85)),
            )
            .into_any_element()
    }

    fn render_temporary_blacklist_sections(
        &self,
        language: crate::domain::Language,
        selection: &crate::ui::state::SelectionState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sections = Rc::new(super::model::build_blacklist_sections(
            &selection.temp_folder_blacklist,
            &selection.temp_ext_blacklist,
            "",
            language,
        ));

        div()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(12.))
            .bg(cx.theme().secondary.opacity(0.22))
            .child(if sections.is_empty() {
                empty_box(
                    tr(language, "temporary_rules_empty_title"),
                    tr(language, "temporary_rules_empty_hint"),
                    IconName::BookOpen,
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
                    .child(v_flex().gap_3().children(sections.iter().enumerate().map(
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
                                            "remove-temporary-blacklist",
                                            section_ix * 1000 + ix,
                                        ))
                                        .ghost()
                                        .compact()
                                        .with_size(Size::Small)
                                        .icon(IconName::Delete)
                                        .disabled(!item.deletable)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.remove_temporary_blacklist_item(
                                                kind,
                                                value.clone(),
                                                window,
                                                cx,
                                            );
                                        }))
                                        .into_any_element(),
                                        cx,
                                    )
                                })
                                .collect::<Vec<_>>();
                            render_blacklist_section(section, tags, cx)
                        },
                    )))
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_temporary_whitelist_sections(
        &self,
        language: crate::domain::Language,
        selection: &crate::ui::state::SelectionState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sections = Rc::new(super::model::build_blacklist_sections(
            &selection.temp_folder_whitelist,
            &selection.temp_ext_whitelist,
            "",
            language,
        ));

        div()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(12.))
            .bg(cx.theme().secondary.opacity(0.22))
            .child(if sections.is_empty() {
                empty_box(
                    tr(language, "temporary_whitelist_empty_title"),
                    tr(language, "temporary_whitelist_empty_hint"),
                    IconName::File,
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
                    .child(v_flex().gap_3().children(sections.iter().enumerate().map(
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
                                            "remove-temporary-whitelist",
                                            section_ix * 1000 + ix,
                                        ))
                                        .ghost()
                                        .compact()
                                        .with_size(Size::Small)
                                        .icon(IconName::Delete)
                                        .disabled(!item.deletable)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.remove_temporary_whitelist_item(
                                                kind,
                                                value.clone(),
                                                window,
                                                cx,
                                            );
                                        }))
                                        .into_any_element(),
                                        cx,
                                    )
                                })
                                .collect::<Vec<_>>();
                            render_blacklist_section(section, tags, cx)
                        },
                    )))
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_temporary_rules_section(
        &self,
        language: crate::domain::Language,
        selection: &crate::ui::state::SelectionState,
        gitignore_label: String,
        has_gitignore_file: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_temporary_rules =
            !selection.temp_folder_blacklist.is_empty() || !selection.temp_ext_blacklist.is_empty();
        let has_temporary_whitelist = !selection.temp_folder_whitelist.is_empty()
            || !selection.temp_ext_whitelist.is_empty()
            || selection.temp_whitelist_mode != TemporaryWhitelistMode::WhitelistThenBlacklist;
        let whitelist_mode_index = match selection.temp_whitelist_mode {
            TemporaryWhitelistMode::WhitelistThenBlacklist => 0,
            TemporaryWhitelistMode::WhitelistOnly => 1,
        };
        let whitelist_hint_key = match selection.temp_whitelist_mode {
            TemporaryWhitelistMode::WhitelistThenBlacklist => {
                "temporary_whitelist_hint_then_blacklist"
            }
            TemporaryWhitelistMode::WhitelistOnly => "temporary_whitelist_hint_whitelist_only",
        };

        v_flex()
            .gap_3()
            .child(section_title(
                tr(language, "panel_temporary_rules"),
                IconName::BookOpen,
                cx,
            ))
            .child(render_info_block(
                tr(language, "gitignore"),
                gitignore_label,
                has_gitignore_file,
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
                            .disabled(!has_gitignore_file)
                            .on_click(cx.listener(Self::apply_gitignore)),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(language, "temporary_rules_hint")),
            )
            .child(Input::new(&self.temp_blacklist_add_input).prefix(IconName::Plus))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("add-temporary-folder-blacklist")
                            .outline()
                            .icon(IconName::Folder)
                            .label(tr(language, "add_temp_folder"))
                            .on_click(cx.listener(Self::add_temporary_folder_blacklist)),
                    )
                    .child(
                        Button::new("add-temporary-ext-blacklist")
                            .outline()
                            .icon(IconName::File)
                            .label(tr(language, "add_temp_ext"))
                            .on_click(cx.listener(Self::add_temporary_ext_blacklist)),
                    ),
            )
            .child(self.render_temporary_blacklist_sections(language, selection, cx))
            .child(
                Button::new("clear-temporary-blacklist")
                    .outline()
                    .icon(IconName::Delete)
                    .label(tr(language, "clear_temporary_rules"))
                    .disabled(!has_temporary_rules)
                    .on_click(cx.listener(Self::clear_temporary_blacklist)),
            )
            .child(
                div()
                    .pt_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex()
                            .gap_3()
                            .child(section_caption(
                                tr(language, "temporary_whitelist_section"),
                                IconName::File,
                                cx,
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(language, whitelist_hint_key)),
                            )
                            .child(
                                TabBar::new("temporary-whitelist-mode")
                                    .selected_index(whitelist_mode_index)
                                    .on_click(cx.listener(Self::set_temporary_whitelist_mode))
                                    .child(
                                        Tab::new()
                                            .label(tr(language, "whitelist_mode_then_blacklist")),
                                    )
                                    .child(
                                        Tab::new()
                                            .label(tr(language, "whitelist_mode_whitelist_only")),
                                    ),
                            )
                            .child(
                                Input::new(&self.temp_whitelist_add_input).prefix(IconName::Plus),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("add-temporary-folder-whitelist")
                                            .outline()
                                            .icon(IconName::Folder)
                                            .label(tr(language, "add_temp_whitelist_folder"))
                                            .on_click(
                                                cx.listener(Self::add_temporary_folder_whitelist),
                                            ),
                                    )
                                    .child(
                                        Button::new("add-temporary-ext-whitelist")
                                            .outline()
                                            .icon(IconName::File)
                                            .label(tr(language, "add_temp_whitelist_ext"))
                                            .on_click(
                                                cx.listener(Self::add_temporary_ext_whitelist),
                                            ),
                                    ),
                            )
                            .child(
                                self.render_temporary_whitelist_sections(language, selection, cx),
                            )
                            .child(
                                Button::new("clear-temporary-whitelist")
                                    .outline()
                                    .icon(IconName::Delete)
                                    .label(tr(language, "clear_temporary_whitelist"))
                                    .disabled(!has_temporary_whitelist)
                                    .on_click(cx.listener(Self::clear_temporary_whitelist)),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_options_section(
        &self,
        language: crate::domain::Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_snapshot(cx);
        let selection = self.selection_snapshot(cx);

        v_flex()
            .gap_3()
            .child(section_title(
                tr(language, "section_options"),
                IconName::Settings2,
                cx,
            ))
            .child(
                v_flex()
                    .gap_2()
                    .child(section_caption(tr(language, "format"), IconName::File, cx))
                    .child(
                        TabBar::new("output-format")
                            .selected_index(match settings.options.output_format {
                                OutputFormat::Default => 0,
                                OutputFormat::Xml => 1,
                                OutputFormat::PlainText => 2,
                                OutputFormat::Markdown => 3,
                            })
                            .on_click(cx.listener(Self::set_output_format))
                            .child(Tab::new().label(tr(language, "format_default")))
                            .child(Tab::new().label(tr(language, "format_xml")))
                            .child(Tab::new().label(tr(language, "format_plain")))
                            .child(Tab::new().label(tr(language, "format_markdown"))),
                    ),
            )
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
            .into_any_element()
    }

    fn render_input_danger_zone(
        &self,
        language: crate::domain::Language,
        pending_confirmation: Option<PendingConfirmation>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                                if pending_confirmation == Some(PendingConfirmation::ClearInputs) {
                                    tr(language, "confirm_clear_inputs")
                                } else {
                                    tr(language, "clear")
                                },
                            )
                            .on_click(cx.listener(Self::clear_inputs)),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_input_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.settings_snapshot(cx);
        let selection = self.selection_snapshot(cx);
        let ui_state = self.ui_state(cx);
        let language = settings.language;
        let has_inputs = self.has_inputs(cx);
        let selected_files = Rc::new(selection.selected_files.clone());
        let selected_files_panel_height = px(f32::from(ui_state.selected_files_panel_height));
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
            .unwrap_or_else(|| tr(language, "temporary_gitignore_empty").to_string());

        flow_card(cx).child(
            v_flex()
                .gap_4()
                .w_full()
                .child(section_title(
                    tr(language, "panel_inputs"),
                    IconName::PanelLeft,
                    cx,
                ))
                .child(self.render_input_toolbar(language, cx))
                .child(render_info_block(
                    tr(language, "folder"),
                    folder_label,
                    has_inputs,
                    IconName::FolderOpen,
                    cx,
                ))
                .child(self.render_selected_files_section(
                    language,
                    selected_files,
                    selected_files_panel_height,
                    ui_state.selected_files_panel_height,
                    cx,
                ))
                .child(self.render_temporary_rules_section(
                    language,
                    &selection,
                    gitignore_label,
                    selection.gitignore_file.is_some(),
                    cx,
                ))
                .child(self.render_options_section(language, cx))
                .child(self.render_input_danger_zone(language, ui_state.pending_confirmation, cx)),
        )
    }

    pub(super) fn render_status_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        card(cx).child(self.render_status_panel_body(cx))
    }

    fn build_status_panel_view_model(&self, cx: &App) -> super::model::StatusPanelViewModel {
        let result = self.result.read(cx);
        let result_state = result.state();
        let merged_file_size_hint = result_state
            .result
            .as_ref()
            .and_then(|result| result.merged_content_path.as_ref())
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|metadata| super::view::format_size(metadata.len()));
        let process = self.process.read(cx);

        super::model::build_status_panel_view_model(
            process.state(),
            result_state.result.as_ref(),
            self.language(cx),
            merged_file_size_hint,
        )
    }

    fn render_status_metric_row(
        &self,
        metrics: &[super::model::StatusMetricViewModel; 3],
        cx: &App,
    ) -> AnyElement {
        h_flex()
            .gap_2()
            .children(
                metrics
                    .iter()
                    .map(|metric| stat_tile(metric.label.as_ref(), metric.value.to_string(), cx)),
            )
            .into_any_element()
    }

    fn render_status_progress_section(
        &self,
        language: crate::domain::Language,
        progress: &super::model::StatusProgressViewModel,
        cx: &App,
    ) -> AnyElement {
        let bar_fill = px((progress.fill_ratio * 240.0).round());

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
                            .child(progress.value_text.clone()),
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
            .child(render_kv(
                tr(language, "elapsed"),
                progress.elapsed_value.to_string(),
                cx,
            ))
            .child(render_kv(
                tr(language, "processing"),
                progress.current_file.to_string(),
                cx,
            ))
            .into_any_element()
    }

    fn render_status_activity_section(
        &self,
        language: crate::domain::Language,
        activity_rows: &[crate::domain::ProcessRecord],
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                let rows = Rc::new(activity_rows.to_vec());
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
            })
            .into_any_element()
    }

    fn render_status_panel_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language(cx);
        let process_actions = self.render_process_actions(cx).into_any_element();
        let vm = self.build_status_panel_view_model(cx);

        let content = v_flex()
            .gap_4()
            .size_full()
            .min_h(px(0.))
            .child(section_title(
                tr(language, "panel_status"),
                IconName::LayoutDashboard,
                cx,
            ))
            .child(process_actions)
            .child(self.render_status_metric_row(&vm.summary_metrics, cx))
            .child(self.render_status_metric_row(&vm.result_metrics, cx))
            .child(status_banner(
                vm.status_title.as_ref(),
                vm.status_message.to_string(),
                vm.status,
                cx,
            ));

        let content = if let Some(alert) = self.config_alert.as_ref() {
            let tone = if alert.is_error {
                cx.theme().danger.opacity(0.16)
            } else {
                cx.theme().warning.opacity(0.16)
            };
            content.child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(tone)
                    .child(div().font_semibold().child(alert.title.clone()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(alert.detail.clone()),
                    )
                    .child(
                        Button::new("config-alert-action")
                            .outline()
                            .with_size(Size::Small)
                            .label(alert.action_label.clone())
                            .on_click(cx.listener(Self::handle_config_alert_action)),
                    ),
            )
        } else {
            content
        };

        let content = if let Some(archive_summary) = vm.archive_summary.as_ref() {
            content.child(render_info_block(
                archive_summary.label.as_ref(),
                archive_summary.value.to_string(),
                true,
                IconName::File,
                cx,
            ))
        } else {
            content
        };

        content
            .child(self.render_status_progress_section(language, &vm.progress, cx))
            .child(self.render_status_activity_section(language, &vm.activity_rows, cx))
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

    fn build_results_panel_view_model(&self, cx: &App) -> super::model::ResultsPanelViewModel {
        let result = self.result.read(cx);
        super::model::build_results_panel_view_model(
            result.state().active_tab,
            result.has_content_result(),
            self.language(cx),
        )
    }

    fn render_results_toolbar(
        &self,
        language: crate::domain::Language,
        view_model: &super::model::ResultsPanelViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let copy_button = match view_model.copy_action {
            super::model::ResultsCopyAction::Tree => Button::new("copy-active")
                .outline()
                .icon(IconName::Copy)
                .label(view_model.copy_label.clone())
                .on_click(cx.listener(Self::copy_tree))
                .into_any_element(),
            super::model::ResultsCopyAction::Preview => Button::new("copy-active")
                .outline()
                .icon(IconName::Copy)
                .label(view_model.copy_label.clone())
                .on_click(cx.listener(Self::copy_preview))
                .into_any_element(),
        };

        h_flex()
            .justify_between()
            .items_center()
            .child(
                TabBar::new("result-tabs")
                    .selected_index(view_model.selected_tab)
                    .on_click(cx.listener(Self::set_tab))
                    .child(
                        Tab::new()
                            .prefix(tab_icon_badge(IconName::FolderOpen, false, cx))
                            .label(tr(language, "tab_tree_preview")),
                    )
                    .child(
                        Tab::new()
                            .prefix(tab_icon_badge(IconName::SquareTerminal, true, cx))
                            .disabled(!view_model.has_content_result)
                            .label(tr(language, "tab_merged_content")),
                    ),
            )
            .child(
                h_flex().gap_2().child(copy_button).child(
                    Button::new("download-result")
                        .outline()
                        .icon(IconName::ArrowDown)
                        .label(tr(language, "download"))
                        .disabled(!view_model.has_content_result)
                        .on_click(cx.listener(Self::download_result)),
                ),
            )
            .into_any_element()
    }

    fn render_results_body(
        &mut self,
        view_model: &super::model::ResultsPanelViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .child(match view_model.body {
                super::model::ResultsPanelBodyViewModel::Tree => {
                    self.tree_pane_view.clone().into_any_element()
                }
                super::model::ResultsPanelBodyViewModel::Content => {
                    self.render_content_panel(cx).into_any_element()
                }
            })
            .into_any_element()
    }

    pub(super) fn render_results_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language(cx);
        let view_model = self.build_results_panel_view_model(cx);

        v_flex()
            .gap_3()
            .size_full()
            .min_h(px(0.))
            .child(self.render_results_toolbar(language, &view_model, cx))
            .child(self.render_results_body(&view_model, cx))
    }

    fn render_rules_editor(
        &self,
        language: crate::domain::Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            )
            .into_any_element()
    }

    fn render_blacklist_transfer_actions(
        &self,
        language: crate::domain::Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            )
            .into_any_element()
    }

    fn render_blacklist_sections(
        &self,
        language: crate::domain::Language,
        sections: Rc<Vec<super::model::BlacklistSectionViewModel>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(12.))
            .bg(cx.theme().secondary.opacity(0.22))
            .child(if sections.is_empty() {
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
                    .child(v_flex().gap_3().children(sections.iter().enumerate().map(
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
                                        Button::new(("remove-blacklist", section_ix * 1000 + ix))
                                            .ghost()
                                            .compact()
                                            .with_size(Size::Small)
                                            .icon(IconName::Delete)
                                            .disabled(!item.deletable)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.remove_blacklist_item(
                                                    kind,
                                                    value.clone(),
                                                    window,
                                                    cx,
                                                );
                                            }))
                                            .into_any_element(),
                                        cx,
                                    )
                                })
                                .collect::<Vec<_>>();
                            render_blacklist_section(section, tags, cx)
                        },
                    )))
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_rules_footer(
        &self,
        language: crate::domain::Language,
        pending_confirmation: Option<PendingConfirmation>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                                if pending_confirmation == Some(PendingConfirmation::ResetBlacklist)
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
                                if pending_confirmation == Some(PendingConfirmation::ClearBlacklist)
                                {
                                    tr(language, "confirm_clear_blacklist")
                                } else {
                                    tr(language, "blacklist_clear_all")
                                },
                            )
                            .on_click(cx.listener(Self::clear_blacklist)),
                    ),
            )
            .into_any_element()
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
            .child(self.render_rules_editor(language, cx))
            .child(
                Input::new(&self.blacklist_filter_input)
                    .prefix(IconName::Search)
                    .cleanable(true),
            )
            .child(self.render_blacklist_transfer_actions(language, cx))
            .child(self.render_blacklist_sections(language, blacklist_sections, cx))
            .child(self.render_rules_footer(language, ui_state.pending_confirmation, cx))
    }

    fn build_content_panel_view_model(&self, cx: &App) -> super::model::ContentPanelViewModel {
        let result = self.result.read(cx);
        let preview_rows_len = result.state().preview_rows.len();
        let filter_active = !self.preview_filter_input.read(cx).value().trim().is_empty();

        super::model::build_content_panel_view_model(
            self.result_is_tree_only(cx),
            preview_rows_len,
            filter_active,
            self.ui_state(cx).content_file_list_collapsed,
            self.language(cx),
        )
    }

    fn render_content_tree_only_state(
        &self,
        empty_state: &super::model::EmptyStateViewModel,
        cx: &App,
    ) -> AnyElement {
        v_flex()
            .gap_3()
            .size_full()
            .min_h(px(0.))
            .child(empty_box(
                empty_state.title.clone(),
                empty_state.hint.clone(),
                IconName::FolderOpen,
                cx,
            ))
            .into_any_element()
    }

    fn render_content_file_list_section(
        &self,
        language: crate::domain::Language,
        file_list: &super::model::ContentFileListViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let file_list_toggle_icon = if file_list.file_list_collapsed {
            IconName::ChevronDown
        } else {
            IconName::ChevronUp
        };

        div()
            .flex_none()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(14.))
            .bg(cx.theme().secondary.opacity(0.18))
            .p_3()
            .child(
                v_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .gap_3()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(section_caption(
                                        tr(language, "content_files_title"),
                                        IconName::File,
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(file_list.visible_row_count.to_string()),
                                    ),
                            )
                            .child(
                                Button::new("toggle-content-file-list")
                                    .ghost()
                                    .compact()
                                    .with_size(Size::Small)
                                    .icon(file_list_toggle_icon)
                                    .label(file_list.toggle_label.clone())
                                    .on_click(
                                        cx.listener(Self::toggle_content_file_list_collapsed),
                                    ),
                            ),
                    )
                    .when(!file_list.file_list_collapsed, |this| {
                        this.child(self.render_content_file_list_body(file_list, cx))
                    }),
            )
            .into_any_element()
    }

    fn render_content_file_list_body(
        &self,
        file_list: &super::model::ContentFileListViewModel,
        cx: &App,
    ) -> AnyElement {
        let preview_filter_input = self.preview_filter_input.clone();
        let preview_table = self.preview_table.clone();

        v_flex()
            .gap_3()
            .child(
                Input::new(&preview_filter_input)
                    .prefix(IconName::Search)
                    .cleanable(true),
            )
            .child(
                div()
                    .h(px(280.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(px(14.))
                    .bg(cx.theme().secondary.opacity(0.35))
                    .child(self.render_content_file_list_table(
                        file_list.empty_state.as_ref(),
                        preview_table,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_content_file_list_table(
        &self,
        empty_state: Option<&super::model::EmptyStateViewModel>,
        preview_table: gpui::Entity<gpui_component::table::TableState<super::PreviewTableDelegate>>,
        cx: &App,
    ) -> AnyElement {
        if empty_state.is_none() {
            return Table::new(&preview_table)
                .with_size(Size::Small)
                .bordered(false)
                .stripe(true)
                .into_any_element();
        }

        let empty_state = empty_state.expect("checked empty state");
        empty_box(
            empty_state.title.clone(),
            empty_state.hint.clone(),
            IconName::File,
            cx,
        )
        .into_any_element()
    }

    fn render_content_preview_section(
        &self,
        language: crate::domain::Language,
        cx: &App,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(14.))
            .bg(cx.theme().secondary.opacity(0.18))
            .p_3()
            .child(
                v_flex()
                    .gap_3()
                    .size_full()
                    .min_h(px(0.))
                    .child(section_caption(
                        tr(language, "content_preview_title"),
                        IconName::SquareTerminal,
                        cx,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_hidden()
                            .child(self.preview_pane_view.clone()),
                    ),
            )
            .into_any_element()
    }

    fn render_content_split_body(
        &self,
        language: crate::domain::Language,
        body: &super::model::ContentBodyViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .gap_3()
            .size_full()
            .min_h(px(0.))
            .child(self.render_content_file_list_section(language, &body.file_list, cx))
            .child(self.render_content_preview_section(language, cx))
            .into_any_element()
    }

    pub(super) fn render_content_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language(cx);
        let view_model = self.build_content_panel_view_model(cx);

        match &view_model.body {
            super::model::ContentPanelBodyViewModel::TreeOnly(empty_state) => {
                self.render_content_tree_only_state(empty_state, cx)
            }
            super::model::ContentPanelBodyViewModel::Split(body) => {
                self.render_content_split_body(language, body, cx)
            }
        }
    }

    fn build_compact_content_panel_view_model(
        &self,
        cx: &App,
    ) -> super::model::CompactContentPanelViewModel {
        super::model::build_compact_content_panel_view_model(self.ui_state(cx).narrow_content_tab)
    }

    fn render_compact_content_body(
        &mut self,
        view_model: &super::model::CompactContentPanelViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .child(match view_model.body {
                super::model::CompactContentBodyViewModel::Status => {
                    self.status_panel_view.clone().into_any_element()
                }
                super::model::CompactContentBodyViewModel::Results => {
                    self.render_right_panel_body(cx).into_any_element()
                }
            })
            .into_any_element()
    }

    pub(super) fn render_compact_content_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let language = self.language(cx);
        let view_model = self.build_compact_content_panel_view_model(cx);

        card(cx).size_full().child(
            v_flex()
                .gap_3()
                .size_full()
                .min_h(px(0.))
                .child(
                    TabBar::new("compact-content-tabs")
                        .selected_index(view_model.selected_tab)
                        .on_click(cx.listener(Self::set_narrow_content_tab))
                        .child(
                            Tab::new()
                                .prefix(tab_icon_badge(IconName::LayoutDashboard, false, cx))
                                .label(tr(language, "panel_status")),
                        )
                        .child(
                            Tab::new()
                                .prefix(tab_icon_badge(IconName::PanelRight, true, cx))
                                .label(tr(language, "panel_results")),
                        ),
                )
                .child(self.render_compact_content_body(&view_model, cx)),
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
    fn build_tree_pane_view_model(
        &self,
        cx: &App,
    ) -> (
        crate::domain::Language,
        gpui::Entity<InputState>,
        gpui::Entity<TreeState>,
        super::model::TreePaneViewModel,
    ) {
        let workspace = self.workspace.read(cx);
        let language = workspace.language(cx);
        let filter_input = workspace.tree_panel.filter_input.clone();
        let tree_state = workspace.tree_panel.state.clone();
        let tree_filter = filter_input.read(cx).value().trim().to_string();
        let result = workspace.result.read(cx);
        let view_model = super::model::build_tree_pane_view_model(
            &workspace.tree_panel.render_state,
            workspace.tree_panel.total_summary,
            tree_filter.as_str(),
            result.state().result.as_ref(),
            language,
            matches!(self.view_mode, TreeViewMode::PlainText),
        );

        (language, filter_input, tree_state, view_model)
    }

    fn render_tree_toolbar(
        &self,
        language: crate::domain::Language,
        filter_input: &gpui::Entity<InputState>,
        disable_structure_actions: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expand_workspace = self.workspace.clone();
        let collapse_workspace = self.workspace.clone();

        h_flex()
            .gap_2()
            .items_center()
            .child(
                Input::new(filter_input)
                    .prefix(IconName::Search)
                    .cleanable(true),
            )
            .child(
                Button::new("tree-expand")
                    .outline()
                    .icon(IconName::ChevronDown)
                    .label(tr(language, "tree_expand_all"))
                    .disabled(disable_structure_actions)
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
                    .disabled(disable_structure_actions)
                    .on_click(cx.listener(move |_, event, window, cx| {
                        collapse_workspace.update(cx, |workspace, cx| {
                            workspace.collapse_tree(event, window, cx);
                        });
                    })),
            )
            .into_any_element()
    }

    fn render_tree_summary_bar(
        &self,
        language: crate::domain::Language,
        view_model: &super::model::TreePaneViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .justify_between()
            .items_center()
            .px_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format_tree_summary(
                        view_model.visible_summary,
                        view_model.total_summary,
                        language,
                    )),
            )
            .child(
                Button::new("tree-view-mode")
                    .outline()
                    .label(view_model.view_mode_label.clone())
                    .on_click(cx.listener(Self::toggle_view_mode)),
            )
            .into_any_element()
    }

    fn render_tree_view(
        &self,
        language: crate::domain::Language,
        tree_state: &gpui::Entity<TreeState>,
    ) -> AnyElement {
        let row_workspace = self.workspace.clone();

        tree(tree_state, move |ix, entry, selected, _, cx| {
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
        .h_full()
        .into_any_element()
    }

    fn render_tree_plain_text_body(&self, lines: &[SharedString], cx: &App) -> AnyElement {
        div()
            .size_full()
            .min_h(px(0.))
            .overflow_y_scrollbar()
            .p_2()
            .child(v_flex().children(lines.iter().cloned().map(|line| {
                div()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_sm()
                    .whitespace_nowrap()
                    .child(line)
            })))
            .into_any_element()
    }

    fn render_tree_body(
        &self,
        view_model: &super::model::TreePaneViewModel,
        tree_view: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let content = match &view_model.body {
            super::model::TreePaneBodyViewModel::Tree => tree_view,
            super::model::TreePaneBodyViewModel::PlainText { lines } => {
                self.render_tree_plain_text_body(lines, cx)
            }
            super::model::TreePaneBodyViewModel::Empty { title, hint } => {
                empty_box(title.clone(), hint.clone(), IconName::FolderOpen, cx).into_any_element()
            }
        };

        div()
            .flex_1()
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(14.))
            .bg(cx.theme().secondary.opacity(0.35))
            .p_2()
            .child(content)
            .into_any_element()
    }

    pub(super) fn render_tree_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (language, filter_input, tree_state, view_model) = self.build_tree_pane_view_model(cx);
        let tree_view = self.render_tree_view(language, &tree_state);

        v_flex()
            .gap_3()
            .size_full()
            .min_h(px(0.))
            .child(self.render_tree_toolbar(
                language,
                &filter_input,
                view_model.disable_structure_actions,
                cx,
            ))
            .child(self.render_tree_summary_bar(language, &view_model, cx))
            .child(self.render_tree_body(&view_model, tree_view, cx))
    }
}

impl PreviewPaneView {
    pub(super) fn scroll_to_top(&mut self) {
        self.scroll_handle
            .scroll_to_item_strict(0, gpui::ScrollStrategy::Top);
        self.last_requested_load_range = 0..0;
        self.render_cache_range = 0..0;
        self.pending_visible_range = Some(0..0);
        self.last_synced_visible_range = None;
        self.last_scroll_anchor = 0;
    }

    fn build_preview_pane_view_model(
        &self,
        cx: &App,
    ) -> (crate::domain::Language, super::model::PreviewPaneViewModel) {
        let language = self.settings.read(cx).language();
        let result = self.result.read(cx);
        let preview = self.preview.read(cx);
        let view_model = super::model::build_preview_pane_view_model(
            result.state().result.as_ref(),
            preview.selected_preview_file_id(),
            preview.state().preview_rx.is_some(),
            preview.state().preview_error.as_deref(),
            preview.deferred_preview(),
            preview
                .preview_document()
                .map(|document| super::model::PreviewDocumentViewModel {
                    line_count: document.line_count(),
                    byte_len: document.byte_len(),
                    document_path: document.path().display().to_string(),
                }),
            language,
        );

        (language, view_model)
    }

    fn refresh_preview_content_cache(&mut self, line_count: usize, cx: &mut Context<Self>) {
        self.flush_pending_visible_range(cx);
        if self.render_cache_range.is_empty() && line_count > 0 {
            let initial =
                0..line_count.min(crate::ui::state::PreviewPanelState::RENDER_WINDOW_LINES);
            self.refresh_render_cache(initial, cx);
        } else {
            self.refresh_render_cache(self.render_cache_range.clone(), cx);
        }
    }

    fn render_preview_body(
        &mut self,
        language: crate::domain::Language,
        view_model: &super::model::PreviewPaneViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match &view_model.body {
            super::model::PreviewPaneBodyViewModel::DeferredMerged(deferred) => {
                self.last_requested_load_range = 0..0;
                self.render_deferred_merged_preview(language, deferred, cx)
            }
            super::model::PreviewPaneBodyViewModel::Error { title, detail } => {
                empty_box(title.clone(), detail.clone(), IconName::TriangleAlert, cx)
                    .into_any_element()
            }
            super::model::PreviewPaneBodyViewModel::Placeholder { title, detail } => {
                self.last_requested_load_range = 0..0;
                empty_box(title.clone(), detail.clone(), IconName::File, cx).into_any_element()
            }
            super::model::PreviewPaneBodyViewModel::Content(content) => {
                self.refresh_preview_content_cache(content.line_count, cx);
                let excerpt_banner = content
                    .excerpt_banner
                    .as_ref()
                    .map(|banner| self.render_deferred_excerpt_banner(language, banner, cx));

                self.render_preview_content(language, content, excerpt_banner, cx)
            }
        }
    }

    pub(super) fn render_preview_pane(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (language, view_model) = self.build_preview_pane_view_model(cx);
        self.render_preview_body(language, &view_model, cx)
    }

    fn render_preview_content(
        &mut self,
        language: crate::domain::Language,
        content: &super::model::PreviewContentViewModel,
        excerpt_banner: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .gap_2()
            .size_full()
            .min_h(px(0.))
            .child(self.render_preview_metadata(language, content, excerpt_banner, cx))
            .child(self.render_preview_lines_view(content.line_count, cx))
            .into_any_element()
    }

    fn render_preview_metadata(
        &self,
        language: crate::domain::Language,
        content: &super::model::PreviewContentViewModel,
        excerpt_banner: Option<AnyElement>,
        cx: &App,
    ) -> AnyElement {
        let metadata = v_flex()
            .gap_2()
            .child(render_kv(
                tr(language, "table_path"),
                content.file_path.clone(),
                cx,
            ))
            .when_some(excerpt_banner, |this, banner| this.child(banner));
        let metadata =
            if let Some((archive_path, archive_entry_path)) = content.archive_paths.as_ref() {
                metadata
                    .child(render_kv(
                        tr(language, "archive_path"),
                        archive_path.clone(),
                        cx,
                    ))
                    .child(render_kv(
                        tr(language, "archive_entry_path"),
                        archive_entry_path.clone(),
                        cx,
                    ))
            } else {
                metadata
            };

        metadata
            .child(
                h_flex()
                    .gap_3()
                    .child(render_kv(
                        tr(language, "line_count"),
                        content.line_count.to_string(),
                        cx,
                    ))
                    .child(render_kv(
                        tr(language, "byte_size"),
                        content.byte_len.to_string(),
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_preview_lines_view(&self, line_count: usize, cx: &mut Context<Self>) -> AnyElement {
        div()
            .relative()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .child(
                uniform_list(
                    "preview-lines",
                    line_count,
                    cx.processor(
                        move |view, visible_range: std::ops::Range<usize>, _, app_cx| {
                            let muted = app_cx.theme().muted_foreground;
                            let mono = app_cx.theme().mono_font_family.clone();
                            view.render_lines_for(visible_range, app_cx)
                                .into_iter()
                                .map(|row| Self::render_preview_line_row(row, muted, mono.clone()))
                                .collect()
                        },
                    ),
                )
                .with_decoration(PreviewVisibleRangeDecoration {
                    preview_pane: cx.entity(),
                })
                .track_scroll(self.scroll_handle.clone())
                .flex_grow()
                .size_full()
                .with_sizing_behavior(ListSizingBehavior::Auto)
                .p_2(),
            )
            .into_any_element()
    }

    fn render_preview_line_row(
        row: crate::ui::preview_model::PreviewRenderLine,
        muted: gpui::Hsla,
        mono: gpui::SharedString,
    ) -> AnyElement {
        h_flex()
            .w_full()
            .gap_3()
            .px_3()
            .h(preview_line_height())
            .overflow_hidden()
            .font_family(mono)
            .child(
                div()
                    .w(px(64.))
                    .flex_none()
                    .text_right()
                    .text_color(muted)
                    .whitespace_nowrap()
                    .child(row.line_number),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .when(row.missing, |this| this.text_color(muted.opacity(0.75)))
                    .child(row.text),
            )
            .into_any_element()
    }

    fn render_deferred_merged_preview(
        &mut self,
        language: crate::domain::Language,
        deferred: &super::model::PreviewDeferredViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .p_4()
            .child(
                div()
                    .flex()
                    .w(px(44.))
                    .h(px(44.))
                    .rounded(px(12.))
                    .bg(cx.theme().accent)
                    .text_color(cx.theme().accent_foreground)
                    .items_center()
                    .justify_center()
                    .child(
                        gpui_component::Icon::new(IconName::SquareTerminal).with_size(Size::Medium),
                    ),
            )
            .child(div().font_semibold().child(deferred.title.clone()))
            .child(
                div()
                    .max_w(px(520.))
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(deferred.detail.clone()),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(render_kv(
                        tr(language, "byte_size"),
                        deferred.source_byte_size.to_string(),
                        cx,
                    ))
                    .child(render_kv(
                        tr(language, "load_1mb"),
                        deferred.excerpt_byte_size.to_string(),
                        cx,
                    )),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("deferred-merged-load-1mb")
                            .primary()
                            .disabled(deferred.actions_disabled)
                            .label(tr(language, "load_1mb"))
                            .on_click(cx.listener(Self::load_deferred_excerpt)),
                    )
                    .child(
                        Button::new("deferred-merged-load-all")
                            .outline()
                            .disabled(deferred.actions_disabled)
                            .label(tr(language, "load_all"))
                            .on_click(cx.listener(Self::load_deferred_full)),
                    ),
            )
            .into_any_element()
    }

    fn render_deferred_excerpt_banner(
        &mut self,
        language: crate::domain::Language,
        banner: &super::model::PreviewExcerptBannerViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .p_3()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary.opacity(0.22))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().font_semibold().child(banner.title.clone()))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(banner.detail.clone()),
                            ),
                    )
                    .child(
                        Button::new("deferred-merged-load-all-after-excerpt")
                            .outline()
                            .disabled(banner.actions_disabled)
                            .label(tr(language, "load_all"))
                            .on_click(cx.listener(Self::load_deferred_full)),
                    ),
            )
            .into_any_element()
    }

    fn load_deferred_excerpt(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, workspace_cx| {
            workspace.load_deferred_merged_content_excerpt(workspace_cx);
        });
    }

    fn load_deferred_full(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, workspace_cx| {
            workspace.load_deferred_merged_content_full(workspace_cx);
        });
    }

    pub(super) fn queue_visible_range_sync(
        &mut self,
        visible: std::ops::Range<usize>,
        cx: &mut App,
    ) {
        if let Some(pending) = self.pending_visible_range.as_ref() {
            if range_effectively_contains_range(
                pending,
                &visible,
                PREVIEW_PENDING_RANGE_PADDING_LINES,
            ) {
                return;
            }
            if self.scheduled_visible_sync
                && range_effectively_contains_range(
                    &visible,
                    pending,
                    PREVIEW_PENDING_RANGE_PADDING_LINES,
                )
            {
                self.pending_visible_range = Some(visible);
                return;
            }
        }
        if !self.scheduled_visible_sync
            && self
                .last_synced_visible_range
                .as_ref()
                .is_some_and(|synced| range_contains_range(synced, &visible))
        {
            return;
        }
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
        self.last_synced_visible_range = Some(visible.clone());
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
        let range_changed = self.render_cache_range != visible;
        let revision_changed = self.render_cache_revision != render_revision;
        let overlaps = self.render_cache_range.start < visible.end
            && visible.start < self.render_cache_range.end
            && !self.render_cache.is_empty();

        if range_changed && overlaps {
            // Reuse the overlapping portion, build only the delta edges.
            self.patch_render_cache_range(visible.clone(), cx);
            if revision_changed {
                // Some cached lines may have stale text — refresh them in-place.
                self.patch_render_cache_contents(cx);
            }
            perf::record_preview_render_cache_partial_update();
        } else if range_changed {
            // No overlap — full rebuild is unavoidable.
            self.render_cache = self.preview.read(cx).build_render_lines(visible.clone());
            perf::record_preview_render_cache_rebuild();
        } else {
            // Same range, only revision changed — patch existing entries in-place.
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
            // Move (drain) instead of clone to avoid redundant ref-count bumps.
            next_cache.extend(self.render_cache.drain(start_ix..end_ix));
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
            let loaded = preview.line_at(ix);
            // Only rebuild when underlying text actually changed.
            let text_matches = match (&loaded, line.missing) {
                (Some(text), false) => *text == line.text,
                (None, true) => true,
                _ => false,
            };
            if !text_matches {
                *line = preview.build_render_line(ix);
            }
        }
    }

    pub(super) fn render_lines_for(
        &self,
        visible_range: std::ops::Range<usize>,
        cx: &App,
    ) -> Vec<crate::ui::preview_model::PreviewRenderLine> {
        // Fast path: entire visible range is within the render cache.
        if visible_range.start >= self.render_cache_range.start
            && visible_range.end <= self.render_cache_range.end
            && !self.render_cache.is_empty()
        {
            let offset = visible_range.start - self.render_cache_range.start;
            let len = visible_range.end - visible_range.start;
            return self.render_cache[offset..offset + len].to_vec();
        }

        let preview = self.preview.read(cx);
        visible_range
            .map(|ix| {
                if ix >= self.render_cache_range.start
                    && ix < self.render_cache_range.end
                    && let Some(line) = self.render_cache.get(ix - self.render_cache_range.start)
                {
                    return line.clone();
                }
                preview.build_render_line(ix)
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

fn range_contains_range(
    container: &std::ops::Range<usize>,
    candidate: &std::ops::Range<usize>,
) -> bool {
    container.start <= candidate.start && container.end >= candidate.end
}

fn range_effectively_contains_range(
    container: &std::ops::Range<usize>,
    candidate: &std::ops::Range<usize>,
    padding: usize,
) -> bool {
    if container.is_empty() {
        return false;
    }
    container.start.saturating_sub(padding) <= candidate.start
        && container.end.saturating_add(padding) >= candidate.end
}

impl UniformListDecoration for PreviewVisibleRangeDecoration {
    fn compute(
        &self,
        visible_range: std::ops::Range<usize>,
        _bounds: gpui::Bounds<gpui::Pixels>,
        _scroll_offset: gpui::Point<gpui::Pixels>,
        _item_height: gpui::Pixels,
        _item_count: usize,
        _window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.preview_pane.update(cx, |view, cx| {
            view.queue_visible_range_sync(visible_range, cx);
        });
        Empty.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{bucket_visible_range, range_effectively_contains_range};

    #[test]
    fn effective_range_containment_accepts_small_scroll_adjustments() {
        assert!(range_effectively_contains_range(
            &(100..200),
            &(120..220),
            24
        ));
        assert!(!range_effectively_contains_range(
            &(100..200),
            &(120..260),
            24
        ));
    }

    #[test]
    fn bucket_visible_range_clamps_to_document_bounds() {
        assert_eq!(bucket_visible_range(0..0, 192, 0), 0..0);
        assert_eq!(bucket_visible_range(190..260, 192, 300), 0..300);
    }
}
