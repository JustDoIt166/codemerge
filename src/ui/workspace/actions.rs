use gpui::{App, ClickEvent, Context, Entity, SharedString, Window};
use gpui_component::{
    WindowExt as _,
    input::{InputEvent, InputState},
    notification::NotificationType,
    table::{TableEvent, TableState},
};
#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::Input::KeyboardAndMouse::ReleaseCapture,
    UI::WindowsAndMessaging::{
        HTCAPTION, SW_RESTORE, SendMessageW, ShowWindowAsync, WM_NCLBUTTONDOWN,
    },
};

use super::model;
use super::view::{TreeExpansionMode, copy_to_clipboard};
use super::{BlacklistItemKind, PreviewTableDelegate, Workspace};
use crate::domain::{FileEntry, OutputFormat, ResultTab};
use crate::services::external_link;
use crate::services::process::ProcessRequest;
use crate::ui::state::{NarrowContentTab, PendingConfirmation, SidePanelTab};
use crate::utils::app_metadata;
use crate::utils::i18n::tr;
use crate::utils::path::filename;

impl Workspace {
    fn parse_blacklist_tokens(raw: &str) -> Vec<String> {
        raw.split(|ch: char| ch == ',' || ch == '\n' || ch.is_whitespace())
            .filter(|part| !part.trim().is_empty())
            .map(str::trim)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    }

    fn notify_active_window(cx: &mut App, kind: NotificationType, message: impl Into<String>) {
        if let Some(window) = cx.active_window() {
            let message = SharedString::from(message.into());
            let _ = window.update(cx, |_, window, cx| {
                window.push_notification((kind, message), cx);
            });
        }
    }

    fn persist_settings_async(&self, cx: &mut Context<Self>) {
        let config = self.settings.read(cx).to_config();
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
        self.selection.update(cx, |selection, selection_cx| {
            selection.set_selected_folder(path, gitignore_rules);
            selection_cx.notify();
        });
        self.refresh_preflight(cx);
    }

    pub(super) fn apply_selected_files(&mut self, files: Vec<FileEntry>, cx: &mut Context<Self>) {
        self.selection.update(cx, |selection, selection_cx| {
            selection.add_selected_files(files);
            selection_cx.notify();
        });
        self.refresh_preflight(cx);
    }

    pub(super) fn apply_selected_gitignore(
        &mut self,
        path: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.selection.update(cx, |selection, selection_cx| {
            selection.set_gitignore_file(path);
            selection_cx.notify();
        });
    }

    pub(super) fn handle_preview_filter_change(&mut self, cx: &mut Context<Self>) {
        self.schedule_preview_table_sync(cx);
    }

    pub(super) fn on_preview_filter_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.handle_preview_filter_change(cx);
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
        _: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.invalidate_rules_panel_cache();
        }
    }

    pub(super) fn on_preview_table_event(
        &mut self,
        table: &Entity<TableState<PreviewTableDelegate>>,
        event: &TableEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.suppress_preview_table_events {
            return;
        }
        if let TableEvent::SelectRow(ix) | TableEvent::DoubleClickedRow(ix) = event
            && let Some(row) = table.read(cx).delegate().rows.get(*ix)
            && self.preview.read(cx).selected_preview_file_id() != Some(row.id)
        {
            self.open_preview_file_from_results(row.id, true, cx);
        }
    }

    pub(super) fn toggle_language(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = self.settings.update(cx, |settings, settings_cx| {
            let language = settings.toggle_language();
            settings_cx.notify();
            language
        });
        self.invalidate_rules_panel_cache();
        self.persist_settings_async(cx);
        self.sync_localized_inputs(window, cx);
        self.push_notice(
            NotificationType::Info,
            tr(language, "language_updated"),
            window,
            cx,
        );
        cx.notify();
    }

    pub(super) fn open_repository(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let language = self.language(cx);
        if let Err(err) = external_link::open_repository() {
            self.push_notice(
                NotificationType::Error,
                format!(
                    "{}{} ({err})",
                    tr(language, "repository_open_failed"),
                    app_metadata::repository_url()
                ),
                window,
                cx,
            );
        }
    }

    pub(super) fn minimize_window_chrome(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.minimize_window();
    }

    pub(super) fn toggle_zoom_window_chrome(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        match model::resolve_window_zoom_action(window.is_maximized(), window.is_fullscreen()) {
            model::WindowZoomAction::Maximize => window.zoom_window(),
            model::WindowZoomAction::Restore => {
                if window.is_fullscreen() {
                    window.toggle_fullscreen();
                } else {
                    let _ = restore_normal_window(window);
                }
            }
        }
        window.refresh();
    }

    pub(super) fn close_window_chrome(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.remove_window();
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
        self.clear_pending_confirmation(cx);
        let selection = self.selection_snapshot(cx);
        let Some(path) = &selection.gitignore_file else {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "gitignore_required"),
                window,
                cx,
            );
            return;
        };
        let path = path.clone();
        let language = self.language(cx);
        let _ = window;
        cx.spawn(async move |this, cx| {
            let loaded = std::fs::read_to_string(&path)
                .map(|content| crate::processor::walker::parse_gitignore_rules(&content));
            let _ = this.update(cx, |this, cx| match loaded {
                Ok(rules) => {
                    let added = this.selection.update(cx, |selection, selection_cx| {
                        let added = selection.append_temporary_gitignore_rules(rules);
                        if added > 0 {
                            selection_cx.notify();
                        }
                        added
                    });
                    if added > 0 {
                        this.refresh_preflight(cx);
                        Self::notify_active_window(
                            cx,
                            NotificationType::Success,
                            tr(language, "temporary_gitignore_applied"),
                        );
                    } else {
                        Self::notify_active_window(
                            cx,
                            NotificationType::Warning,
                            tr(language, "blacklist_empty"),
                        );
                    }
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
        if self.ui.update(cx, |ui, ui_cx| {
            let changed = ui.set_pending_confirmation(PendingConfirmation::ClearInputs);
            if changed {
                ui_cx.notify();
            }
            changed
        }) {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "confirm_clear_notice"),
                window,
                cx,
            );
            return;
        }
        self.clear_pending_confirmation(cx);
        if let Some(handle) = self.process.read(cx).state().process_handle.as_ref() {
            handle.cancel.cancel();
        }
        self.process.update(cx, |process, process_cx| {
            process.state_mut().preflight_rx = None;
            process_cx.notify();
        });
        self.preview.update(cx, |preview, preview_cx| {
            preview.set_preview_rx(None);
            preview_cx.notify();
        });
        self.cleanup_current_result_artifacts();
        self.preview_filter_task = None;
        self.preview_table_cache = super::PreviewTableCache::default();
        let status_ready = tr(self.language(cx), "status_ready").to_string();
        self.state.clear_inputs();
        self.selection.update(cx, |selection, selection_cx| {
            selection.clear();
            selection_cx.notify();
        });
        self.preview.update(cx, |preview, preview_cx| {
            preview.clear();
            preview_cx.notify();
        });
        self.result.update(cx, |result, result_cx| {
            result.clear();
            result_cx.notify();
        });
        self.process.update(cx, |process, process_cx| {
            process.clear_runtime(status_ready);
            process_cx.notify();
        });
        self.suppress_tree_interaction_sync.set(true);
        self.tree_panel.state.update(cx, |state, tree_cx| {
            state.set_selected_index(None, tree_cx);
        });
        let tree_interaction_guard = self.suppress_tree_interaction_sync.clone();
        cx.defer(move |_| {
            tree_interaction_guard.set(false);
        });
        self.tree_panel.data = None;
        self.tree_panel.projection = model::TreeProjectionState::default();
        self.tree_panel.render_state = model::TreeRenderState::default();
        self.tree_panel.total_summary = model::TreeCountSummary::default();
        self.tree_panel.last_filter.clear();
        self.tree_panel.last_interaction = None;
        self.sync_tree(cx);
        self.sync_preview_table(cx);
        self.refresh_preflight(cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.language(cx), "files_cleared"),
            window,
            cx,
        );
    }

    pub(super) fn start_process(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_processing(cx) {
            return;
        }
        if !self.has_inputs(cx) {
            self.push_notice(
                NotificationType::Error,
                tr(self.language(cx), "no_input_selected"),
                window,
                cx,
            );
            return;
        }
        self.cleanup_current_result_artifacts();
        self.preview_filter_task = None;
        self.preview_table_cache = super::PreviewTableCache::default();
        self.result.update(cx, |result, result_cx| {
            result.clear();
            result_cx.notify();
        });
        self.state.workspace.reset_tree();
        self.tree_panel.data = None;
        self.tree_panel.projection = model::TreeProjectionState::default();
        self.tree_panel.render_state = model::TreeRenderState::default();
        self.tree_panel.total_summary = model::TreeCountSummary::default();
        self.tree_panel.last_filter.clear();
        self.tree_panel.last_interaction = None;
        self.clear_preview_state(cx);
        self.clear_pending_confirmation(cx);
        self.sync_tree(cx);
        self.sync_preview_table(cx);
        let settings = self.settings_snapshot(cx);
        let selection = self.selection_snapshot(cx);
        let effective_blacklists = self.effective_blacklists(cx);
        let handle = crate::services::process::start(ProcessRequest {
            selected_folder: selection.selected_folder.clone(),
            selected_files: selection
                .selected_files
                .iter()
                .map(|entry| entry.path.clone())
                .collect(),
            folder_blacklist: effective_blacklists.folder_blacklist,
            ext_blacklist: effective_blacklists.ext_blacklist,
            options: settings.options.clone(),
            language: settings.language,
        });
        self.process.update(cx, |process, process_cx| {
            process.start_run(handle, tr(settings.language, "scanning_files").to_string());
            process_cx.notify();
        });
        self.ensure_background_polling(cx);
        self.push_notice(
            NotificationType::Info,
            tr(settings.language, "process_started"),
            window,
            cx,
        );
    }

    pub(super) fn cancel_process(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.process.update(cx, |process, process_cx| {
            let cancelled = process.cancel_running();
            process_cx.notify();
            cancelled
        }) {
            self.push_notice(
                NotificationType::Info,
                tr(self.language(cx), "cancelled"),
                window,
                cx,
            );
        }
    }

    pub(super) fn add_folder_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation(cx);
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
        self.clear_pending_confirmation(cx);
        let added = self.consume_blacklist_input(true, window, cx);
        if added > 0 {
            self.refresh_preflight(cx);
        }
    }

    pub(super) fn add_temporary_folder_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation(cx);
        let added = self.consume_temporary_blacklist_input(false, window, cx);
        if added > 0 {
            self.refresh_preflight(cx);
        }
    }

    pub(super) fn add_temporary_ext_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation(cx);
        let added = self.consume_temporary_blacklist_input(true, window, cx);
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
        let tokens = Self::parse_blacklist_tokens(&raw);
        if tokens.is_empty() {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "blacklist_empty"),
                window,
                cx,
            );
            return 0;
        }

        let added = self.settings.update(cx, |settings, settings_cx| {
            let added = settings.add_blacklist_tokens(&tokens, as_ext);
            if added > 0 {
                settings_cx.notify();
            }
            added
        });
        if added == 0 {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "blacklist_empty"),
                window,
                cx,
            );
            return 0;
        }
        self.blacklist_add_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.invalidate_rules_panel_cache();
        self.persist_settings_async(cx);
        self.push_notice(
            NotificationType::Success,
            tr(self.language(cx), "blacklist_added"),
            window,
            cx,
        );
        added
    }

    pub(super) fn consume_temporary_blacklist_input(
        &mut self,
        as_ext: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        let raw = self.temp_blacklist_add_input.read(cx).value().to_string();
        let tokens = Self::parse_blacklist_tokens(&raw);
        if tokens.is_empty() {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "blacklist_empty"),
                window,
                cx,
            );
            return 0;
        }

        let added = self.selection.update(cx, |selection, selection_cx| {
            let added = selection.add_temporary_blacklist_tokens(&tokens, as_ext);
            if added > 0 {
                selection_cx.notify();
            }
            added
        });
        if added == 0 {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "blacklist_empty"),
                window,
                cx,
            );
            return 0;
        }
        self.temp_blacklist_add_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.push_notice(
            NotificationType::Success,
            tr(self.language(cx), "temporary_rules_added"),
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
        let settings = self.settings_snapshot(cx);
        let mut body = String::new();
        body.push_str("# folders\n");
        for item in &settings.folder_blacklist {
            body.push_str(item);
            body.push('\n');
        }
        body.push_str("# extensions\n");
        for item in &settings.ext_blacklist {
            body.push_str(item);
            body.push('\n');
        }
        let language = settings.language;
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
        self.clear_pending_confirmation(cx);
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
                    let language = this.language(cx);
                    this.settings.update(cx, |settings, settings_cx| {
                        let added = settings.import_blacklist_content(&content);
                        settings_cx.notify();
                        added
                    });
                    this.invalidate_rules_panel_cache();
                    this.persist_settings_async(cx);
                    this.refresh_preflight(cx);
                    Self::notify_active_window(
                        cx,
                        NotificationType::Success,
                        tr(language, "blacklist_imported"),
                    );
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
        if self.ui.update(cx, |ui, ui_cx| {
            let changed = ui.set_pending_confirmation(PendingConfirmation::ResetBlacklist);
            if changed {
                ui_cx.notify();
            }
            changed
        }) {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "confirm_reset_notice"),
                window,
                cx,
            );
            return;
        }
        self.clear_pending_confirmation(cx);
        self.settings.update(cx, |settings, settings_cx| {
            settings.reset_blacklist();
            settings_cx.notify();
        });
        self.invalidate_rules_panel_cache();
        self.persist_settings_async(cx);
        self.refresh_preflight(cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.language(cx), "blacklist_reset_default"),
            window,
            cx,
        );
    }

    pub(super) fn clear_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ui.update(cx, |ui, ui_cx| {
            let changed = ui.set_pending_confirmation(PendingConfirmation::ClearBlacklist);
            if changed {
                ui_cx.notify();
            }
            changed
        }) {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "confirm_clear_notice"),
                window,
                cx,
            );
            return;
        }
        self.clear_pending_confirmation(cx);
        self.settings.update(cx, |settings, settings_cx| {
            settings.clear_blacklist();
            settings_cx.notify();
        });
        self.invalidate_rules_panel_cache();
        self.persist_settings_async(cx);
        self.refresh_preflight(cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.language(cx), "blacklist_cleared"),
            window,
            cx,
        );
    }

    pub(super) fn clear_temporary_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation(cx);
        let cleared = self.selection.update(cx, |selection, selection_cx| {
            let cleared = selection.clear_temporary_blacklist();
            if cleared {
                selection_cx.notify();
            }
            cleared
        });
        if !cleared {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "blacklist_empty"),
                window,
                cx,
            );
            return;
        }
        self.refresh_preflight(cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.language(cx), "temporary_rules_cleared"),
            window,
            cx,
        );
    }

    pub(super) fn toggle_compress(
        &mut self,
        checked: &bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation(cx);
        self.settings.update(cx, |settings, settings_cx| {
            settings.set_compress(*checked);
            settings_cx.notify();
        });
        self.persist_settings_async(cx);
    }

    pub(super) fn toggle_use_gitignore(
        &mut self,
        checked: &bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation(cx);
        self.settings.update(cx, |settings, settings_cx| {
            settings.set_use_gitignore(*checked);
            settings_cx.notify();
        });
        self.persist_settings_async(cx);
        self.refresh_preflight(cx);
    }

    pub(super) fn toggle_ignore_git(
        &mut self,
        checked: &bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation(cx);
        self.settings.update(cx, |settings, settings_cx| {
            settings.set_ignore_git(*checked);
            settings_cx.notify();
        });
        self.invalidate_rules_panel_cache();
        self.persist_settings_async(cx);
        self.refresh_preflight(cx);
    }

    pub(super) fn toggle_dedupe(&mut self, checked: &bool, _: &mut Window, cx: &mut Context<Self>) {
        self.clear_pending_confirmation(cx);
        self.selection.update(cx, |selection, selection_cx| {
            selection.set_dedupe_exact_path(*checked);
            selection_cx.notify();
        });
    }

    pub(super) fn set_output_format(&mut self, ix: &usize, _: &mut Window, cx: &mut Context<Self>) {
        let format = match *ix {
            1 => OutputFormat::Xml,
            2 => OutputFormat::PlainText,
            3 => OutputFormat::Markdown,
            _ => OutputFormat::Default,
        };
        self.clear_pending_confirmation(cx);
        self.settings.update(cx, |settings, settings_cx| {
            settings.set_output_format(format);
            settings_cx.notify();
        });
        self.persist_settings_async(cx);
    }

    pub(super) fn set_tab(&mut self, ix: &usize, _: &mut Window, cx: &mut Context<Self>) {
        if *ix == 1 && !self.result_has_content(cx) {
            self.result.update(cx, |result, result_cx| {
                result.set_active_tab(ResultTab::Tree);
                result_cx.notify();
            });
            return;
        }
        let active_tab = if *ix == 0 {
            ResultTab::Tree
        } else {
            ResultTab::Content
        };
        self.result.update(cx, |result, result_cx| {
            result.set_active_tab(active_tab);
            result_cx.notify();
        });
        if active_tab == ResultTab::Content {
            self.load_merged_content_preview(cx);
            self.ensure_background_polling(cx);
        }
    }

    pub(super) fn set_side_panel_tab(
        &mut self,
        ix: &usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = if *ix == 0 {
            SidePanelTab::Results
        } else {
            SidePanelTab::Rules
        };
        self.ui.update(cx, |ui, ui_cx| {
            if ui.set_side_panel_tab(tab) {
                ui_cx.notify();
            }
        });
    }

    pub(super) fn set_narrow_content_tab(
        &mut self,
        ix: &usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = if *ix == 0 {
            NarrowContentTab::Status
        } else {
            NarrowContentTab::Results
        };
        self.ui.update(cx, |ui, ui_cx| {
            if ui.set_narrow_content_tab(tab) {
                ui_cx.notify();
            }
        });
    }

    pub(super) fn toggle_content_file_list_collapsed(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ui.update(cx, |ui, ui_cx| {
            let collapsed = !ui.state().content_file_list_collapsed;
            if ui.set_content_file_list_collapsed(collapsed) {
                ui_cx.notify();
            }
        });
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
        let result = self.result.read(cx).state().result.clone();
        if let Some(result) = result.as_ref() {
            copy_to_clipboard(&result.tree_string, self.language(cx), window, cx);
        } else {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "no_tree"),
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
        let path = self
            .preview
            .read(cx)
            .preview_document()
            .map(|document| document.path().to_path_buf());
        let Some(path) = path else {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "no_content"),
                window,
                cx,
            );
            return;
        };
        let language = self.language(cx);
        let _ = window;
        cx.spawn(async move |this, cx| {
            let result = std::fs::read_to_string(&path)
                .map_err(|e| crate::error::AppError::new(format!("read preview file failed: {e}")));
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
        self.clear_pending_confirmation(cx);
        self.settings.update(cx, |settings, settings_cx| {
            settings.remove_blacklist_item(kind, &value);
            settings_cx.notify();
        });
        self.invalidate_rules_panel_cache();
        self.persist_settings_async(cx);
        self.refresh_preflight(cx);
        self.push_notice(
            NotificationType::Info,
            tr(self.language(cx), "blacklist_item_removed"),
            window,
            cx,
        );
    }

    pub(super) fn remove_temporary_blacklist_item(
        &mut self,
        kind: BlacklistItemKind,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_pending_confirmation(cx);
        let removed = self.selection.update(cx, |selection, selection_cx| {
            let before = selection.snapshot();
            selection.remove_temporary_blacklist_item(kind, &value);
            let after = selection.snapshot();
            let removed = before.temp_folder_blacklist != after.temp_folder_blacklist
                || before.temp_ext_blacklist != after.temp_ext_blacklist;
            if removed {
                selection_cx.notify();
            }
            removed
        });
        if removed {
            self.refresh_preflight(cx);
            self.push_notice(
                NotificationType::Info,
                tr(self.language(cx), "temporary_rule_removed"),
                window,
                cx,
            );
        }
    }

    pub(super) fn download_result(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self.result.read(cx).state().result.clone();
        let Some(result) = result.as_ref() else {
            return;
        };
        let Some(path) = &result.merged_content_path else {
            self.push_notice(
                NotificationType::Warning,
                tr(self.language(cx), "mode_tree_only_desc"),
                window,
                cx,
            );
            return;
        };
        let source_path = path.clone();
        let suggested_name = result.suggested_result_name.clone();
        let language = self.language(cx);
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

#[cfg(target_os = "windows")]
pub(super) fn begin_window_drag(window: &Window) -> bool {
    let Some(hwnd) = win32_window_handle(window) else {
        return false;
    };

    unsafe {
        let _ = ReleaseCapture();
        let _ = SendMessageW(
            hwnd,
            WM_NCLBUTTONDOWN,
            Some(WPARAM(HTCAPTION as usize)),
            Some(LPARAM(0)),
        );
    }

    true
}

#[cfg(not(target_os = "windows"))]
pub(super) fn begin_window_drag(_: &Window) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn restore_normal_window(window: &Window) -> bool {
    let Some(hwnd) = win32_window_handle(window) else {
        return false;
    };

    unsafe { ShowWindowAsync(hwnd, SW_RESTORE) }.as_bool()
}

#[cfg(target_os = "windows")]
fn win32_window_handle(window: &Window) -> Option<HWND> {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return None;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };

    Some(HWND(handle.hwnd.get() as *mut core::ffi::c_void))
}

#[cfg(not(target_os = "windows"))]
fn restore_normal_window(_: &Window) -> bool {
    false
}
