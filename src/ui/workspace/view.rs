use gpui::{
    AnyElement, App, Hsla, IntoElement, ParentElement, SharedString, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable, Size, StyledExt as _, WindowExt as _, h_flex,
    notification::NotificationType, scroll::ScrollableElement, v_flex,
};

use super::Workspace;
use crate::domain::{FileEntry, Language, ProcessStatus, TreeNode};
use crate::ui::state::ProcessUiStatus;
use crate::utils::i18n::tr;

pub(super) fn build_tree_items(
    nodes: &[TreeNode],
    filter: &str,
    mode: TreeExpansionMode,
) -> Vec<gpui_component::tree::TreeItem> {
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
) -> Option<gpui_component::tree::TreeItem> {
    let children = node
        .children
        .iter()
        .filter_map(|child| build_tree_item(child, filter, mode, depth + 1))
        .collect::<Vec<_>>();
    let matches = tree_matches_filter(node, filter);
    if !matches && children.is_empty() {
        return None;
    }

    let mut item = gpui_component::tree::TreeItem::new(node.id.clone(), node.label.clone());
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

pub(super) fn summarize_visible_tree(nodes: &[TreeNode], filter: &str) -> TreeSummary {
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

pub(super) fn card(cx: &App) -> gpui::Div {
    div()
        .p_4()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .rounded(cx.theme().radius)
}

pub(super) fn panel_viewport(content: AnyElement, min_height: gpui::Pixels) -> gpui::Div {
    div().size_full().overflow_x_hidden().child(
        div()
            .size_full()
            .min_h(min_height)
            .child(content)
            .overflow_y_scrollbar(),
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

pub(super) fn empty_box(title: &str, hint: &str, icon: IconName, cx: &App) -> gpui::Div {
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

pub(super) fn accent_icon_badge(icon: IconName, fg: Hsla, bg: Hsla) -> gpui::Div {
    div()
        .w(px(24.))
        .h(px(24.))
        .rounded(px(8.))
        .bg(bg)
        .items_center()
        .justify_center()
        .child(Icon::new(icon).text_color(fg).with_size(Size::Small))
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

#[derive(Default, Clone, Copy)]
pub(super) struct TreeSummary {
    pub folders: usize,
    pub files: usize,
}

impl TreeSummary {
    pub fn total(self) -> usize {
        self.folders + self.files
    }

    pub fn merge(&mut self, other: Self) {
        self.folders += other.folders;
        self.files += other.files;
    }
}
