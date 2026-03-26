use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::ops::Range;

use gpui::SharedString;
use gpui_component::{Icon, IconName, tree::TreeItem};

use crate::domain::{ArchiveEntrySource, Language, PreviewFileEntry, ProcessResult};
use crate::services::tree::{IndexedTreeNode, TreeIndex};
use crate::ui::state::TreePanelState;
use crate::utils::i18n::tr;

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::ui::workspace) struct TreeCountSummary {
    pub folders: usize,
    pub files: usize,
}

impl TreeCountSummary {
    pub fn total(self) -> usize {
        self.folders + self.files
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum TreeIconKind {
    FolderClosed,
    FolderOpen,
    Rust,
    Toml,
    Json,
    Markdown,
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
            Self::Rust => Icon::new(IconName::SquareTerminal),
            Self::Toml => Icon::new(IconName::Settings2),
            Self::Json => Icon::new(IconName::LayoutDashboard),
            Self::Markdown => Icon::new(IconName::BookOpen),
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
pub(in crate::ui::workspace) enum FilterMatchKind {
    Label,
    Path,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct TreeRowViewModel {
    pub node_id: SharedString,
    pub label: SharedString,
    pub relative_path: SharedString,
    pub is_folder: bool,
    pub depth: usize,
    pub extension: Option<SharedString>,
    pub preview_file_id: Option<u32>,
    pub preview_chars: Option<usize>,
    pub preview_tokens: Option<usize>,
    pub archive: Option<ArchiveEntrySource>,
    pub child_file_count: usize,
    pub child_folder_count: usize,
    pub icon_kind: TreeIconKind,
    pub is_expanded: bool,
    pub is_filter_match: bool,
    pub match_range: Option<Range<usize>>,
    pub match_kind: Option<FilterMatchKind>,
    pub matched_descendants: usize,
}

#[derive(Default)]
pub(in crate::ui::workspace) struct TreeRenderState {
    pub items: Vec<TreeItem>,
    pub rows: Vec<TreeRowViewModel>,
    pub rows_by_id: HashMap<String, TreeRowViewModel>,
    pub visible_summary: TreeCountSummary,
    pub selected_row_ix: Option<usize>,
    pub structure_signature: u64,
}

#[derive(Clone, Debug)]
pub(in crate::ui::workspace) struct TreePanelData {
    pub index: TreeIndex,
    preview_files: HashMap<String, PreviewFileMeta>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct TreeInteractionSnapshot {
    pub node_id: Option<String>,
    pub is_folder: bool,
    pub is_expanded: bool,
    pub preview_file_id: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum TreePanelEffect {
    None,
    RefreshVisibleTree,
    SwitchToContentAndOpen(u32),
}

#[derive(Clone, Debug, Default)]
pub(in crate::ui::workspace) struct TreeProjectionState {
    pub roots: Vec<TreeProjectionNode>,
    pub total_summary: TreeCountSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum TreePaneBodyViewModel {
    Tree,
    PlainText {
        lines: Vec<SharedString>,
    },
    Empty {
        title: SharedString,
        hint: SharedString,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct TreePaneViewModel {
    pub filter_active: bool,
    pub visible_summary: TreeCountSummary,
    pub total_summary: TreeCountSummary,
    pub view_mode_label: SharedString,
    pub disable_structure_actions: bool,
    pub body: TreePaneBodyViewModel,
}

struct FilterMatch {
    kind: FilterMatchKind,
    range: Range<usize>,
}

struct VisibleTreeContext<'a> {
    filter_active: bool,
    expanded_ids: &'a BTreeSet<String>,
}

struct TreeProjectionContext<'a> {
    filter: &'a str,
    preview_files: &'a HashMap<String, PreviewFileMeta>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreviewFileMeta {
    id: u32,
    chars: usize,
    tokens: usize,
    archive: Option<ArchiveEntrySource>,
}

#[derive(Clone, Debug)]
pub(in crate::ui::workspace) struct TreeProjectionNode {
    node_id: String,
    label: String,
    relative_path: String,
    is_folder: bool,
    extension: Option<String>,
    preview: Option<PreviewFileMeta>,
    child_file_count: usize,
    child_folder_count: usize,
    is_filter_match: bool,
    match_range: Option<Range<usize>>,
    match_kind: Option<FilterMatchKind>,
    matched_descendants: usize,
    children: Vec<TreeProjectionNode>,
}

pub(in crate::ui::workspace) fn ancestor_node_ids(node_id: &str) -> BTreeSet<String> {
    node_id
        .match_indices('/')
        .filter(|(ix, _)| *ix > 0)
        .map(|(ix, _)| node_id[..ix].to_string())
        .collect()
}

pub(in crate::ui::workspace) fn build_tree_pane_view_model(
    render_state: &TreeRenderState,
    total_summary: TreeCountSummary,
    tree_filter: &str,
    result: Option<&ProcessResult>,
    language: Language,
    plain_text_mode: bool,
) -> TreePaneViewModel {
    let filter_active = !tree_filter.trim().is_empty();
    let has_result = result.is_some();

    TreePaneViewModel {
        filter_active,
        visible_summary: render_state.visible_summary,
        total_summary,
        view_mode_label: SharedString::from(if plain_text_mode {
            tr(language, "tree_view_tree")
        } else {
            tr(language, "tree_view_text")
        }),
        disable_structure_actions: !has_result || filter_active || plain_text_mode,
        body: if plain_text_mode {
            build_tree_plain_text_body(result, language)
        } else if !render_state.rows.is_empty() {
            TreePaneBodyViewModel::Tree
        } else {
            TreePaneBodyViewModel::Empty {
                title: SharedString::from(if has_result {
                    tr(language, "tree_no_match")
                } else {
                    tr(language, "tree_empty")
                }),
                hint: SharedString::from(if has_result && filter_active {
                    tr(language, "tree_no_match_hint")
                } else {
                    tr(language, "tree_empty_hint")
                }),
            }
        },
    }
}

pub(in crate::ui::workspace) fn build_tree_panel_data(
    result: Option<&ProcessResult>,
) -> Option<TreePanelData> {
    result.map(|result| TreePanelData {
        index: crate::services::tree::build_tree_index(&result.tree_nodes),
        preview_files: preview_file_map(&result.preview_files),
    })
}

pub(in crate::ui::workspace) fn build_tree_projection(
    data: Option<&TreePanelData>,
    filter: &str,
) -> TreeProjectionState {
    let Some(data) = data else {
        return TreeProjectionState::default();
    };

    let filter = filter.trim();
    let filter_lower = filter.to_ascii_lowercase();
    let context = TreeProjectionContext {
        filter: filter_lower.as_str(),
        preview_files: &data.preview_files,
    };
    let mut roots = Vec::new();

    for node in &data.index.roots {
        if let Some(projected) = build_tree_projection_node(node, &context) {
            roots.push(projected);
        }
    }

    TreeProjectionState {
        roots,
        total_summary: TreeCountSummary {
            folders: data.index.total_folders,
            files: data.index.total_files,
        },
    }
}

pub(in crate::ui::workspace) fn build_tree_render_state(
    projection: &TreeProjectionState,
    filter_active: bool,
    expanded_ids: &BTreeSet<String>,
    selected_node_id: Option<&str>,
) -> TreeRenderState {
    let visible_context = VisibleTreeContext {
        filter_active,
        expanded_ids,
    };
    let items = projection
        .roots
        .iter()
        .map(|node| node.to_tree_item(&visible_context))
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut visible_summary = TreeCountSummary::default();
    for node in &projection.roots {
        append_visible_tree_rows(node, 0, &visible_context, &mut rows, &mut visible_summary);
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
        selected_row_ix,
        structure_signature: tree_structure_signature(projection, &visible_context),
    }
}

pub(in crate::ui::workspace) fn apply_tree_interaction(
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
            return TreePanelEffect::RefreshVisibleTree;
        }
        return TreePanelEffect::RefreshVisibleTree;
    }

    match snapshot.preview_file_id {
        Some(file_id) => TreePanelEffect::SwitchToContentAndOpen(file_id),
        None => TreePanelEffect::None,
    }
}

fn build_tree_projection_node(
    node: &IndexedTreeNode,
    context: &TreeProjectionContext<'_>,
) -> Option<TreeProjectionNode> {
    let filter_match = filter_node(node, context.filter);
    let mut children = Vec::new();
    for child in &node.children {
        if let Some(projected) = build_tree_projection_node(child, context) {
            children.push(projected);
        }
    }

    if filter_match.is_none() && children.is_empty() && !context.filter.is_empty() {
        return None;
    }

    Some(TreeProjectionNode {
        node_id: node.id.clone(),
        label: node.label.clone(),
        relative_path: node.relative_path.clone(),
        is_folder: node.is_folder,
        extension: extension_for_path(node.relative_path.as_str()),
        preview: context
            .preview_files
            .get(node.relative_path.as_str())
            .cloned(),
        child_file_count: node.stats.descendant_files,
        child_folder_count: node.stats.descendant_folders,
        is_filter_match: filter_match.is_some(),
        match_range: filter_match.as_ref().map(|matched| matched.range.clone()),
        match_kind: filter_match.as_ref().map(|matched| matched.kind),
        matched_descendants: children.len(),
        children,
    })
}

fn preview_file_map(preview_files: &[PreviewFileEntry]) -> HashMap<String, PreviewFileMeta> {
    preview_files
        .iter()
        .map(|entry| {
            (
                entry.display_path.clone(),
                PreviewFileMeta {
                    id: entry.id,
                    chars: entry.chars,
                    tokens: entry.tokens,
                    archive: entry.archive.clone(),
                },
            )
        })
        .collect()
}

impl TreeProjectionNode {
    fn to_tree_item(&self, context: &VisibleTreeContext<'_>) -> TreeItem {
        if self.is_folder {
            TreeItem::new(self.node_id.clone(), self.label.clone())
                .expanded(self.is_expanded(context))
                .children(
                    self.children
                        .iter()
                        .map(|child| child.to_tree_item(context))
                        .collect::<Vec<_>>(),
                )
        } else {
            TreeItem::new(self.node_id.clone(), self.label.clone())
        }
    }

    fn is_expanded(&self, context: &VisibleTreeContext<'_>) -> bool {
        self.is_folder
            && (context.filter_active || context.expanded_ids.contains(self.node_id.as_str()))
    }

    fn icon_kind(&self, context: &VisibleTreeContext<'_>) -> TreeIconKind {
        if self.is_folder {
            if self.is_expanded(context) {
                TreeIconKind::FolderOpen
            } else {
                TreeIconKind::FolderClosed
            }
        } else {
            icon_kind_for_extension(self.extension.clone())
        }
    }
}

fn build_tree_plain_text_body(
    result: Option<&ProcessResult>,
    language: Language,
) -> TreePaneBodyViewModel {
    let tree_string = result
        .map(|result| result.tree_string.as_str())
        .unwrap_or_default();
    if tree_string.is_empty() {
        return TreePaneBodyViewModel::Empty {
            title: SharedString::from(tr(language, "tree_empty")),
            hint: SharedString::from(tr(language, "tree_empty_hint")),
        };
    }

    TreePaneBodyViewModel::PlainText {
        lines: tree_string
            .split('\n')
            .map(|line| SharedString::from(line.trim_end_matches('\r').replace(' ', "\u{00A0}")))
            .collect(),
    }
}

fn append_visible_tree_rows(
    node: &TreeProjectionNode,
    depth: usize,
    context: &VisibleTreeContext<'_>,
    rows: &mut Vec<TreeRowViewModel>,
    summary: &mut TreeCountSummary,
) {
    let is_expanded = node.is_expanded(context);
    if node.is_folder {
        summary.folders += 1;
    } else {
        summary.files += 1;
    }

    rows.push(TreeRowViewModel {
        node_id: SharedString::from(node.node_id.clone()),
        label: SharedString::from(node.label.clone()),
        relative_path: SharedString::from(node.relative_path.clone()),
        is_folder: node.is_folder,
        depth,
        extension: node.extension.clone().map(SharedString::from),
        preview_file_id: node.preview.as_ref().map(|preview| preview.id),
        preview_chars: node.preview.as_ref().map(|preview| preview.chars),
        preview_tokens: node.preview.as_ref().map(|preview| preview.tokens),
        archive: node
            .preview
            .as_ref()
            .and_then(|preview| preview.archive.clone()),
        child_file_count: node.child_file_count,
        child_folder_count: node.child_folder_count,
        icon_kind: node.icon_kind(context),
        is_expanded,
        is_filter_match: node.is_filter_match,
        match_range: node.match_range.clone(),
        match_kind: node.match_kind,
        matched_descendants: node.matched_descendants,
    });

    if !(node.is_folder && is_expanded) {
        return;
    }

    for child in &node.children {
        append_visible_tree_rows(child, depth + 1, context, rows, summary);
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

pub(in crate::ui::workspace) fn icon_kind_for_extension(extension: Option<String>) -> TreeIconKind {
    match extension.as_deref() {
        Some("rs") => TreeIconKind::Rust,
        Some("toml") => TreeIconKind::Toml,
        Some("json") => TreeIconKind::Json,
        Some("md" | "mdx") => TreeIconKind::Markdown,
        Some(
            "js" | "jsx" | "ts" | "tsx" | "py" | "go" | "java" | "kt" | "swift" | "c" | "cc"
            | "cpp" | "h" | "hpp" | "cs" | "php" | "rb" | "sh" | "ps1" | "yaml" | "yml",
        ) => TreeIconKind::Code,
        Some("txt" | "rtf") => TreeIconKind::Document,
        Some("lock" | "ini" | "conf" | "config" | "env") => TreeIconKind::Config,
        Some("csv" | "tsv" | "sql") => TreeIconKind::Data,
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "mp4" | "mov" | "mp3" | "wav") => {
            TreeIconKind::Media
        }
        _ => TreeIconKind::Text,
    }
}

fn tree_structure_signature(
    projection: &TreeProjectionState,
    context: &VisibleTreeContext<'_>,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for node in &projection.roots {
        hash_visible_tree(node, context, &mut hasher);
    }
    hasher.finish()
}

fn hash_visible_tree(
    node: &TreeProjectionNode,
    context: &VisibleTreeContext<'_>,
    hasher: &mut impl Hasher,
) {
    node.node_id.hash(hasher);
    node.is_folder.hash(hasher);
    node.is_expanded(context).hash(hasher);
    if !(node.is_folder && node.is_expanded(context)) {
        return;
    }
    for child in &node.children {
        hash_visible_tree(child, context, hasher);
    }
}
