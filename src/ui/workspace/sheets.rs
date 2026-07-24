use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, Context, InteractiveElement as _, IntoElement as _,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    tab::Tab,
    tab::TabBar,
    v_flex, v_virtual_list,
};

use super::{Workspace, design_tokens};
use crate::domain::ProcessStatus;
use crate::ui::state::{ActivityFilter, WorkspaceRightTab, WorkspaceSheet};
use crate::utils::i18n::tr;

impl Workspace {
    pub(super) fn open_rules_sheet(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Navigation(
                crate::application::store::NavigationAction::SetNarrowInputSheetOpen(false),
            ),
            cx,
        );
        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Navigation(
                crate::application::store::NavigationAction::SetActiveSheet(None),
            ),
            cx,
        );
        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Navigation(
                crate::application::store::NavigationAction::SetRightTab(WorkspaceRightTab::Rules),
            ),
            cx,
        );
        self.focus_handle.focus(window);
    }

    pub(super) fn close_workspace_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let ui_state = self.ui_state(cx);
        if let Some(sheet) = ui_state.active_sheet {
            self.handle_workspace_sheet_closed(sheet, cx);
            self.focus_handle.focus(window);
            return true;
        }
        if ui_state.narrow_input_sheet_open {
            self.handle_input_sheet_closed(cx);
            self.focus_handle.focus(window);
            return true;
        }
        false
    }

    pub(super) fn render_workspace_sheet_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let language = self.language(cx);
        let ui_state = self.ui_state(cx);
        let (title, width, body, surface_id) = match ui_state.active_sheet {
            Some(WorkspaceSheet::Rules) => (
                tr(language, "panel_rules"),
                design_tokens::RULES_SHEET_WIDTH,
                self.rules_panel_view.clone().into_any_element(),
                "rules-sheet",
            ),
            Some(WorkspaceSheet::Activity) => (
                tr(language, "activity_details"),
                design_tokens::ACTIVITY_SHEET_WIDTH,
                self.activity_panel_view.clone().into_any_element(),
                "activity-sheet",
            ),
            None if ui_state.narrow_input_sheet_open => (
                tr(language, "panel_inputs"),
                design_tokens::INPUT_SIDEBAR_DEFAULT,
                self.input_panel_view.clone().into_any_element(),
                "input-sheet",
            ),
            None => return None,
        };

        Some(
            div()
                .id("workspace-sheet-overlay")
                .debug_selector(|| "workspace-sheet-overlay".to_string())
                .absolute()
                .inset_0()
                .bg(cx.theme().background.opacity(0.55))
                .on_click(cx.listener(Self::dismiss_workspace_sheet))
                .child(
                    v_flex()
                        .id(surface_id)
                        .debug_selector(move || surface_id.to_string())
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .w(width)
                        .bg(cx.theme().background)
                        .border_l_1()
                        .border_color(cx.theme().border)
                        .shadow_xl()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            h_flex()
                                .h(design_tokens::RESULTS_TOOLBAR_HEIGHT)
                                .flex_none()
                                .justify_between()
                                .px_4()
                                .border_b_1()
                                .border_color(cx.theme().border)
                                .child(div().font_semibold().child(title))
                                .child(
                                    Button::new("close-workspace-sheet")
                                        .ghost()
                                        .compact()
                                        .icon(IconName::Close)
                                        .on_click(cx.listener(Self::dismiss_workspace_sheet)),
                                ),
                        )
                        .child(div().flex_1().min_h(px(0.)).overflow_hidden().child(body)),
                )
                .into_any_element(),
        )
    }

    fn dismiss_workspace_sheet(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.close_workspace_surface(window, cx);
    }

    pub(super) fn handle_input_sheet_closed(&mut self, cx: &mut Context<Self>) {
        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Navigation(
                crate::application::store::NavigationAction::SetNarrowInputSheetOpen(false),
            ),
            cx,
        );
    }

    pub(super) fn handle_workspace_sheet_closed(
        &mut self,
        sheet: WorkspaceSheet,
        cx: &mut Context<Self>,
    ) {
        if self.ui_state(cx).active_sheet == Some(sheet) {
            let _ = self.dispatch(
                crate::application::store::WorkspaceAction::Navigation(
                    crate::application::store::NavigationAction::SetActiveSheet(None),
                ),
                cx,
            );
        }
    }

    pub(super) fn render_activity_sheet_panel(
        &mut self,
        scroll_handle: gpui_component::VirtualListScrollHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let language = self.language(cx);
        let ui_state = self.ui_state(cx);
        self.refresh_activity_panel_cache(cx);
        let records = self.activity_panel_cache.records.clone();
        let row_sizes = self.activity_panel_cache.row_sizes.clone();
        let failed_diagnostics = self.activity_panel_cache.failed_diagnostics.clone();
        let selected_filter = match ui_state.activity_filter {
            ActivityFilter::All => 0,
            ActivityFilter::Failed => 1,
            ActivityFilter::Skipped => 2,
        };
        let expanded = ui_state.expanded_activity_record;

        v_flex()
            .size_full()
            .min_h(px(0.))
            .gap_3()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .id("activity-filter-tabs")
                            .debug_selector(|| "activity-filter-tabs".to_string())
                            .child(
                                TabBar::new("activity-filters")
                                    .selected_index(selected_filter)
                                    .on_click(cx.listener(Self::set_activity_filter))
                                    .child(Tab::new().label(tr(language, "filter_all")))
                                    .child(Tab::new().label(tr(language, "failed")))
                                    .child(Tab::new().label(tr(language, "skipped"))),
                            ),
                    )
                    .child(
                        div()
                            .id("copy-all-failures-slot")
                            .debug_selector(|| "copy-all-failures-slot".to_string())
                            .child(
                                Button::new("copy-all-failures")
                                    .outline()
                                    .compact()
                                    .icon(IconName::Copy)
                                    .label(tr(language, "copy_all_failures"))
                                    .disabled(failed_diagnostics.is_empty())
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        super::view::copy_to_clipboard(
                                            failed_diagnostics.as_ref(),
                                            language,
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    ),
            )
            .child(if records.is_empty() {
                super::view::empty_box(
                    tr(language, "activity_empty"),
                    tr(language, "activity_empty_hint"),
                    IconName::Inbox,
                    cx,
                )
                .into_any_element()
            } else {
                div()
                    .id("activity-list-viewport")
                    .debug_selector(|| "activity-list-viewport".to_string())
                    .flex_1()
                    .min_h(px(0.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(design_tokens::RADIUS_PANEL)
                    .child(
                        v_virtual_list(
                            cx.entity().clone(),
                            "activity-sheet-rows",
                            row_sizes,
                            move |_, visible, _, cx| {
                                visible
                                    .filter_map(|row_ix| records.get(row_ix))
                                    .map(|(record_ix, record)| {
                                        let record_ix = *record_ix;
                                        let is_expanded = expanded == Some(record_ix);
                                        let error = record.error.clone();
                                        v_flex()
                                            .id(("activity-record", record_ix))
                                            .debug_selector(move || {
                                                format!("activity-record-{record_ix}")
                                            })
                                            .border_b_1()
                                            .border_color(cx.theme().border)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.toggle_activity_record(record_ix, cx);
                                            }))
                                            .child(super::view::activity_row(record, cx))
                                            .when(is_expanded, |row| {
                                                row.child(
                                                    h_flex()
                                                        .px_3()
                                                        .pb_2()
                                                        .gap_2()
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .text_xs()
                                                                .text_color(cx.theme().danger)
                                                                .child(
                                                                    error.clone().unwrap_or_else(
                                                                        || {
                                                                            tr(
                                                                                language,
                                                                                "no_diagnostic",
                                                                            )
                                                                            .to_string()
                                                                        },
                                                                    ),
                                                                ),
                                                        )
                                                        .when_some(error, |row, error| {
                                                            row.child(
                                                            Button::new((
                                                                "copy-diagnostic",
                                                                record_ix,
                                                            ))
                                                            .outline()
                                                            .compact()
                                                            .icon(IconName::Copy)
                                                            .on_click(cx.listener(
                                                                move |_, _, window, cx| {
                                                                    super::view::copy_to_clipboard(
                                                                        &error, language, window,
                                                                        cx,
                                                                    );
                                                                },
                                                            )),
                                                        )
                                                        }),
                                                )
                                            })
                                            .into_any_element()
                                    })
                                    .collect::<Vec<_>>()
                            },
                        )
                        .track_scroll(&scroll_handle),
                    )
                    .into_any_element()
            })
            .into_any_element()
    }

    fn refresh_activity_panel_cache(&mut self, cx: &App) {
        let revisions = self.store.read(cx).revisions();
        let ui_state = self.ui_state(cx);
        if self.activity_panel_cache.execution_revision == Some(revisions.execution)
            && self.activity_panel_cache.filter == ui_state.activity_filter
            && self.activity_panel_cache.expanded == ui_state.expanded_activity_record
        {
            return;
        }

        let process_records = self
            .store
            .read(cx)
            .process()
            .processing_records
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let failed_diagnostics = process_records
            .iter()
            .filter(|record| record.status == ProcessStatus::Failed)
            .filter_map(|record| {
                record
                    .error
                    .as_ref()
                    .map(|error| format!("{}: {error}", record.file_name))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let records = process_records
            .into_iter()
            .enumerate()
            .filter(|(_, record)| activity_record_matches(ui_state.activity_filter, record.status))
            .collect::<Vec<_>>();
        let row_sizes = records
            .iter()
            .map(|(ix, _)| {
                gpui::size(
                    px(100.),
                    if ui_state.expanded_activity_record == Some(*ix) {
                        px(88.)
                    } else {
                        px(44.)
                    },
                )
            })
            .collect::<Vec<_>>();
        self.activity_panel_cache = super::ActivityPanelCache {
            execution_revision: Some(revisions.execution),
            filter: ui_state.activity_filter,
            expanded: ui_state.expanded_activity_record,
            records: Rc::new(records),
            row_sizes: Rc::new(row_sizes),
            failed_diagnostics: Rc::from(failed_diagnostics),
        };
    }

    fn set_activity_filter(&mut self, index: &usize, _: &mut Window, cx: &mut Context<Self>) {
        let filter = match index {
            1 => ActivityFilter::Failed,
            2 => ActivityFilter::Skipped,
            _ => ActivityFilter::All,
        };
        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Navigation(
                crate::application::store::NavigationAction::SetActivityFilter(filter),
            ),
            cx,
        );
    }

    fn toggle_activity_record(&mut self, record: usize, cx: &mut Context<Self>) {
        let expanded = self.ui_state(cx).expanded_activity_record;
        let next = if expanded == Some(record) {
            None
        } else {
            Some(record)
        };
        let _ = self.dispatch(
            crate::application::store::WorkspaceAction::Navigation(
                crate::application::store::NavigationAction::SetExpandedActivityRecord(next),
            ),
            cx,
        );
    }
}

fn activity_record_matches(filter: ActivityFilter, status: ProcessStatus) -> bool {
    match filter {
        ActivityFilter::All => true,
        ActivityFilter::Failed => status == ProcessStatus::Failed,
        ActivityFilter::Skipped => status == ProcessStatus::Skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::activity_record_matches;
    use crate::domain::ProcessStatus;
    use crate::ui::state::ActivityFilter;

    #[test]
    fn activity_filters_match_only_the_requested_status() {
        assert!(activity_record_matches(
            ActivityFilter::All,
            ProcessStatus::Success
        ));
        assert!(activity_record_matches(
            ActivityFilter::Failed,
            ProcessStatus::Failed
        ));
        assert!(!activity_record_matches(
            ActivityFilter::Failed,
            ProcessStatus::Skipped
        ));
        assert!(activity_record_matches(
            ActivityFilter::Skipped,
            ProcessStatus::Skipped
        ));
    }
}
