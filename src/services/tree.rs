use std::collections::BTreeMap;

use crate::domain::TreeNode;
use crate::processor::walker::CandidateFile;

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
    use super::build_tree_nodes;
    use crate::processor::walker::CandidateFile;
    use std::path::PathBuf;

    #[test]
    fn builds_hierarchical_nodes() {
        let nodes = build_tree_nodes(&[
            CandidateFile {
                absolute: PathBuf::from("a"),
                relative: "src/main.rs".to_string(),
            },
            CandidateFile {
                absolute: PathBuf::from("b"),
                relative: "src/lib.rs".to_string(),
            },
            CandidateFile {
                absolute: PathBuf::from("c"),
                relative: "README.md".to_string(),
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
}
