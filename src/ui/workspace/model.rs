use crate::domain::{PreviewRowViewModel, ProcessResult};
use crate::services::preflight::PreflightEvent;
use crate::ui::state::{ProcessState, ProcessUiStatus};

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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{apply_preflight_event, build_preview_table_model};
    use crate::domain::{PreflightStats, PreviewFileEntry, ProcessResult};
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
        let result = ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: Vec::new(),
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
        };

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
}
