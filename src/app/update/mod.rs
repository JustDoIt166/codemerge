pub mod blacklist;
pub mod config;
pub mod file;
pub mod i18n;
pub mod process;
pub mod ui;

use futures::stream;
use iced::Task;
use tokio::sync::mpsc;

use crate::app::message::{Message, PreflightUpdate, UiMessage};
use crate::app::model::{Model, Toast, ToastStyle};
use crate::processor::walker::collect_candidates_with_progress;
use crate::utils::i18n::tr;

pub const TICK_MS: u64 = 120;
pub const CONFIG_SAVE_DEBOUNCE_MS: u64 = 320;
const CONFIG_SAVE_RETRY_MS: u64 = 240;

pub fn update(model: &mut Model, message: Message) -> Task<Message> {
    match message {
        Message::File(m) => file::update_file(model, m),
        Message::Config(m) => config::update_config(model, m),
        Message::Blacklist(m) => blacklist::update_blacklist(model, m),
        Message::Process(m) => process::update_process(model, m),
        Message::Ui(m) => ui::update_ui(model, m),
        Message::I18n(m) => i18n::update_i18n(model, m),
        Message::ConfigSaved(res) => on_config_saved(model, res),
        Message::Tick => ui::on_tick(model),
    }
}

pub fn queue_config_save(model: &mut Model, debounce_ms: u64, success_toast: Option<Toast>) {
    model.ui.config_save_dirty = true;
    model.ui.config_save_due_ms = Some(debounce_ms.max(TICK_MS));
    if let Some(toast) = success_toast {
        model.ui.config_save_success_toast = Some(toast);
    }
}

pub fn take_due_config_save_task(model: &mut Model) -> Task<Message> {
    if model.ui.config_save_in_flight {
        return Task::none();
    }

    let Some(remaining) = model.ui.config_save_due_ms else {
        return Task::none();
    };

    if remaining > TICK_MS {
        model.ui.config_save_due_ms = Some(remaining - TICK_MS);
        return Task::none();
    }

    model.ui.config_save_due_ms = None;
    if !model.ui.config_save_dirty {
        return Task::none();
    }
    model.ui.config_save_dirty = false;
    model.ui.config_save_in_flight = true;

    let cfg = crate::utils::config_store::AppConfigV1 {
        language: model.language,
        options: model.options.clone(),
        folder_blacklist: model.folder_blacklist.clone(),
        ext_blacklist: model.ext_blacklist.clone(),
    };

    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || crate::utils::config_store::save_config(&cfg))
                .await
                .map_err(|e| format!("save config task failed: {e}"))?
        },
        Message::ConfigSaved,
    )
}

fn on_config_saved(model: &mut Model, result: Result<(), String>) -> Task<Message> {
    model.ui.config_save_in_flight = false;

    match result {
        Ok(_) => {
            if let Some(toast) = model.ui.config_save_success_toast.take() {
                model.ui.toast = Some(toast);
            }
        }
        Err(err) => {
            model.ui.toast = Some(Toast {
                message: format!("{}{}", tr(model.language, "save_config_failed"), err),
                style: ToastStyle::Error,
                duration: std::time::Duration::from_secs(3),
            });
        }
    }

    if model.ui.config_save_dirty && model.ui.config_save_due_ms.is_none() {
        model.ui.config_save_due_ms = Some(CONFIG_SAVE_RETRY_MS);
    }

    Task::none()
}

pub fn refresh_preflight(model: &mut Model) -> Task<Message> {
    model.preflight_revision = model.preflight_revision.wrapping_add(1);
    let revision = model.preflight_revision;
    model.preflight.is_scanning = true;
    model.preflight.scanned_entries = 0;
    model.preflight.total_files = 0;
    model.preflight.skipped_files = 0;
    model.preflight.to_process_files = 0;

    let selected_folder = model.selected_folder.clone();
    let selected_files = model
        .selected_files
        .iter()
        .map(|f| f.path.clone())
        .collect::<Vec<_>>();
    if selected_folder.is_none() && selected_files.is_empty() {
        model.preflight.is_scanning = false;
        return Task::none();
    }
    let folder_blacklist = model.folder_blacklist.clone();
    let ext_blacklist = model.ext_blacklist.clone();

    Task::run(
        spawn_preflight_stream(
            revision,
            selected_folder,
            selected_files,
            folder_blacklist,
            ext_blacklist,
        ),
        |update| Message::Ui(UiMessage::PreflightUpdate(update)),
    )
}

fn spawn_preflight_stream(
    revision: u64,
    selected_folder: Option<std::path::PathBuf>,
    selected_files: Vec<std::path::PathBuf>,
    folder_blacklist: Vec<String>,
    ext_blacklist: Vec<String>,
) -> impl futures::Stream<Item = PreflightUpdate> {
    let (tx, rx) = mpsc::unbounded_channel::<PreflightUpdate>();

    tokio::spawn(async move {
        let _ = tx.send(PreflightUpdate::Started { revision });
        let progress_tx = tx.clone();
        let handle = tokio::task::spawn_blocking(move || {
            collect_candidates_with_progress(
                selected_folder.as_ref(),
                &selected_files,
                &folder_blacklist,
                &ext_blacklist,
                move |scanned, candidates, skipped| {
                    let _ = progress_tx.send(PreflightUpdate::Progress {
                        revision,
                        scanned,
                        candidates,
                        skipped,
                    });
                },
            )
        });

        match handle.await {
            Ok(res) => {
                let _ = tx.send(PreflightUpdate::Completed {
                    revision,
                    stats: crate::app::model::PreflightStats {
                        total_files: res.candidates.len() + res.skipped,
                        skipped_files: res.skipped,
                        to_process_files: res.candidates.len(),
                        scanned_entries: res.candidates.len() + res.skipped,
                        is_scanning: false,
                    },
                });
            }
            Err(e) => {
                let _ = tx.send(PreflightUpdate::Failed {
                    revision,
                    error: format!("preflight failed: {e}"),
                });
            }
        }
    });

    stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}
