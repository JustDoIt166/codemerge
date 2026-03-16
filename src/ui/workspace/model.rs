use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

use gpui::SharedString;
use gpui_component::{Icon, IconName, tree::TreeItem};

use crate::domain::{PreviewFileEntry, PreviewRowViewModel, ProcessResult, TreeNode};
use crate::services::preflight::PreflightEvent;
use crate::ui::state::{ProcessState, ProcessUiStatus};

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
}

impl TreeRowViewModel {}

pub(super) struct TreePanelModel {
    pub items: Vec<TreeItem>,
    pub rows: Vec<TreeRowViewModel>,
    pub visible_summary: TreeCountSummary,
    pub total_summary: TreeCountSummary,
    pub selected_row_ix: Option<usize>,
}

pub(super) struct PreviewTableModel {
    pub rows: Vec<PreviewRowViewModel>,
    pub selected_row_ix: Option<usize>,
    pub next_selected_file_id: Option<u32>,
}

pub(super) fn apply_preflight_event(
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

pub(super) fn default_expanded_ids(nodes: &[TreeNode]) -> BTreeSet<String> {
    let mut expanded = BTreeSet::new();
    collect_default_expanded_ids(nodes, 0, &mut expanded);
    expanded
}

fn collect_default_expanded_ids(nodes: &[TreeNode], depth: usize, expanded: &mut BTreeSet<String>) {
    for node in nodes {
        if node.is_folder && depth < 2 {
            expanded.insert(node.id.clone());
            collect_default_expanded_ids(&node.children, depth + 1, expanded);
        }
    }
}

pub(super) fn collect_folder_ids(nodes: &[TreeNode]) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    collect_folder_ids_inner(nodes, &mut ids);
    ids
}

fn collect_folder_ids_inner(nodes: &[TreeNode], ids: &mut BTreeSet<String>) {
    for node in nodes {
        if node.is_folder {
            ids.insert(node.id.clone());
            collect_folder_ids_inner(&node.children, ids);
        }
    }
}

pub(super) fn build_tree_panel_model(
    result: Option<&ProcessResult>,
    filter: &str,
    expanded_ids: &BTreeSet<String>,
    selected_node_id: Option<&str>,
) -> TreePanelModel {
    let Some(result) = result else {
        return TreePanelModel {
            items: Vec::new(),
            rows: Vec::new(),
            visible_summary: TreeCountSummary::default(),
            total_summary: TreeCountSummary::default(),
            selected_row_ix: None,
        };
    };

    let preview_map = preview_file_id_map(&result.preview_files);
    let total_summary = summarize_tree_counts(&result.tree_nodes);
    let filter = filter.trim();
    let filter_lower = filter.to_ascii_lowercase();

    let mut rows = Vec::new();
    let mut items = Vec::new();
    let mut visible_summary = TreeCountSummary::default();
    for node in &result.tree_nodes {
        if let Some(built) = build_tree_node(
            node,
            &filter_lower,
            expanded_ids,
            &preview_map,
            0,
            true,
            &mut rows,
            &mut visible_summary,
        ) {
            items.push(built);
        }
    }

    let selected_row_ix = selected_node_id
        .and_then(|selected| rows.iter().position(|row| row.node_id.as_ref() == selected));

    TreePanelModel {
        items,
        rows,
        visible_summary,
        total_summary,
        selected_row_ix,
    }
}

fn build_tree_node(
    node: &TreeNode,
    filter: &str,
    expanded_ids: &BTreeSet<String>,
    preview_map: &HashMap<&str, u32>,
    depth: usize,
    is_visible: bool,
    rows: &mut Vec<TreeRowViewModel>,
    summary: &mut TreeCountSummary,
) -> Option<TreeItem> {
    let filter_match = filter_node(node, filter);
    let child_counts = summarize_tree_counts(&node.children);
    let is_expanded = if node.is_folder {
        if !filter.is_empty() {
            true
        } else {
            expanded_ids.contains(node.id.as_str())
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
    let mut child_items = Vec::new();
    let mut visible_child_rows = Vec::new();
    let mut matched_descendants = 0;
    let child_is_visible = is_visible && node.is_folder && is_expanded;
    for child in &node.children {
        if let Some(item) = build_tree_node(
            child,
            filter,
            expanded_ids,
            preview_map,
            depth + 1,
            child_is_visible,
            &mut visible_child_rows,
            summary,
        ) {
            matched_descendants += 1;
            child_items.push(item);
        }
    }

    if filter_match.is_none() && child_items.is_empty() && !filter.is_empty() {
        return None;
    }

    let item = if node.is_folder {
        TreeItem::new(node.id.clone(), node.label.clone())
            .expanded(is_expanded)
            .children(child_items)
    } else {
        TreeItem::new(node.id.clone(), node.label.clone())
    };

    if is_visible {
        if node.is_folder {
            summary.folders += 1;
        } else {
            summary.files += 1;
        }
        rows.push(TreeRowViewModel {
            node_id: SharedString::from(node.id.clone()),
            label: SharedString::from(node.label.clone()),
            relative_path: SharedString::from(node.relative_path.clone()),
            is_folder: node.is_folder,
            depth,
            extension: extension_for_path(node.relative_path.as_str()).map(SharedString::from),
            preview_file_id: preview_map.get(node.relative_path.as_str()).copied(),
            child_file_count: child_counts.files,
            child_folder_count: child_counts.folders,
            icon_kind,
            is_expanded,
            is_filter_match: filter_match.is_some(),
            match_range: filter_match.as_ref().map(|matched| matched.range.clone()),
            match_kind: filter_match.as_ref().map(|matched| matched.kind),
            matched_descendants,
        });
        rows.extend(visible_child_rows);
    }

    Some(item)
}

fn preview_file_id_map(preview_files: &[PreviewFileEntry]) -> HashMap<&str, u32> {
    preview_files
        .iter()
        .map(|entry| (entry.display_path.as_str(), entry.id))
        .collect()
}

fn summarize_tree_counts(nodes: &[TreeNode]) -> TreeCountSummary {
    let mut summary = TreeCountSummary::default();
    for node in nodes {
        if node.is_folder {
            summary.folders += 1;
        } else {
            summary.files += 1;
        }
        let child_summary = summarize_tree_counts(&node.children);
        summary.folders += child_summary.folders;
        summary.files += child_summary.files;
    }
    summary
}

struct FilterMatch {
    kind: FilterMatchKind,
    range: Range<usize>,
}

fn filter_node(node: &TreeNode, filter: &str) -> Option<FilterMatch> {
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
        Some("png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" | "mp3" | "wav" | "mp4") => {
            TreeIconKind::Media
        }
        _ => TreeIconKind::Text,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::{
        TreeCountSummary, TreeIconKind, apply_preflight_event, build_preview_table_model,
        build_tree_panel_model, collect_folder_ids, default_expanded_ids,
    };
    use crate::domain::{PreflightStats, PreviewFileEntry, ProcessResult, TreeNode};
    use crate::processor::stats::ProcessingStats;
    use crate::services::preflight::PreflightEvent;
    use crate::ui::state::{ProcessState, ProcessUiStatus};

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
    fn tree_panel_model_links_preview_file_and_icons() {
        let result = sample_result();
        let expanded = default_expanded_ids(&result.tree_nodes);

        let model = build_tree_panel_model(Some(&result), "", &expanded, Some("src/lib.rs"));

        let lib = model
            .rows
            .iter()
            .find(|row| row.node_id.as_ref() == "src/lib.rs")
            .expect("lib row");
        assert_eq!(lib.preview_file_id, Some(2));
        assert_eq!(lib.icon_kind, TreeIconKind::Code);
        assert_eq!(model.selected_row_ix, Some(2));
    }

    #[test]
    fn tree_panel_filter_keeps_ancestor_context_and_counts_visible_nodes() {
        let result = sample_result();
        let expanded = default_expanded_ids(&result.tree_nodes);

        let model = build_tree_panel_model(Some(&result), "lib", &expanded, None);

        assert_eq!(
            model.visible_summary,
            TreeCountSummary {
                folders: 1,
                files: 1
            }
        );
        assert_eq!(model.rows.len(), 2);
        assert_eq!(model.rows[0].node_id.as_ref(), "src");
        assert_eq!(model.rows[1].node_id.as_ref(), "src/lib.rs");
        assert_eq!(model.rows[1].match_range, Some(0..3));
    }

    #[test]
    fn folder_summaries_include_nested_children() {
        let result = sample_result();
        let expanded = default_expanded_ids(&result.tree_nodes);

        let model = build_tree_panel_model(Some(&result), "", &expanded, None);
        let src = model
            .rows
            .iter()
            .find(|row| row.node_id.as_ref() == "src")
            .expect("src row");

        assert_eq!(src.child_folder_count, 0);
        assert_eq!(src.child_file_count, 2);
    }

    #[test]
    fn collect_expand_state_helpers_cover_folder_ids() {
        let result = sample_result();

        let default_ids = default_expanded_ids(&result.tree_nodes);
        let folder_ids = collect_folder_ids(&result.tree_nodes);

        assert!(default_ids.contains("src"));
        assert!(folder_ids.contains("src"));
        assert!(!folder_ids.contains("README.md"));
    }

    #[test]
    fn collapsed_folders_do_not_emit_hidden_descendant_rows() {
        let result = nested_result();
        let expanded = BTreeSet::from(["src".to_string()]);

        let model = build_tree_panel_model(Some(&result), "", &expanded, None);

        assert_eq!(
            model
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
            model.visible_summary,
            TreeCountSummary {
                folders: 2,
                files: 2,
            }
        );
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
                                id: "src/nested/deep.rs".to_string(),
                                label: "deep.rs".to_string(),
                                relative_path: "src/nested/deep.rs".to_string(),
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
            file_details: Vec::new(),
            preview_files: Vec::new(),
            preview_blob_dir: None,
        }
    }
}
