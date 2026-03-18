use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::domain::PreflightStats;
use crate::error::AppError;
use crate::processor::walker::{WalkerOptions, collect_candidates_with_progress};
use crate::services::settings;

#[derive(Debug, Clone)]
pub enum PreflightEvent {
    Started {
        revision: u64,
    },
    Progress {
        revision: u64,
        scanned: usize,
        candidates: usize,
        skipped: usize,
    },
    Completed {
        revision: u64,
        stats: PreflightStats,
    },
    Failed {
        revision: u64,
        error: AppError,
    },
}

#[derive(Debug, Clone)]
pub struct PreflightRequest {
    pub revision: u64,
    pub selected_folder: Option<PathBuf>,
    pub selected_files: Vec<PathBuf>,
    pub folder_blacklist: Vec<String>,
    pub ext_blacklist: Vec<String>,
}

pub fn start(request: PreflightRequest) -> Receiver<PreflightEvent> {
    let config = settings::load();
    start_with_options(
        request,
        WalkerOptions {
            use_gitignore: config.options.use_gitignore,
            ignore_git: config.options.ignore_git,
        },
    )
}

pub fn start_with_options(
    request: PreflightRequest,
    options: WalkerOptions,
) -> Receiver<PreflightEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let revision = request.revision;
        let _ = tx.send(PreflightEvent::Started { revision });
        let progress_tx = tx.clone();
        let result = std::panic::catch_unwind(|| {
            collect_candidates_with_progress(
                request.selected_folder.as_ref(),
                &request.selected_files,
                &request.folder_blacklist,
                &request.ext_blacklist,
                options,
                move |scanned, candidates, skipped| {
                    let _ = progress_tx.send(PreflightEvent::Progress {
                        revision,
                        scanned,
                        candidates,
                        skipped,
                    });
                },
            )
        });

        match result {
            Ok(out) => {
                let stats = PreflightStats {
                    total_files: out.candidates.len() + out.skipped,
                    skipped_files: out.skipped,
                    to_process_files: out.candidates.len(),
                    scanned_entries: out.candidates.len() + out.skipped,
                    is_scanning: false,
                };
                let _ = tx.send(PreflightEvent::Completed { revision, stats });
            }
            Err(_) => {
                let _ = tx.send(PreflightEvent::Failed {
                    revision,
                    error: AppError::new("preflight failed"),
                });
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use tempfile::tempdir;

    use super::{PreflightEvent, PreflightRequest, start_with_options};
    use crate::processor::walker::WalkerOptions;

    #[test]
    fn completion_scanned_entries_does_not_go_backwards() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        for index in 0..205 {
            fs::write(root.join(format!("file_{index:03}.txt")), "content").expect("write file");
        }

        let rx = start_with_options(
            PreflightRequest {
                revision: 1,
                selected_folder: Some(root.to_path_buf()),
                selected_files: Vec::new(),
                folder_blacklist: Vec::new(),
                ext_blacklist: Vec::new(),
            },
            WalkerOptions {
                use_gitignore: false,
                ignore_git: false,
            },
        );

        let mut last_progress = 0usize;
        let completed = loop {
            let event = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("preflight event");
            match event {
                PreflightEvent::Started { .. } => {}
                PreflightEvent::Progress { scanned, .. } => {
                    last_progress = last_progress.max(scanned);
                }
                PreflightEvent::Completed { stats, .. } => {
                    break stats;
                }
                PreflightEvent::Failed { error, .. } => panic!("unexpected failure: {error}"),
            }
        };
        assert!(completed.scanned_entries >= last_progress);
        assert_eq!(completed.scanned_entries, completed.total_files);
    }

    #[test]
    fn start_with_options_can_disable_gitignore_rules() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(root.join(".gitignore"), "ignored.txt\n").expect("write gitignore");
        fs::write(root.join("ignored.txt"), "ignored").expect("write ignored");
        fs::write(root.join("kept.txt"), "kept").expect("write kept");

        let rx = start_with_options(
            PreflightRequest {
                revision: 2,
                selected_folder: Some(root.to_path_buf()),
                selected_files: Vec::new(),
                folder_blacklist: Vec::new(),
                ext_blacklist: Vec::new(),
            },
            WalkerOptions {
                use_gitignore: false,
                ignore_git: false,
            },
        );

        let mut stats = None;
        while let Ok(event) = rx.recv_timeout(Duration::from_secs(10)) {
            if let PreflightEvent::Completed {
                stats: completed, ..
            } = event
            {
                stats = Some(completed);
                break;
            }
        }

        let stats = stats.expect("completed stats");
        assert_eq!(stats.to_process_files, 3);
        assert_eq!(stats.scanned_entries, 3);
    }
}
