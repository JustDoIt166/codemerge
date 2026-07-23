use std::path::PathBuf;

#[cfg(test)]
use gpui::AppContext;
use gpui::{App, Entity, EventEmitter};

use crate::domain::{
    AppConfigV1, FileEntry, Language, OutputFormat, ProcessResult, TemporaryWhitelistMode,
};
use crate::services::preflight::PreflightEvent;
use crate::services::preview::{PreviewEvent, PreviewRequest};
use crate::services::process::{ProcessEvent, ProcessRunId};
use crate::ui::models::{
    EffectiveMergeFilters, ProcessEventEffect, ProcessModel, SettingsModel, WorkspaceUiModel,
};
use crate::ui::preview_model::{PreviewEventEffect, PreviewModel, PreviewScrollDirection};
use crate::ui::result_model::{ResultModel, ResultState};
use crate::ui::selection_model::SelectionModel;
use crate::ui::state::{
    NarrowContentTab, PendingConfirmation, PreviewPanelState, ProcessState, SelectionState,
    SettingsState, SidePanelTab, TreePanelState, WorkspaceUiState,
};
use crate::ui::view_model::ResultTab;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlacklistItemKind {
    Folder,
    Ext,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ChangeSet(u32);

impl ChangeSet {
    pub const NONE: Self = Self(0);
    pub const DRAFT_SELECTION: Self = Self(1 << 0);
    pub const DRAFT_RULES: Self = Self(1 << 1);
    pub const DRAFT_SETTINGS: Self = Self(1 << 2);
    pub const EXECUTION: Self = Self(1 << 3);
    pub const EXECUTION_RESULT: Self = Self(1 << 4);
    pub const PREVIEW: Self = Self(1 << 5);
    pub const NAVIGATION: Self = Self(1 << 6);
    pub const NAVIGATION_TREE: Self = Self(1 << 7);
    pub const ALL: Self = Self(u32::MAX);

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceRevisions {
    pub selection: u64,
    pub rules: u64,
    pub settings: u64,
    pub execution: u64,
    pub result: u64,
    pub preview: u64,
    pub navigation: u64,
    pub tree: u64,
}

#[derive(Clone)]
pub struct DraftState {
    selection: SelectionState,
    settings: SettingsState,
}

impl DraftState {
    pub fn selection(&self) -> &SelectionState {
        &self.selection
    }

    pub fn settings(&self) -> &SettingsState {
        &self.settings
    }
}

#[derive(Clone)]
pub struct ExecutionState {
    process: ProcessState,
    result: ResultState,
}

impl ExecutionState {
    pub fn process(&self) -> &ProcessState {
        &self.process
    }

    pub fn result(&self) -> &ResultState {
        &self.result
    }
}

#[derive(Clone)]
pub struct PreviewState {
    state: PreviewPanelState,
}

impl PreviewState {
    pub fn state(&self) -> &PreviewPanelState {
        &self.state
    }
}

#[derive(Clone)]
pub struct NavigationState {
    ui: WorkspaceUiState,
    tree: crate::ui::state::TreePanelState,
}

impl NavigationState {
    pub fn ui(&self) -> WorkspaceUiState {
        self.ui
    }

    pub fn tree(&self) -> &crate::ui::state::TreePanelState {
        &self.tree
    }
}

#[derive(Clone)]
pub struct WorkspaceState {
    draft: DraftState,
    execution: ExecutionState,
    preview: PreviewState,
    navigation: NavigationState,
    revisions: WorkspaceRevisions,
}

impl WorkspaceState {
    pub fn draft(&self) -> &DraftState {
        &self.draft
    }

    pub fn execution(&self) -> &ExecutionState {
        &self.execution
    }

    pub fn preview(&self) -> &PreviewState {
        &self.preview
    }

    pub fn navigation(&self) -> &NavigationState {
        &self.navigation
    }

    pub fn revisions(&self) -> WorkspaceRevisions {
        self.revisions
    }
}

impl WorkspaceRevisions {
    fn advance(&mut self, changes: ChangeSet) {
        if changes.intersects(ChangeSet::DRAFT_SELECTION) {
            self.selection = self.selection.wrapping_add(1);
        }
        if changes.intersects(ChangeSet::DRAFT_RULES) {
            self.rules = self.rules.wrapping_add(1);
        }
        if changes.intersects(ChangeSet::DRAFT_SETTINGS) {
            self.settings = self.settings.wrapping_add(1);
        }
        if changes.intersects(ChangeSet::EXECUTION) {
            self.execution = self.execution.wrapping_add(1);
        }
        if changes.intersects(ChangeSet::EXECUTION_RESULT) {
            self.result = self.result.wrapping_add(1);
        }
        if changes.intersects(ChangeSet::PREVIEW) {
            self.preview = self.preview.wrapping_add(1);
        }
        if changes.intersects(ChangeSet::NAVIGATION) {
            self.navigation = self.navigation.wrapping_add(1);
        }
        if changes.intersects(ChangeSet::NAVIGATION_TREE) {
            self.tree = self.tree.wrapping_add(1);
        }
    }
}

#[derive(Debug)]
pub enum WorkspaceEffect {
    RestartPreflight { preserve_completed_status: bool },
    CancelAllTasks,
    CleanupResultArtifacts,
    StartPreview(PreviewRequest),
}

#[derive(Debug, Default)]
pub struct Transition {
    pub changes: ChangeSet,
    pub effects: Vec<WorkspaceEffect>,
    pub output: ActionOutput,
}

impl Transition {
    fn changed(changes: ChangeSet) -> Self {
        Self {
            changes,
            ..Self::default()
        }
    }

    fn with_output(mut self, output: ActionOutput) -> Self {
        self.output = output;
        self
    }
}

#[derive(Debug, Default)]
pub enum ActionOutput {
    #[default]
    None,
    Changed(bool),
    Count(usize),
    Language(Language),
    Process(ProcessEventEffect),
    Preview(PreviewEventEffect),
    PreviewRequest(Option<PreviewRequest>),
    SaveRevision(Option<u64>),
    PreflightRevision(u64),
}

#[derive(Debug)]
pub enum WorkspaceAction {
    Draft(DraftAction),
    Execution(ExecutionAction),
    Preview(PreviewAction),
    Navigation(NavigationAction),
    PrepareProcess,
    ClearInputs {
        ready_label: String,
    },
    ResetWorkspace {
        config: AppConfigV1,
        ready_label: String,
    },
}

#[derive(Debug)]
pub enum DraftAction {
    SelectFolder {
        path: PathBuf,
        gitignore_rules: Vec<String>,
    },
    UpdateSelectedFolderGitignore(Vec<String>),
    AddSelectedFiles(Vec<FileEntry>),
    RemoveSelectedFile(PathBuf),
    ClearSelectedFolder,
    ExcludeFolderFile(String),
    SetGitignoreFile(Option<PathBuf>),
    SetDedupe(bool),
    AddTemporaryBlacklist {
        tokens: Vec<String>,
        as_ext: bool,
    },
    AppendTemporaryGitignore(Vec<String>),
    AddTemporaryWhitelist {
        tokens: Vec<String>,
        as_ext: bool,
    },
    RemoveTemporaryBlacklist {
        kind: BlacklistItemKind,
        value: String,
    },
    RemoveTemporaryWhitelist {
        kind: BlacklistItemKind,
        value: String,
    },
    ClearTemporaryBlacklist,
    ClearTemporaryWhitelist,
    ClearTemporaryMergeFilters,
    AddBlacklist {
        tokens: Vec<String>,
        as_ext: bool,
    },
    ImportBlacklist(String),
    ResetBlacklist,
    ClearBlacklist,
    RemoveBlacklist {
        kind: BlacklistItemKind,
        value: String,
    },
    ToggleLanguage,
    SetCompress(bool),
    SetUseGitignore(bool),
    SetIgnoreGit(bool),
    SetOutputFormat(OutputFormat),
    SetWhitelistMode(TemporaryWhitelistMode),
    ApplyConfig(AppConfigV1),
}

#[derive(Debug)]
pub enum ExecutionAction {
    ClearRuntime {
        ready_label: String,
    },
    SetIdleLabel(String),
    StartRun {
        run_id: ProcessRunId,
        scanning_label: String,
    },
    BeginPreflight {
        preserve_completed_status: bool,
    },
    CancelRequested,
    Preflight(PreflightEvent),
    Process(ProcessEvent),
    ProcessBatch(Vec<ProcessEvent>),
    FinishRun,
    FailDisconnected(String),
    SetResult(ProcessResult),
    ClearResult,
    SetResultTab(ResultTab),
    SetPreviewRowCount(usize),
    BeginSave,
    FinishSave {
        revision: u64,
        succeeded: bool,
    },
    CancelSave(u64),
}

#[derive(Debug)]
pub enum PreviewAction {
    Clear,
    Apply(PreviewEvent),
    ApplyMany(Vec<PreviewEvent>),
    ClearRequest,
    Open {
        file_id: u32,
        path: PathBuf,
    },
    OpenDeferredFull {
        file_id: u32,
        path: PathBuf,
    },
    OpenDeferredExcerpt {
        file_id: u32,
        source_path: PathBuf,
        source_byte_len: u64,
        excerpt_byte_len: u64,
        excerpt_path: PathBuf,
    },
    Defer {
        file_id: u32,
        source_path: PathBuf,
        source_byte_len: u64,
        excerpt_byte_len: u64,
    },
    SetError(String),
    RequestRange {
        range: std::ops::Range<usize>,
        direction: PreviewScrollDirection,
    },
    RequestQueuedRange {
        direction: PreviewScrollDirection,
    },
}

#[derive(Debug)]
pub enum NavigationAction {
    ClearPendingConfirmation,
    SetPendingConfirmation(PendingConfirmation),
    SetSidePanelTab(SidePanelTab),
    SetNarrowContentTab(NarrowContentTab),
    SetContentFileListCollapsed(bool),
    SetSelectedFilesPanelHeight(u16),
    ResetTree,
    SetTreeState(crate::ui::state::TreePanelState),
}

pub struct WorkspaceEvent(pub ChangeSet);

#[cfg(test)]
pub struct SliceContext {
    notified: bool,
}

#[cfg(test)]
impl SliceContext {
    pub fn notify(&mut self) {
        self.notified = true;
    }
}

pub(crate) struct StoreSlice<T> {
    store: Entity<WorkspaceStore>,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T> Clone for StoreSlice<T> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> StoreSlice<T> {
    fn new(store: Entity<WorkspaceStore>) -> Self {
        Self {
            store,
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct WorkspaceStore {
    selection: SelectionModel,
    settings: SettingsModel,
    process: ProcessModel,
    result: ResultModel,
    preview: PreviewModel,
    navigation: WorkspaceUiModel,
    navigation_tree: TreePanelState,
    revisions: WorkspaceRevisions,
}

impl EventEmitter<WorkspaceEvent> for WorkspaceStore {}

macro_rules! impl_store_slice {
    ($type:ty, $field:ident, $changes:expr) => {
        impl StoreSlice<$type> {
            pub fn read<'a>(&self, cx: &'a App) -> &'a $type {
                &self.store.read(cx).$field
            }

            #[cfg(test)]
            #[allow(dead_code)]
            pub fn update<R, C: AppContext>(
                &self,
                cx: &mut C,
                update: impl FnOnce(&mut $type, &mut SliceContext) -> R,
            ) -> C::Result<R> {
                self.store.update(cx, |store, store_cx| {
                    let mut slice_cx = SliceContext { notified: false };
                    let result = update(&mut store.$field, &mut slice_cx);
                    if slice_cx.notified {
                        store.revisions.advance($changes);
                        crate::ui::perf::record_store_dispatch();
                        store_cx.emit(WorkspaceEvent($changes));
                    }
                    result
                })
            }
        }
    };
}

impl_store_slice!(
    SelectionModel,
    selection,
    ChangeSet::DRAFT_SELECTION.union(ChangeSet::DRAFT_RULES)
);
impl_store_slice!(
    SettingsModel,
    settings,
    ChangeSet::DRAFT_SETTINGS.union(ChangeSet::DRAFT_RULES)
);
impl_store_slice!(ProcessModel, process, ChangeSet::EXECUTION);
impl_store_slice!(ResultModel, result, ChangeSet::EXECUTION_RESULT);
impl_store_slice!(PreviewModel, preview, ChangeSet::PREVIEW);
impl_store_slice!(WorkspaceUiModel, navigation, ChangeSet::NAVIGATION);

impl WorkspaceStore {
    pub fn new(config: AppConfigV1, ready_label: String) -> Self {
        Self {
            selection: SelectionModel::new(),
            settings: SettingsModel::from_config(config.clone()),
            process: ProcessModel::new(ready_label.clone()),
            result: ResultModel::new(),
            preview: PreviewModel::new(),
            navigation: WorkspaceUiModel::new(),
            navigation_tree: TreePanelState::default(),
            revisions: WorkspaceRevisions::default(),
        }
    }

    pub fn state(&self) -> WorkspaceState {
        WorkspaceState {
            draft: DraftState {
                selection: self.selection.snapshot(),
                settings: self.settings.snapshot(),
            },
            execution: ExecutionState {
                process: self.process.state().clone(),
                result: self.result.state().clone(),
            },
            preview: PreviewState {
                state: self.preview.state().clone(),
            },
            navigation: NavigationState {
                ui: self.navigation.state(),
                tree: self.navigation_tree.clone(),
            },
            revisions: self.revisions,
        }
    }

    pub(crate) fn selection_slice(store: Entity<Self>) -> StoreSlice<SelectionModel> {
        StoreSlice::new(store)
    }

    pub(crate) fn settings_slice(store: Entity<Self>) -> StoreSlice<SettingsModel> {
        StoreSlice::new(store)
    }

    pub(crate) fn process_slice(store: Entity<Self>) -> StoreSlice<ProcessModel> {
        StoreSlice::new(store)
    }

    pub(crate) fn result_slice(store: Entity<Self>) -> StoreSlice<ResultModel> {
        StoreSlice::new(store)
    }

    pub(crate) fn preview_slice(store: Entity<Self>) -> StoreSlice<PreviewModel> {
        StoreSlice::new(store)
    }

    pub(crate) fn navigation_slice(store: Entity<Self>) -> StoreSlice<WorkspaceUiModel> {
        StoreSlice::new(store)
    }

    pub fn dispatch(&mut self, action: WorkspaceAction) -> Transition {
        let transition = match action {
            WorkspaceAction::Draft(action) => self.reduce_draft(action),
            WorkspaceAction::Execution(action) => self.reduce_execution(action),
            WorkspaceAction::Preview(action) => self.reduce_preview(action),
            WorkspaceAction::Navigation(action) => self.reduce_navigation(action),
            WorkspaceAction::PrepareProcess => {
                self.result.clear();
                self.preview.clear();
                self.navigation.clear_pending_confirmation();
                self.navigation_tree = TreePanelState::default();
                Transition::changed(
                    ChangeSet::EXECUTION_RESULT
                        .union(ChangeSet::PREVIEW)
                        .union(ChangeSet::NAVIGATION)
                        .union(ChangeSet::NAVIGATION_TREE),
                )
            }
            WorkspaceAction::ClearInputs { ready_label } => {
                self.selection.clear();
                self.process.clear_runtime(ready_label);
                self.result.clear();
                self.preview.clear();
                self.navigation = WorkspaceUiModel::new();
                self.navigation_tree = TreePanelState::default();
                Transition {
                    changes: ChangeSet::ALL,
                    effects: vec![
                        WorkspaceEffect::CancelAllTasks,
                        WorkspaceEffect::CleanupResultArtifacts,
                    ],
                    output: ActionOutput::None,
                }
            }
            WorkspaceAction::ResetWorkspace {
                config,
                ready_label,
            } => {
                *self = Self::new(config, ready_label);
                Transition {
                    changes: ChangeSet::ALL,
                    effects: vec![
                        WorkspaceEffect::CancelAllTasks,
                        WorkspaceEffect::CleanupResultArtifacts,
                        WorkspaceEffect::RestartPreflight {
                            preserve_completed_status: false,
                        },
                    ],
                    output: ActionOutput::None,
                }
            }
        };
        self.revisions.advance(transition.changes);
        transition
    }

    fn reduce_draft(&mut self, action: DraftAction) -> Transition {
        let selection_change = ChangeSet::DRAFT_SELECTION;
        let rules_change = ChangeSet::DRAFT_RULES;
        let settings_change = ChangeSet::DRAFT_SETTINGS;
        match action {
            DraftAction::SelectFolder {
                path,
                gitignore_rules,
            } => {
                self.selection.set_selected_folder(path, gitignore_rules);
                Transition::changed(selection_change)
            }
            DraftAction::UpdateSelectedFolderGitignore(rules) => {
                let changed = self.selection.set_selected_folder_gitignore_rules(rules);
                Transition::changed(if changed {
                    selection_change
                } else {
                    ChangeSet::NONE
                })
                .with_output(ActionOutput::Changed(changed))
            }
            DraftAction::AddSelectedFiles(files) => {
                let before = self.selection.state().selected_files.len();
                self.selection.add_selected_files(files);
                let changed = before != self.selection.state().selected_files.len();
                Transition::changed(if changed {
                    selection_change
                } else {
                    ChangeSet::NONE
                })
            }
            DraftAction::RemoveSelectedFile(path) => {
                let changed = self.selection.remove_selected_file(&path);
                Transition::changed(if changed {
                    selection_change
                } else {
                    ChangeSet::NONE
                })
                .with_output(ActionOutput::Changed(changed))
            }
            DraftAction::ClearSelectedFolder => {
                let changed = self.selection.clear_selected_folder();
                Transition::changed(if changed {
                    selection_change
                } else {
                    ChangeSet::NONE
                })
                .with_output(ActionOutput::Changed(changed))
            }
            DraftAction::ExcludeFolderFile(path) => {
                let changed = self.selection.exclude_folder_file(path);
                Transition::changed(if changed {
                    selection_change
                } else {
                    ChangeSet::NONE
                })
                .with_output(ActionOutput::Changed(changed))
            }
            DraftAction::SetGitignoreFile(path) => {
                let changed = self.selection.state().gitignore_file != path;
                if changed {
                    self.selection.set_gitignore_file(path);
                }
                changed_transition(changed, selection_change)
            }
            DraftAction::SetDedupe(value) => {
                let changed = self.selection.state().dedupe_exact_path != value;
                if changed {
                    self.selection.set_dedupe_exact_path(value);
                }
                changed_transition(changed, selection_change)
            }
            DraftAction::AddTemporaryBlacklist { tokens, as_ext } => {
                let count = self
                    .selection
                    .add_temporary_blacklist_tokens(&tokens, as_ext);
                Transition::changed(if count > 0 {
                    rules_change
                } else {
                    ChangeSet::NONE
                })
                .with_output(ActionOutput::Count(count))
            }
            DraftAction::AppendTemporaryGitignore(rules) => {
                let count = self.selection.append_temporary_gitignore_rules(rules);
                Transition::changed(if count > 0 {
                    rules_change
                } else {
                    ChangeSet::NONE
                })
                .with_output(ActionOutput::Count(count))
            }
            DraftAction::AddTemporaryWhitelist { tokens, as_ext } => {
                let count = self
                    .selection
                    .add_temporary_whitelist_tokens(&tokens, as_ext);
                Transition::changed(if count > 0 {
                    rules_change
                } else {
                    ChangeSet::NONE
                })
                .with_output(ActionOutput::Count(count))
            }
            DraftAction::RemoveTemporaryBlacklist { kind, value } => {
                let before = self.selection.state().temp_folder_blacklist.len()
                    + self.selection.state().temp_ext_blacklist.len();
                self.selection.remove_temporary_blacklist_item(kind, &value);
                let after = self.selection.state().temp_folder_blacklist.len()
                    + self.selection.state().temp_ext_blacklist.len();
                changed_transition(before != after, rules_change)
            }
            DraftAction::RemoveTemporaryWhitelist { kind, value } => {
                let before = self.selection.state().temp_folder_whitelist.len()
                    + self.selection.state().temp_ext_whitelist.len();
                self.selection.remove_temporary_whitelist_item(kind, &value);
                let after = self.selection.state().temp_folder_whitelist.len()
                    + self.selection.state().temp_ext_whitelist.len();
                changed_transition(before != after, rules_change)
            }
            DraftAction::ClearTemporaryBlacklist => {
                changed_transition(self.selection.clear_temporary_blacklist(), rules_change)
            }
            DraftAction::ClearTemporaryWhitelist => {
                changed_transition(self.selection.clear_temporary_whitelist(), rules_change)
            }
            DraftAction::ClearTemporaryMergeFilters => {
                changed_transition(self.selection.clear_temporary_merge_filters(), rules_change)
            }
            DraftAction::AddBlacklist { tokens, as_ext } => {
                let count = self.settings.add_blacklist_tokens(&tokens, as_ext);
                Transition::changed(if count > 0 {
                    rules_change
                } else {
                    ChangeSet::NONE
                })
                .with_output(ActionOutput::Count(count))
            }
            DraftAction::ImportBlacklist(content) => {
                let count = self.settings.import_blacklist_content(&content);
                Transition::changed(if count > 0 {
                    rules_change
                } else {
                    ChangeSet::NONE
                })
                .with_output(ActionOutput::Count(count))
            }
            DraftAction::ResetBlacklist => {
                self.settings.reset_blacklist();
                Transition::changed(rules_change)
            }
            DraftAction::ClearBlacklist => {
                self.settings.clear_blacklist();
                Transition::changed(rules_change)
            }
            DraftAction::RemoveBlacklist { kind, value } => {
                let before = self.settings.snapshot().folder_blacklist.len()
                    + self.settings.snapshot().ext_blacklist.len();
                self.settings.remove_blacklist_item(kind, &value);
                let after = self.settings.snapshot().folder_blacklist.len()
                    + self.settings.snapshot().ext_blacklist.len();
                changed_transition(before != after, rules_change)
            }
            DraftAction::ToggleLanguage => {
                let language = self.settings.toggle_language();
                Transition::changed(settings_change).with_output(ActionOutput::Language(language))
            }
            DraftAction::SetCompress(value) => {
                let changed = self.settings.snapshot().options.compress != value;
                if changed {
                    self.settings.set_compress(value);
                }
                changed_transition(changed, settings_change)
            }
            DraftAction::SetUseGitignore(value) => {
                let changed = self.settings.snapshot().options.use_gitignore != value;
                if changed {
                    self.settings.set_use_gitignore(value);
                }
                let mut transition = changed_transition(changed, settings_change);
                if changed {
                    transition.effects.push(WorkspaceEffect::RestartPreflight {
                        preserve_completed_status: false,
                    });
                }
                transition
            }
            DraftAction::SetIgnoreGit(value) => {
                let changed = self.settings.snapshot().options.ignore_git != value;
                if changed {
                    self.settings.set_ignore_git(value);
                }
                changed_transition(changed, settings_change.union(rules_change))
            }
            DraftAction::SetOutputFormat(value) => {
                let changed = self.settings.snapshot().options.output_format != value;
                if changed {
                    self.settings.set_output_format(value);
                }
                changed_transition(changed, settings_change)
            }
            DraftAction::SetWhitelistMode(value) => changed_transition(
                self.selection.set_temporary_whitelist_mode(value),
                rules_change,
            ),
            DraftAction::ApplyConfig(config) => {
                self.settings.apply_config(config);
                Transition::changed(settings_change.union(rules_change))
            }
        }
    }

    fn reduce_execution(&mut self, action: ExecutionAction) -> Transition {
        match action {
            ExecutionAction::ClearRuntime { ready_label } => {
                self.process.clear_runtime(ready_label);
                Transition::changed(ChangeSet::EXECUTION)
            }
            ExecutionAction::SetIdleLabel(label) => {
                let state = self.process.state_mut();
                let changed = state.ui_status == crate::ui::state::ProcessUiStatus::Idle
                    && state.processing_current_file != label;
                if changed {
                    state.processing_current_file = label;
                }
                changed_transition(changed, ChangeSet::EXECUTION)
            }
            ExecutionAction::StartRun {
                run_id,
                scanning_label,
            } => {
                self.process.start_run(run_id, scanning_label);
                Transition::changed(ChangeSet::EXECUTION)
            }
            ExecutionAction::BeginPreflight {
                preserve_completed_status,
            } => {
                let revision = self.process.begin_preflight(preserve_completed_status);
                Transition::changed(ChangeSet::EXECUTION)
                    .with_output(ActionOutput::PreflightRevision(revision))
            }
            ExecutionAction::CancelRequested => {
                changed_transition(self.process.cancel_running(), ChangeSet::EXECUTION)
            }
            ExecutionAction::Preflight(event) => {
                let changed = self
                    .process
                    .apply_preflight_event(event, self.settings.language());
                changed_transition(changed, ChangeSet::EXECUTION)
            }
            ExecutionAction::Process(event) => {
                let effect = self
                    .process
                    .apply_process_event(event, self.settings.language());
                let mut changes = ChangeSet::EXECUTION;
                let mut effects = Vec::new();
                match &effect {
                    ProcessEventEffect::Ignored => changes = ChangeSet::NONE,
                    ProcessEventEffect::Continue => {}
                    ProcessEventEffect::Completed(result) => {
                        self.result.set_result((**result).clone());
                        self.preview.clear();
                        self.process.state_mut().finish_run();
                        changes = changes
                            .union(ChangeSet::EXECUTION_RESULT)
                            .union(ChangeSet::PREVIEW);
                        if self.selection.clear_temporary_merge_filters() {
                            changes = changes.union(ChangeSet::DRAFT_RULES);
                        }
                        effects.push(WorkspaceEffect::RestartPreflight {
                            preserve_completed_status: true,
                        });
                    }
                    ProcessEventEffect::Cancelled | ProcessEventEffect::Failed => {
                        self.process.state_mut().finish_run();
                    }
                }
                Transition {
                    changes,
                    effects,
                    output: ActionOutput::Process(effect),
                }
            }
            ExecutionAction::ProcessBatch(events) => {
                let mut batch = Transition::default();
                for event in events {
                    let transition = self.reduce_execution(ExecutionAction::Process(event));
                    batch.changes = batch.changes.union(transition.changes);
                    batch.effects.extend(transition.effects);
                    if !matches!(transition.output, ActionOutput::None) {
                        batch.output = transition.output;
                    }
                }
                batch
            }
            ExecutionAction::FinishRun => {
                self.process.state_mut().finish_run();
                Transition::changed(ChangeSet::EXECUTION)
            }
            ExecutionAction::FailDisconnected(error) => {
                self.process.state_mut().fail_disconnected_run(error);
                Transition::changed(ChangeSet::EXECUTION)
            }
            ExecutionAction::SetResult(result) => {
                self.result.set_result(result);
                self.preview.clear();
                Transition::changed(ChangeSet::EXECUTION_RESULT.union(ChangeSet::PREVIEW))
            }
            ExecutionAction::ClearResult => {
                self.result.clear();
                Transition::changed(ChangeSet::EXECUTION_RESULT)
            }
            ExecutionAction::SetResultTab(tab) => {
                let changed = self.result.state().active_tab != tab;
                if changed {
                    self.result.set_active_tab(tab);
                }
                changed_transition(changed, ChangeSet::EXECUTION_RESULT)
            }
            ExecutionAction::SetPreviewRowCount(count) => {
                let changed = self.result.state().preview_row_count != count;
                if changed {
                    self.result.set_preview_row_count(count);
                }
                changed_transition(changed, ChangeSet::EXECUTION_RESULT)
            }
            ExecutionAction::BeginSave => Transition::changed(ChangeSet::EXECUTION_RESULT)
                .with_output(ActionOutput::SaveRevision(self.result.begin_save())),
            ExecutionAction::FinishSave {
                revision,
                succeeded,
            } => changed_transition(
                self.result.finish_save(revision, succeeded),
                ChangeSet::EXECUTION_RESULT,
            ),
            ExecutionAction::CancelSave(revision) => changed_transition(
                self.result.cancel_save(revision),
                ChangeSet::EXECUTION_RESULT,
            ),
        }
    }

    fn reduce_preview(&mut self, action: PreviewAction) -> Transition {
        let (changes, output, effects) = match action {
            PreviewAction::Clear => {
                let state = self.preview.state();
                let changed = state.selected_preview_file_id.is_some()
                    || state.preview_document.is_some()
                    || state.preview_error.is_some()
                    || state.deferred_preview.is_some()
                    || state.pending_request_type.is_some()
                    || !state.preview_chunks.is_empty();
                if changed {
                    self.preview.clear();
                }
                (
                    if changed {
                        ChangeSet::PREVIEW
                    } else {
                        ChangeSet::NONE
                    },
                    ActionOutput::Changed(changed),
                    Vec::new(),
                )
            }
            PreviewAction::Apply(event) => {
                let effect = self.preview.apply_event(event);
                (
                    if matches!(&effect, PreviewEventEffect::Ignored) {
                        ChangeSet::NONE
                    } else {
                        ChangeSet::PREVIEW
                    },
                    ActionOutput::Preview(effect),
                    Vec::new(),
                )
            }
            PreviewAction::ApplyMany(events) => {
                let effect = self.preview.apply_events(events);
                (
                    if matches!(&effect, PreviewEventEffect::Ignored) {
                        ChangeSet::NONE
                    } else {
                        ChangeSet::PREVIEW
                    },
                    ActionOutput::Preview(effect),
                    Vec::new(),
                )
            }
            PreviewAction::ClearRequest => {
                self.preview.clear_request();
                (ChangeSet::PREVIEW, ActionOutput::None, Vec::new())
            }
            PreviewAction::Open { file_id, path } => {
                let request = self.preview.open_preview(file_id, path);
                (
                    ChangeSet::PREVIEW,
                    ActionOutput::PreviewRequest(Some(request.clone())),
                    vec![WorkspaceEffect::StartPreview(request)],
                )
            }
            PreviewAction::OpenDeferredFull { file_id, path } => {
                let request = self.preview.open_deferred_full_preview(file_id, path);
                (
                    ChangeSet::PREVIEW,
                    ActionOutput::PreviewRequest(Some(request.clone())),
                    vec![WorkspaceEffect::StartPreview(request)],
                )
            }
            PreviewAction::OpenDeferredExcerpt {
                file_id,
                source_path,
                source_byte_len,
                excerpt_byte_len,
                excerpt_path,
            } => {
                let request = self.preview.open_deferred_excerpt_preview(
                    file_id,
                    source_path,
                    source_byte_len,
                    excerpt_byte_len,
                    excerpt_path,
                );
                (
                    ChangeSet::PREVIEW,
                    ActionOutput::PreviewRequest(Some(request.clone())),
                    vec![WorkspaceEffect::StartPreview(request)],
                )
            }
            PreviewAction::Defer {
                file_id,
                source_path,
                source_byte_len,
                excerpt_byte_len,
            } => {
                self.preview
                    .defer_preview(file_id, source_path, source_byte_len, excerpt_byte_len);
                (ChangeSet::PREVIEW, ActionOutput::None, Vec::new())
            }
            PreviewAction::SetError(error) => {
                self.preview.set_preview_error_message(error);
                (ChangeSet::PREVIEW, ActionOutput::None, Vec::new())
            }
            PreviewAction::RequestRange { range, direction } => {
                let request = self.preview.load_preview_range_request(range, direction);
                let effects = request
                    .clone()
                    .map(WorkspaceEffect::StartPreview)
                    .into_iter()
                    .collect();
                (
                    if request.is_some() {
                        ChangeSet::PREVIEW
                    } else {
                        ChangeSet::NONE
                    },
                    ActionOutput::PreviewRequest(request),
                    effects,
                )
            }
            PreviewAction::RequestQueuedRange { direction } => {
                let request = if self.preview.state().preview_requested_range.is_some() {
                    None
                } else {
                    self.preview
                        .take_queued_preview_range()
                        .and_then(|range| self.preview.load_preview_range_request(range, direction))
                };
                let effects = request
                    .clone()
                    .map(WorkspaceEffect::StartPreview)
                    .into_iter()
                    .collect();
                (
                    if request.is_some() {
                        ChangeSet::PREVIEW
                    } else {
                        ChangeSet::NONE
                    },
                    ActionOutput::PreviewRequest(request),
                    effects,
                )
            }
        };
        Transition {
            changes,
            effects,
            output,
        }
    }

    fn reduce_navigation(&mut self, action: NavigationAction) -> Transition {
        let (changed, changes) = match action {
            NavigationAction::ClearPendingConfirmation => (
                self.navigation.clear_pending_confirmation(),
                ChangeSet::NAVIGATION,
            ),
            NavigationAction::SetPendingConfirmation(value) => (
                self.navigation.set_pending_confirmation(value),
                ChangeSet::NAVIGATION,
            ),
            NavigationAction::SetSidePanelTab(value) => (
                self.navigation.set_side_panel_tab(value),
                ChangeSet::NAVIGATION,
            ),
            NavigationAction::SetNarrowContentTab(value) => (
                self.navigation.set_narrow_content_tab(value),
                ChangeSet::NAVIGATION,
            ),
            NavigationAction::SetContentFileListCollapsed(value) => (
                self.navigation.set_content_file_list_collapsed(value),
                ChangeSet::NAVIGATION,
            ),
            NavigationAction::SetSelectedFilesPanelHeight(value) => (
                self.navigation.set_selected_files_panel_height(value),
                ChangeSet::NAVIGATION,
            ),
            NavigationAction::ResetTree => {
                let changed = self.navigation_tree != Default::default();
                self.navigation_tree = TreePanelState::default();
                (changed, ChangeSet::NAVIGATION_TREE)
            }
            NavigationAction::SetTreeState(value) => {
                let changed = self.navigation_tree != value;
                self.navigation_tree = value;
                (changed, ChangeSet::NAVIGATION_TREE)
            }
        };
        changed_transition(changed, changes)
    }

    pub fn selection(&self) -> &SelectionState {
        self.selection.state()
    }

    pub fn selection_snapshot(&self) -> SelectionState {
        self.selection.snapshot()
    }

    pub fn settings(&self) -> SettingsState {
        self.settings.snapshot()
    }

    pub fn config(&self) -> AppConfigV1 {
        self.settings.to_config()
    }

    pub fn language(&self) -> Language {
        self.settings.language()
    }

    pub fn effective_filters(&self) -> EffectiveMergeFilters {
        self.settings.effective_filters(self.selection.state())
    }

    pub fn process(&self) -> &ProcessState {
        self.process.state()
    }

    pub fn result(&self) -> &ResultState {
        self.result.state()
    }

    pub fn preview(&self) -> &PreviewPanelState {
        self.preview.state()
    }

    pub fn preview_model(&self) -> &PreviewModel {
        &self.preview
    }

    pub fn navigation(&self) -> WorkspaceUiState {
        self.navigation.state()
    }

    pub fn tree(&self) -> &crate::ui::state::TreePanelState {
        &self.navigation_tree
    }

    pub fn revisions(&self) -> WorkspaceRevisions {
        self.revisions
    }

    pub fn has_inputs(&self) -> bool {
        self.selection.has_inputs()
    }

    pub fn is_processing(&self) -> bool {
        self.process.is_processing()
    }

    pub fn result_has_content(&self) -> bool {
        self.result.has_content_result()
    }

    pub fn result_is_tree_only(&self) -> bool {
        self.result.is_tree_only_result()
    }
}

fn changed_transition(changed: bool, changes: ChangeSet) -> Transition {
    Transition::changed(if changed { changes } else { ChangeSet::NONE })
        .with_output(ActionOutput::Changed(changed))
}

#[cfg(test)]
mod tests {
    use crate::application::store::{
        ActionOutput, ChangeSet, DraftAction, ExecutionAction, NavigationAction, WorkspaceAction,
        WorkspaceEffect, WorkspaceStore,
    };
    use crate::domain::{AppConfigV1, ProcessResult};
    use crate::processor::stats::ProcessingStats;
    use crate::services::process::ProcessEvent;
    use crate::ui::state::SidePanelTab;

    #[test]
    fn no_op_navigation_action_does_not_advance_revision() {
        let mut store = WorkspaceStore::new(AppConfigV1::default(), "ready".into());

        let transition = store.dispatch(WorkspaceAction::Navigation(
            NavigationAction::SetSidePanelTab(SidePanelTab::Results),
        ));

        assert!(transition.changes.is_empty());
        assert_eq!(store.revisions().navigation, 0);
    }

    #[test]
    fn gitignore_toggle_changes_settings_and_requests_preflight_only() {
        let mut store = WorkspaceStore::new(AppConfigV1::default(), "ready".into());

        let transition =
            store.dispatch(WorkspaceAction::Draft(DraftAction::SetUseGitignore(false)));

        assert!(transition.changes.intersects(ChangeSet::DRAFT_SETTINGS));
        assert!(!transition.changes.intersects(ChangeSet::DRAFT_SELECTION));
        assert_eq!(transition.effects.len(), 1);
    }

    #[test]
    fn clear_inputs_is_one_atomic_transition() {
        let mut store = WorkspaceStore::new(AppConfigV1::default(), "ready".into());
        let _ = store.dispatch(WorkspaceAction::Draft(DraftAction::SetDedupe(true)));

        let transition = store.dispatch(WorkspaceAction::ClearInputs {
            ready_label: "ready".into(),
        });

        assert_eq!(transition.changes, ChangeSet::ALL);
        assert!(!store.has_inputs());
        assert!(matches!(transition.output, ActionOutput::None));
        assert_eq!(transition.effects.len(), 2);
    }

    #[test]
    fn stale_process_event_is_a_true_no_op() {
        let mut store = WorkspaceStore::new(AppConfigV1::default(), "ready".into());
        let _ = store.dispatch(WorkspaceAction::Execution(ExecutionAction::StartRun {
            run_id: 2,
            scanning_label: "scanning".into(),
        }));
        let revision = store.revisions().execution;

        let transition = store.dispatch(WorkspaceAction::Execution(ExecutionAction::Process(
            ProcessEvent::scanning(1, 100, 80, 20),
        )));

        assert!(transition.changes.is_empty());
        assert_eq!(store.revisions().execution, revision);
    }

    #[test]
    fn process_completion_installs_result_and_clears_temporary_rules_atomically() {
        let mut store = WorkspaceStore::new(AppConfigV1::default(), "ready".into());
        let _ = store.dispatch(WorkspaceAction::Draft(DraftAction::AddTemporaryBlacklist {
            tokens: vec!["target".into()],
            as_ext: false,
        }));
        let _ = store.dispatch(WorkspaceAction::Execution(ExecutionAction::StartRun {
            run_id: 7,
            scanning_label: "scanning".into(),
        }));
        let result = ProcessResult {
            stats: ProcessingStats::default(),
            tree_string: String::new(),
            tree_nodes: Vec::new(),
            process_dir: None,
            merged_content_path: None,
            suggested_result_name: "result.txt".into(),
            file_details: Vec::new(),
            preview_files: Vec::new(),
            preview_blob_dir: None,
        };

        let transition = store.dispatch(WorkspaceAction::Execution(ExecutionAction::Process(
            ProcessEvent::completed(7, result),
        )));

        assert!(transition.changes.intersects(ChangeSet::EXECUTION));
        assert!(transition.changes.intersects(ChangeSet::EXECUTION_RESULT));
        assert!(transition.changes.intersects(ChangeSet::DRAFT_RULES));
        assert!(store.result().result.is_some());
        assert!(store.selection().temp_folder_blacklist.is_empty());
        assert!(store.process().current_run_id.is_none());
        assert!(matches!(
            transition.effects.as_slice(),
            [WorkspaceEffect::RestartPreflight {
                preserve_completed_status: true
            }]
        ));
    }
}
