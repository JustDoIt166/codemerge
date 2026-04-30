use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::domain::PreflightStats;
use crate::domain::TemporaryWhitelistMode;
use crate::error::AppError;
use crate::processor::walker::{
    WalkerFilterRules, WalkerOptions, collect_candidates_with_progress,
};
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
    pub folder_whitelist: Vec<String>,
    pub ext_whitelist: Vec<String>,
    pub whitelist_mode: TemporaryWhitelistMode,
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
                WalkerFilterRules {
                    folder_blacklist: &request.folder_blacklist,
                    ext_blacklist: &request.ext_blacklist,
                    folder_whitelist: &request.folder_whitelist,
                    ext_whitelist: &request.ext_whitelist,
                    whitelist_mode: request.whitelist_mode,
                },
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
    use std::io::Write;
    use std::time::Duration;

    use tempfile::tempdir;
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    use super::{PreflightEvent, PreflightRequest, start_with_options};
    use crate::domain::TemporaryWhitelistMode;
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
                folder_whitelist: Vec::new(),
                ext_whitelist: Vec::new(),
                whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
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
                folder_whitelist: Vec::new(),
                ext_whitelist: Vec::new(),
                whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
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

    #[test]
    fn start_with_options_keeps_explicit_selected_file_even_if_blacklisted() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("blocked.log");
        fs::write(&file_path, "selected file").expect("write selected file");

        let rx = start_with_options(
            PreflightRequest {
                revision: 3,
                selected_folder: None,
                selected_files: vec![file_path],
                folder_blacklist: vec!["blocked.log".to_string()],
                ext_blacklist: vec![".log".to_string()],
                folder_whitelist: Vec::new(),
                ext_whitelist: Vec::new(),
                whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            },
            WalkerOptions {
                use_gitignore: false,
                ignore_git: false,
            },
        );

        let stats = loop {
            let event = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("preflight event");
            match event {
                PreflightEvent::Completed { stats, .. } => break stats,
                PreflightEvent::Failed { error, .. } => panic!("unexpected failure: {error}"),
                PreflightEvent::Started { .. } | PreflightEvent::Progress { .. } => {}
            }
        };

        assert_eq!(stats.to_process_files, 1);
        assert_eq!(stats.skipped_files, 0);
        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.scanned_entries, 1);
    }

    #[test]
    fn start_with_options_selected_zip_honors_blacklist_inside_archive() {
        let dir = tempdir().expect("tempdir");
        let zip_path = dir.path().join("bundle.zip");
        write_test_zip(
            &zip_path,
            &[
                ("src/lib.rs", "pub fn zipped() {}\n"),
                ("README.md", "# zipped\n"),
                ("assets/logo.png", "binary"),
            ],
        );

        let rx = start_with_options(
            PreflightRequest {
                revision: 4,
                selected_folder: None,
                selected_files: vec![zip_path],
                folder_blacklist: vec!["src".to_string()],
                ext_blacklist: vec![".png".to_string()],
                folder_whitelist: Vec::new(),
                ext_whitelist: Vec::new(),
                whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            },
            WalkerOptions {
                use_gitignore: false,
                ignore_git: false,
            },
        );

        let stats = loop {
            let event = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("preflight event");
            match event {
                PreflightEvent::Completed { stats, .. } => break stats,
                PreflightEvent::Failed { error, .. } => panic!("unexpected failure: {error}"),
                PreflightEvent::Started { .. } | PreflightEvent::Progress { .. } => {}
            }
        };

        assert_eq!(stats.to_process_files, 1);
        assert_eq!(stats.skipped_files, 2);
        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.scanned_entries, 3);
    }

    #[test]
    fn start_with_options_whitelist_then_blacklist_updates_stats() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::create_dir_all(root.join("docs")).expect("mkdir docs");
        fs::write(root.join("src/lib.rs"), "lib").expect("write lib");
        fs::write(root.join("docs/guide.md"), "guide").expect("write guide");

        let rx = start_with_options(
            PreflightRequest {
                revision: 5,
                selected_folder: Some(root.to_path_buf()),
                selected_files: Vec::new(),
                folder_blacklist: Vec::new(),
                ext_blacklist: Vec::new(),
                folder_whitelist: vec!["src".to_string()],
                ext_whitelist: Vec::new(),
                whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            },
            WalkerOptions {
                use_gitignore: false,
                ignore_git: false,
            },
        );

        let stats = loop {
            let event = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("preflight event");
            match event {
                PreflightEvent::Completed { stats, .. } => break stats,
                PreflightEvent::Failed { error, .. } => panic!("unexpected failure: {error}"),
                PreflightEvent::Started { .. } | PreflightEvent::Progress { .. } => {}
            }
        };

        assert_eq!(stats.to_process_files, 1);
        assert_eq!(stats.skipped_files, 1);
        assert_eq!(stats.total_files, 2);
    }

    fn write_test_zip(path: &std::path::Path, files: &[(&str, &str)]) {
        let file = fs::File::create(path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, content) in files {
            zip.start_file(name, options).expect("start file");
            zip.write_all(content.as_bytes()).expect("write entry");
        }
        zip.finish().expect("finish zip");
    }
}
