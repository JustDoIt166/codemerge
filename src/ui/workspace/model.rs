use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

use gpui::SharedString;
use gpui_component::{Icon, IconName, tree::TreeItem};

use super::BlacklistItemKind;
use crate::domain::{Language, PreviewFileEntry, PreviewRowViewModel, ProcessResult};
use crate::services::preflight::PreflightEvent;
use crate::services::tree::{IndexedTreeNode, TreeIndex};
use crate::ui::state::{ProcessState, ProcessUiStatus, TreePanelState};
use crate::utils::i18n::tr;

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TreeCountSummary {
    pub folders: usize,
    pub files: usize,
}

impl TreeCountSummary {
    pub fn total(self) -> usize {
        self.folders + self.files
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TreeIconKind {
    FolderClosed,
    FolderOpen,
    Code,
    Document,
    Config,
    Data,
    Media,
    Text,
}

impl TreeIconKind {
    pub fn icon(self) -> Icon {
        match self {
            Self::FolderClosed => Icon::new(IconName::Folder),
            Self::FolderOpen => Icon::new(IconName::FolderOpen),
            Self::Code => Icon::new(IconName::SquareTerminal),
            Self::Document => Icon::new(IconName::BookOpen),
            Self::Config => Icon::new(IconName::Settings2),
            Self::Data => Icon::new(IconName::LayoutDashboard),
            Self::Media => Icon::new(IconName::GalleryVerticalEnd),
            Self::Text => Icon::new(IconName::File),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FilterMatchKind {
    Label,
    Path,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TreeRowViewModel {
    pub node_id: SharedString,
    pub label: SharedString,
    pub relative_path: SharedString,
    pub is_folder: bool,
    pub depth: usize,
    pub extension: Option<SharedString>,
    pub preview_file_id: Option<u32>,
    pub child_file_count: usize,
    pub child_folder_count: usize,
    pub icon_kind: TreeIconKind,
    pub is_expanded: bool,
    pub is_filter_match: bool,
    pub match_range: Option<Range<usize>>,
    pub match_kind: Option<FilterMatchKind>,
    pub matched_descendants: usize,
    pub guide_continuations: Vec<bool>,
}

#[derive(Default)]
pub(super) struct TreeRenderState {
    pub items: Vec<TreeItem>,
    pub rows: Vec<TreeRowViewModel>,
    pub rows_by_id: HashMap<String, TreeRowViewModel>,
    pub visible_summary: TreeCountSummary,
    pub total_summary: TreeCountSummary,
    pub selected_row_ix: Option<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct TreePanelData {
    pub index: TreeIndex,
    pub preview_file_ids: HashMap<String, u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TreeInteractionSnapshot {
    pub node_id: Option<String>,
    pub is_folder: bool,
    pub is_expanded: bool,
    pub preview_file_id: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TreePanelEffect {
    None,
    RefreshTree,
    OpenPreview(u32),
    SwitchToContentAndOpen(u32),
}

pub(super) struct PreviewTableModel {
    pub rows: Vec<PreviewRowViewModel>,
    pub selected_row_ix: Option<usize>,
    pub next_selected_file_id: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BlacklistTagViewModel {
    pub kind: BlacklistItemKind,
    pub value: String,
    pub display_label: SharedString,
    pub deletable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BlacklistSectionViewModel {
    pub kind: BlacklistItemKind,
    pub title: SharedString,
    pub count: usize,
    pub items: Vec<BlacklistTagViewModel>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn apply_preflight_event(
    process: &mut ProcessState,
    event: PreflightEvent,
    is_processing: bool,
    status_ready: &str,
) {
    match event {
        PreflightEvent::Started { revision } => {
            if revision == process.preflight_revision {
                process.preflight.is_scanning = true;
                if !is_processing {
                    process.ui_status = ProcessUiStatus::Preflight;
                }
            }
        }
        PreflightEvent::Progress {
            revision,
            scanned,
            candidates,
            skipped,
        } => {
            if revision == process.preflight_revision {
                process.preflight.scanned_entries = scanned;
                process.preflight.to_process_files = candidates;
                process.preflight.skipped_files = skipped;
                process.preflight.total_files = candidates + skipped;
                process.preflight.is_scanning = true;
                if !is_processing {
                    process.ui_status = ProcessUiStatus::Preflight;
                }
            }
        }
        PreflightEvent::Completed { revision, stats } => {
            if revision == process.preflight_revision {
                process.preflight = stats;
                if !is_processing {
                    process.ui_status = ProcessUiStatus::Idle;
                    process.processing_current_file = status_ready.to_string();
                }
            }
        }
        PreflightEvent::Failed { revision, error } => {
            if revision == process.preflight_revision {
                process.preflight.is_scanning = false;
                process.ui_status = ProcessUiStatus::Error;
                process.last_error = Some(error.to_string());
            }
        }
    }
}

pub(super) fn build_preview_table_model(
    result: Option<&ProcessResult>,
    filter: &str,
    current_selected_id: Option<u32>,
) -> PreviewTableModel {
    let rows = result
        .map(|result| {
            result
                .preview_files
                .iter()
                .filter(|entry| {
                    filter.is_empty() || entry.display_path.to_ascii_lowercase().contains(filter)
                })
                .map(|entry| PreviewRowViewModel {
                    id: entry.id,
                    display_path: entry.display_path.clone(),
                    chars: entry.chars,
                    tokens: entry.tokens,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let selected_row_ix = rows
        .iter()
        .position(|row| Some(row.id) == current_selected_id);
    let next_selected_file_id = selected_row_ix
        .and_then(|ix| rows.get(ix))
        .map(|row| row.id)
        .or_else(|| rows.first().map(|row| row.id));

    PreviewTableModel {
        rows,
        selected_row_ix,
        next_selected_file_id,
    }
}

pub(super) fn build_blacklist_sections(
    folder_blacklist: &[String],
    ext_blacklist: &[String],
    filter: &str,
    language: Language,
) -> Vec<BlacklistSectionViewModel> {
    let filter = filter.trim().to_ascii_lowercase();
    let mut sections = Vec::new();

    let folder_items =
        build_blacklist_section_items(folder_blacklist, BlacklistItemKind::Folder, &filter);
    if !folder_items.is_empty() {
        sections.push(BlacklistSectionViewModel {
            kind: BlacklistItemKind::Folder,
            title: SharedString::from(tr(language, "rules_group_folders")),
            count: folder_items.len(),
            items: folder_items,
        });
    }

    let ext_items = build_blacklist_section_items(ext_blacklist, BlacklistItemKind::Ext, &filter);
    if !ext_items.is_empty() {
        sections.push(BlacklistSectionViewModel {
            kind: BlacklistItemKind::Ext,
            title: SharedString::from(tr(language, "rules_group_extensions")),
            count: ext_items.len(),
            items: ext_items,
        });
    }

    sections
}

pub(super) fn build_tree_panel_data(result: Option<&ProcessResult>) -> Option<TreePanelData> {
    result.map(|result| TreePanelData {
        index: crate::services::tree::build_tree_index(&result.tree_nodes),
        preview_file_ids: preview_file_id_map(&result.preview_files),
    })
}

pub(super) fn project_tree_panel(
    data: Option<&TreePanelData>,
    filter: &str,
    expanded_ids: &BTreeSet<String>,
    selected_node_id: Option<&str>,
) -> TreeRenderState {
    let Some(data) = data else {
        return TreeRenderState::default();
    };

    let filter = filter.trim();
    let filter_lower = filter.to_ascii_lowercase();
    let context = TreeProjectionContext {
        filter: filter_lower.as_str(),
        expanded_ids,
        preview_file_ids: &data.preview_file_ids,
    };
    let mut projected_roots = Vec::new();

    for node in &data.index.roots {
        if let Some(projected) = build_tree_projection_node(node, &context) {
            projected_roots.push(projected);
        }
    }

    let items = projected_roots
        .iter()
        .map(ProjectedTreeNode::to_tree_item)
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut visible_summary = TreeCountSummary::default();
    let last_root_ix = projected_roots.len().saturating_sub(1);
    for (ix, node) in projected_roots.iter().enumerate() {
        append_visible_tree_rows(
            node,
            0,
            &[],
            ix < last_root_ix,
            &mut rows,
            &mut visible_summary,
        );
    }

    let rows_by_id = rows
        .iter()
        .cloned()
        .map(|row| (row.node_id.to_string(), row))
        .collect::<HashMap<_, _>>();
    let selected_row_ix = selected_node_id
        .and_then(|selected| rows.iter().position(|row| row.node_id.as_ref() == selected));

    TreeRenderState {
        items,
        rows,
        rows_by_id,
        visible_summary,
        total_summary: TreeCountSummary {
            folders: data.index.total_folders,
            files: data.index.total_files,
        },
        selected_row_ix,
    }
}

pub(super) fn apply_tree_interaction(
    state: &mut TreePanelState,
    previous: Option<&TreeInteractionSnapshot>,
    next: Option<TreeInteractionSnapshot>,
) -> TreePanelEffect {
    if previous == next.as_ref() {
        return TreePanelEffect::None;
    }

    let Some(snapshot) = next else {
        state.selected_node_id = None;
        return TreePanelEffect::None;
    };

    state.selected_node_id = snapshot.node_id.clone();
    if snapshot.is_folder {
        if let Some(node_id) = snapshot.node_id.as_ref() {
            if snapshot.is_expanded {
                state.expanded_ids.insert(node_id.clone());
            } else {
                state.expanded_ids.remove(node_id);
            }
        }

        if previous.and_then(|snapshot| snapshot.node_id.as_ref()) != snapshot.node_id.as_ref() {
            return TreePanelEffect::RefreshTree;
        }
        return TreePanelEffect::None;
    }

    match snapshot.preview_file_id {
        Some(file_id) => TreePanelEffect::SwitchToContentAndOpen(file_id),
        None => TreePanelEffect::None,
    }
}

fn build_tree_projection_node(
    node: &IndexedTreeNode,
    context: &TreeProjectionContext<'_>,
) -> Option<ProjectedTreeNode> {
    let filter_match = filter_node(node, context.filter);
    let is_expanded = if node.is_folder {
        if !context.filter.is_empty() {
            true
        } else {
            context.expanded_ids.contains(node.id.as_str())
        }
    } else {
        false
    };
    let icon_kind = if node.is_folder {
        if is_expanded {
            TreeIconKind::FolderOpen
        } else {
            TreeIconKind::FolderClosed
        }
    } else {
        icon_kind_for_extension(extension_for_path(node.relative_path.as_str()))
    };
    let mut children = Vec::new();
    for child in &node.children {
        if let Some(projected) = build_tree_projection_node(child, context) {
            children.push(projected);
        }
    }

    if filter_match.is_none() && children.is_empty() && !context.filter.is_empty() {
        return None;
    }

    Some(ProjectedTreeNode {
        node_id: node.id.clone(),
        label: node.label.clone(),
        relative_path: node.relative_path.clone(),
        is_folder: node.is_folder,
        extension: extension_for_path(node.relative_path.as_str()),
        preview_file_id: context
            .preview_file_ids
            .get(node.relative_path.as_str())
            .copied(),
        child_file_count: node.stats.descendant_files,
        child_folder_count: node.stats.descendant_folders,
        icon_kind,
        is_expanded,
        is_filter_match: filter_match.is_some(),
        match_range: filter_match.as_ref().map(|matched| matched.range.clone()),
        match_kind: filter_match.as_ref().map(|matched| matched.kind),
        matched_descendants: children.len(),
        children,
    })
}

fn preview_file_id_map(preview_files: &[PreviewFileEntry]) -> HashMap<String, u32> {
    preview_files
        .iter()
        .map(|entry| (entry.display_path.clone(), entry.id))
        .collect()
}

struct FilterMatch {
    kind: FilterMatchKind,
    range: Range<usize>,
}

struct TreeProjectionContext<'a> {
    filter: &'a str,
    expanded_ids: &'a BTreeSet<String>,
    preview_file_ids: &'a HashMap<String, u32>,
}

#[derive(Clone, Debug)]
struct ProjectedTreeNode {
    node_id: String,
    label: String,
    relative_path: String,
    is_folder: bool,
    extension: Option<String>,
    preview_file_id: Option<u32>,
    child_file_count: usize,
    child_folder_count: usize,
    icon_kind: TreeIconKind,
    is_expanded: bool,
    is_filter_match: bool,
    match_range: Option<Range<usize>>,
    match_kind: Option<FilterMatchKind>,
    matched_descendants: usize,
    children: Vec<ProjectedTreeNode>,
}

impl ProjectedTreeNode {
    fn to_tree_item(&self) -> TreeItem {
        if self.is_folder {
            TreeItem::new(self.node_id.clone(), self.label.clone())
                .expanded(self.is_expanded)
                .children(
                    self.children
                        .iter()
                        .map(ProjectedTreeNode::to_tree_item)
                        .collect::<Vec<_>>(),
                )
        } else {
            TreeItem::new(self.node_id.clone(), self.label.clone())
        }
    }
}

fn append_visible_tree_rows(
    node: &ProjectedTreeNode,
    depth: usize,
    ancestor_guides: &[bool],
    has_next_sibling: bool,
    rows: &mut Vec<TreeRowViewModel>,
    summary: &mut TreeCountSummary,
) {
    if node.is_folder {
        summary.folders += 1;
    } else {
        summary.files += 1;
    }

    let mut guide_continuations = ancestor_guides.to_vec();
    if depth > 0 {
        guide_continuations.push(has_next_sibling);
    }

    rows.push(TreeRowViewModel {
        node_id: SharedString::from(node.node_id.clone()),
        label: SharedString::from(node.label.clone()),
        relative_path: SharedString::from(node.relative_path.clone()),
        is_folder: node.is_folder,
        depth,
        extension: node.extension.clone().map(SharedString::from),
        preview_file_id: node.preview_file_id,
        child_file_count: node.child_file_count,
        child_folder_count: node.child_folder_count,
        icon_kind: node.icon_kind,
        is_expanded: node.is_expanded,
        is_filter_match: node.is_filter_match,
        match_range: node.match_range.clone(),
        match_kind: node.match_kind,
        matched_descendants: node.matched_descendants,
        guide_continuations: guide_continuations.clone(),
    });

    if !(node.is_folder && node.is_expanded) {
        return;
    }

    let last_child_ix = node.children.len().saturating_sub(1);
    for (ix, child) in node.children.iter().enumerate() {
        append_visible_tree_rows(
            child,
            depth + 1,
            &guide_continuations,
            ix < last_child_ix,
            rows,
            summary,
        );
    }
}

fn filter_node(node: &IndexedTreeNode, filter: &str) -> Option<FilterMatch> {
    if filter.is_empty() {
        return None;
    }
    find_match_range(&node.label, filter)
        .map(|range| FilterMatch {
            kind: FilterMatchKind::Label,
            range,
        })
        .or_else(|| {
            find_match_range(&node.relative_path, filter).map(|range| FilterMatch {
                kind: FilterMatchKind::Path,
                range,
            })
        })
}

fn find_match_range(input: &str, filter: &str) -> Option<Range<usize>> {
    if filter.is_empty() {
        return None;
    }
    let input_lower = input.to_ascii_lowercase();
    input_lower
        .find(filter)
        .map(|start| start..start + filter.len())
}

fn extension_for_path(path: &str) -> Option<String> {
    path.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| !ext.contains('/') && !ext.is_empty())
}

fn icon_kind_for_extension(extension: Option<String>) -> TreeIconKind {
    match extension.as_deref() {
        Some(
            "rs" | "js" | "jsx" | "ts" | "tsx" | "py" | "go" | "java" | "kt" | "swift" | "c" | "cc"
            | "cpp" | "h" | "hpp" | "cs" | "php" | "rb" | "sh" | "ps1" | "toml" | "yaml" | "yml",
        ) => TreeIconKind::Code,
        Some("md" | "mdx" | "txt" | "rtf") => TreeIconKind::Document,
        Some("json" | "lock" | "ini" | "conf" | "config" | "env") => TreeIconKind::Config,
        Some("csv" | "tsv" | "sql") => TreeIconKind::Data,
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "mp4" | "mov" | "mp3" | "wav") => {
            TreeIconKind::Media
        }
        _ => TreeIconKind::Text,
    }
}

fn build_blacklist_section_items(
    items: &[String],
    kind: BlacklistItemKind,
    filter: &str,
) -> Vec<BlacklistTagViewModel> {
    items
        .iter()
        .filter(|item| filter.is_empty() || item.to_ascii_lowercase().contains(filter))
        .map(|item| BlacklistTagViewModel {
            kind,
            value: item.clone(),
            display_label: SharedString::from(item.clone()),
            deletable: true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::{
        TreeCountSummary, TreeIconKind, TreeInteractionSnapshot, TreePanelEffect,
        apply_preflight_event, apply_tree_interaction, build_blacklist_sections,
        build_preview_table_model, build_tree_panel_data, project_tree_panel,
    };
    use crate::domain::{Language, PreflightStats, PreviewFileEntry, ProcessResult, TreeNode};
    use crate::processor::stats::ProcessingStats;
    use crate::services::preflight::PreflightEvent;
    use crate::ui::state::{ProcessState, ProcessUiStatus, TreePanelState};

    #[test]
    fn stale_preflight_event_does_not_override_current_state() {
        let mut process = ProcessState {
            preflight_revision: 2,
            ui_status: ProcessUiStatus::Idle,
            processing_current_file: "ready".to_string(),
            ..ProcessState::default()
        };

        apply_preflight_event(
            &mut process,
            PreflightEvent::Completed {
                revision: 1,
                stats: PreflightStats {
                    total_files: 99,
                    ..PreflightStats::default()
                },
            },
            false,
            "ready",
        );

        assert_eq!(process.preflight.total_files, 0);
        assert_eq!(process.ui_status, ProcessUiStatus::Idle);
        assert_eq!(process.processing_current_file, "ready");
    }

    #[test]
    fn preview_table_keeps_selected_row_when_filter_matches() {
        let result = sample_result();
        let model = build_preview_table_model(Some(&result), "lib", Some(2));

        assert_eq!(model.rows.len(), 1);
        assert_eq!(model.selected_row_ix, Some(0));
        assert_eq!(model.next_selected_file_id, Some(2));
    }

    #[test]
    fn preview_table_selects_first_row_when_current_selection_disappears() {
        let result = ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: Vec::new(),
            merged_content_path: None,
            suggested_result_name: "workspace-20260319.txt".to_string(),
            file_details: Vec::new(),
            preview_files: vec![PreviewFileEntry {
                id: 7,
                display_path: "src/lib.rs".to_string(),
                chars: 12,
                tokens: 4,
                preview_blob_path: PathBuf::from("b"),
                byte_len: 12,
            }],
            preview_blob_dir: None,
        };

        let model = build_preview_table_model(Some(&result), "lib", Some(2));

        assert_eq!(model.selected_row_ix, None);
        assert_eq!(model.next_selected_file_id, Some(7));
    }

    #[test]
    fn blacklist_sections_keep_group_order_and_item_order_without_filter() {
        let sections = build_blacklist_sections(
            &["target".into(), "dist".into()],
            &[".log".into(), ".tmp".into()],
            "",
            Language::En,
        );

        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title.as_ref(), "Folders");
        assert_eq!(
            sections[0]
                .items
                .iter()
                .map(|item| item.value.as_str())
                .collect::<Vec<_>>(),
            vec!["target", "dist"]
        );
        assert_eq!(sections[1].title.as_ref(), "Extensions");
        assert_eq!(
            sections[1]
                .items
                .iter()
                .map(|item| item.value.as_str())
                .collect::<Vec<_>>(),
            vec![".log", ".tmp"]
        );
    }

    #[test]
    fn blacklist_sections_filter_case_insensitively() {
        let sections = build_blacklist_sections(
            &["Node_Modules".into(), "target".into()],
            &[".PNG".into(), ".tmp".into()],
            "png",
            Language::En,
        );

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title.as_ref(), "Extensions");
        assert_eq!(sections[0].count, 1);
        assert_eq!(sections[0].items[0].value, ".PNG");
    }

    #[test]
    fn blacklist_sections_return_empty_when_filter_matches_nothing() {
        let sections = build_blacklist_sections(
            &["target".into()],
            &[".tmp".into()],
            "missing",
            Language::En,
        );

        assert!(sections.is_empty());
    }

    #[test]
    fn tree_panel_projection_links_preview_file_and_icons() {
        let result = sample_result();
        let data = build_tree_panel_data(Some(&result));
        let expanded = data
            .as_ref()
            .map(|data| data.index.default_expanded_ids.clone())
            .unwrap_or_default();

        let render = project_tree_panel(data.as_ref(), "", &expanded, Some("src/lib.rs"));

        let lib = render
            .rows
            .iter()
            .find(|row| row.node_id.as_ref() == "src/lib.rs")
            .expect("lib row");
        assert_eq!(lib.preview_file_id, Some(2));
        assert_eq!(lib.icon_kind, TreeIconKind::Code);
        assert_eq!(render.selected_row_ix, Some(2));
    }

    #[test]
    fn tree_panel_filter_keeps_ancestor_context_and_counts_visible_nodes() {
        let result = sample_result();
        let data = build_tree_panel_data(Some(&result));
        let expanded = data
            .as_ref()
            .map(|data| data.index.default_expanded_ids.clone())
            .unwrap_or_default();

        let render = project_tree_panel(data.as_ref(), "lib", &expanded, None);

        assert_eq!(
            render.visible_summary,
            TreeCountSummary {
                folders: 1,
                files: 1,
            }
        );
        assert_eq!(render.rows.len(), 2);
        assert_eq!(render.rows[0].node_id.as_ref(), "src");
        assert_eq!(render.rows[1].node_id.as_ref(), "src/lib.rs");
        assert_eq!(render.rows[1].match_range, Some(0..3));
    }

    #[test]
    fn folder_summaries_include_nested_children() {
        let result = nested_result();
        let data = build_tree_panel_data(Some(&result));
        let expanded = data
            .as_ref()
            .map(|data| data.index.default_expanded_ids.clone())
            .unwrap_or_default();

        let render = project_tree_panel(data.as_ref(), "", &expanded, None);
        let src = render
            .rows
            .iter()
            .find(|row| row.node_id.as_ref() == "src")
            .expect("src row");
        let nested = render
            .rows
            .iter()
            .find(|row| row.node_id.as_ref() == "src/nested")
            .expect("nested row");
        let nested_file = render
            .rows
            .iter()
            .find(|row| row.node_id.as_ref() == "src/nested/lib.rs")
            .expect("nested file row");
        let main = render
            .rows
            .iter()
            .find(|row| row.node_id.as_ref() == "src/main.rs")
            .expect("main row");

        assert_eq!(src.child_folder_count, 1);
        assert_eq!(src.child_file_count, 2);
        assert_eq!(nested.guide_continuations, vec![true]);
        assert_eq!(nested_file.guide_continuations, vec![true, false]);
        assert_eq!(main.guide_continuations, vec![false]);
    }

    #[test]
    fn collapsed_folders_do_not_emit_hidden_descendant_rows() {
        let data = build_tree_panel_data(Some(&nested_result()));
        let expanded = BTreeSet::from(["src".to_string()]);

        let render = project_tree_panel(data.as_ref(), "", &expanded, None);

        assert_eq!(
            render
                .rows
                .iter()
                .map(|row| (row.node_id.as_ref(), row.depth))
                .collect::<Vec<_>>(),
            vec![
                ("src", 0),
                ("src/nested", 1),
                ("src/main.rs", 1),
                ("README.md", 0),
            ]
        );
        assert_eq!(
            render.visible_summary,
            TreeCountSummary {
                folders: 2,
                files: 2,
            }
        );
        assert_eq!(render.rows[1].guide_continuations, vec![true]);
        assert_eq!(render.rows[2].guide_continuations, vec![false]);
    }

    #[test]
    fn tree_interaction_folder_selection_refreshes_once_and_updates_expansion() {
        let mut state = TreePanelState::default();
        let effect = apply_tree_interaction(
            &mut state,
            None,
            Some(TreeInteractionSnapshot {
                node_id: Some("src".to_string()),
                is_folder: true,
                is_expanded: true,
                preview_file_id: None,
            }),
        );

        assert_eq!(effect, TreePanelEffect::RefreshTree);
        assert_eq!(state.selected_node_id.as_deref(), Some("src"));
        assert!(state.expanded_ids.contains("src"));

        let same = apply_tree_interaction(
            &mut state,
            Some(&TreeInteractionSnapshot {
                node_id: Some("src".to_string()),
                is_folder: true,
                is_expanded: true,
                preview_file_id: None,
            }),
            Some(TreeInteractionSnapshot {
                node_id: Some("src".to_string()),
                is_folder: true,
                is_expanded: true,
                preview_file_id: None,
            }),
        );

        assert_eq!(same, TreePanelEffect::None);
    }

    #[test]
    fn tree_interaction_folder_expansion_change_does_not_force_refresh() {
        let mut state = TreePanelState {
            selected_node_id: Some("src".to_string()),
            expanded_ids: BTreeSet::from(["src".to_string()]),
        };
        let effect = apply_tree_interaction(
            &mut state,
            Some(&TreeInteractionSnapshot {
                node_id: Some("src".to_string()),
                is_folder: true,
                is_expanded: true,
                preview_file_id: None,
            }),
            Some(TreeInteractionSnapshot {
                node_id: Some("src".to_string()),
                is_folder: true,
                is_expanded: false,
                preview_file_id: None,
            }),
        );

        assert_eq!(effect, TreePanelEffect::None);
        assert!(!state.expanded_ids.contains("src"));
    }

    #[test]
    fn tree_interaction_file_selection_switches_to_preview_once() {
        let mut state = TreePanelState::default();
        let effect = apply_tree_interaction(
            &mut state,
            None,
            Some(TreeInteractionSnapshot {
                node_id: Some("src/lib.rs".to_string()),
                is_folder: false,
                is_expanded: false,
                preview_file_id: Some(2),
            }),
        );

        assert_eq!(effect, TreePanelEffect::SwitchToContentAndOpen(2));
        assert_eq!(state.selected_node_id.as_deref(), Some("src/lib.rs"));
    }

    fn sample_result() -> ProcessResult {
        ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: vec![
                TreeNode {
                    id: "src".to_string(),
                    label: "src".to_string(),
                    relative_path: "src".to_string(),
                    is_folder: true,
                    children: vec![
                        TreeNode {
                            id: "src/main.rs".to_string(),
                            label: "main.rs".to_string(),
                            relative_path: "src/main.rs".to_string(),
                            is_folder: false,
                            children: Vec::new(),
                        },
                        TreeNode {
                            id: "src/lib.rs".to_string(),
                            label: "lib.rs".to_string(),
                            relative_path: "src/lib.rs".to_string(),
                            is_folder: false,
                            children: Vec::new(),
                        },
                    ],
                },
                TreeNode {
                    id: "README.md".to_string(),
                    label: "README.md".to_string(),
                    relative_path: "README.md".to_string(),
                    is_folder: false,
                    children: Vec::new(),
                },
            ],
            merged_content_path: None,
            suggested_result_name: "workspace-20260319.txt".to_string(),
            file_details: Vec::new(),
            preview_files: vec![
                PreviewFileEntry {
                    id: 1,
                    display_path: "src/main.rs".to_string(),
                    chars: 10,
                    tokens: 3,
                    preview_blob_path: PathBuf::from("a"),
                    byte_len: 10,
                },
                PreviewFileEntry {
                    id: 2,
                    display_path: "src/lib.rs".to_string(),
                    chars: 12,
                    tokens: 4,
                    preview_blob_path: PathBuf::from("b"),
                    byte_len: 12,
                },
            ],
            preview_blob_dir: None,
        }
    }

    fn nested_result() -> ProcessResult {
        ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: vec![
                TreeNode {
                    id: "src".to_string(),
                    label: "src".to_string(),
                    relative_path: "src".to_string(),
                    is_folder: true,
                    children: vec![
                        TreeNode {
                            id: "src/nested".to_string(),
                            label: "nested".to_string(),
                            relative_path: "src/nested".to_string(),
                            is_folder: true,
                            children: vec![TreeNode {
                                id: "src/nested/lib.rs".to_string(),
                                label: "lib.rs".to_string(),
                                relative_path: "src/nested/lib.rs".to_string(),
                                is_folder: false,
                                children: Vec::new(),
                            }],
                        },
                        TreeNode {
                            id: "src/main.rs".to_string(),
                            label: "main.rs".to_string(),
                            relative_path: "src/main.rs".to_string(),
                            is_folder: false,
                            children: Vec::new(),
                        },
                    ],
                },
                TreeNode {
                    id: "README.md".to_string(),
                    label: "README.md".to_string(),
                    relative_path: "README.md".to_string(),
                    is_folder: false,
                    children: Vec::new(),
                },
            ],
            merged_content_path: None,
            suggested_result_name: "workspace-20260319.txt".to_string(),
            file_details: Vec::new(),
            preview_files: vec![
                PreviewFileEntry {
                    id: 1,
                    display_path: "src/main.rs".to_string(),
                    chars: 10,
                    tokens: 3,
                    preview_blob_path: PathBuf::from("a"),
                    byte_len: 10,
                },
                PreviewFileEntry {
                    id: 2,
                    display_path: "src/nested/lib.rs".to_string(),
                    chars: 12,
                    tokens: 4,
                    preview_blob_path: PathBuf::from("b"),
                    byte_len: 12,
                },
            ],
            preview_blob_dir: None,
        }
    }
}
