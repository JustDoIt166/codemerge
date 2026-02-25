use std::path::PathBuf;

use crate::app::model::{
    Language, OutputFormat, OutputTab, PreflightStats, ProcessResult, ProcessingMode,
    StatsDetailType,
};

#[derive(Debug, Clone)]
pub enum Message {
    File(FileMessage),
    Config(ConfigMessage),
    Blacklist(BlacklistMessage),
    Process(ProcessMessage),
    Ui(UiMessage),
    I18n(I18nMessage),
    ConfigSaved(Result<(), String>),
    Tick,
}

#[derive(Debug, Clone)]
pub enum FileMessage {
    SelectFolder,
    SelectFiles,
    SelectGitignore,
    ApplyGitignore,
    RemoveFile(usize),
    ClearAllFiles,
}

#[derive(Debug, Clone)]
pub enum ConfigMessage {
    ToggleCompress(bool),
    ToggleUseGitignore(bool),
    ToggleIgnoreGit(bool),
    SetOutputFormat(OutputFormat),
    SetMode(ProcessingMode),
    ToggleDedupe(bool),
}

#[derive(Debug, Clone)]
pub enum BlacklistMessage {
    SharedInputChanged(String),
    FolderInputChanged(String),
    ExtInputChanged(String),
    FilterInputChanged(String),
    AddFolder,
    RemoveFolder(String),
    AddExt,
    RemoveExt(String),
    ToggleSelectAll,
    ToggleInvertSelection,
    ToggleSelect(String),
    DeleteSelected,
    ResetToDefault,
    ClearAll,
    Export,
    ImportAppend,
    ImportReplace,
    SaveSettings,
}

#[derive(Debug, Clone)]
pub enum ProcessMessage {
    Start,
    Cancel,
    Completed(Result<ProcessResult, String>),
    Record(ProgressUpdate),
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    Reset,
    ConfirmReset,
    CancelReset,
    RequestCancel,
    ConfirmCancel,
    CancelCancel,
    ExpandStats(Option<StatsDetailType>),
    CopyTree,
    CopyContent,
    DownloadContent,
    PreviewFilterChanged(String),
    SelectPreviewFile(u32),
    LoadPreviewPage { file_id: u32, offset: u64 },
    PreviewPageLoaded(Result<PreviewPagePayload, String>),
    PreviewNextPage,
    PreviewPrevPage,
    SwitchOutputTab(OutputTab),
    ToggleConfigExpanded,
    ToggleBlacklistExpanded,
    DismissToast,
    PreflightUpdate(PreflightUpdate),
    Resize(f32, f32),
}

#[derive(Debug, Clone)]
pub struct PreviewPagePayload {
    pub file_id: u32,
    pub offset: u64,
    pub loaded_bytes: u64,
    pub total_bytes: u64,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum I18nMessage {
    ToggleLanguage,
    Set(Language),
}

#[derive(Debug, Clone)]
pub enum ProgressUpdate {
    Scanning {
        scanned: usize,
        candidates: usize,
        skipped: usize,
    },
    Success {
        file: String,
        chars: usize,
        tokens: usize,
    },
    Skipped {
        file: String,
        reason: String,
    },
    Failed {
        file: String,
        error: String,
    },
    Finished(ProcessResult),
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum PreflightUpdate {
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
pub struct ProcessContext {
    pub selected_folder: Option<PathBuf>,
    pub selected_files: Vec<PathBuf>,
    pub folder_blacklist: Vec<String>,
    pub ext_blacklist: Vec<String>,
    pub options: crate::app::model::ProcessingOptions,
    pub cancel_token: tokio_util::sync::CancellationToken,
    pub language: Language,
}

impl ProcessContext {
    pub fn new(
        model: &crate::app::model::Model,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Self {
        Self {
            selected_folder: model.selected_folder.clone(),
            selected_files: model
                .selected_files
                .iter()
                .map(|f| f.path.clone())
                .collect(),
            folder_blacklist: model.folder_blacklist.clone(),
            ext_blacklist: model.ext_blacklist.clone(),
            options: model.options.clone(),
            cancel_token,
            language: model.language,
        }
    }
}
