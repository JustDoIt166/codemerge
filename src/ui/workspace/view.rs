use gpui::{
    AnyElement, App, Hsla, IntoElement, ParentElement, SharedString, Styled, Window, div, hsla,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable, Size, StyledExt as _, WindowExt as _, h_flex,
    list::ListItem, notification::NotificationType, scroll::ScrollableElement, tag::Tag,
    tag::TagVariant, v_flex,
};

use super::BlacklistItemKind;
use super::Workspace;
use super::model::{
    BlacklistSectionViewModel, BlacklistTagViewModel, FilterMatchKind, TreeCountSummary,
    TreeRowViewModel,
};
use super::tree_palette::{ResolvedTreeRowPalette, TreeRowPalette};
use crate::domain::{FileEntry, Language, ProcessStatus};
use crate::ui::state::ProcessUiStatus;
use crate::utils::i18n::tr;

pub(super) fn render_tree_row(
    ix: usize,
    row: &TreeRowViewModel,
    selected: bool,
    language: Language,
    cx: &App,
) -> ListItem {
    let chevron = row.is_folder.then_some(if row.is_expanded {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    });
    let badge = tree_filter_badge(row, language);
    let palette = TreeRowPalette::new(selected, row.icon_kind, row.is_filter_match, row.match_kind)
        .resolve(cx.theme());

    ListItem::new(ix)
        .w_full()
        .h(px(42.))
        .rounded(px(10.))
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .children(render_tree_guides(row, &palette))
                .child(
                    div()
                        .flex()
                        .w(px(14.))
                        .items_center()
                        .justify_center()
                        .text_color(palette.chevron_fg)
                        .when_some(chevron, |this, chevron| this.child(chevron)),
                )
                .child(
                    div()
                        .w(px(2.))
                        .h(px(18.))
                        .rounded(px(999.))
                        .bg(palette.selection_bar_bg),
                )
                .child(
                    div()
                        .flex()
                        .w(px(26.))
                        .h(px(26.))
                        .rounded(px(8.))
                        .items_center()
                        .justify_center()
                        .bg(palette.icon_bg)
                        .child(
                            row.icon_kind
                                .icon()
                                .text_color(palette.icon_fg)
                                .with_size(Size::Small),
                        ),
                )
                .child(
                    v_flex()
                        .min_w(px(0.))
                        .flex_1()
                        .gap_1()
                        .child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(render_match_label(
                                    row.label.as_ref(),
                                    row.match_kind,
                                    row.match_range.as_ref(),
                                    &palette,
                                ))
                                .when_some(badge, |this, badge| {
                                    this.child(
                                        div()
                                            .text_xs()
                                            .px_2()
                                            .py(px(1.))
                                            .rounded(px(999.))
                                            .bg(palette.badge_bg)
                                            .text_color(palette.badge_fg)
                                            .child(badge),
                                    )
                                }),
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .min_w(px(0.))
                                        .text_xs()
                                        .text_color(palette.secondary_fg)
                                        .truncate()
                                        .child(
                                            if matches!(row.match_kind, Some(FilterMatchKind::Path))
                                            {
                                                row.relative_path.clone()
                                            } else {
                                                SharedString::from(tree_secondary_label(
                                                    row, language,
                                                ))
                                            },
                                        ),
                                )
                                .when(!row.is_folder, |this| {
                                    this.child(
                                        div()
                                            .text_xs()
                                            .px_2()
                                            .py(px(1.))
                                            .rounded(px(999.))
                                            .bg(palette.extension_bg)
                                            .text_color(palette.extension_fg)
                                            .child(
                                                row.extension
                                                    .clone()
                                                    .unwrap_or_else(|| row.label.clone()),
                                            ),
                                    )
                                }),
                        ),
                ),
        )
}

fn render_tree_guides(row: &TreeRowViewModel, palette: &ResolvedTreeRowPalette) -> Vec<AnyElement> {
    let last_ix = row.guide_continuations.len().saturating_sub(1);
    row.guide_continuations
        .iter()
        .enumerate()
        .map(|(depth, continues)| {
            if depth == last_ix {
                render_branch_guide(depth, *continues, palette)
            } else {
                render_ancestor_guide(depth, *continues, palette)
            }
        })
        .collect()
}

fn render_ancestor_guide(
    depth: usize,
    continues: bool,
    palette: &ResolvedTreeRowPalette,
) -> AnyElement {
    let color = palette.guide_color(depth);
    div()
        .flex()
        .w(px(10.))
        .h(px(28.))
        .items_center()
        .justify_center()
        .child(if continues {
            render_vertical_guide_line(px(28.), color)
        } else {
            div().w(px(1.)).h(px(28.)).into_any_element()
        })
        .into_any_element()
}

fn render_branch_guide(
    depth: usize,
    continues: bool,
    palette: &ResolvedTreeRowPalette,
) -> AnyElement {
    let color = palette.guide_color(depth);
    div()
        .flex()
        .w(px(10.))
        .h(px(28.))
        .items_center()
        .justify_center()
        .child(
            v_flex()
                .w(px(10.))
                .h_full()
                .items_center()
                .justify_center()
                .child(render_vertical_guide_line(px(13.), color))
                .child(
                    h_flex()
                        .w(px(10.))
                        .h(px(1.))
                        .items_center()
                        .child(div().w(px(4.)).h(px(1.)))
                        .child(render_horizontal_guide_line(px(6.), color)),
                )
                .child(if continues {
                    render_vertical_guide_line(px(14.), color)
                } else {
                    div().w(px(1.)).h(px(14.)).into_any_element()
                }),
        )
        .into_any_element()
}

fn render_vertical_guide_line(length: gpui::Pixels, color: Hsla) -> AnyElement {
    div().w(px(1.)).h(length).bg(color).into_any_element()
}

fn render_horizontal_guide_line(length: gpui::Pixels, color: Hsla) -> AnyElement {
    div().w(length).h(px(1.)).bg(color).into_any_element()
}

fn tree_secondary_label(row: &TreeRowViewModel, language: Language) -> String {
    if row.is_folder {
        return format!(
            "{} {} · {} {}",
            row.child_folder_count,
            tr(language, "folders"),
            row.child_file_count,
            tr(language, "files")
        );
    }

    row.extension
        .as_ref()
        .map(|ext| ext.to_string())
        .unwrap_or_else(|| row.relative_path.to_string())
}

fn tree_filter_badge(row: &TreeRowViewModel, language: Language) -> Option<String> {
    if !row.is_filter_match && row.matched_descendants == 0 {
        return None;
    }

    if row.is_filter_match {
        return Some(match row.match_kind {
            Some(FilterMatchKind::Path) => tr(language, "tree_path_match").to_string(),
            _ => tr(language, "tree_match").to_string(),
        });
    }

    Some(format!(
        "{} {}",
        row.matched_descendants,
        tr(language, "tree_hits")
    ))
}

fn render_match_label(
    text: &str,
    match_kind: Option<FilterMatchKind>,
    match_range: Option<&std::ops::Range<usize>>,
    palette: &ResolvedTreeRowPalette,
) -> AnyElement {
    let base = div()
        .font_semibold()
        .text_color(palette.label_fg)
        .truncate()
        .whitespace_nowrap();

    if !matches!(match_kind, Some(FilterMatchKind::Label)) {
        return base.child(text.to_string()).into_any_element();
    }

    let Some(range) = match_range else {
        return base.child(text.to_string()).into_any_element();
    };
    if range.start >= text.len() || range.end > text.len() || range.start >= range.end {
        return base.child(text.to_string()).into_any_element();
    }

    let prefix = &text[..range.start];
    let matched = &text[range.start..range.end];
    let suffix = &text[range.end..];

    h_flex()
        .gap_1()
        .items_center()
        .child(
            div()
                .font_semibold()
                .text_color(palette.label_fg)
                .truncate()
                .child(prefix.to_string()),
        )
        .child(
            div()
                .font_semibold()
                .px_1()
                .rounded(px(4.))
                .bg(palette.match_bg)
                .text_color(palette.match_fg)
                .child(matched.to_string()),
        )
        .child(
            div()
                .font_semibold()
                .text_color(palette.label_fg)
                .truncate()
                .child(suffix.to_string()),
        )
        .into_any_element()
}

pub(super) fn format_tree_summary(
    visible: TreeCountSummary,
    total: TreeCountSummary,
    language: Language,
) -> String {
    if total.total() == 0 {
        return format!(
            "0 {} · 0 {}",
            tr(language, "folders"),
            tr(language, "files")
        );
    }
    if visible == total {
        return format!(
            "{} {} · {} {}",
            total.folders,
            tr(language, "folders"),
            total.files,
            tr(language, "files")
        );
    }
    format!(
        "{} / {} {} · {} / {} {}",
        visible.folders,
        total.folders,
        tr(language, "folders"),
        visible.files,
        total.files,
        tr(language, "files")
    )
}

pub(super) fn card(cx: &App) -> gpui::Div {
    div()
        .size_full()
        .min_h(px(0.))
        .overflow_hidden()
        .p_4()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .rounded(cx.theme().radius)
}

pub(super) fn panel_viewport(content: AnyElement, min_height: gpui::Pixels) -> gpui::Div {
    div().size_full().min_h(px(0.)).overflow_hidden().child(
        div()
            .size_full()
            .min_h(min_height)
            .overflow_x_hidden()
            .overflow_y_scrollbar()
            .child(content),
    )
}

pub(super) fn panel_frame(content: AnyElement, min_height: gpui::Pixels) -> gpui::Div {
    div().size_full().min_h(px(0.)).overflow_hidden().child(
        div()
            .size_full()
            .min_h(min_height)
            .overflow_hidden()
            .child(content),
    )
}

pub(super) fn section_title(title: &str, icon: IconName, cx: &App) -> AnyElement {
    h_flex()
        .gap_2()
        .items_center()
        .child(accent_icon_badge(
            icon,
            cx.theme().primary,
            cx.theme().primary.opacity(0.14),
        ))
        .child(
            div()
                .font_semibold()
                .text_color(cx.theme().foreground)
                .child(title.to_string()),
        )
        .into_any_element()
}

pub(super) fn section_caption(title: &str, icon: IconName, cx: &App) -> AnyElement {
    h_flex()
        .gap_2()
        .items_center()
        .child(
            Icon::new(icon)
                .text_color(cx.theme().primary)
                .with_size(Size::Small),
        )
        .child(
            div()
                .text_sm()
                .font_semibold()
                .text_color(cx.theme().foreground)
                .child(title.to_string()),
        )
        .into_any_element()
}

pub(super) fn render_info_block(
    label: &str,
    value: String,
    emphasized: bool,
    icon: IconName,
    cx: &App,
) -> AnyElement {
    h_flex()
        .gap_3()
        .items_start()
        .p_3()
        .rounded(px(12.))
        .border_1()
        .border_color(cx.theme().border)
        .bg(if emphasized {
            cx.theme().secondary.opacity(0.25)
        } else {
            cx.theme().background
        })
        .child(div().mt(px(2.)).child(accent_icon_badge(
            icon,
            if emphasized {
                cx.theme().primary
            } else {
                cx.theme().accent
            },
            if emphasized {
                cx.theme().primary.opacity(0.16)
            } else {
                cx.theme().accent.opacity(0.12)
            },
        )))
        .child(
            v_flex()
                .gap_1()
                .min_w(px(0.))
                .flex_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(label.to_string()),
                )
                .child(div().text_sm().child(value)),
        )
        .into_any_element()
}

pub(super) fn render_kv(label: &str, value: String, cx: &App) -> AnyElement {
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

pub(super) fn stat_tile(label: &str, value: String, cx: &App) -> AnyElement {
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

pub(super) fn status_banner(
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

pub(super) fn empty_box(
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    icon: IconName,
    cx: &App,
) -> gpui::Div {
    let title = title.into();
    let hint = hint.into();
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
                .flex()
                .w(px(40.))
                .h(px(40.))
                .rounded(px(12.))
                .bg(cx.theme().accent)
                .text_color(cx.theme().accent_foreground)
                .items_center()
                .justify_center()
                .child(Icon::new(icon).with_size(Size::Medium)),
        )
        .child(div().font_semibold().child(title))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(hint),
        )
}

pub(super) fn render_blacklist_section(
    section: &BlacklistSectionViewModel,
    tags: Vec<AnyElement>,
    cx: &App,
) -> AnyElement {
    let (icon, fg, bg) = blacklist_palette(section.kind, cx);

    div()
        .w_full()
        .p_3()
        .rounded(px(12.))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary.opacity(0.18))
        .child(
            v_flex()
                .gap_3()
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .gap_2()
                        .child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(accent_icon_badge(icon, fg, bg))
                                .child(
                                    div()
                                        .font_semibold()
                                        .text_color(cx.theme().foreground)
                                        .child(section.title.clone()),
                                ),
                        )
                        .child(pill_label(&section.count.to_string(), cx)),
                )
                .child(div().flex().flex_wrap().gap_2().children(tags)),
        )
        .into_any_element()
}

pub(super) fn render_blacklist_tag(
    item: &BlacklistTagViewModel,
    action: AnyElement,
    cx: &App,
) -> AnyElement {
    let (icon, fg, bg) = blacklist_palette(item.kind, cx);
    let border = fg.opacity(0.38);
    let text_fg = cx.theme().foreground;

    h_flex()
        .items_center()
        .gap_1()
        .child(
            Tag::new()
                .with_variant(TagVariant::Custom {
                    color: bg,
                    foreground: text_fg,
                    border,
                })
                .with_size(Size::Small)
                .rounded_full()
                .child(Icon::new(icon).text_color(fg).with_size(Size::Small))
                .child(
                    div()
                        .max_w(px(220.))
                        .truncate()
                        .child(item.display_label.clone()),
                ),
        )
        .child(action)
        .into_any_element()
}

pub(super) fn pill_label(label: &str, cx: &App) -> AnyElement {
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

pub(super) fn selected_file_row(entry: &FileEntry, cx: &App) -> AnyElement {
    v_flex()
        .gap_1()
        .px_3()
        .py_2()
        .h(px(52.))
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .justify_between()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .min_w(px(0.))
                        .child(accent_icon_badge(
                            IconName::File,
                            cx.theme().accent,
                            cx.theme().accent.opacity(0.14),
                        ))
                        .child(div().font_semibold().truncate().child(entry.name.clone())),
                )
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

pub(super) fn activity_row(record: &crate::domain::ProcessRecord, cx: &App) -> AnyElement {
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
                        .flex()
                        .w(px(20.))
                        .h(px(20.))
                        .rounded(px(999.))
                        .bg(accent.opacity(0.15))
                        .items_center()
                        .justify_center()
                        .child(Icon::new(icon).text_color(accent).with_size(Size::Small)),
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

pub(super) fn accent_icon_badge(icon: IconName, fg: Hsla, bg: Hsla) -> gpui::Div {
    div()
        .flex()
        .w(px(24.))
        .h(px(24.))
        .rounded(px(8.))
        .bg(bg)
        .items_center()
        .justify_center()
        .child(Icon::new(icon).text_color(fg).with_size(Size::Small))
}

fn blacklist_palette(kind: BlacklistItemKind, cx: &App) -> (IconName, Hsla, Hsla) {
    match kind {
        BlacklistItemKind::Folder => (
            IconName::Folder,
            cx.theme().warning,
            cx.theme().warning.opacity(0.24),
        ),
        BlacklistItemKind::Ext => {
            let fg = if cx.theme().is_dark() {
                hsla(0.58, 0.90, 0.78, 1.0)
            } else {
                hsla(0.58, 0.70, 0.42, 1.0)
            };
            let bg = if cx.theme().is_dark() {
                hsla(0.58, 0.75, 0.28, 0.55)
            } else {
                hsla(0.58, 0.95, 0.90, 1.0)
            };
            (IconName::File, fg, bg)
        }
    }
}

pub(super) fn tab_icon_badge(icon: IconName, warm: bool, cx: &App) -> gpui::Div {
    let fg = if warm {
        cx.theme().warning
    } else {
        cx.theme().primary
    };
    let bg = if warm {
        cx.theme().warning.opacity(0.16)
    } else {
        cx.theme().primary.opacity(0.14)
    };
    accent_icon_badge(icon, fg, bg)
}

pub(super) fn process_status_title(status: ProcessUiStatus, language: Language) -> &'static str {
    match status {
        ProcessUiStatus::Idle => tr(language, "status_idle"),
        ProcessUiStatus::Preflight => tr(language, "status_preflight"),
        ProcessUiStatus::Running => tr(language, "status_running"),
        ProcessUiStatus::Completed => tr(language, "status_completed"),
        ProcessUiStatus::Cancelled => tr(language, "status_cancelled"),
        ProcessUiStatus::Error => tr(language, "status_error"),
    }
}

pub(super) fn process_status_message(workspace: &Workspace, language: Language) -> String {
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

pub(super) fn copy_to_clipboard(
    content: &str,
    language: Language,
    window: &mut Window,
    cx: &mut App,
) {
    match arboard::Clipboard::new().and_then(|mut clip| clip.set_text(content.to_string())) {
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

pub(super) fn format_duration(duration: std::time::Duration) -> String {
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

#[derive(Clone, Copy)]
pub(super) enum TreeExpansionMode {
    Default,
    ExpandAll,
    CollapseAll,
}
