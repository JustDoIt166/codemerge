use iced::Task;

use crate::app::message::{I18nMessage, Message};
use crate::app::model::Model;

pub fn update_i18n(model: &mut Model, msg: I18nMessage) -> Task<Message> {
    match msg {
        I18nMessage::ToggleLanguage => model.language = model.language.toggle(),
        I18nMessage::Set(v) => model.language = v,
    }

    super::queue_config_save(model, super::CONFIG_SAVE_DEBOUNCE_MS, None);
    Task::none()
}
