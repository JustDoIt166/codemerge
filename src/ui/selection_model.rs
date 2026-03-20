use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::domain::FileEntry;
use crate::ui::state::SelectionState;

pub struct SelectionModel {
    state: SelectionState,
}

impl SelectionModel {
    pub fn new() -> Self {
        Self {
            state: SelectionState {
                dedupe_exact_path: true,
                ..SelectionState::default()
            },
        }
    }

    #[cfg(test)]
    pub fn state(&self) -> &SelectionState {
        &self.state
    }

    pub fn snapshot(&self) -> SelectionState {
        SelectionState {
            dedupe_exact_path: self.state.dedupe_exact_path,
            selected_folder: self.state.selected_folder.clone(),
            selected_files: self.state.selected_files.clone(),
            gitignore_file: self.state.gitignore_file.clone(),
            gitignore_rules: self.state.gitignore_rules.clone(),
        }
    }

    pub fn clear(&mut self) {
        self.state = SelectionState {
            dedupe_exact_path: self.state.dedupe_exact_path,
            ..SelectionState::default()
        };
    }

    pub fn has_inputs(&self) -> bool {
        self.state.selected_folder.is_some() || !self.state.selected_files.is_empty()
    }

    pub fn set_selected_folder(&mut self, path: PathBuf, gitignore_rules: Vec<String>) {
        self.state.selected_folder = Some(path);
        self.state.gitignore_rules = gitignore_rules;
    }

    pub fn add_selected_files(&mut self, files: Vec<FileEntry>) {
        let mut existing = self
            .state
            .selected_files
            .iter()
            .map(|entry| entry.path.to_string_lossy().to_string())
            .collect::<BTreeSet<_>>();

        for entry in files {
            let key = entry.path.to_string_lossy().to_string();
            if self.state.dedupe_exact_path && !existing.insert(key) {
                continue;
            }
            self.state.selected_files.push(entry);
        }
    }

    pub fn set_gitignore_file(&mut self, path: Option<PathBuf>) {
        self.state.gitignore_file = path;
    }

    pub fn set_dedupe_exact_path(&mut self, checked: bool) {
        self.state.dedupe_exact_path = checked;
    }
}

#[cfg(test)]
mod tests {
    use super::SelectionModel;
    use crate::domain::FileEntry;
    use std::path::PathBuf;

    #[test]
    fn add_selected_files_dedupes_when_enabled() {
        let mut model = SelectionModel::new();
        let entry = FileEntry {
            path: PathBuf::from("src/main.rs"),
            name: "main.rs".into(),
            size: 1,
        };
        model.add_selected_files(vec![entry.clone(), entry]);
        assert_eq!(model.state().selected_files.len(), 1);
    }

    #[test]
    fn clear_keeps_dedupe_toggle() {
        let mut model = SelectionModel::new();
        model.set_dedupe_exact_path(false);
        model.set_selected_folder(PathBuf::from("root"), vec!["node_modules".into()]);
        model.clear();
        assert!(!model.state().dedupe_exact_path);
        assert!(model.state().selected_folder.is_none());
        assert!(model.state().gitignore_rules.is_empty());
    }
}
