use std::collections::{BTreeMap, BTreeSet};

use crate::domain::TreeNode;
use crate::processor::walker::CandidateFile;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeNodeStats {
    pub subtree_files: usize,
    pub subtree_folders: usize,
    pub descendant_files: usize,
    pub descendant_folders: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedTreeNode {
    pub id: String,
    pub label: String,
    pub relative_path: String,
    pub is_folder: bool,
    pub stats: TreeNodeStats,
    pub children: Vec<IndexedTreeNode>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeIndex {
    pub roots: Vec<IndexedTreeNode>,
    pub total_files: usize,
    pub total_folders: usize,
    pub default_expanded_ids: BTreeSet<String>,
    pub folder_ids: BTreeSet<String>,
}

#[derive(Default)]
struct NodeBuilder {
    label: String,
    relative_path: String,
    is_folder: bool,
    children: BTreeMap<String, NodeBuilder>,
}

impl NodeBuilder {
    fn folder(label: String, relative_path: String) -> Self {
        Self {
            label,
            relative_path,
            is_folder: true,
            children: BTreeMap::new(),
        }
    }

    fn file(label: String, relative_path: String) -> Self {
        Self {
            label,
            relative_path,
            is_folder: false,
            children: BTreeMap::new(),
        }
    }

    fn into_node(self, parent_id: &str) -> TreeNode {
        let id = if parent_id.is_empty() {
            self.relative_path.clone()
        } else if self.relative_path.is_empty() {
            parent_id.to_string()
        } else {
            self.relative_path.clone()
        };
        let mut children = self
            .children
            .into_values()
            .map(|child| child.into_node(&id))
            .collect::<Vec<_>>();
        children.sort_by(|a, b| {
            b.is_folder
                .cmp(&a.is_folder)
                .then_with(|| a.label.cmp(&b.label))
        });
        TreeNode {
            id,
            label: self.label,
            relative_path: self.relative_path,
            is_folder: self.is_folder,
            children,
        }
    }
}

pub fn build_tree_nodes(candidates: &[CandidateFile]) -> Vec<TreeNode> {
    let mut roots = BTreeMap::<String, NodeBuilder>::new();
    for candidate in candidates {
        let parts = candidate.relative.split('/').collect::<Vec<_>>();
        insert_parts(&mut roots, &parts, String::new());
    }

    roots
        .into_values()
        .map(|node| node.into_node(""))
        .collect::<Vec<_>>()
}

pub fn build_tree_index(nodes: &[TreeNode]) -> TreeIndex {
    let mut default_expanded_ids = BTreeSet::new();
    let mut folder_ids = BTreeSet::new();
    let roots = nodes
        .iter()
        .map(|node| index_node(node, 0, &mut default_expanded_ids, &mut folder_ids))
        .collect::<Vec<_>>();
    let total_files = roots.iter().map(|node| node.stats.subtree_files).sum();
    let total_folders = roots.iter().map(|node| node.stats.subtree_folders).sum();
    TreeIndex {
        roots,
        total_files,
        total_folders,
        default_expanded_ids,
        folder_ids,
    }
}

fn index_node(
    node: &TreeNode,
    depth: usize,
    default_expanded_ids: &mut BTreeSet<String>,
    folder_ids: &mut BTreeSet<String>,
) -> IndexedTreeNode {
    let children = node
        .children
        .iter()
        .map(|child| index_node(child, depth + 1, default_expanded_ids, folder_ids))
        .collect::<Vec<_>>();
    let descendant_files = children.iter().map(|child| child.stats.subtree_files).sum();
    let descendant_folders = children
        .iter()
        .map(|child| child.stats.subtree_folders)
        .sum();
    if node.is_folder {
        folder_ids.insert(node.id.clone());
        if depth < 2 {
            default_expanded_ids.insert(node.id.clone());
        }
    }
    IndexedTreeNode {
        id: node.id.clone(),
        label: node.label.clone(),
        relative_path: node.relative_path.clone(),
        is_folder: node.is_folder,
        stats: TreeNodeStats {
            subtree_files: descendant_files + usize::from(!node.is_folder),
            subtree_folders: descendant_folders + usize::from(node.is_folder),
            descendant_files,
            descendant_folders,
        },
        children,
    }
}

fn insert_parts(nodes: &mut BTreeMap<String, NodeBuilder>, parts: &[&str], mut prefix: String) {
    let Some((head, tail)) = parts.split_first() else {
        return;
    };
    if !prefix.is_empty() {
        prefix.push('/');
    }
    prefix.push_str(head);
    let is_last = tail.is_empty();
    let entry = nodes.entry((*head).to_string()).or_insert_with(|| {
        if is_last {
            NodeBuilder::file((*head).to_string(), prefix.clone())
        } else {
            NodeBuilder::folder((*head).to_string(), prefix.clone())
        }
    });
    if !is_last {
        insert_parts(&mut entry.children, tail, prefix);
    }
}

#[cfg(test)]
mod tests {
    use super::{build_tree_index, build_tree_nodes};
    use crate::domain::TreeNode;
    use crate::processor::walker::CandidateFile;
    use std::path::PathBuf;

    #[test]
    fn builds_hierarchical_nodes() {
        let nodes = build_tree_nodes(&[
            CandidateFile {
                absolute: PathBuf::from("a"),
                relative: "src/main.rs".to_string(),
                archive_entry: None,
                archive_path: None,
            },
            CandidateFile {
                absolute: PathBuf::from("b"),
                relative: "src/lib.rs".to_string(),
                archive_entry: None,
                archive_path: None,
            },
            CandidateFile {
                absolute: PathBuf::from("c"),
                relative: "README.md".to_string(),
                archive_entry: None,
                archive_path: None,
            },
        ]);

        assert_eq!(nodes.len(), 2);
        let src = nodes
            .iter()
            .find(|node| node.label == "src")
            .expect("src folder");
        assert!(src.is_folder);
        assert_eq!(src.children.len(), 2);
    }

    #[test]
    fn builds_tree_index_with_nested_stats_and_expand_sets() {
        let index = build_tree_index(&[
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
        ]);

        assert_eq!(index.total_folders, 2);
        assert_eq!(index.total_files, 3);
        assert!(index.default_expanded_ids.contains("src"));
        assert!(index.default_expanded_ids.contains("src/nested"));
        assert!(index.folder_ids.contains("src"));
        assert!(!index.folder_ids.contains("README.md"));

        let src = index
            .roots
            .iter()
            .find(|node| node.id == "src")
            .expect("src");
        assert_eq!(src.stats.descendant_folders, 1);
        assert_eq!(src.stats.descendant_files, 2);
        assert_eq!(src.stats.subtree_folders, 2);
        assert_eq!(src.stats.subtree_files, 2);
    }
}
