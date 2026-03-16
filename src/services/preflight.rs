use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::domain::PreflightStats;
use crate::processor::walker::collect_candidates_with_progress;

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
        error: String,
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
                    error: "preflight failed".to_string(),
                });
            }
        }
    });
    rx
}
