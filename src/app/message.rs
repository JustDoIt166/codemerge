use std::path::PathBuf;

use crate::app::model::{
    Language, OutputFormat, OutputTab, ProcessResult, ProcessingMode, StatsDetailType,
};

#[derive(Debug, Clone)]
pub enum Message {
    File(FileMessage),
    Config(ConfigMessage),
    Blacklist(BlacklistMessage),
    Process(ProcessMessage),
    Ui(UiMessage),
    I18n(I18nMessage),
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
    LoadPreview,
    PreviewLoaded(Result<PreviewPayload, String>),
    LoadAllPreview,
    ConfirmLoadAllPreview,
    CancelLoadAllPreview,
    SwitchOutputTab(OutputTab),
    ToggleConfigExpanded,
    ToggleBlacklistExpanded,
    DismissToast,
    Resize(f32, f32),
}

#[derive(Debug, Clone)]
pub struct PreviewPayload {
    pub content: String,
    pub loaded_all: bool,
}

#[derive(Debug, Clone)]
pub enum I18nMessage {
    ToggleLanguage,
    Set(Language),
}

#[derive(Debug, Clone)]
pub enum ProgressUpdate {
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
