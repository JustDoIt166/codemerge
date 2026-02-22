use iced::Task;

use crate::app::message::{I18nMessage, Message};
use crate::app::model::Model;

pub fn update_i18n(model: &mut Model, msg: I18nMessage) -> Task<Message> {
    match msg {
        I18nMessage::ToggleLanguage => model.language = model.language.toggle(),
        I18nMessage::Set(v) => model.language = v,
    }

    let cfg = crate::utils::config_store::AppConfigV1 {
        language: model.language,
        options: model.options.clone(),
        folder_blacklist: model.folder_blacklist.clone(),
        ext_blacklist: model.ext_blacklist.clone(),
    };
    let _ = crate::utils::config_store::save_config(&cfg);

    Task::none()
}
