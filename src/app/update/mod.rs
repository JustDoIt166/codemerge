pub mod blacklist;
pub mod config;
pub mod file;
pub mod i18n;
pub mod process;
pub mod ui;

use iced::Task;

use crate::app::message::Message;
use crate::app::model::Model;
use crate::processor::walker::collect_candidates;

pub fn update(model: &mut Model, message: Message) -> Task<Message> {
    match message {
        Message::File(m) => file::update_file(model, m),
        Message::Config(m) => config::update_config(model, m),
        Message::Blacklist(m) => blacklist::update_blacklist(model, m),
        Message::Process(m) => process::update_process(model, m),
        Message::Ui(m) => ui::update_ui(model, m),
        Message::I18n(m) => i18n::update_i18n(model, m),
        Message::Tick => ui::on_tick(model),
    }
}

pub fn refresh_preflight(model: &mut Model) {
    let selected_files = model
        .selected_files
        .iter()
        .map(|f| f.path.clone())
        .collect::<Vec<_>>();
    let res = collect_candidates(
        model.selected_folder.as_ref(),
        &selected_files,
        &model.folder_blacklist,
        &model.ext_blacklist,
    );
    model.preflight.total_files = res.candidates.len() + res.skipped;
    model.preflight.skipped_files = res.skipped;
    model.preflight.to_process_files = res.candidates.len();
}
