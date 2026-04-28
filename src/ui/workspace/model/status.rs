use std::collections::BTreeSet;

use gpui::{Decorations, SharedString};

use crate::domain::{Language, ProcessRecord, ProcessResult, ProcessStatus};
use crate::services::preflight::PreflightEvent;
use crate::ui::state::{ProcessState, ProcessUiStatus};
use crate::utils::{app_metadata, i18n::tr};

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::ui::workspace) struct ArchiveResultSummary {
    pub archives: usize,
    pub entries: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum WindowChromeMode {
    CustomTitleBar,
    CompactHeaderFallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum WorkspaceChromeTone {
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum WindowZoomAction {
    Maximize,
    Restore,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct WorkspaceChromeViewModel {
    pub title: SharedString,
    pub status_label: SharedString,
    pub status_message: SharedString,
    pub status_tone: WorkspaceChromeTone,
    pub version_label: SharedString,
    pub repository_tooltip: SharedString,
    pub language_button_label: SharedString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct StatusMetricViewModel {
    pub label: SharedString,
    pub value: SharedString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct StatusInfoViewModel {
    pub label: SharedString,
    pub value: SharedString,
}

#[derive(Clone, Debug)]
pub(in crate::ui::workspace) struct StatusProgressViewModel {
    pub value_text: SharedString,
    pub fill_ratio: f32,
    pub elapsed_value: SharedString,
    pub current_file: SharedString,
}

#[derive(Clone, Debug)]
pub(in crate::ui::workspace) struct StatusPanelViewModel {
    pub summary_metrics: [StatusMetricViewModel; 3],
    pub result_metrics: [StatusMetricViewModel; 3],
    pub status: ProcessUiStatus,
    pub status_title: SharedString,
    pub status_message: SharedString,
    pub archive_summary: Option<StatusInfoViewModel>,
    pub progress: StatusProgressViewModel,
    pub activity_rows: Vec<ProcessRecord>,
}

pub(in crate::ui::workspace) fn resolve_window_chrome_mode(
    decorations: Decorations,
    is_windows: bool,
    is_linux: bool,
    is_macos: bool,
    prefer_custom_titlebar: bool,
) -> WindowChromeMode {
    if !prefer_custom_titlebar {
        return WindowChromeMode::CompactHeaderFallback;
    }
    if is_windows || is_macos {
        return WindowChromeMode::CustomTitleBar;
    }

    if is_linux {
        return match decorations {
            Decorations::Client { .. } => WindowChromeMode::CustomTitleBar,
            Decorations::Server => WindowChromeMode::CompactHeaderFallback,
        };
    }

    WindowChromeMode::CompactHeaderFallback
}

pub(in crate::ui::workspace) fn build_workspace_chrome_view_model(
    process: &ProcessState,
    language: Language,
    merged_file_size_hint: Option<String>,
) -> WorkspaceChromeViewModel {
    WorkspaceChromeViewModel {
        title: SharedString::from("CodeMerge"),
        status_label: SharedString::from(process_status_title(process.ui_status, language)),
        status_message: SharedString::from(process_status_message(
            process,
            language,
            merged_file_size_hint,
        )),
        status_tone: workspace_chrome_tone(process.ui_status),
        version_label: SharedString::from(format!(
            "{}{}",
            tr(language, "version_prefix"),
            app_metadata::version()
        )),
        repository_tooltip: SharedString::from(format!(
            "{}{}",
            tr(language, "repository_tooltip"),
            app_metadata::repository_url()
        )),
        language_button_label: SharedString::from(match language {
            Language::Zh => tr(language, "language_switch_en"),
            Language::En => tr(language, "language_switch_zh"),
        }),
    }
}

pub(in crate::ui::workspace) fn build_status_panel_view_model(
    process: &ProcessState,
    result: Option<&ProcessResult>,
    language: Language,
    merged_file_size_hint: Option<String>,
) -> StatusPanelViewModel {
    let archive_totals = summarize_archive_entries(result);
    let result_stats = result.map(|result| &result.stats);
    let failed_count = process
        .processing_records
        .iter()
        .filter(|record| matches!(record.status, ProcessStatus::Failed))
        .count();
    let activity_rows = process
        .processing_records
        .iter()
        .rev()
        .take(16)
        .cloned()
        .collect::<Vec<_>>();
    let processed_count = process.processing_records.len();
    let progress_total = process
        .processing_candidates
        .max(process.preflight.to_process_files)
        .max(1);
    let progress_value = processed_count.min(progress_total);
    let elapsed = process
        .processing_started_at
        .map(|start| super::super::view::format_duration(start.elapsed()))
        .unwrap_or_else(|| "--:--".to_string());

    StatusPanelViewModel {
        summary_metrics: [
            status_metric(
                tr(language, "total"),
                process.preflight.total_files.to_string(),
            ),
            status_metric(
                tr(language, "process"),
                process.preflight.to_process_files.to_string(),
            ),
            status_metric(
                tr(language, "skip"),
                process.preflight.skipped_files.to_string(),
            ),
        ],
        result_metrics: [
            status_metric(
                tr(language, "chars"),
                result_stats
                    .map(|stats| stats.total_chars.to_string())
                    .unwrap_or_else(|| "--".to_string()),
            ),
            status_metric(
                tr(language, "tokens"),
                result_stats
                    .map(|stats| stats.total_tokens.to_string())
                    .unwrap_or_else(|| "--".to_string()),
            ),
            status_metric(tr(language, "failed_count"), failed_count.to_string()),
        ],
        status: process.ui_status,
        status_title: SharedString::from(process_status_title(process.ui_status, language)),
        status_message: SharedString::from(process_status_message(
            process,
            language,
            merged_file_size_hint,
        )),
        archive_summary: (archive_totals.entries > 0).then(|| StatusInfoViewModel {
            label: SharedString::from(tr(language, "archive_sources")),
            value: SharedString::from(format!(
                "{} {} · {} {}",
                archive_totals.archives,
                tr(language, "archive_files"),
                archive_totals.entries,
                tr(language, "archive_entries")
            )),
        }),
        progress: StatusProgressViewModel {
            value_text: SharedString::from(format!("{progress_value}/{progress_total}")),
            fill_ratio: progress_value as f32 / progress_total as f32,
            elapsed_value: SharedString::from(elapsed),
            current_file: SharedString::from(process.processing_current_file.clone()),
        },
        activity_rows,
    }
}

pub(in crate::ui::workspace) fn resolve_window_zoom_action(
    is_maximized: bool,
    is_fullscreen: bool,
) -> WindowZoomAction {
    if is_maximized || is_fullscreen {
        WindowZoomAction::Restore
    } else {
        WindowZoomAction::Maximize
    }
}

pub(in crate::ui::workspace) fn process_status_title(
    status: ProcessUiStatus,
    language: Language,
) -> &'static str {
    match status {
        ProcessUiStatus::Idle => tr(language, "status_idle"),
        ProcessUiStatus::Preflight => tr(language, "status_preflight"),
        ProcessUiStatus::Running => tr(language, "status_running"),
        ProcessUiStatus::Completed => tr(language, "status_completed"),
        ProcessUiStatus::Cancelled => tr(language, "status_cancelled"),
        ProcessUiStatus::Error => tr(language, "status_error"),
    }
}

pub(in crate::ui::workspace) fn process_status_message(
    process: &ProcessState,
    language: Language,
    merged_file_size_hint: Option<String>,
) -> String {
    match process.ui_status {
        ProcessUiStatus::Idle => tr(language, "status_idle_hint").to_string(),
        ProcessUiStatus::Preflight => format!(
            "{} {}",
            tr(language, "status_preflight_hint"),
            process.preflight.scanned_entries
        ),
        ProcessUiStatus::Running => process.processing_current_file.clone(),
        ProcessUiStatus::Completed => {
            let base = tr(language, "status_completed_hint").to_string();
            match merged_file_size_hint {
                Some(size) => format!("{base} ({size})"),
                None => base,
            }
        }
        ProcessUiStatus::Cancelled => tr(language, "status_cancelled_hint").to_string(),
        ProcessUiStatus::Error => process
            .last_error
            .clone()
            .unwrap_or_else(|| tr(language, "status_error_hint").to_string()),
    }
}

fn workspace_chrome_tone(status: ProcessUiStatus) -> WorkspaceChromeTone {
    match status {
        ProcessUiStatus::Idle => WorkspaceChromeTone::Neutral,
        ProcessUiStatus::Preflight | ProcessUiStatus::Running => WorkspaceChromeTone::Accent,
        ProcessUiStatus::Completed => WorkspaceChromeTone::Success,
        ProcessUiStatus::Cancelled => WorkspaceChromeTone::Warning,
        ProcessUiStatus::Error => WorkspaceChromeTone::Danger,
    }
}

fn status_metric(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
) -> StatusMetricViewModel {
    StatusMetricViewModel {
        label: label.into(),
        value: value.into(),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn apply_preflight_event(
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

pub(in crate::ui::workspace) fn summarize_archive_entries(
    result: Option<&ProcessResult>,
) -> ArchiveResultSummary {
    let Some(result) = result else {
        return ArchiveResultSummary::default();
    };

    let mut archives = BTreeSet::new();
    let mut entries = 0usize;
    for preview in &result.preview_files {
        let Some(archive) = preview.archive.as_ref() else {
            continue;
        };
        archives.insert(archive.archive_path.clone());
        entries += 1;
    }

    ArchiveResultSummary {
        archives: archives.len(),
        entries,
    }
}
