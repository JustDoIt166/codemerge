use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::domain::FileEntry;
use crate::domain::TemporaryWhitelistMode;
use crate::ui::state::SelectionState;
use crate::ui::workspace::BlacklistItemKind;

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
            temp_folder_blacklist: self.state.temp_folder_blacklist.clone(),
            temp_ext_blacklist: self.state.temp_ext_blacklist.clone(),
            temp_folder_whitelist: self.state.temp_folder_whitelist.clone(),
            temp_ext_whitelist: self.state.temp_ext_whitelist.clone(),
            temp_whitelist_mode: self.state.temp_whitelist_mode,
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

    pub fn set_selected_folder_gitignore_rules(&mut self, gitignore_rules: Vec<String>) -> bool {
        if self.state.gitignore_rules == gitignore_rules {
            return false;
        }
        self.state.gitignore_rules = gitignore_rules;
        true
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

    pub fn add_temporary_blacklist_tokens(&mut self, tokens: &[String], as_ext: bool) -> usize {
        let mut added = 0;
        for token in tokens {
            if as_ext {
                let normalized = crate::processor::walker::normalize_ext(token);
                if push_unique(&mut self.state.temp_ext_blacklist, normalized) {
                    added += 1;
                }
            } else if push_unique(&mut self.state.temp_folder_blacklist, token.clone()) {
                added += 1;
            }
        }
        added
    }

    pub fn append_temporary_gitignore_rules(&mut self, rules: Vec<String>) -> usize {
        let mut added = 0;
        for rule in rules {
            if push_unique(&mut self.state.temp_folder_blacklist, rule) {
                added += 1;
            }
        }
        added
    }

    pub fn add_temporary_whitelist_tokens(&mut self, tokens: &[String], as_ext: bool) -> usize {
        let mut added = 0;
        for token in tokens {
            if as_ext {
                let normalized = crate::processor::walker::normalize_ext(token);
                if push_unique(&mut self.state.temp_ext_whitelist, normalized) {
                    added += 1;
                }
            } else if push_unique(&mut self.state.temp_folder_whitelist, token.clone()) {
                added += 1;
            }
        }
        added
    }

    pub fn remove_temporary_blacklist_item(&mut self, kind: BlacklistItemKind, value: &str) {
        match kind {
            BlacklistItemKind::Folder => {
                self.state
                    .temp_folder_blacklist
                    .retain(|item| item != value);
            }
            BlacklistItemKind::Ext => {
                self.state.temp_ext_blacklist.retain(|item| item != value);
            }
        }
    }

    pub fn remove_temporary_whitelist_item(&mut self, kind: BlacklistItemKind, value: &str) {
        match kind {
            BlacklistItemKind::Folder => {
                self.state
                    .temp_folder_whitelist
                    .retain(|item| item != value);
            }
            BlacklistItemKind::Ext => {
                self.state.temp_ext_whitelist.retain(|item| item != value);
            }
        }
    }

    pub fn clear_temporary_blacklist(&mut self) -> bool {
        let changed = !self.state.temp_folder_blacklist.is_empty()
            || !self.state.temp_ext_blacklist.is_empty();
        self.state.temp_folder_blacklist.clear();
        self.state.temp_ext_blacklist.clear();
        changed
    }

    pub fn clear_temporary_whitelist(&mut self) -> bool {
        let changed = !self.state.temp_folder_whitelist.is_empty()
            || !self.state.temp_ext_whitelist.is_empty()
            || self.state.temp_whitelist_mode != TemporaryWhitelistMode::default();
        self.state.temp_folder_whitelist.clear();
        self.state.temp_ext_whitelist.clear();
        self.state.temp_whitelist_mode = TemporaryWhitelistMode::default();
        changed
    }

    pub fn set_temporary_whitelist_mode(&mut self, mode: TemporaryWhitelistMode) -> bool {
        let changed = self.state.temp_whitelist_mode != mode;
        self.state.temp_whitelist_mode = mode;
        changed
    }

    pub fn clear_temporary_merge_filters(&mut self) -> bool {
        let cleared_blacklist = self.clear_temporary_blacklist();
        let cleared_whitelist = self.clear_temporary_whitelist();
        let had_gitignore_file = self.state.gitignore_file.is_some();
        let changed = cleared_blacklist || cleared_whitelist || had_gitignore_file;
        self.state.gitignore_file = None;
        changed
    }
}

fn push_unique(values: &mut Vec<String>, value: String) -> bool {
    if values.contains(&value) {
        return false;
    }
    values.push(value);
    true
}

#[cfg(test)]
mod tests {
    use super::SelectionModel;
    use crate::domain::{FileEntry, TemporaryWhitelistMode};
    use crate::ui::workspace::BlacklistItemKind;
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
        assert!(model.state().temp_folder_blacklist.is_empty());
        assert!(model.state().temp_ext_blacklist.is_empty());
        assert!(model.state().temp_folder_whitelist.is_empty());
        assert!(model.state().temp_ext_whitelist.is_empty());
        assert_eq!(
            model.state().temp_whitelist_mode,
            TemporaryWhitelistMode::WhitelistThenBlacklist
        );
    }

    #[test]
    fn temporary_blacklist_normalizes_and_dedupes() {
        let mut model = SelectionModel::new();

        let added_folders = model.add_temporary_blacklist_tokens(
            &["target".into(), "target".into(), "build".into()],
            false,
        );
        let added_exts = model
            .add_temporary_blacklist_tokens(&["log".into(), ".tmp".into(), ".log".into()], true);

        assert_eq!(added_folders, 2);
        assert_eq!(added_exts, 2);
        assert_eq!(
            model.state().temp_folder_blacklist,
            vec!["target".to_string(), "build".to_string()]
        );
        assert_eq!(
            model.state().temp_ext_blacklist,
            vec![".log".to_string(), ".tmp".to_string()]
        );
    }

    #[test]
    fn clear_temporary_merge_filters_keeps_selected_inputs() {
        let mut model = SelectionModel::new();
        model.set_selected_folder(PathBuf::from("root"), vec!["node_modules".into()]);
        model.set_gitignore_file(Some(PathBuf::from(".gitignore")));
        model.add_temporary_blacklist_tokens(&["target".into()], false);
        model.add_temporary_blacklist_tokens(&["tmp".into()], true);
        model.add_temporary_whitelist_tokens(&["src".into()], false);
        model.add_temporary_whitelist_tokens(&["rs".into()], true);
        assert!(model.set_temporary_whitelist_mode(TemporaryWhitelistMode::WhitelistOnly));

        let changed = model.clear_temporary_merge_filters();

        assert!(changed);
        assert_eq!(model.state().selected_folder, Some(PathBuf::from("root")));
        assert!(model.state().gitignore_file.is_none());
        assert!(
            model
                .state()
                .gitignore_rules
                .contains(&"node_modules".to_string())
        );
        assert!(model.state().temp_folder_blacklist.is_empty());
        assert!(model.state().temp_ext_blacklist.is_empty());
        assert!(model.state().temp_folder_whitelist.is_empty());
        assert!(model.state().temp_ext_whitelist.is_empty());
        assert_eq!(
            model.state().temp_whitelist_mode,
            TemporaryWhitelistMode::WhitelistThenBlacklist
        );
    }

    #[test]
    fn selected_folder_gitignore_rules_update_only_when_changed() {
        let mut model = SelectionModel::new();
        model.set_selected_folder(PathBuf::from("root"), vec!["target".into()]);

        assert!(!model.set_selected_folder_gitignore_rules(vec!["target".into()]));
        assert!(model.set_selected_folder_gitignore_rules(vec!["dist".into()]));
        assert_eq!(model.state().gitignore_rules, vec!["dist".to_string()]);
    }

    #[test]
    fn remove_temporary_blacklist_item_updates_matching_collection() {
        let mut model = SelectionModel::new();
        model.add_temporary_blacklist_tokens(&["target".into()], false);
        model.add_temporary_blacklist_tokens(&["log".into()], true);

        model.remove_temporary_blacklist_item(BlacklistItemKind::Folder, "target");
        model.remove_temporary_blacklist_item(BlacklistItemKind::Ext, ".log");

        assert!(model.state().temp_folder_blacklist.is_empty());
        assert!(model.state().temp_ext_blacklist.is_empty());
    }

    #[test]
    fn temporary_whitelist_normalizes_dedupes_and_tracks_mode() {
        let mut model = SelectionModel::new();

        let added_folders = model.add_temporary_whitelist_tokens(
            &["src".into(), "src".into(), "workspace".into()],
            false,
        );
        let added_exts =
            model.add_temporary_whitelist_tokens(&["rs".into(), ".md".into(), ".rs".into()], true);

        assert_eq!(added_folders, 2);
        assert_eq!(added_exts, 2);
        assert_eq!(
            model.state().temp_folder_whitelist,
            vec!["src".to_string(), "workspace".to_string()]
        );
        assert_eq!(
            model.state().temp_ext_whitelist,
            vec![".rs".to_string(), ".md".to_string()]
        );
        assert!(model.set_temporary_whitelist_mode(TemporaryWhitelistMode::WhitelistOnly));
        assert_eq!(
            model.state().temp_whitelist_mode,
            TemporaryWhitelistMode::WhitelistOnly
        );
        assert!(!model.set_temporary_whitelist_mode(TemporaryWhitelistMode::WhitelistOnly));
    }

    #[test]
    fn remove_temporary_whitelist_item_updates_matching_collection() {
        let mut model = SelectionModel::new();
        model.add_temporary_whitelist_tokens(&["src".into()], false);
        model.add_temporary_whitelist_tokens(&["rs".into()], true);

        model.remove_temporary_whitelist_item(BlacklistItemKind::Folder, "src");
        model.remove_temporary_whitelist_item(BlacklistItemKind::Ext, ".rs");

        assert!(model.state().temp_folder_whitelist.is_empty());
        assert!(model.state().temp_ext_whitelist.is_empty());
    }
}
