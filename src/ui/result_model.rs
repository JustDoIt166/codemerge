use crate::domain::ProcessResult;
use crate::ui::view_model::ResultTab;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResultSaveState {
    #[default]
    Idle,
    Saving,
    Saved,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ResultCopyState {
    #[default]
    Idle,
    Confirming {
        revision: u64,
        total: u64,
    },
    Copying {
        revision: u64,
        read: u64,
        total: u64,
    },
    Copied {
        revision: u64,
    },
    Failed {
        revision: u64,
        error: String,
    },
    Cancelled {
        revision: u64,
    },
}

#[derive(Default, Clone)]
pub struct ResultState {
    pub result: Option<Arc<ProcessResult>>,
    pub active_tab: ResultTab,
    pub preview_row_count: usize,
    pub result_revision: u64,
    pub preview_rows_revision: u64,
    pub save_state: ResultSaveState,
    pub copy_state: ResultCopyState,
    save_revision: u64,
    copy_revision: u64,
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
        self.state.copy_state = ResultCopyState::Idle;
        self.state.save_revision = self.state.save_revision.wrapping_add(1);
        self.state.copy_revision = self.state.copy_revision.wrapping_add(1);
    }

    pub fn set_result(&mut self, result: impl Into<Arc<ProcessResult>>) {
        self.state.result = Some(result.into());
        self.state.active_tab = ResultTab::Tree;
        self.state.preview_row_count = 0;
        self.state.result_revision = self.state.result_revision.wrapping_add(1);
        self.state.preview_rows_revision = self.state.preview_rows_revision.wrapping_add(1);
        self.state.save_state = ResultSaveState::Idle;
        self.state.copy_state = ResultCopyState::Idle;
        self.state.save_revision = self.state.save_revision.wrapping_add(1);
        self.state.copy_revision = self.state.copy_revision.wrapping_add(1);
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

    pub fn prepare_copy(&mut self, total: u64, requires_confirmation: bool) -> u64 {
        self.state.copy_revision = self.state.copy_revision.wrapping_add(1);
        let revision = self.state.copy_revision;
        self.state.copy_state = if requires_confirmation {
            ResultCopyState::Confirming { revision, total }
        } else {
            ResultCopyState::Copying {
                revision,
                read: 0,
                total,
            }
        };
        revision
    }

    pub fn confirm_copy(&mut self, revision: u64) -> bool {
        let ResultCopyState::Confirming {
            revision: current,
            total,
        } = self.state.copy_state
        else {
            return false;
        };
        if current != revision {
            return false;
        }
        self.state.copy_state = ResultCopyState::Copying {
            revision,
            read: 0,
            total,
        };
        true
    }

    pub fn update_copy_progress(&mut self, revision: u64, read: u64, total: u64) -> bool {
        let ResultCopyState::Copying {
            revision: current, ..
        } = self.state.copy_state
        else {
            return false;
        };
        if current != revision {
            return false;
        }
        self.state.copy_state = ResultCopyState::Copying {
            revision,
            read: read.min(total),
            total,
        };
        true
    }

    pub fn finish_copy(&mut self, revision: u64) -> bool {
        if !self.copy_is_current(revision) {
            return false;
        }
        self.state.copy_state = ResultCopyState::Copied { revision };
        true
    }

    pub fn fail_copy(&mut self, revision: u64, error: String) -> bool {
        if !self.copy_is_current(revision) {
            return false;
        }
        self.state.copy_state = ResultCopyState::Failed { revision, error };
        true
    }

    pub fn cancel_copy(&mut self, revision: u64) -> bool {
        if !self.copy_is_current(revision) {
            return false;
        }
        self.state.copy_state = ResultCopyState::Cancelled { revision };
        true
    }

    fn copy_is_current(&self, revision: u64) -> bool {
        matches!(
            self.state.copy_state,
            ResultCopyState::Confirming { revision: current, .. }
                | ResultCopyState::Copying { revision: current, .. }
                if current == revision
        )
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
    use crate::domain::{PreviewFileEntry, ProcessResult};
    use crate::processor::stats::ProcessingStats;
    use crate::ui::view_model::ResultTab;
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
            merged_content_bytes: 0,
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
            merged_content_bytes: 0,
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
            merged_content_bytes: 0,
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

    #[test]
    fn stale_copy_events_cannot_replace_a_new_copy_job() {
        let mut model = ResultModel::new();
        let stale_revision = model.prepare_copy(64, false);
        let current_revision = model.prepare_copy(128, false);

        assert!(!model.update_copy_progress(stale_revision, 64, 64));
        assert!(!model.finish_copy(stale_revision));
        assert!(model.update_copy_progress(current_revision, 32, 128));
        assert_eq!(
            model.state().copy_state,
            super::ResultCopyState::Copying {
                revision: current_revision,
                read: 32,
                total: 128,
            }
        );
    }

    #[test]
    fn large_copy_requires_confirmation_before_progress_is_accepted() {
        let mut model = ResultModel::new();
        let revision = model.prepare_copy(64 * 1024 * 1024, true);

        assert!(matches!(
            model.state().copy_state,
            super::ResultCopyState::Confirming { .. }
        ));
        assert!(!model.update_copy_progress(revision, 1, 64 * 1024 * 1024));
        assert!(model.confirm_copy(revision));
        assert!(model.update_copy_progress(revision, 1024 * 1024, 64 * 1024 * 1024));
    }
}
