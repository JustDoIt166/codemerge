use crate::domain::{ProcessResult, ResultTab};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResultSaveState {
    #[default]
    Idle,
    Saving,
    Saved,
    Failed,
}

#[derive(Default, Clone)]
pub struct ResultState {
    pub result: Option<ProcessResult>,
    pub active_tab: ResultTab,
    pub preview_row_count: usize,
    pub result_revision: u64,
    pub preview_rows_revision: u64,
    pub save_state: ResultSaveState,
    save_revision: u64,
}

pub struct ResultModel {
    state: ResultState,
}

impl ResultModel {
    pub fn new() -> Self {
        Self {
            state: ResultState::default(),
        }
    }

    pub fn state(&self) -> &ResultState {
        &self.state
    }

    pub fn clear(&mut self) {
        self.state.result = None;
        self.state.active_tab = ResultTab::Tree;
        self.state.preview_row_count = 0;
        self.state.result_revision = self.state.result_revision.wrapping_add(1);
        self.state.preview_rows_revision = self.state.preview_rows_revision.wrapping_add(1);
        self.state.save_state = ResultSaveState::Idle;
        self.state.save_revision = self.state.save_revision.wrapping_add(1);
    }

    pub fn set_result(&mut self, result: ProcessResult) {
        self.state.result = Some(result);
        self.state.active_tab = ResultTab::Tree;
        self.state.preview_row_count = 0;
        self.state.result_revision = self.state.result_revision.wrapping_add(1);
        self.state.preview_rows_revision = self.state.preview_rows_revision.wrapping_add(1);
        self.state.save_state = ResultSaveState::Idle;
        self.state.save_revision = self.state.save_revision.wrapping_add(1);
    }

    pub fn set_active_tab(&mut self, tab: ResultTab) {
        self.state.active_tab = tab;
    }

    pub fn set_preview_row_count(&mut self, row_count: usize) {
        self.state.preview_row_count = row_count;
        self.state.preview_rows_revision = self.state.preview_rows_revision.wrapping_add(1);
    }

    pub fn begin_save(&mut self) -> Option<u64> {
        if self.state.save_state == ResultSaveState::Saving {
            return None;
        }
        self.state.save_revision = self.state.save_revision.wrapping_add(1);
        self.state.save_state = ResultSaveState::Saving;
        Some(self.state.save_revision)
    }

    pub fn finish_save(&mut self, revision: u64, succeeded: bool) -> bool {
        if self.state.save_revision != revision || self.state.save_state != ResultSaveState::Saving
        {
            return false;
        }
        self.state.save_state = if succeeded {
            ResultSaveState::Saved
        } else {
            ResultSaveState::Failed
        };
        true
    }

    pub fn cancel_save(&mut self, revision: u64) -> bool {
        if self.state.save_revision != revision || self.state.save_state != ResultSaveState::Saving
        {
            return false;
        }
        self.state.save_state = ResultSaveState::Idle;
        true
    }

    pub fn has_content_result(&self) -> bool {
        self.state.result.as_ref().is_some_and(|result| {
            result.merged_content_path.is_some() || !result.preview_files.is_empty()
        })
    }

    pub fn is_tree_only_result(&self) -> bool {
        self.state.result.as_ref().is_some_and(|result| {
            result.merged_content_path.is_none() && result.preview_files.is_empty()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ResultModel;
    use crate::domain::{PreviewFileEntry, ProcessResult, ResultTab};
    use crate::processor::stats::ProcessingStats;
    use std::path::PathBuf;

    #[test]
    fn set_result_resets_active_tab_preview_count_and_save_state() {
        let mut model = ResultModel::new();
        model.set_active_tab(ResultTab::Content);
        model.set_preview_row_count(1);
        assert!(model.begin_save().is_some());

        model.set_result(ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: Vec::new(),
            process_dir: None,
            merged_content_path: Some(PathBuf::from("merged.txt")),
            suggested_result_name: "workspace-20260319.txt".into(),
            file_details: Vec::new(),
            preview_files: vec![PreviewFileEntry {
                id: 1,
                display_path: "file.rs".into(),
                chars: 1,
                tokens: 1,
                preview_blob_path: PathBuf::from("preview.txt"),
                byte_len: 1,
                archive: None,
            }],
            preview_blob_dir: None,
        });

        assert_eq!(model.state().active_tab, ResultTab::Tree);
        assert_eq!(model.state().preview_row_count, 0);
        assert_eq!(model.state().save_state, super::ResultSaveState::Idle);
    }

    #[test]
    fn result_shape_flags_follow_current_result() {
        let mut model = ResultModel::new();
        assert!(!model.has_content_result());
        assert!(!model.is_tree_only_result());

        model.set_result(ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: Vec::new(),
            process_dir: None,
            merged_content_path: None,
            suggested_result_name: "workspace-20260319.txt".into(),
            file_details: Vec::new(),
            preview_files: Vec::new(),
            preview_blob_dir: None,
        });

        assert!(!model.has_content_result());
        assert!(model.is_tree_only_result());
    }

    #[test]
    fn revisions_advance_when_result_or_preview_rows_change() {
        let mut model = ResultModel::new();
        let initial_result_revision = model.state().result_revision;
        let initial_preview_rows_revision = model.state().preview_rows_revision;

        model.set_preview_row_count(1);
        assert_eq!(model.state().result_revision, initial_result_revision);
        assert_eq!(
            model.state().preview_rows_revision,
            initial_preview_rows_revision + 1
        );

        model.set_result(ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: Vec::new(),
            process_dir: None,
            merged_content_path: Some(PathBuf::from("merged.txt")),
            suggested_result_name: "workspace-20260319.txt".into(),
            file_details: Vec::new(),
            preview_files: Vec::new(),
            preview_blob_dir: None,
        });
        assert_eq!(model.state().result_revision, initial_result_revision + 1);
        assert_eq!(
            model.state().preview_rows_revision,
            initial_preview_rows_revision + 2
        );
    }

    #[test]
    fn save_state_prevents_overlapping_saves_and_records_outcome() {
        let mut model = ResultModel::new();

        let failed_revision = model.begin_save().expect("save should start");
        assert!(model.begin_save().is_none());
        assert_eq!(model.state().save_state, super::ResultSaveState::Saving);

        assert!(model.finish_save(failed_revision, false));
        assert_eq!(model.state().save_state, super::ResultSaveState::Failed);
        let saved_revision = model.begin_save().expect("retry should start");
        assert!(model.finish_save(saved_revision, true));
        assert_eq!(model.state().save_state, super::ResultSaveState::Saved);

        let cancelled_revision = model.begin_save().expect("another save should start");
        assert!(model.cancel_save(cancelled_revision));
        assert_eq!(model.state().save_state, super::ResultSaveState::Idle);
    }

    #[test]
    fn replacing_result_ignores_stale_save_completion() {
        let mut model = ResultModel::new();
        let revision = model.begin_save().expect("save should start");
        model.clear();

        assert!(!model.finish_save(revision, true));
        assert_eq!(model.state().save_state, super::ResultSaveState::Idle);
    }
}
