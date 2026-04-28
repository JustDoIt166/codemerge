use crate::domain::{AppConfigV1, ProcessRecord, ProcessResult};
use crate::services::preflight::PreflightEvent;
use crate::services::process::{ProcessEvent, ProcessHandle};
use crate::ui::state::{
    NarrowContentTab, PendingConfirmation, ProcessState, ProcessUiStatus, SelectionState,
    SettingsState, SidePanelTab, WorkspaceUiState, clamp_selected_files_panel_height,
};
use crate::utils::i18n::tr;

const MAX_PROCESSING_RECORDS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveBlacklistRules {
    pub folder_blacklist: Vec<String>,
    pub ext_blacklist: Vec<String>,
}

pub struct SettingsModel {
    state: SettingsState,
}

impl SettingsModel {
    pub fn from_config(config: AppConfigV1) -> Self {
        Self {
            state: SettingsState {
                language: config.language,
                options: config.options,
                folder_blacklist: config.folder_blacklist,
                ext_blacklist: config.ext_blacklist,
            },
        }
    }

    pub fn snapshot(&self) -> SettingsState {
        self.state.clone()
    }

    pub fn to_config(&self) -> AppConfigV1 {
        AppConfigV1 {
            language: self.state.language,
            options: self.state.options.clone(),
            folder_blacklist: self.state.folder_blacklist.clone(),
            ext_blacklist: self.state.ext_blacklist.clone(),
        }
    }

    pub fn apply_config(&mut self, config: AppConfigV1) {
        self.state.language = config.language;
        self.state.options = config.options;
        self.state.folder_blacklist = config.folder_blacklist;
        self.state.ext_blacklist = config.ext_blacklist;
    }

    pub fn language(&self) -> crate::domain::Language {
        self.state.language
    }

    pub fn toggle_language(&mut self) -> crate::domain::Language {
        self.state.language = self.state.language.toggle();
        self.state.language
    }

    pub fn effective_blacklists(&self, selection: &SelectionState) -> EffectiveBlacklistRules {
        let mut folder_blacklist = self.state.folder_blacklist.clone();
        if self.state.options.use_gitignore {
            append_unique_rules(&mut folder_blacklist, &selection.gitignore_rules);
        }
        append_unique_rules(&mut folder_blacklist, &selection.temp_folder_blacklist);

        let mut ext_blacklist = self.state.ext_blacklist.clone();
        append_unique_rules(&mut ext_blacklist, &selection.temp_ext_blacklist);

        EffectiveBlacklistRules {
            folder_blacklist,
            ext_blacklist,
        }
    }

    pub fn add_blacklist_tokens(&mut self, tokens: &[String], as_ext: bool) -> usize {
        let mut added = 0;
        for token in tokens {
            if as_ext {
                let normalized = crate::processor::walker::normalize_ext(token);
                if !self.state.ext_blacklist.contains(&normalized) {
                    self.state.ext_blacklist.push(normalized);
                    added += 1;
                }
            } else if !self.state.folder_blacklist.contains(token) {
                self.state.folder_blacklist.push(token.clone());
                added += 1;
            }
        }
        added
    }

    pub fn import_blacklist_content(&mut self, content: &str) -> usize {
        let mut added = 0;
        for line in content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            if line.starts_with('.') {
                let ext = crate::processor::walker::normalize_ext(line);
                if !self.state.ext_blacklist.contains(&ext) {
                    self.state.ext_blacklist.push(ext);
                    added += 1;
                }
            } else if !self.state.folder_blacklist.contains(&line.to_string()) {
                self.state.folder_blacklist.push(line.to_string());
                added += 1;
            }
        }
        added
    }

    pub fn reset_blacklist(&mut self) {
        self.state.folder_blacklist = crate::domain::default_folder_blacklist();
        self.state.ext_blacklist = crate::domain::default_ext_blacklist();
    }

    pub fn clear_blacklist(&mut self) {
        self.state.folder_blacklist.clear();
        self.state.ext_blacklist.clear();
    }

    pub fn remove_blacklist_item(
        &mut self,
        kind: crate::ui::workspace::BlacklistItemKind,
        value: &str,
    ) {
        match kind {
            crate::ui::workspace::BlacklistItemKind::Folder => {
                self.state.folder_blacklist.retain(|item| item != value)
            }
            crate::ui::workspace::BlacklistItemKind::Ext => {
                self.state.ext_blacklist.retain(|item| item != value)
            }
        }
    }

    pub fn set_compress(&mut self, checked: bool) {
        self.state.options.compress = checked;
    }

    pub fn set_use_gitignore(&mut self, checked: bool) {
        self.state.options.use_gitignore = checked;
    }

    pub fn set_ignore_git(&mut self, checked: bool) {
        self.state.options.ignore_git = checked;
        if checked {
            if !self.state.folder_blacklist.contains(&".git".to_string()) {
                self.state.folder_blacklist.push(".git".to_string());
            }
        } else {
            self.state.folder_blacklist.retain(|item| item != ".git");
        }
    }

    pub fn set_output_format(&mut self, format: crate::domain::OutputFormat) {
        self.state.options.output_format = format;
    }
}

fn append_unique_rules(target: &mut Vec<String>, rules: &[String]) {
    for rule in rules {
        if !target.contains(rule) {
            target.push(rule.clone());
        }
    }
}

pub struct ProcessModel {
    state: ProcessState,
}

pub struct WorkspaceUiModel {
    state: WorkspaceUiState,
}

impl WorkspaceUiModel {
    pub fn new() -> Self {
        Self {
            state: WorkspaceUiState::default(),
        }
    }

    pub fn state(&self) -> WorkspaceUiState {
        self.state
    }

    pub fn clear_pending_confirmation(&mut self) -> bool {
        let changed = self.state.pending_confirmation.is_some();
        self.state.pending_confirmation = None;
        changed
    }

    pub fn set_pending_confirmation(&mut self, pending_confirmation: PendingConfirmation) -> bool {
        let changed = self.state.pending_confirmation != Some(pending_confirmation);
        self.state.pending_confirmation = Some(pending_confirmation);
        changed
    }

    pub fn set_side_panel_tab(&mut self, tab: SidePanelTab) -> bool {
        let changed = self.state.side_panel_tab != tab;
        self.state.side_panel_tab = tab;
        changed
    }

    pub fn set_narrow_content_tab(&mut self, tab: NarrowContentTab) -> bool {
        let changed = self.state.narrow_content_tab != tab;
        self.state.narrow_content_tab = tab;
        changed
    }

    pub fn set_content_file_list_collapsed(&mut self, collapsed: bool) -> bool {
        let changed = self.state.content_file_list_collapsed != collapsed;
        self.state.content_file_list_collapsed = collapsed;
        changed
    }

    pub fn set_selected_files_panel_height(&mut self, height: u16) -> bool {
        let height = clamp_selected_files_panel_height(height);
        let changed = self.state.selected_files_panel_height != height;
        self.state.selected_files_panel_height = height;
        changed
    }
}

impl ProcessModel {
    pub fn new(status_ready: String) -> Self {
        Self {
            state: ProcessState {
                processing_current_file: status_ready,
                ..ProcessState::default()
            },
        }
    }

    pub fn state(&self) -> &ProcessState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut ProcessState {
        &mut self.state
    }

    pub fn is_processing(&self) -> bool {
        self.state.process_handle.is_some()
    }

    pub fn clear_runtime(&mut self, status_ready: String) {
        self.state = ProcessState {
            processing_current_file: status_ready,
            ..ProcessState::default()
        };
    }

    pub fn start_run(&mut self, handle: ProcessHandle, scanning_label: String) {
        self.state.discard_preflight_for_run();
        self.state.reset_for_run(scanning_label);
        self.state.process_handle = Some(handle);
    }

    pub fn cancel_running(&mut self) -> bool {
        let Some(handle) = &self.state.process_handle else {
            return false;
        };
        handle.cancel.cancel();
        self.state.ui_status = ProcessUiStatus::Cancelled;
        true
    }

    pub fn apply_preflight_event(
        &mut self,
        event: PreflightEvent,
        language: crate::domain::Language,
    ) {
        let ready_label = tr(language, "status_ready");
        let is_processing = self.is_processing();
        match event {
            PreflightEvent::Started { revision } => {
                if revision == self.state.preflight_revision {
                    self.state.preflight.is_scanning = true;
                    if !is_processing {
                        self.state.ui_status = ProcessUiStatus::Preflight;
                    }
                }
            }
            PreflightEvent::Progress {
                revision,
                scanned,
                candidates,
                skipped,
            } => {
                if revision == self.state.preflight_revision {
                    self.state.preflight.scanned_entries = scanned;
                    self.state.preflight.to_process_files = candidates;
                    self.state.preflight.skipped_files = skipped;
                    self.state.preflight.total_files = candidates + skipped;
                    self.state.preflight.is_scanning = true;
                    if !is_processing {
                        self.state.ui_status = ProcessUiStatus::Preflight;
                    }
                }
            }
            PreflightEvent::Completed { revision, stats } => {
                if revision == self.state.preflight_revision {
                    self.state.preflight = stats;
                    if !is_processing {
                        self.state.ui_status = ProcessUiStatus::Idle;
                        self.state.processing_current_file = ready_label.to_string();
                    }
                }
            }
            PreflightEvent::Failed { revision, error } => {
                if revision == self.state.preflight_revision {
                    self.state.preflight.is_scanning = false;
                    self.state.ui_status = ProcessUiStatus::Error;
                    self.state.last_error = Some(error.to_string());
                }
            }
        }
    }

    pub fn apply_process_event(
        &mut self,
        event: ProcessEvent,
        language: crate::domain::Language,
    ) -> ProcessEventEffect {
        match event {
            ProcessEvent::Scanning {
                scanned,
                candidates,
                skipped,
            } => {
                self.state.preflight.scanned_entries = scanned;
                self.state.preflight.to_process_files = candidates;
                self.state.preflight.skipped_files = skipped;
                self.state.preflight.total_files = candidates + skipped;
                self.state.preflight.is_scanning = true;
                self.state.processing_scanned = scanned;
                self.state.processing_candidates = candidates;
                self.state.processing_skipped = skipped;
                self.state.ui_status = ProcessUiStatus::Running;
                self.state.processing_current_file =
                    format!("{} {}", tr(language, "scanning_files"), scanned);
                ProcessEventEffect::Continue
            }
            ProcessEvent::Record(record) => {
                self.push_record(record);
                ProcessEventEffect::Continue
            }
            ProcessEvent::Completed(result) => {
                self.state.preflight.is_scanning = false;
                self.state.ui_status = ProcessUiStatus::Completed;
                self.state.last_error = None;
                self.state.processing_current_file =
                    tr(language, "status_completed_hint").to_string();
                ProcessEventEffect::Completed(Box::new(result))
            }
            ProcessEvent::Cancelled => {
                self.state.preflight.is_scanning = false;
                self.state.ui_status = ProcessUiStatus::Cancelled;
                self.state.processing_current_file =
                    tr(language, "status_cancelled_hint").to_string();
                ProcessEventEffect::Finish
            }
            ProcessEvent::Failed(err) => {
                self.state.preflight.is_scanning = false;
                self.state.ui_status = ProcessUiStatus::Error;
                self.state.last_error = Some(err.to_string());
                self.state.processing_current_file = err.to_string();
                ProcessEventEffect::Finish
            }
        }
    }

    fn push_record(&mut self, record: ProcessRecord) {
        self.state.ui_status = ProcessUiStatus::Running;
        self.state.processing_current_file = record.file_name.clone();
        if !matches!(record.status, crate::domain::ProcessStatus::Success) {
            self.state.processing_skipped += 1;
        }
        self.state.processing_records.push(record);
        if self.state.processing_records.len() > MAX_PROCESSING_RECORDS {
            let overflow = self.state.processing_records.len() - MAX_PROCESSING_RECORDS;
            self.state.processing_records.drain(0..overflow);
        }
    }
}

pub enum ProcessEventEffect {
    Continue,
    Completed(Box<ProcessResult>),
    Finish,
}

#[cfg(test)]
mod tests {
    use super::{
        EffectiveBlacklistRules, ProcessEventEffect, ProcessModel, SettingsModel, WorkspaceUiModel,
    };
    use crate::domain::{
        AppConfigV1, Language, OutputFormat, ProcessResult, ProcessStatus, ProcessingMode,
        ProcessingOptions,
    };
    use crate::processor::stats::ProcessingStats;
    use crate::services::preflight::PreflightEvent;
    use crate::services::process::ProcessEvent;
    use crate::services::process::ProcessHandle;
    use crate::ui::state::{NarrowContentTab, PendingConfirmation, SelectionState, SidePanelTab};
    use std::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn effective_blacklists_respect_use_gitignore_and_temporary_rules() {
        let config = AppConfigV1 {
            language: Language::En,
            options: ProcessingOptions {
                compress: false,
                use_gitignore: true,
                ignore_git: false,
                output_format: OutputFormat::Markdown,
                mode: ProcessingMode::Full,
            },
            folder_blacklist: vec!["target".into()],
            ext_blacklist: vec![".log".into()],
        };
        let model = SettingsModel::from_config(config);
        let selection = SelectionState {
            gitignore_rules: vec!["node_modules".into()],
            temp_folder_blacklist: vec!["coverage".into()],
            temp_ext_blacklist: vec![".tmp".into()],
            ..SelectionState::default()
        };

        let mut settings = model;
        assert_eq!(
            settings.effective_blacklists(&selection),
            EffectiveBlacklistRules {
                folder_blacklist: vec![
                    "target".to_string(),
                    "node_modules".to_string(),
                    "coverage".to_string()
                ],
                ext_blacklist: vec![".log".to_string(), ".tmp".to_string()],
            }
        );

        settings.set_use_gitignore(false);
        assert_eq!(
            settings.effective_blacklists(&selection),
            EffectiveBlacklistRules {
                folder_blacklist: vec!["target".to_string(), "coverage".to_string()],
                ext_blacklist: vec![".log".to_string(), ".tmp".to_string()],
            }
        );
    }

    #[test]
    fn import_blacklist_content_normalizes_and_dedupes() {
        let config = AppConfigV1 {
            language: Language::En,
            options: ProcessingOptions {
                compress: false,
                use_gitignore: false,
                ignore_git: false,
                output_format: OutputFormat::Markdown,
                mode: ProcessingMode::Full,
            },
            folder_blacklist: vec!["target".into()],
            ext_blacklist: vec![".log".into()],
        };
        let mut settings = SettingsModel::from_config(config);

        let added =
            settings.import_blacklist_content("# comment\n target\nbuild\nrs\n.log\n.tmp\n");

        assert_eq!(added, 3);
        let snapshot = settings.snapshot();
        assert_eq!(
            snapshot.folder_blacklist,
            vec!["target".to_string(), "build".to_string(), "rs".to_string()]
        );
        assert_eq!(
            snapshot.ext_blacklist,
            vec![".log".to_string(), ".tmp".to_string()]
        );
    }

    #[test]
    fn settings_model_updates_output_format() {
        let mut settings = SettingsModel::from_config(AppConfigV1::default());

        settings.set_output_format(OutputFormat::Xml);

        assert_eq!(settings.snapshot().options.output_format, OutputFormat::Xml);
    }

    #[test]
    fn process_model_reports_completed_runs() {
        let mut process = ProcessModel::new("ready".into());
        let result = ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: Vec::new(),
            process_dir: None,
            merged_content_path: None,
            suggested_result_name: "workspace-20260319.txt".into(),
            file_details: Vec::new(),
            preview_files: Vec::new(),
            preview_blob_dir: None,
        };

        let effect = process.apply_process_event(ProcessEvent::Completed(result), Language::En);

        assert!(matches!(effect, ProcessEventEffect::Completed(_)));
        assert_eq!(
            process.state().ui_status,
            crate::ui::state::ProcessUiStatus::Completed
        );
        assert_eq!(
            process.state().processing_current_file,
            "Processing finished. Review the tree or merged content."
        );
    }

    #[test]
    fn process_model_start_run_discards_stale_preflight_events() {
        let mut process = ProcessModel::new("ready".into());
        let (_preflight_tx, preflight_rx) = mpsc::channel();
        process.state_mut().preflight_revision = 7;
        process.state_mut().preflight_rx = Some(preflight_rx);
        process.state_mut().preflight.total_files = 9;
        process.state_mut().preflight.to_process_files = 8;

        let (_tx, rx) = mpsc::channel();
        process.start_run(
            ProcessHandle {
                receiver: rx,
                cancel: CancellationToken::new(),
            },
            "Scanning files".into(),
        );

        assert!(process.state().preflight_rx.is_none());
        assert_eq!(process.state().preflight_revision, 8);
        assert_eq!(process.state().preflight.total_files, 0);

        process.apply_preflight_event(
            PreflightEvent::Completed {
                revision: 7,
                stats: crate::domain::PreflightStats {
                    total_files: 11,
                    skipped_files: 2,
                    to_process_files: 9,
                    scanned_entries: 11,
                    is_scanning: false,
                },
            },
            Language::En,
        );

        assert_eq!(process.state().preflight.total_files, 0);
        assert_eq!(process.state().preflight.to_process_files, 0);
    }

    #[test]
    fn process_model_scanning_event_updates_visible_preflight_metrics() {
        let mut process = ProcessModel::new("ready".into());

        let effect = process.apply_process_event(
            ProcessEvent::Scanning {
                scanned: 12,
                candidates: 9,
                skipped: 3,
            },
            Language::En,
        );

        assert!(matches!(effect, ProcessEventEffect::Continue));
        assert_eq!(process.state().preflight.scanned_entries, 12);
        assert_eq!(process.state().preflight.total_files, 12);
        assert_eq!(process.state().preflight.to_process_files, 9);
        assert_eq!(process.state().preflight.skipped_files, 3);
        assert!(process.state().preflight.is_scanning);
    }

    #[test]
    fn process_model_counts_failed_records_as_skipped() {
        let mut process = ProcessModel::new("ready".into());

        let effect = process.apply_process_event(
            ProcessEvent::Record(crate::domain::ProcessRecord {
                file_name: "broken.rs".into(),
                status: ProcessStatus::Failed,
                chars: None,
                tokens: None,
                error: Some("boom".into()),
            }),
            Language::En,
        );

        assert!(matches!(effect, ProcessEventEffect::Continue));
        assert_eq!(process.state().processing_skipped, 1);
        assert_eq!(process.state().processing_records.len(), 1);
    }

    #[test]
    fn process_model_caps_processing_records() {
        let mut process = ProcessModel::new("ready".into());

        for ix in 0..1_050 {
            let _ = process.apply_process_event(
                ProcessEvent::Record(crate::domain::ProcessRecord {
                    file_name: format!("file-{ix}.rs"),
                    status: ProcessStatus::Success,
                    chars: Some(1),
                    tokens: Some(1),
                    error: None,
                }),
                Language::En,
            );
        }

        assert_eq!(process.state().processing_records.len(), 1_000);
        assert_eq!(
            process.state().processing_records[0].file_name,
            "file-50.rs"
        );
    }

    #[test]
    fn workspace_ui_model_skips_noop_tab_updates() {
        let mut model = WorkspaceUiModel::new();
        assert!(!model.set_side_panel_tab(SidePanelTab::Results));
        assert!(model.set_side_panel_tab(SidePanelTab::Rules));
        assert!(!model.set_side_panel_tab(SidePanelTab::Rules));
        assert!(!model.set_narrow_content_tab(NarrowContentTab::Status));
        assert!(model.set_narrow_content_tab(NarrowContentTab::Results));
        assert!(!model.set_narrow_content_tab(NarrowContentTab::Results));
        assert!(!model.set_content_file_list_collapsed(false));
        assert!(model.set_content_file_list_collapsed(true));
        assert!(!model.set_content_file_list_collapsed(true));
    }

    #[test]
    fn workspace_ui_model_tracks_pending_confirmation_changes() {
        let mut model = WorkspaceUiModel::new();

        assert!(model.set_pending_confirmation(PendingConfirmation::ClearInputs));
        assert!(!model.set_pending_confirmation(PendingConfirmation::ClearInputs));
        assert!(model.clear_pending_confirmation());
        assert!(!model.clear_pending_confirmation());
    }

    #[test]
    fn workspace_ui_model_clamps_selected_files_panel_height() {
        let mut model = WorkspaceUiModel::new();

        assert!(!model.set_selected_files_panel_height(180));
        assert!(model.set_selected_files_panel_height(40));
        assert_eq!(model.state().selected_files_panel_height, 120);
        assert!(model.set_selected_files_panel_height(999));
        assert_eq!(model.state().selected_files_panel_height, 560);
        assert!(!model.set_selected_files_panel_height(560));
    }
}
