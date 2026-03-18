use std::collections::BTreeSet;

use gpui::{App, ClickEvent, Context, Entity, SharedString, Window};
use gpui_component::{
    WindowExt as _,
    input::{InputEvent, InputState},
    notification::NotificationType,
    table::{TableEvent, TableState},
};

use super::model;
use super::view::{TreeExpansionMode, copy_to_clipboard};
use super::{
    BlacklistItemKind, NarrowContentTab, PendingConfirmation, PreviewTableDelegate, SidePanelTab,
    Workspace,
};
use crate::domain::{FileEntry, ResultTab};
use crate::services::preview::load_text;
use crate::services::process::ProcessRequest;
use crate::utils::i18n::tr;
use crate::utils::path::filename;

impl Workspace {
    fn notify_active_window(cx: &mut App, kind: NotificationType, message: impl Into<String>) {
        if let Some(window) = cx.active_window() {
            let message = SharedString::from(message.into());
            let _ = window.update(cx, |_, window, cx| {
                window.push_notification((kind, message), cx);
            });
        }
    }

    fn persist_settings_async(&self, cx: &mut Context<Self>) {
        let config = self.state.to_config();
        cx.spawn(async move |this, cx| {
            let result =
                crate::services::settings::execute(crate::domain::SettingsCommand::Save(config));
            let _ = this.update(cx, |_, cx| {
                if let Err(err) = result {
                    Self::notify_active_window(cx, NotificationType::Error, err.to_string());
                }
            });
        })
        .detach();
    }

    pub(super) fn push_notice(
        &self,
        kind: NotificationType,
        message: impl Into<String>,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.push_notification((kind, SharedString::from(message.into())), cx);
    }

    pub(super) fn apply_selected_folder_path(
        &mut self,
        path: std::path::PathBuf,
        gitignore_rules: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.state.selection.selected_folder = Some(path);
        self.state.selection.gitignore_rules = gitignore_rules;
        self.refresh_preflight(cx);
        cx.notify();
    }

    pub(super) fn apply_selected_files(&mut self, files: Vec<FileEntry>, cx: &mut Context<Self>) {
        let mut existing = self
            .state
            .selection
            .selected_files
            .iter()
            .map(|entry| entry.path.to_string_lossy().to_string())
            .collect::<BTreeSet<_>>();
        for entry in files {
            let key = entry.path.to_string_lossy().to_string();
            if self.state.selection.dedupe_exact_path && !existing.insert(key) {
                continue;
            }
            self.state.selection.selected_files.push(entry);
        }
        self.refresh_preflight(cx);
        cx.notify();
    }

    pub(super) fn apply_selected_gitignore(
        &mut self,
        path: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.state.selection.gitignore_file = path;
        cx.notify();
    }

    pub(super) fn on_preview_filter_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.clear_preview_state();
            self.sync_preview_table(cx);
        }
    }

    pub(super) fn on_tree_filter_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.sync_tree(cx);
        }
    }

    pub(super) fn on_blacklist_filter_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            cx.notify();
        }
    }

    pub(super) fn on_preview_table_event(
        &mut self,
        table: &Entity<TableState<PreviewTableDelegate>>,
        event: &TableEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let TableEvent::SelectRow(ix) | TableEvent::DoubleClickedRow(ix) = event
            && let Some(row) = table.read(cx).delegate().rows.get(*ix)
        {
            self.load_preview(row.id, cx);
        }
    }

    pub(super) fn toggle_language(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.settings.language = self.state.settings.language.toggle();
        self.persist_settings_async(cx);
        self.sync_localized_inputs(window, cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.state.settings.language, "language_updated"),
            window,
            cx,
        );
        cx.notify();
    }

    pub(super) fn select_folder(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = window;
        cx.spawn(async move |this, cx| {
            let picked = rfd::AsyncFileDialog::new()
                .pick_folder()
                .await
                .map(|handle| handle.path().to_path_buf());
            let gitignore_rules = picked
                .as_ref()
                .map(|path| {
                    let gitignore = crate::processor::walker::auto_gitignore_path(path);
                    std::fs::read_to_string(gitignore)
                        .ok()
                        .map(|content| crate::processor::walker::parse_gitignore_rules(&content))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            let _ = this.update(cx, |this, cx| {
                if let Some(path) = picked {
                    this.apply_selected_folder_path(path, gitignore_rules, cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn select_files(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = window;
        cx.spawn(async move |this, cx| {
            let picked = rfd::AsyncFileDialog::new()
                .pick_files()
                .await
                .map(|handles| {
                    handles
                        .into_iter()
                        .map(|handle| {
                            let path = handle.path().to_path_buf();
                            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            FileEntry {
                                name: filename(&path),
                                path,
                                size,
                            }
                        })
                        .collect::<Vec<_>>()
                });
            let _ = this.update(cx, |this, cx| {
                if let Some(files) = picked {
                    this.apply_selected_files(files, cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn select_gitignore(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = window;
        cx.spawn(async move |this, cx| {
            let picked = rfd::AsyncFileDialog::new()
                .add_filter("gitignore", &["gitignore"])
                .pick_file()
                .await
                .map(|handle| handle.path().to_path_buf());
            let _ = this.update(cx, |this, cx| this.apply_selected_gitignore(picked, cx));
        })
        .detach();
    }

    pub(super) fn apply_gitignore(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = &self.state.selection.gitignore_file else {
            self.push_notice(
                NotificationType::Warning,
                tr(self.state.settings.language, "gitignore_required"),
                window,
                cx,
            );
            return;
        };
        let path = path.clone();
        let language = self.state.settings.language;
        let _ = window;
        cx.spawn(async move |this, cx| {
            let loaded = std::fs::read_to_string(&path)
                .map(|content| crate::processor::walker::parse_gitignore_rules(&content));
            let _ = this.update(cx, |this, cx| match loaded {
                Ok(rules) => {
                    for rule in rules {
                        if !this.state.settings.folder_blacklist.contains(&rule) {
                            this.state.settings.folder_blacklist.push(rule);
                        }
                    }
                    this.persist_settings_async(cx);
                    this.refresh_preflight(cx);
                    Self::notify_active_window(
                        cx,
                        NotificationType::Success,
                        tr(language, "blacklist_saved"),
                    );
                    cx.notify();
                }
                Err(err) => Self::notify_active_window(
                    cx,
                    NotificationType::Error,
                    format!("{}{}", tr(language, "read_gitignore_failed"), err),
                ),
            });
        })
        .detach();
    }

    pub(super) fn clear_inputs(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_confirmation != Some(PendingConfirmation::ClearInputs) {
            self.pending_confirmation = Some(PendingConfirmation::ClearInputs);
            self.push_notice(
                NotificationType::Warning,
                tr(self.state.settings.language, "confirm_clear_notice"),
                window,
                cx,
            );
            cx.notify();
            return;
        }
        self.clear_pending_confirmation();
        if let Some(handle) = self.state.process.process_handle.as_ref() {
            handle.cancel.cancel();
        }
        self.state.process.preflight_rx = None;
        self.state.workspace.preview_panel.preview_rx = None;
        self.cleanup_current_result_artifacts();
        let status_ready = tr(self.state.settings.language, "status_ready").to_string();
        self.state.clear_inputs(status_ready);
        self.tree_panel.state.update(cx, |state, tree_cx| {
            state.set_selected_index(None, tree_cx);
        });
        self.tree_panel.data = None;
        self.tree_panel.render_state = model::TreeRenderState::default();
        self.tree_panel.last_interaction = None;
        self.sync_tree(cx);
        self.sync_preview_table(cx);
        self.refresh_preflight(cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.state.settings.language, "files_cleared"),
            window,
            cx,
        );
        cx.notify();
    }

    pub(super) fn start_process(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_processing() {
            return;
        }
        if self.state.selection.selected_folder.is_none()
            && self.state.selection.selected_files.is_empty()
        {
            self.push_notice(
                NotificationType::Error,
                tr(self.state.settings.language, "no_input_selected"),
                window,
                cx,
            );
            return;
        }
        self.cleanup_current_result_artifacts();
        self.state.result = crate::ui::state::ResultState::default();
        self.state.workspace.reset_tree();
        self.tree_panel.data = None;
        self.tree_panel.render_state = model::TreeRenderState::default();
        self.tree_panel.last_interaction = None;
        self.clear_preview_state();
        self.clear_pending_confirmation();
        self.sync_tree(cx);
        self.state
            .process
            .reset_for_run(tr(self.state.settings.language, "scanning_files").to_string());
        self.state.process.process_handle = Some(crate::services::process::start(ProcessRequest {
            selected_folder: self.state.selection.selected_folder.clone(),
            selected_files: self
                .state
                .selection
                .selected_files
                .iter()
                .map(|entry| entry.path.clone())
                .collect(),
            folder_blacklist: self.state.effective_folder_blacklist(),
            ext_blacklist: self.state.settings.ext_blacklist.clone(),
            options: self.state.settings.options.clone(),
            language: self.state.settings.language,
        }));
        self.ensure_background_polling(cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.state.settings.language, "process_started"),
            window,
            cx,
        );
        cx.notify();
    }

    pub(super) fn cancel_process(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(handle) = &self.state.process.process_handle {
            handle.cancel.cancel();
            self.state.process.ui_status = crate::ui::state::ProcessUiStatus::Cancelled;
            self.push_notice(
                NotificationType::Info,
                tr(self.state.settings.language, "cancelled"),
                window,
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn add_folder_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation();
        let added = self.consume_blacklist_input(false, window, cx);
        if added > 0 {
            self.refresh_preflight(cx);
        }
    }

    pub(super) fn add_ext_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation();
        let added = self.consume_blacklist_input(true, window, cx);
        if added > 0 {
            self.refresh_preflight(cx);
        }
    }

    pub(super) fn consume_blacklist_input(
        &mut self,
        as_ext: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        let raw = self.blacklist_add_input.read(cx).value().to_string();
        let tokens = raw
            .split(|ch: char| ch == ',' || ch == '\n' || ch.is_whitespace())
            .filter(|part| !part.trim().is_empty())
            .map(str::trim)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if tokens.is_empty() {
            self.push_notice(
                NotificationType::Warning,
                tr(self.state.settings.language, "blacklist_empty"),
                window,
                cx,
            );
            return 0;
        }

        let mut added = 0;
        for token in tokens {
            if as_ext {
                let normalized = crate::processor::walker::normalize_ext(&token);
                if !self.state.settings.ext_blacklist.contains(&normalized) {
                    self.state.settings.ext_blacklist.push(normalized);
                    added += 1;
                }
            } else if !self.state.settings.folder_blacklist.contains(&token) {
                self.state.settings.folder_blacklist.push(token);
                added += 1;
            }
        }
        self.blacklist_add_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.persist_settings_async(cx);
        self.push_notice(
            NotificationType::Success,
            tr(self.state.settings.language, "blacklist_added"),
            window,
            cx,
        );
        added
    }

    pub(super) fn export_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut body = String::new();
        body.push_str("# folders\n");
        for item in &self.state.settings.folder_blacklist {
            body.push_str(item);
            body.push('\n');
        }
        body.push_str("# extensions\n");
        for item in &self.state.settings.ext_blacklist {
            body.push_str(item);
            body.push('\n');
        }
        let language = self.state.settings.language;
        let _ = window;
        cx.spawn(async move |this, cx| {
            let path = rfd::AsyncFileDialog::new()
                .set_file_name("codemerge-blacklist.txt")
                .save_file()
                .await
                .map(|handle| handle.path().to_path_buf());

            let Some(path) = path else {
                return;
            };

            let notice = match std::fs::write(path, body) {
                Ok(_) => (
                    NotificationType::Success,
                    tr(language, "blacklist_exported").to_string(),
                ),
                Err(err) => (NotificationType::Error, err.to_string()),
            };

            let _ = this.update(cx, |_, cx| {
                if let Some(window) = cx.active_window() {
                    let _ = window.update(cx, |_, window, cx| {
                        window.push_notification((notice.0, SharedString::from(notice.1)), cx);
                    });
                }
            });
        })
        .detach();
    }

    pub(super) fn import_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation();
        let _ = window;
        cx.spawn(async move |this, cx| {
            let path = rfd::AsyncFileDialog::new()
                .pick_file()
                .await
                .map(|handle| handle.path().to_path_buf());

            let Some(path) = path else {
                return;
            };

            let content = std::fs::read_to_string(path);
            let _ = this.update(cx, |this, cx| match content {
                Ok(content) => {
                    for line in content
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    {
                        if line.starts_with('.') {
                            let ext = crate::processor::walker::normalize_ext(line);
                            if !this.state.settings.ext_blacklist.contains(&ext) {
                                this.state.settings.ext_blacklist.push(ext);
                            }
                        } else if !this
                            .state
                            .settings
                            .folder_blacklist
                            .contains(&line.to_string())
                        {
                            this.state.settings.folder_blacklist.push(line.to_string());
                        }
                    }
                    this.persist_settings_async(cx);
                    this.refresh_preflight(cx);
                    Self::notify_active_window(
                        cx,
                        NotificationType::Success,
                        tr(this.state.settings.language, "blacklist_imported"),
                    );
                    cx.notify();
                }
                Err(err) => {
                    Self::notify_active_window(cx, NotificationType::Error, err.to_string())
                }
            });
        })
        .detach();
    }

    pub(super) fn reset_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_confirmation != Some(PendingConfirmation::ResetBlacklist) {
            self.pending_confirmation = Some(PendingConfirmation::ResetBlacklist);
            self.push_notice(
                NotificationType::Warning,
                tr(self.state.settings.language, "confirm_reset_notice"),
                window,
                cx,
            );
            cx.notify();
            return;
        }
        self.clear_pending_confirmation();
        self.state.settings.folder_blacklist = crate::domain::default_folder_blacklist();
        self.state.settings.ext_blacklist = crate::domain::default_ext_blacklist();
        self.persist_settings_async(cx);
        self.refresh_preflight(cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.state.settings.language, "blacklist_reset_default"),
            window,
            cx,
        );
        cx.notify();
    }

    pub(super) fn clear_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_confirmation != Some(PendingConfirmation::ClearBlacklist) {
            self.pending_confirmation = Some(PendingConfirmation::ClearBlacklist);
            self.push_notice(
                NotificationType::Warning,
                tr(self.state.settings.language, "confirm_clear_notice"),
                window,
                cx,
            );
            cx.notify();
            return;
        }
        self.clear_pending_confirmation();
        self.state.settings.folder_blacklist.clear();
        self.state.settings.ext_blacklist.clear();
        self.persist_settings_async(cx);
        self.refresh_preflight(cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.state.settings.language, "blacklist_cleared"),
            window,
            cx,
        );
        cx.notify();
    }

    pub(super) fn toggle_compress(
        &mut self,
        checked: &bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation();
        self.state.settings.options.compress = *checked;
        self.persist_settings_async(cx);
        cx.notify();
    }

    pub(super) fn toggle_use_gitignore(
        &mut self,
        checked: &bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation();
        self.state.settings.options.use_gitignore = *checked;
        self.persist_settings_async(cx);
        self.refresh_preflight(cx);
        cx.notify();
    }

    pub(super) fn toggle_ignore_git(
        &mut self,
        checked: &bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation();
        self.state.settings.options.ignore_git = *checked;
        if *checked {
            if !self
                .state
                .settings
                .folder_blacklist
                .contains(&".git".to_string())
            {
                self.state
                    .settings
                    .folder_blacklist
                    .push(".git".to_string());
            }
        } else {
            self.state
                .settings
                .folder_blacklist
                .retain(|item| item != ".git");
        }
        self.persist_settings_async(cx);
        self.refresh_preflight(cx);
        cx.notify();
    }

    pub(super) fn toggle_dedupe(&mut self, checked: &bool, _: &mut Window, cx: &mut Context<Self>) {
        self.clear_pending_confirmation();
        self.state.selection.dedupe_exact_path = *checked;
        cx.notify();
    }

    pub(super) fn set_tab(&mut self, ix: &usize, _: &mut Window, cx: &mut Context<Self>) {
        if *ix == 1 && !self.state.has_content_result() {
            self.state.result.active_tab = ResultTab::Tree;
            cx.notify();
            return;
        }
        self.state.result.active_tab = if *ix == 0 {
            ResultTab::Tree
        } else {
            ResultTab::Content
        };
        if self.state.result.active_tab == ResultTab::Content {
            self.ensure_background_polling(cx);
        }
        cx.notify();
    }

    pub(super) fn set_side_panel_tab(
        &mut self,
        ix: &usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.side_panel_tab = if *ix == 0 {
            SidePanelTab::Results
        } else {
            SidePanelTab::Rules
        };
        cx.notify();
    }

    pub(super) fn set_narrow_content_tab(
        &mut self,
        ix: &usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.narrow_content_tab = if *ix == 0 {
            NarrowContentTab::Status
        } else {
            NarrowContentTab::Results
        };
        cx.notify();
    }

    pub(super) fn expand_tree(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.sync_tree_with_mode(TreeExpansionMode::ExpandAll, cx);
    }

    pub(super) fn collapse_tree(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.sync_tree_with_mode(TreeExpansionMode::CollapseAll, cx);
    }

    pub(super) fn copy_tree(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(result) = &self.state.result.result {
            copy_to_clipboard(
                &result.tree_string,
                self.state.settings.language,
                window,
                cx,
            );
        } else {
            self.push_notice(
                NotificationType::Warning,
                tr(self.state.settings.language, "no_tree"),
                window,
                cx,
            );
        }
    }

    pub(super) fn copy_preview(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = &self.state.workspace.preview_panel.preview_document else {
            self.push_notice(
                NotificationType::Warning,
                tr(self.state.settings.language, "no_content"),
                window,
                cx,
            );
            return;
        };
        let document = document.clone();
        let language = self.state.settings.language;
        let _ = window;
        cx.spawn(async move |this, cx| {
            let result = load_text(&document);
            let _ = this.update(cx, |_, cx| match result {
                Ok(content) => {
                    if let Some(window) = cx.active_window() {
                        let _ = window.update(cx, |_, window, cx| {
                            copy_to_clipboard(&content, language, window, cx);
                        });
                    }
                }
                Err(err) => Self::notify_active_window(
                    cx,
                    NotificationType::Error,
                    format!("{}{}", tr(language, "copy_failed"), err),
                ),
            });
        })
        .detach();
    }

    pub(super) fn remove_blacklist_item(
        &mut self,
        kind: BlacklistItemKind,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation();
        match kind {
            BlacklistItemKind::Folder => self
                .state
                .settings
                .folder_blacklist
                .retain(|item| item != &value),
            BlacklistItemKind::Ext => self
                .state
                .settings
                .ext_blacklist
                .retain(|item| item != &value),
        }
        self.persist_settings_async(cx);
        self.refresh_preflight(cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.state.settings.language, "blacklist_item_removed"),
            window,
            cx,
        );
        cx.notify();
    }

    pub(super) fn download_result(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(result) = &self.state.result.result else {
            return;
        };
        let Some(path) = &result.merged_content_path else {
            self.push_notice(
                NotificationType::Warning,
                tr(self.state.settings.language, "mode_tree_only_desc"),
                window,
                cx,
            );
            return;
        };
        let extension = match self.state.settings.options.output_format {
            crate::domain::OutputFormat::Xml => "xml",
            crate::domain::OutputFormat::Markdown => "md",
            crate::domain::OutputFormat::PlainText => "txt",
            crate::domain::OutputFormat::Default => "txt",
        };
        let source_path = path.clone();
        let suggested_name = format!("codemerge-output.{extension}");
        let language = self.state.settings.language;
        let _ = window;
        cx.spawn(async move |this, cx| {
            let save_path = rfd::AsyncFileDialog::new()
                .set_file_name(&suggested_name)
                .save_file()
                .await
                .map(|handle| handle.path().to_path_buf());

            let Some(save_path) = save_path else {
                return;
            };

            let notice = match std::fs::copy(&source_path, save_path) {
                Ok(_) => (NotificationType::Success, tr(language, "saved").to_string()),
                Err(err) => (
                    NotificationType::Error,
                    format!("{}{}", tr(language, "save_failed"), err),
                ),
            };

            let _ = this.update(cx, |_, cx| {
                if let Some(window) = cx.active_window() {
                    let _ = window.update(cx, |_, window, cx| {
                        window.push_notification((notice.0, SharedString::from(notice.1)), cx);
                    });
                }
            });
        })
        .detach();
    }
}
