use std::ops::Range;
use std::rc::Rc;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use arboard::Clipboard;
use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, Entity, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, SharedString, Styled,
    Subscription, Task, Timer, Window, div, px, size,
};
use gpui_component::{
    ActiveTheme as _, Disableable, IconName, Sizable, Size, StyledExt as _,
    VirtualListScrollHandle, WindowExt as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState},
    list::ListItem,
    notification::NotificationType,
    resizable::{h_resizable, resizable_panel},
    tab::{Tab, TabBar},
    table::{Column, Table, TableDelegate, TableEvent, TableState},
    tree::{TreeItem, TreeState, tree},
    v_flex, v_virtual_list,
};

use crate::domain::{
    AppConfigV1, FileEntry, Language, PreviewRowViewModel, ProcessResult, ProcessStatus,
    ProgressRowViewModel, ResultTab, TreeNode,
};
use crate::services::preflight::{PreflightEvent, PreflightRequest};
use crate::services::preview::{PreviewEvent, PreviewRequest, load_text, start as start_preview};
use crate::services::process::{ProcessEvent, ProcessRequest};
use crate::services::settings;
use crate::ui::state::AppState;
use crate::utils::i18n::tr;
use crate::utils::path::filename;

fn preview_line_height() -> Pixels {
    px(22.)
}

fn fixed_list_sizes(len: usize, height: Pixels) -> Rc<Vec<gpui::Size<Pixels>>> {
    Rc::new((0..len).map(|_| size(px(100.), height)).collect::<Vec<_>>())
}

struct PreviewTableDelegate {
    columns: Vec<Column>,
    rows: Vec<PreviewRowViewModel>,
}

impl PreviewTableDelegate {
    fn new() -> Self {
        Self {
            columns: vec![
                Column::new("path", "Path").width(420.),
                Column::new("chars", "Chars").width(100.).text_right(),
                Column::new("tokens", "Tokens").width(100.).text_right(),
            ],
            rows: Vec::new(),
        }
    }
}

impl TableDelegate for PreviewTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> &Column {
        &self.columns[col_ix]
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        self.columns[col_ix].name.clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        match col_ix {
            0 => SharedString::from(row.display_path.clone()).into_any_element(),
            1 => row.chars.to_string().into_any_element(),
            _ => row.tokens.to_string().into_any_element(),
        }
    }
}

pub struct Workspace {
    focus_handle: FocusHandle,
    state: AppState,
    preview_scroll_handle: VirtualListScrollHandle,
    tree_state: Entity<TreeState>,
    preview_table: Entity<TableState<PreviewTableDelegate>>,
    preview_filter_input: Entity<InputState>,
    blacklist_filter_input: Entity<InputState>,
    blacklist_add_input: Entity<InputState>,
    _poll_task: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let cfg = settings::load();
        let preview_filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr(cfg.language, "file_filter")));
        let blacklist_filter_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr(cfg.language, "blacklist_filter"))
        });
        let blacklist_add_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr(cfg.language, "blacklist_unified_hint"))
        });
        let tree_state = cx.new(|cx| TreeState::new(cx));
        let preview_table = cx.new(|cx| TableState::new(PreviewTableDelegate::new(), window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&preview_filter_input, window, Self::on_preview_filter_event),
            cx.subscribe_in(
                &blacklist_filter_input,
                window,
                Self::on_blacklist_filter_event,
            ),
            cx.subscribe_in(&preview_table, window, Self::on_preview_table_event),
        ];
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(33)).await;
                let _ = this.update(cx, |this, cx| this.poll_background(cx));
            }
        });
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            state: AppState::from_config(cfg.clone(), tr(cfg.language, "status_ready").to_string()),
            preview_scroll_handle: VirtualListScrollHandle::new(),
            tree_state,
            preview_table,
            preview_filter_input,
            blacklist_filter_input,
            blacklist_add_input,
            _poll_task: poll_task,
            _subscriptions: subscriptions,
        };
        this.refresh_preflight();
        this
    }

    fn poll_background(&mut self, cx: &mut Context<Self>) {
        let mut dirty = false;

        if let Some(rx) = self.state.process.preflight_rx.take() {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        self.apply_preflight_event(event);
                        dirty = true;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        keep = false;
                        break;
                    }
                }
            }
            if keep {
                self.state.process.preflight_rx = Some(rx);
            }
        }

        let mut finish_processing = false;
        if let Some(handle) = self.state.process.process_handle.as_mut() {
            let mut events = Vec::new();
            loop {
                match handle.receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finish_processing = true;
                        break;
                    }
                }
            }
            for event in events {
                dirty = true;
                finish_processing = self.apply_process_event(event, cx) || finish_processing;
            }
        }
        if finish_processing {
            self.state.process.process_handle = None;
            self.state.process.processing_started_at = None;
            dirty = true;
        }
        if let Some(rx) = self.state.workspace.preview_rx.take() {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        dirty = self.apply_preview_event(event) || dirty;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        keep = false;
                        self.state.workspace.preview_requested_range = None;
                        break;
                    }
                }
            }
            if keep {
                self.state.workspace.preview_rx = Some(rx);
            }
        }
        dirty = self.refresh_preview_window() || dirty;
        if dirty {
            cx.notify();
        }
    }

    fn apply_preflight_event(&mut self, event: PreflightEvent) {
        match event {
            PreflightEvent::Started { revision } => {
                if revision == self.state.process.preflight_revision {
                    self.state.process.preflight.is_scanning = true;
                }
            }
            PreflightEvent::Progress {
                revision,
                scanned,
                candidates,
                skipped,
            } => {
                if revision == self.state.process.preflight_revision {
                    self.state.process.preflight.scanned_entries = scanned;
                    self.state.process.preflight.to_process_files = candidates;
                    self.state.process.preflight.skipped_files = skipped;
                    self.state.process.preflight.total_files = candidates + skipped;
                    self.state.process.preflight.is_scanning = true;
                }
            }
            PreflightEvent::Completed { revision, stats } => {
                if revision == self.state.process.preflight_revision {
                    self.state.process.preflight = stats;
                }
            }
            PreflightEvent::Failed { revision, .. } => {
                if revision == self.state.process.preflight_revision {
                    self.state.process.preflight.is_scanning = false;
                }
            }
        }
    }

    fn apply_process_event(&mut self, event: ProcessEvent, cx: &mut Context<Self>) -> bool {
        match event {
            ProcessEvent::Scanning {
                scanned,
                candidates,
                skipped,
            } => {
                self.state.process.processing_scanned = scanned;
                self.state.process.processing_candidates = candidates;
                self.state.process.processing_skipped = skipped;
                self.state.process.processing_current_file = format!(
                    "{} {}",
                    tr(self.state.settings.language, "scanning_files"),
                    scanned
                );
                false
            }
            ProcessEvent::Record(record) => {
                self.state.process.processing_current_file = record.file_name.clone();
                if !matches!(record.status, ProcessStatus::Success) {
                    self.state.process.processing_skipped += 1;
                }
                self.state.process.processing_records.push(record);
                false
            }
            ProcessEvent::Completed(result) => {
                self.set_result(result, cx);
                true
            }
            ProcessEvent::Cancelled | ProcessEvent::Failed(_) => true,
        }
    }

    fn apply_preview_event(&mut self, event: PreviewEvent) -> bool {
        match event {
            PreviewEvent::Opened {
                revision,
                file_id,
                document,
                loaded_range,
                lines,
            } => {
                if revision != self.state.workspace.preview_revision
                    || self.state.workspace.selected_preview_file_id != Some(file_id)
                {
                    return false;
                }
                let line_count = document.line_count().max(1);
                self.state.workspace.preview_document = Some(document);
                self.state.workspace.preview_loaded_range = loaded_range;
                self.state.workspace.preview_loaded_lines =
                    lines.into_iter().map(SharedString::from).collect();
                self.state.workspace.preview_requested_range = None;
                self.state.workspace.preview_sizes = Rc::new(
                    (0..line_count)
                        .map(|_| size(px(100.), preview_line_height()))
                        .collect::<Vec<_>>(),
                );
                self.preview_scroll_handle.scroll_to_top_of_item(0);
                true
            }
            PreviewEvent::Loaded {
                revision,
                file_id,
                loaded_range,
                lines,
            } => {
                if revision != self.state.workspace.preview_revision
                    || self.state.workspace.selected_preview_file_id != Some(file_id)
                {
                    return false;
                }
                self.state.workspace.preview_loaded_range = loaded_range;
                self.state.workspace.preview_loaded_lines =
                    lines.into_iter().map(SharedString::from).collect();
                self.state.workspace.preview_requested_range = None;
                true
            }
            PreviewEvent::Failed {
                revision, file_id, ..
            } => {
                if revision != self.state.workspace.preview_revision
                    || self.state.workspace.selected_preview_file_id != Some(file_id)
                {
                    return false;
                }
                self.state.workspace.preview_requested_range = None;
                self.state.workspace.preview_loaded_range = 0..0;
                self.state.workspace.preview_loaded_lines.clear();
                true
            }
        }
    }

    fn set_result(&mut self, result: ProcessResult, cx: &mut Context<Self>) {
        if let Some(prev_dir) = self
            .state
            .result
            .result
            .as_ref()
            .and_then(|result| result.preview_blob_dir.as_ref())
        {
            let _ = crate::utils::temp_file::cleanup_preview_dir(prev_dir);
        }
        self.state.result.result = Some(result);
        self.state.result.active_tab = ResultTab::Tree;
        self.sync_tree(cx);
        self.sync_preview_table(cx);
    }

    fn sync_tree(&mut self, cx: &mut Context<Self>) {
        let items = self
            .state
            .result
            .result
            .as_ref()
            .map(|result| build_tree_items(&result.tree_nodes))
            .unwrap_or_default();
        self.tree_state
            .update(cx, |state, cx| state.set_items(items, cx));
    }

    fn sync_preview_table(&mut self, cx: &mut Context<Self>) {
        let filter = self
            .preview_filter_input
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        let rows = self
            .state
            .result
            .result
            .as_ref()
            .map(|result| {
                result
                    .preview_files
                    .iter()
                    .filter(|entry| {
                        filter.is_empty()
                            || entry
                                .display_path
                                .to_ascii_lowercase()
                                .contains(filter.as_str())
                    })
                    .map(|entry| PreviewRowViewModel {
                        id: entry.id,
                        display_path: entry.display_path.clone(),
                        chars: entry.chars,
                        tokens: entry.tokens,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.state.result.preview_rows = rows.clone();
        let selected_row_ix = rows
            .iter()
            .position(|row| Some(row.id) == self.state.workspace.selected_preview_file_id);
        let next_selected_id = selected_row_ix
            .and_then(|ix| rows.get(ix))
            .map(|row| row.id)
            .or_else(|| rows.first().map(|row| row.id));
        self.preview_table.update(cx, |table, cx| {
            table.delegate_mut().rows = rows;
            if let Some(row_ix) = selected_row_ix.or(if next_selected_id.is_some() {
                Some(0)
            } else {
                None
            }) {
                table.set_selected_row(row_ix, cx);
            } else {
                table.clear_selection(cx);
            }
            cx.notify();
        });

        match next_selected_id {
            Some(file_id) => self.load_preview(file_id, cx),
            None => self.clear_preview_state(),
        }
    }

    fn refresh_preflight(&mut self) {
        self.state.process.preflight_revision += 1;
        self.state.process.preflight_rx =
            Some(crate::services::preflight::start(PreflightRequest {
                revision: self.state.process.preflight_revision,
                selected_folder: self.state.selection.selected_folder.clone(),
                selected_files: self
                    .state
                    .selection
                    .selected_files
                    .iter()
                    .map(|f| f.path.clone())
                    .collect(),
                folder_blacklist: self.state.settings.folder_blacklist.clone(),
                ext_blacklist: self.state.settings.ext_blacklist.clone(),
            }));
    }

    fn save_settings(&self) -> Result<(), String> {
        settings::execute(crate::domain::SettingsCommand::Save(AppConfigV1 {
            language: self.state.settings.language,
            options: self.state.settings.options.clone(),
            folder_blacklist: self.state.settings.folder_blacklist.clone(),
            ext_blacklist: self.state.settings.ext_blacklist.clone(),
        }))
        .map(|_| ())
    }

    fn clear_preview_state(&mut self) {
        self.state.workspace.selected_preview_file_id = None;
        self.state.workspace.preview_rx = None;
        self.state.workspace.preview_requested_range = None;
        self.state.workspace.preview_document = None;
        self.state.workspace.preview_loaded_range = 0..0;
        self.state.workspace.preview_loaded_lines.clear();
        self.state.workspace.preview_visible_range = 0..0;
        self.state.workspace.preview_sizes = Rc::new(vec![size(px(10.), preview_line_height())]);
    }

    fn padded_preview_range(&self, range: Range<usize>, line_count: usize) -> Range<usize> {
        if line_count == 0 {
            return 0..0;
        }
        let start = range.start.min(line_count.saturating_sub(1));
        let end = range.end.max(start + 1).min(line_count);
        start.saturating_sub(50)..(end + 100).min(line_count)
    }

    fn request_preview_range(&mut self, range: Range<usize>) -> bool {
        let Some(document) = &self.state.workspace.preview_document else {
            return false;
        };
        let Some(file_id) = self.state.workspace.selected_preview_file_id else {
            return false;
        };

        let padded = self.padded_preview_range(range, document.line_count());
        if padded.start >= padded.end {
            return false;
        }
        if self.state.workspace.preview_loaded_range.start <= padded.start
            && self.state.workspace.preview_loaded_range.end >= padded.end
        {
            return false;
        }
        if self.state.workspace.preview_requested_range.as_ref() == Some(&padded)
            || self.state.workspace.preview_rx.is_some()
        {
            return false;
        }

        self.state.workspace.preview_requested_range = Some(padded.clone());
        self.state.workspace.preview_rx = Some(start_preview(PreviewRequest::LoadRange {
            revision: self.state.workspace.preview_revision,
            file_id,
            document: document.clone(),
            range: padded,
        }));
        true
    }

    fn refresh_preview_window(&mut self) -> bool {
        let Some(document) = &self.state.workspace.preview_document else {
            return false;
        };
        let line_count = document.line_count();
        if line_count == 0 {
            return false;
        }

        let visible = if self.state.workspace.preview_visible_range.start
            < self.state.workspace.preview_visible_range.end
        {
            self.state.workspace.preview_visible_range.clone()
        } else {
            0..line_count.min(1)
        };

        if self.state.workspace.preview_loaded_range.start <= visible.start
            && self.state.workspace.preview_loaded_range.end >= visible.end
        {
            return false;
        }

        self.request_preview_range(visible)
    }

    fn load_preview(&mut self, file_id: u32, cx: &mut Context<Self>) {
        if self.state.workspace.selected_preview_file_id == Some(file_id)
            && (self.state.workspace.preview_document.is_some()
                || self.state.workspace.preview_rx.is_some())
        {
            return;
        }
        let Some(entry) = self.state.result.result.as_ref().and_then(|result| {
            result
                .preview_files
                .iter()
                .find(|entry| entry.id == file_id)
        }) else {
            return;
        };
        self.state.workspace.preview_revision += 1;
        self.state.workspace.selected_preview_file_id = Some(file_id);
        self.state.workspace.preview_rx = Some(start_preview(PreviewRequest::Open {
            revision: self.state.workspace.preview_revision,
            file_id,
            path: entry.preview_blob_path.clone(),
            initial_range: 0..200,
        }));
        self.state.workspace.preview_requested_range = Some(0..200);
        self.state.workspace.preview_document = None;
        self.state.workspace.preview_loaded_range = 0..0;
        self.state.workspace.preview_loaded_lines.clear();
        self.state.workspace.preview_visible_range = 0..0;
        self.state.workspace.preview_sizes = Rc::new(vec![size(px(100.), preview_line_height())]);
        self.preview_scroll_handle.scroll_to_top_of_item(0);
        cx.notify();
    }

    fn push_notice(
        &self,
        kind: NotificationType,
        message: impl Into<String>,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.push_notification((kind, SharedString::from(message.into())), cx);
    }

    fn on_preview_filter_event(
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

    fn on_blacklist_filter_event(
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

    fn on_preview_table_event(
        &mut self,
        table: &Entity<TableState<PreviewTableDelegate>>,
        event: &TableEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let TableEvent::SelectRow(ix) | TableEvent::DoubleClickedRow(ix) = event {
            if let Some(row) = table.read(cx).delegate().rows.get(*ix) {
                self.load_preview(row.id, cx);
            }
        }
    }

    fn toggle_language(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.state.settings.language = self.state.settings.language.toggle();
        let _ = self.save_settings();
        self.push_notice(NotificationType::Info, "Language updated", window, cx);
        cx.notify();
    }

    fn select_folder(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.state.selection.selected_folder = Some(path.clone());
            if self.state.settings.options.use_gitignore {
                let gitignore = crate::processor::walker::auto_gitignore_path(&path);
                if gitignore.exists() {
                    if let Ok(content) = std::fs::read_to_string(&gitignore) {
                        for rule in crate::processor::walker::parse_gitignore_rules(&content) {
                            if !self.state.settings.folder_blacklist.contains(&rule) {
                                self.state.settings.folder_blacklist.push(rule);
                            }
                        }
                    }
                }
            }
            let _ = self.save_settings();
            self.refresh_preflight();
            self.push_notice(
                NotificationType::Success,
                tr(self.state.settings.language, "folder_selected"),
                window,
                cx,
            );
            cx.notify();
        }
    }

    fn select_files(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(files) = rfd::FileDialog::new().pick_files() else {
            return;
        };
        let mut existing = self
            .state
            .selection
            .selected_files
            .iter()
            .map(|entry| entry.path.to_string_lossy().to_string())
            .collect::<std::collections::BTreeSet<_>>();
        for path in files {
            let key = path.to_string_lossy().to_string();
            if self.state.selection.dedupe_exact_path && !existing.insert(key) {
                continue;
            }
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            self.state.selection.selected_files.push(FileEntry {
                name: filename(&path),
                path,
                size,
            });
        }
        self.refresh_preflight();
        self.push_notice(
            NotificationType::Success,
            tr(self.state.settings.language, "files_added"),
            window,
            cx,
        );
        cx.notify();
    }

    fn select_gitignore(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.state.selection.gitignore_file = rfd::FileDialog::new()
            .add_filter("gitignore", &["gitignore"])
            .pick_file();
        if self.state.selection.gitignore_file.is_some() {
            self.push_notice(
                NotificationType::Info,
                tr(self.state.settings.language, "gitignore_selected"),
                window,
                cx,
            );
        }
        cx.notify();
    }

    fn apply_gitignore(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = &self.state.selection.gitignore_file else {
            self.push_notice(
                NotificationType::Warning,
                tr(self.state.settings.language, "gitignore_required"),
                window,
                cx,
            );
            return;
        };
        match std::fs::read_to_string(path) {
            Ok(content) => {
                for rule in crate::processor::walker::parse_gitignore_rules(&content) {
                    if !self.state.settings.folder_blacklist.contains(&rule) {
                        self.state.settings.folder_blacklist.push(rule);
                    }
                }
                let _ = self.save_settings();
                self.refresh_preflight();
                self.push_notice(
                    NotificationType::Success,
                    tr(self.state.settings.language, "blacklist_saved"),
                    window,
                    cx,
                );
                cx.notify();
            }
            Err(err) => self.push_notice(
                NotificationType::Error,
                format!(
                    "{}{}",
                    tr(self.state.settings.language, "read_gitignore_failed"),
                    err
                ),
                window,
                cx,
            ),
        }
    }

    fn clear_inputs(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.state.selection.selected_folder = None;
        self.state.selection.selected_files.clear();
        self.state.selection.gitignore_file = None;
        self.state.result.result = None;
        self.clear_preview_state();
        self.sync_tree(cx);
        self.sync_preview_table(cx);
        self.refresh_preflight();
        self.push_notice(
            NotificationType::Info,
            tr(self.state.settings.language, "files_cleared"),
            window,
            cx,
        );
        cx.notify();
    }

    fn start_process(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
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
        if let Some(prev_dir) = self
            .state
            .result
            .result
            .as_ref()
            .and_then(|result| result.preview_blob_dir.as_ref())
        {
            let _ = crate::utils::temp_file::cleanup_preview_dir(prev_dir);
        }
        self.state.result.result = None;
        self.clear_preview_state();
        self.state.process.processing_records.clear();
        self.state.process.processing_scanned = 0;
        self.state.process.processing_candidates = 0;
        self.state.process.processing_skipped = 0;
        self.state.process.processing_current_file =
            tr(self.state.settings.language, "scanning_files").to_string();
        self.state.process.processing_started_at = Some(Instant::now());
        self.state.process.process_handle = Some(crate::services::process::start(ProcessRequest {
            selected_folder: self.state.selection.selected_folder.clone(),
            selected_files: self
                .state
                .selection
                .selected_files
                .iter()
                .map(|entry| entry.path.clone())
                .collect(),
            folder_blacklist: self.state.settings.folder_blacklist.clone(),
            ext_blacklist: self.state.settings.ext_blacklist.clone(),
            options: self.state.settings.options.clone(),
            language: self.state.settings.language,
        }));
        self.push_notice(
            NotificationType::Info,
            tr(self.state.settings.language, "process_started"),
            window,
            cx,
        );
        cx.notify();
    }

    fn cancel_process(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = &self.state.process.process_handle {
            handle.cancel.cancel();
            self.push_notice(
                NotificationType::Info,
                tr(self.state.settings.language, "cancelled"),
                window,
                cx,
            );
        }
        cx.notify();
    }

    fn save_blacklists(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        match self.save_settings() {
            Ok(_) => self.push_notice(
                NotificationType::Success,
                tr(self.state.settings.language, "blacklist_saved"),
                window,
                cx,
            ),
            Err(err) => self.push_notice(NotificationType::Error, err, window, cx),
        }
    }

    fn add_folder_blacklist(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let added = self.consume_blacklist_input(false, window, cx);
        if added > 0 {
            self.refresh_preflight();
        }
    }

    fn add_ext_blacklist(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let added = self.consume_blacklist_input(true, window, cx);
        if added > 0 {
            self.refresh_preflight();
        }
    }

    fn consume_blacklist_input(
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
        let _ = self.save_settings();
        self.push_notice(
            NotificationType::Success,
            tr(self.state.settings.language, "blacklist_added"),
            window,
            cx,
        );
        added
    }

    fn export_blacklist(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("codemerge-blacklist.txt")
            .save_file()
        else {
            return;
        };
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
        match std::fs::write(path, body) {
            Ok(_) => self.push_notice(
                NotificationType::Success,
                tr(self.state.settings.language, "blacklist_exported"),
                window,
                cx,
            ),
            Err(err) => self.push_notice(NotificationType::Error, err.to_string(), window, cx),
        }
    }

    fn import_blacklist(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = rfd::FileDialog::new().pick_file() else {
            return;
        };
        match std::fs::read_to_string(path) {
            Ok(content) => {
                for line in content
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                {
                    if line.starts_with('.') {
                        let ext = crate::processor::walker::normalize_ext(line);
                        if !self.state.settings.ext_blacklist.contains(&ext) {
                            self.state.settings.ext_blacklist.push(ext);
                        }
                    } else if !self
                        .state
                        .settings
                        .folder_blacklist
                        .contains(&line.to_string())
                    {
                        self.state.settings.folder_blacklist.push(line.to_string());
                    }
                }
                let _ = self.save_settings();
                self.refresh_preflight();
                self.push_notice(
                    NotificationType::Success,
                    tr(self.state.settings.language, "blacklist_imported"),
                    window,
                    cx,
                );
                cx.notify();
            }
            Err(err) => self.push_notice(NotificationType::Error, err.to_string(), window, cx),
        }
    }

    fn reset_blacklist(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.state.settings.folder_blacklist = crate::domain::default_folder_blacklist();
        self.state.settings.ext_blacklist = crate::domain::default_ext_blacklist();
        let _ = self.save_settings();
        self.refresh_preflight();
        self.push_notice(
            NotificationType::Info,
            tr(self.state.settings.language, "blacklist_reset_default"),
            window,
            cx,
        );
        cx.notify();
    }

    fn clear_blacklist(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.state.settings.folder_blacklist.clear();
        self.state.settings.ext_blacklist.clear();
        let _ = self.save_settings();
        self.refresh_preflight();
        self.push_notice(
            NotificationType::Info,
            tr(self.state.settings.language, "blacklist_cleared"),
            window,
            cx,
        );
        cx.notify();
    }

    fn toggle_compress(&mut self, checked: &bool, _: &mut Window, cx: &mut Context<Self>) {
        self.state.settings.options.compress = *checked;
        let _ = self.save_settings();
        cx.notify();
    }

    fn toggle_use_gitignore(&mut self, checked: &bool, _: &mut Window, cx: &mut Context<Self>) {
        self.state.settings.options.use_gitignore = *checked;
        let _ = self.save_settings();
        cx.notify();
    }

    fn toggle_ignore_git(&mut self, checked: &bool, _: &mut Window, cx: &mut Context<Self>) {
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
        let _ = self.save_settings();
        self.refresh_preflight();
        cx.notify();
    }

    fn toggle_dedupe(&mut self, checked: &bool, _: &mut Window, cx: &mut Context<Self>) {
        self.state.selection.dedupe_exact_path = *checked;
        cx.notify();
    }

    fn set_tab(&mut self, ix: &usize, _: &mut Window, cx: &mut Context<Self>) {
        self.state.result.active_tab = if *ix == 0 {
            ResultTab::Tree
        } else {
            ResultTab::Content
        };
        cx.notify();
    }

    fn copy_tree(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
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

    fn copy_preview(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = &self.state.workspace.preview_document else {
            self.push_notice(
                NotificationType::Warning,
                tr(self.state.settings.language, "no_content"),
                window,
                cx,
            );
            return;
        };
        match load_text(document) {
            Ok(content) => copy_to_clipboard(&content, self.state.settings.language, window, cx),
            Err(err) => self.push_notice(
                NotificationType::Error,
                format!("{}{}", tr(self.state.settings.language, "copy_failed"), err),
                window,
                cx,
            ),
        }
    }

    fn download_result(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(result) = &self.state.result.result else {
            return;
        };
        let Some(path) = &result.merged_content_path else {
            return;
        };
        let extension = match self.state.settings.options.output_format {
            crate::domain::OutputFormat::Xml => "xml",
            crate::domain::OutputFormat::Markdown => "md",
            crate::domain::OutputFormat::PlainText => "txt",
            crate::domain::OutputFormat::Default => "txt",
        };
        let Some(save_path) = rfd::FileDialog::new()
            .set_file_name(&format!("codemerge-output.{extension}"))
            .save_file()
        else {
            return;
        };
        match std::fs::copy(path, save_path) {
            Ok(_) => self.push_notice(
                NotificationType::Success,
                tr(self.state.settings.language, "saved"),
                window,
                cx,
            ),
            Err(err) => self.push_notice(
                NotificationType::Error,
                format!("{}{}", tr(self.state.settings.language, "save_failed"), err),
                window,
                cx,
            ),
        }
    }

    fn render_header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .justify_between()
            .items_center()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xl().font_semibold().child("CodeMerge"))
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child("GPUI Component Workspace"),
                    ),
            )
            .child(
                Button::new("toggle-language")
                    .outline()
                    .label(match self.state.settings.language {
                        Language::Zh => "EN",
                        Language::En => "中文",
                    })
                    .on_click(cx.listener(Self::toggle_language)),
            )
    }

    fn render_left_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let blacklist_filter = self
            .blacklist_filter_input
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        let blacklist_items = self
            .state
            .settings
            .folder_blacklist
            .iter()
            .map(|item| format!("folder: {item}"))
            .chain(
                self.state
                    .settings
                    .ext_blacklist
                    .iter()
                    .map(|item| format!("ext: {item}")),
            )
            .filter(|item| {
                blacklist_filter.is_empty() || item.to_ascii_lowercase().contains(&blacklist_filter)
            })
            .collect::<Vec<_>>();
        let blacklist_rows = Rc::new(
            blacklist_items
                .iter()
                .cloned()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        );

        card(cx).size_full().child(
            v_flex()
                .gap_4()
                .child(section_title(
                    tr(self.state.settings.language, "section_files"),
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("select-folder")
                                .primary()
                                .label(tr(self.state.settings.language, "select_folder"))
                                .on_click(cx.listener(Self::select_folder)),
                        )
                        .child(
                            Button::new("select-files")
                                .outline()
                                .label(tr(self.state.settings.language, "select_files"))
                                .on_click(cx.listener(Self::select_files)),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("select-gitignore")
                                .outline()
                                .label(tr(self.state.settings.language, "select_gitignore"))
                                .on_click(cx.listener(Self::select_gitignore)),
                        )
                        .child(
                            Button::new("apply-gitignore")
                                .outline()
                                .label(tr(self.state.settings.language, "apply_gitignore"))
                                .on_click(cx.listener(Self::apply_gitignore)),
                        ),
                )
                .child(render_kv(
                    tr(self.state.settings.language, "folder"),
                    self.state
                        .selection
                        .selected_folder
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| tr(self.state.settings.language, "none").to_string()),
                    cx,
                ))
                .child(render_kv(
                    tr(self.state.settings.language, "files"),
                    self.state.selection.selected_files.len().to_string(),
                    cx,
                ))
                .child(
                    v_flex().gap_1().children(
                        self.state
                            .selection
                            .selected_files
                            .iter()
                            .take(6)
                            .map(|entry| {
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} ({:.1} KB)",
                                        entry.name,
                                        entry.size as f64 / 1024.0
                                    ))
                                    .into_any_element()
                            }),
                    ),
                )
                .child(
                    Button::new("clear-inputs")
                        .danger()
                        .label(tr(self.state.settings.language, "clear"))
                        .on_click(cx.listener(Self::clear_inputs)),
                )
                .child(section_title(
                    tr(self.state.settings.language, "section_options"),
                    cx,
                ))
                .child(
                    Checkbox::new("compress")
                        .checked(self.state.settings.options.compress)
                        .label(tr(self.state.settings.language, "compress"))
                        .on_click(cx.listener(Self::toggle_compress)),
                )
                .child(
                    Checkbox::new("use-gitignore")
                        .checked(self.state.settings.options.use_gitignore)
                        .label(tr(self.state.settings.language, "use_gitignore"))
                        .on_click(cx.listener(Self::toggle_use_gitignore)),
                )
                .child(
                    Checkbox::new("ignore-git")
                        .checked(self.state.settings.options.ignore_git)
                        .label(tr(self.state.settings.language, "ignore_git"))
                        .on_click(cx.listener(Self::toggle_ignore_git)),
                )
                .child(
                    Checkbox::new("dedupe")
                        .checked(self.state.selection.dedupe_exact_path)
                        .label(tr(self.state.settings.language, "dedupe_exact_path"))
                        .on_click(cx.listener(Self::toggle_dedupe)),
                )
                .child(section_title(
                    tr(self.state.settings.language, "section_blacklist"),
                    cx,
                ))
                .child(Input::new(&self.blacklist_filter_input).cleanable(true))
                .child(Input::new(&self.blacklist_add_input))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("add-folder-blacklist")
                                .outline()
                                .label(tr(self.state.settings.language, "add_folder"))
                                .on_click(cx.listener(Self::add_folder_blacklist)),
                        )
                        .child(
                            Button::new("add-ext-blacklist")
                                .outline()
                                .label(tr(self.state.settings.language, "add_ext"))
                                .on_click(cx.listener(Self::add_ext_blacklist)),
                        ),
                )
                .child(if blacklist_rows.is_empty() {
                    div()
                        .max_h(px(220.))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(self.state.settings.language, "none"))
                        .into_any_element()
                } else {
                    let rows = blacklist_rows.clone();
                    div()
                        .h(px(220.))
                        .child(
                            v_virtual_list(
                                cx.entity().clone(),
                                "blacklist-items",
                                fixed_list_sizes(rows.len(), px(34.)),
                                move |_, visible_range, _, cx| {
                                    visible_range
                                        .filter_map(|ix| rows.get(ix).cloned())
                                        .map(|item| {
                                            div()
                                                .text_sm()
                                                .p_2()
                                                .h(px(34.))
                                                .rounded(cx.theme().radius)
                                                .bg(cx.theme().secondary)
                                                .child(item)
                                                .into_any_element()
                                        })
                                        .collect::<Vec<_>>()
                                },
                            )
                            .p_1(),
                        )
                        .into_any_element()
                })
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("import-blacklist")
                                .outline()
                                .label(tr(self.state.settings.language, "blacklist_import_append"))
                                .on_click(cx.listener(Self::import_blacklist)),
                        )
                        .child(
                            Button::new("export-blacklist")
                                .outline()
                                .label(tr(self.state.settings.language, "blacklist_export"))
                                .on_click(cx.listener(Self::export_blacklist)),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("reset-blacklist")
                                .outline()
                                .label(tr(self.state.settings.language, "blacklist_reset_default"))
                                .on_click(cx.listener(Self::reset_blacklist)),
                        )
                        .child(
                            Button::new("clear-blacklist")
                                .danger()
                                .label(tr(self.state.settings.language, "blacklist_clear_all"))
                                .on_click(cx.listener(Self::clear_blacklist)),
                        ),
                )
                .child(
                    Button::new("save-settings")
                        .primary()
                        .label(tr(self.state.settings.language, "save_settings"))
                        .on_click(cx.listener(Self::save_blacklists)),
                ),
        )
    }

    fn render_center_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let progress_rows = Rc::new(
            self.state
                .process
                .processing_records
                .iter()
                .rev()
                .take(2000)
                .map(|record| ProgressRowViewModel {
                    file_name: record.file_name.clone(),
                    status_label: match record.status {
                        ProcessStatus::Success => "[OK]".to_string(),
                        ProcessStatus::Skipped => "[SKIP]".to_string(),
                        ProcessStatus::Failed => "[ERR]".to_string(),
                    },
                })
                .collect::<Vec<_>>(),
        );
        let processed = self
            .state
            .process
            .processing_records
            .iter()
            .filter(|record| matches!(record.status, ProcessStatus::Success))
            .count();
        let elapsed = self
            .state
            .process
            .processing_started_at
            .map(|start| format_duration(start.elapsed()))
            .unwrap_or_else(|| "--:--".to_string());
        card(cx).size_full().child(
            v_flex()
                .gap_4()
                .child(section_title(
                    tr(self.state.settings.language, "section_summary"),
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .child(stat_tile(
                            tr(self.state.settings.language, "total"),
                            self.state.process.preflight.total_files.to_string(),
                            cx,
                        ))
                        .child(stat_tile(
                            tr(self.state.settings.language, "skip"),
                            self.state.process.preflight.skipped_files.to_string(),
                            cx,
                        ))
                        .child(stat_tile(
                            tr(self.state.settings.language, "process"),
                            self.state.process.preflight.to_process_files.to_string(),
                            cx,
                        )),
                )
                .child(if let Some(result) = &self.state.result.result {
                    h_flex()
                        .gap_2()
                        .child(stat_tile(
                            tr(self.state.settings.language, "chars"),
                            result.stats.total_chars.to_string(),
                            cx,
                        ))
                        .child(stat_tile(
                            tr(self.state.settings.language, "tokens"),
                            result.stats.total_tokens.to_string(),
                            cx,
                        ))
                        .into_any_element()
                } else {
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(self.state.settings.language, "no_stats"))
                        .into_any_element()
                })
                .child(section_title(
                    tr(self.state.settings.language, "section_progress"),
                    cx,
                ))
                .child(render_kv(
                    tr(self.state.settings.language, "progress_count"),
                    format!(
                        "{processed}/{}",
                        self.state.process.processing_candidates.max(1)
                    ),
                    cx,
                ))
                .child(render_kv(
                    tr(self.state.settings.language, "elapsed"),
                    elapsed,
                    cx,
                ))
                .child(render_kv(
                    tr(self.state.settings.language, "processing"),
                    self.state.process.processing_current_file.clone(),
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("start-process")
                                .primary()
                                .label(tr(self.state.settings.language, "start"))
                                .on_click(cx.listener(Self::start_process)),
                        )
                        .child(
                            Button::new("cancel-process")
                                .danger()
                                .label(tr(self.state.settings.language, "cancel"))
                                .on_click(cx.listener(Self::cancel_process)),
                        ),
                )
                .child(if progress_rows.is_empty() {
                    div()
                        .max_h(px(460.))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(self.state.settings.language, "status_ready"))
                        .into_any_element()
                } else {
                    let rows = progress_rows.clone();
                    div()
                        .h(px(460.))
                        .child(
                            v_virtual_list(
                                cx.entity().clone(),
                                "progress-rows",
                                fixed_list_sizes(rows.len(), px(26.)),
                                move |_, visible_range, _, cx| {
                                    visible_range
                                        .filter_map(|ix| rows.get(ix))
                                        .map(|row| {
                                            div()
                                                .text_sm()
                                                .h(px(26.))
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .child(format!(
                                                    "{} {}",
                                                    row.status_label, row.file_name
                                                ))
                                                .into_any_element()
                                        })
                                        .collect::<Vec<_>>()
                                },
                            )
                            .p_1(),
                        )
                        .into_any_element()
                }),
        )
    }

    fn render_right_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_tab = if self.state.result.active_tab == ResultTab::Tree {
            0
        } else {
            1
        };
        card(cx).size_full().child(
            v_flex()
                .gap_3()
                .size_full()
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            TabBar::new("result-tabs")
                                .selected_index(selected_tab)
                                .on_click(cx.listener(Self::set_tab))
                                .child(
                                    Tab::new().label(tr(
                                        self.state.settings.language,
                                        "tab_tree_preview",
                                    )),
                                )
                                .child(
                                    Tab::new().label(tr(
                                        self.state.settings.language,
                                        "tab_merged_content",
                                    )),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new("copy-active")
                                        .outline()
                                        .label(if self.state.result.active_tab == ResultTab::Tree {
                                            tr(self.state.settings.language, "copy_tree")
                                        } else {
                                            tr(self.state.settings.language, "copy_current_page")
                                        })
                                        .on_click(cx.listener(
                                            if self.state.result.active_tab == ResultTab::Tree {
                                                Self::copy_tree
                                            } else {
                                                Self::copy_preview
                                            },
                                        )),
                                )
                                .child(
                                    Button::new("download-result")
                                        .outline()
                                        .label(tr(self.state.settings.language, "download"))
                                        .disabled(self.state.result.active_tab == ResultTab::Tree)
                                        .on_click(cx.listener(Self::download_result)),
                                ),
                        ),
                )
                .child(match self.state.result.active_tab {
                    ResultTab::Tree => self.render_tree_panel(cx).into_any_element(),
                    ResultTab::Content => self.render_content_panel(cx).into_any_element(),
                }),
        )
    }

    fn render_tree_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        tree(&self.tree_state, move |ix, entry, _, _, cx| {
            view.update(cx, |_, cx| {
                let item = entry.item();
                let icon = if !entry.is_folder() {
                    IconName::File
                } else if entry.is_expanded() {
                    IconName::FolderOpen
                } else {
                    IconName::Folder
                };
                ListItem::new(ix)
                    .w_full()
                    .px_3()
                    .pl(px(16.) * entry.depth() + px(12.))
                    .rounded(cx.theme().radius)
                    .child(h_flex().gap_2().child(icon).child(item.label.clone()))
            })
        })
        .h_full()
    }

    fn render_content_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .size_full()
            .child(Input::new(&self.preview_filter_input).cleanable(true))
            .child(
                div().h(px(220.)).child(
                    Table::new(&self.preview_table)
                        .with_size(Size::Small)
                        .stripe(true),
                ),
            )
            .child(self.render_preview(cx))
    }

    fn render_preview(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(document) = &self.state.workspace.preview_document else {
            return div()
                .flex_1()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .items_center()
                .justify_center()
                .child(tr(self.state.settings.language, "no_content"));
        };
        let line_count = document.line_count();
        v_flex()
            .gap_2()
            .flex_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} lines | {} bytes | cached {:?} | visible {:?}",
                        line_count,
                        document.byte_len(),
                        self.state.workspace.preview_loaded_range,
                        self.state.workspace.preview_visible_range
                    )),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(cx.theme().radius)
                    .child(
                        v_virtual_list(
                            cx.entity().clone(),
                            "preview-lines",
                            self.state.workspace.preview_sizes.clone(),
                            move |view, visible_range, _, cx| {
                                view.state.workspace.preview_visible_range = visible_range.clone();
                                let Some(document) = &view.state.workspace.preview_document else {
                                    return Vec::new();
                                };
                                let line_count = document.line_count();
                                visible_range
                                    .filter(|ix| *ix < line_count)
                                    .map(|ix| {
                                        let line = if ix
                                            >= view.state.workspace.preview_loaded_range.start
                                            && ix < view.state.workspace.preview_loaded_range.end
                                        {
                                            view.state
                                                .workspace
                                                .preview_loaded_lines
                                                .get(
                                                    ix - view
                                                        .state
                                                        .workspace
                                                        .preview_loaded_range
                                                        .start,
                                                )
                                                .cloned()
                                                .unwrap_or_default()
                                        } else {
                                            SharedString::from("")
                                        };
                                        h_flex()
                                            .gap_3()
                                            .px_3()
                                            .h(preview_line_height())
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .child(
                                                div()
                                                    .w(px(64.))
                                                    .text_right()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child((ix + 1).to_string()),
                                            )
                                            .child(div().flex_1().child(line))
                                    })
                                    .collect()
                            },
                        )
                        .track_scroll(&self.preview_scroll_handle)
                        .p_2(),
                    ),
            )
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("codemerge-root")
            .track_focus(&self.focus_handle)
            .size_full()
            .p_4()
            .gap_4()
            .child(self.render_header(cx))
            .child(
                div().flex_1().child(
                    h_resizable("codemerge-layout")
                        .child(
                            resizable_panel()
                                .size(px(340.))
                                .size_range(px(280.)..px(460.))
                                .child(self.render_left_panel(cx)),
                        )
                        .child(
                            resizable_panel()
                                .size(px(360.))
                                .size_range(px(280.)..px(520.))
                                .child(self.render_center_panel(cx)),
                        )
                        .child(resizable_panel().child(self.render_right_panel(cx))),
                ),
            )
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if let Some(dir) = self
            .state
            .result
            .result
            .as_ref()
            .and_then(|result| result.preview_blob_dir.as_ref())
        {
            let _ = crate::utils::temp_file::cleanup_preview_dir(dir);
        }
    }
}

fn build_tree_items(nodes: &[TreeNode]) -> Vec<TreeItem> {
    nodes
        .iter()
        .map(|node| {
            let mut item = TreeItem::new(node.id.clone(), node.label.clone());
            if node.is_folder {
                item = item.children(build_tree_items(&node.children));
            }
            item
        })
        .collect()
}

fn card(cx: &App) -> gpui::Div {
    div()
        .p_4()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .rounded(cx.theme().radius)
}

fn section_title(title: &str, cx: &App) -> AnyElement {
    div()
        .font_semibold()
        .text_color(cx.theme().foreground)
        .child(title.to_string())
        .into_any_element()
}

fn render_kv(label: &str, value: String, cx: &App) -> AnyElement {
    h_flex()
        .justify_between()
        .gap_3()
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(div().text_sm().child(value))
        .into_any_element()
}

fn stat_tile(label: &str, value: String, cx: &App) -> AnyElement {
    div()
        .flex_1()
        .p_3()
        .rounded(cx.theme().radius)
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(label.to_string()),
                )
                .child(div().text_lg().font_semibold().child(value)),
        )
        .into_any_element()
}

fn copy_to_clipboard(content: &str, language: Language, window: &mut Window, cx: &mut App) {
    match Clipboard::new().and_then(|mut clip| clip.set_text(content.to_string())) {
        Ok(_) => window.push_notification((NotificationType::Success, tr(language, "copied")), cx),
        Err(err) => window.push_notification(
            (
                NotificationType::Error,
                SharedString::from(format!("{}{}", tr(language, "copy_failed"), err)),
            ),
            cx,
        ),
    }
}

fn format_duration(duration: Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}
