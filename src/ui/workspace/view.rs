use gpui::{
    AnyElement, App, Div, Hsla, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, hsla, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable, Size, StyledExt as _, WindowExt as _, h_flex,
    list::ListItem, notification::NotificationType, scroll::ScrollableElement, tag::Tag,
    tag::TagVariant, v_flex,
};

use super::BlacklistItemKind;
use super::model::{
    BlacklistSectionViewModel, BlacklistTagViewModel, FilterMatchKind, TreeCountSummary,
    TreeRowViewModel,
};
use super::tree_palette::{ResolvedTreeRowPalette, TreeRowPalette};
use crate::domain::{FileEntry, Language, ProcessStatus};
use crate::ui::state::ProcessUiStatus;
use crate::utils::i18n::tr;

#[derive(IntoElement)]
pub(super) struct Card {
    base: Div,
    style: StyleRefinement,
}

impl Card {
    fn new() -> Self {
        Self {
            base: div(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.base.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.base
            .size_full()
            .min_h(px(0.))
            .overflow_hidden()
            .p_4()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .rounded(cx.theme().radius)
            .refine_style(&self.style)
    }
}

#[derive(IntoElement)]
struct SectionTitle {
    base: Div,
    style: StyleRefinement,
    title: SharedString,
    icon: IconName,
}

impl SectionTitle {
    fn new(title: impl Into<SharedString>, icon: IconName) -> Self {
        Self {
            base: div(),
            style: StyleRefinement::default(),
            title: title.into(),
            icon,
        }
    }
}

impl Styled for SectionTitle {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SectionTitle {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .gap_2()
            .items_center()
            .child(accent_icon_badge(
                self.icon,
                cx.theme().primary,
                cx.theme().primary.opacity(0.14),
            ))
            .child(
                self.base
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child(self.title)
                    .refine_style(&self.style),
            )
    }
}

#[derive(IntoElement)]
struct StatTile {
    base: Div,
    style: StyleRefinement,
    label: SharedString,
    value: SharedString,
}

impl StatTile {
    fn new(label: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            base: div(),
            style: StyleRefinement::default(),
            label: label.into(),
            value: value.into(),
        }
    }
}

impl Styled for StatTile {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for StatTile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.base
            .flex_1()
            .p_3()
            .rounded(cx.theme().radius)
            .bg(cx.theme().secondary)
            .border_1()
            .border_color(cx.theme().border)
            .refine_style(&self.style)
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.label),
                    )
                    .child(div().text_lg().font_semibold().child(self.value)),
            )
    }
}

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
    let badges = tree_badges(row, language);
    let palette = TreeRowPalette::new(selected, row.icon_kind, row.is_filter_match, row.match_kind)
        .resolve(cx.theme());
    let row_indent = px((row.depth as f32) * 20.);

    ListItem::new(ix)
        .w_full()
        .h(px(48.))
        .rounded(px(8.))
        .bg(palette.row_bg)
        .child(
            div().w_full().h_full().child(
                h_flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    .gap_3()
                    .px(px(12.))
                    .pl(px(12.) + row_indent)
                    .hover(|style| style.bg(palette.row_hover_bg))
                    .child(render_tree_row_chevron(chevron, &palette))
                    .child(render_tree_row_icon(row, &palette))
                    .child(render_tree_row_body(row, badges, language, &palette)),
            ),
        )
}

fn render_tree_row_chevron(
    chevron: Option<IconName>,
    palette: &ResolvedTreeRowPalette,
) -> gpui::Div {
    div()
        .flex()
        .w(px(14.))
        .items_center()
        .justify_center()
        .text_color(palette.chevron_fg)
        .when_some(chevron, |this, chevron| this.child(chevron))
}

fn render_tree_row_icon(row: &TreeRowViewModel, palette: &ResolvedTreeRowPalette) -> gpui::Div {
    div()
        .flex()
        .w(px(20.))
        .h(px(20.))
        .items_center()
        .justify_center()
        .child(
            row.icon_kind
                .icon()
                .text_color(palette.icon_fg)
                .with_size(Size::Small),
        )
}

fn render_tree_row_body(
    row: &TreeRowViewModel,
    badges: Vec<String>,
    language: Language,
    palette: &ResolvedTreeRowPalette,
) -> gpui::Div {
    v_flex()
        .min_w(px(0.))
        .flex_1()
        .gap_1()
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(div().min_w(px(0.)).flex_1().child(render_match_label(
                    row.label.as_ref(),
                    row.match_kind,
                    row.match_range.as_ref(),
                    palette,
                )))
                .when(!badges.is_empty(), |this| {
                    this.child(render_tree_badges_row(badges, palette))
                }),
        )
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap(px(6.))
                .child(render_secondary_label(row, language, palette)),
        )
}

fn render_tree_badges_row(badges: Vec<String>, palette: &ResolvedTreeRowPalette) -> AnyElement {
    h_flex()
        .gap_1()
        .children(badges.into_iter().map(|badge| {
            div()
                .text_xs()
                .px(px(7.))
                .py(px(2.))
                .rounded(px(6.))
                .bg(palette.badge_bg)
                .text_color(palette.badge_fg)
                .child(badge)
        }))
        .into_any_element()
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

    match (row.preview_chars, row.preview_tokens) {
        (Some(chars), Some(tokens)) => format!(
            "{} {} · {} {}",
            chars,
            tr(language, "chars"),
            tokens,
            tr(language, "tokens")
        ),
        _ => String::new(),
    }
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

fn tree_badges(row: &TreeRowViewModel, language: Language) -> Vec<String> {
    let mut badges = Vec::new();
    if row.archive.is_some() {
        badges.push(tr(language, "archive_badge").to_string());
    }
    if let Some(filter_badge) = tree_filter_badge(row, language) {
        badges.push(filter_badge);
    }
    badges
}

fn render_match_label(
    text: &str,
    match_kind: Option<FilterMatchKind>,
    match_range: Option<&std::ops::Range<usize>>,
    palette: &ResolvedTreeRowPalette,
) -> AnyElement {
    if !matches!(match_kind, Some(FilterMatchKind::Label)) {
        return div()
            .text_sm()
            .font_medium()
            .text_color(palette.label_fg)
            .truncate()
            .whitespace_nowrap()
            .child(text.to_string())
            .into_any_element();
    }

    let Some(range) = match_range else {
        return div()
            .text_sm()
            .font_medium()
            .text_color(palette.label_fg)
            .truncate()
            .whitespace_nowrap()
            .child(text.to_string())
            .into_any_element();
    };
    if range.start >= text.len() || range.end > text.len() || range.start >= range.end {
        return div()
            .text_sm()
            .font_medium()
            .text_color(palette.label_fg)
            .truncate()
            .whitespace_nowrap()
            .child(text.to_string())
            .into_any_element();
    }

    render_inline_match(text, range, palette, false)
}

fn render_secondary_label(
    row: &TreeRowViewModel,
    language: Language,
    palette: &ResolvedTreeRowPalette,
) -> AnyElement {
    if matches!(row.match_kind, Some(FilterMatchKind::Path))
        && let Some(range) = row.match_range.as_ref()
    {
        return render_inline_match(row.relative_path.as_ref(), range, palette, true);
    }

    div()
        .min_w(px(0.))
        .text_xs()
        .text_color(palette.secondary_fg)
        .truncate()
        .child(SharedString::from(tree_secondary_label(row, language)))
        .into_any_element()
}

fn render_inline_match(
    text: &str,
    range: &std::ops::Range<usize>,
    palette: &ResolvedTreeRowPalette,
    secondary: bool,
) -> AnyElement {
    if range.start >= text.len() || range.end > text.len() || range.start >= range.end {
        return if secondary {
            div()
                .min_w(px(0.))
                .text_xs()
                .text_color(palette.secondary_fg)
                .truncate()
                .child(text.to_string())
                .into_any_element()
        } else {
            div()
                .text_sm()
                .font_medium()
                .text_color(palette.label_fg)
                .truncate()
                .whitespace_nowrap()
                .child(text.to_string())
                .into_any_element()
        };
    }

    let prefix = &text[..range.start];
    let matched = &text[range.start..range.end];
    let suffix = &text[range.end..];

    let base_color = if secondary {
        palette.secondary_fg
    } else {
        palette.label_fg
    };

    div()
        .min_w(px(0.))
        .overflow_hidden()
        .child(
            h_flex()
                .items_center()
                .child(if secondary {
                    div()
                        .min_w(px(0.))
                        .text_xs()
                        .text_color(base_color)
                        .whitespace_nowrap()
                        .child(prefix.to_string())
                } else {
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(base_color)
                        .whitespace_nowrap()
                        .child(prefix.to_string())
                })
                .child(if secondary {
                    div()
                        .min_w(px(0.))
                        .text_xs()
                        .text_color(palette.match_fg)
                        .whitespace_nowrap()
                        .px(px(1.))
                        .rounded(px(4.))
                        .bg(palette.match_bg)
                        .text_decoration_1()
                        .text_decoration_color(palette.match_fg)
                        .child(matched.to_string())
                } else {
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(palette.match_fg)
                        .whitespace_nowrap()
                        .px(px(1.))
                        .rounded(px(4.))
                        .bg(palette.match_bg)
                        .text_decoration_1()
                        .text_decoration_color(palette.match_fg)
                        .child(matched.to_string())
                })
                .child(if secondary {
                    div()
                        .min_w(px(0.))
                        .text_xs()
                        .text_color(base_color)
                        .whitespace_nowrap()
                        .truncate()
                        .child(suffix.to_string())
                } else {
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(base_color)
                        .whitespace_nowrap()
                        .truncate()
                        .child(suffix.to_string())
                }),
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

pub(super) fn card(cx: &App) -> Card {
    let _ = cx;
    Card::new()
}

pub(super) fn flow_card(cx: &App) -> Div {
    div()
        .w_full()
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
    let _ = cx;
    SectionTitle::new(title.to_string(), icon).into_any_element()
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
    let _ = cx;
    StatTile::new(label.to_string(), value).into_any_element()
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
                .child(render_tag_flow(tags)),
        )
        .into_any_element()
}

pub(super) fn render_tag_flow(tags: Vec<AnyElement>) -> gpui::Div {
    div().flex().flex_wrap().gap_2().children(tags)
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

pub(super) fn format_size(size: u64) -> String {
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

#[cfg(test)]
mod tests {
    use super::super::model::process_status_message;
    use crate::domain::Language;
    use crate::ui::state::{ProcessState, ProcessUiStatus};
    use crate::utils::i18n::tr;

    #[test]
    fn completed_status_message_appends_merged_file_size_hint() {
        let process = ProcessState {
            ui_status: ProcessUiStatus::Completed,
            ..ProcessState::default()
        };

        let message = process_status_message(&process, Language::En, Some("1.2 MB".to_string()));
        let expected = format!("{} (1.2 MB)", tr(Language::En, "status_completed_hint"));

        assert_eq!(message, expected);
    }

    #[test]
    fn completed_status_message_omits_hint_when_missing() {
        let process = ProcessState {
            ui_status: ProcessUiStatus::Completed,
            ..ProcessState::default()
        };

        let message = process_status_message(&process, Language::En, None);

        assert_eq!(message, tr(Language::En, "status_completed_hint"));
    }
}

#[derive(Clone, Copy)]
pub(super) enum TreeExpansionMode {
    Default,
    ExpandAll,
    CollapseAll,
}
