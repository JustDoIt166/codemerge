use iced::Task;

use crate::app::message::{ConfigMessage, Message};
use crate::app::model::{Model, Toast, ToastStyle};
use crate::utils::i18n::tr;

pub fn update_config(model: &mut Model, msg: ConfigMessage) -> Task<Message> {
    match msg {
        ConfigMessage::ToggleCompress(v) => model.options.compress = v,
        ConfigMessage::ToggleUseGitignore(v) => model.options.use_gitignore = v,
        ConfigMessage::ToggleIgnoreGit(v) => {
            model.options.ignore_git = v;
            if v {
                if !model.folder_blacklist.contains(&".git".to_string()) {
                    model.folder_blacklist.push(".git".to_string());
                }
            } else {
                model.folder_blacklist.retain(|x| x != ".git");
            }
        }
        ConfigMessage::SetOutputFormat(v) => model.options.output_format = v,
        ConfigMessage::SetMode(v) => model.options.mode = v,
        ConfigMessage::ToggleDedupe(v) => model.dedupe_exact_path = v,
    }

    let cfg = crate::utils::config_store::AppConfigV1 {
        language: model.language,
        options: model.options.clone(),
        folder_blacklist: model.folder_blacklist.clone(),
        ext_blacklist: model.ext_blacklist.clone(),
    };

    if let Err(e) = crate::utils::config_store::save_config(&cfg) {
        model.ui.toast = Some(Toast {
            message: format!("save config failed: {e}"),
            style: ToastStyle::Error,
            duration: std::time::Duration::from_secs(3),
        });
    } else {
        model.ui.toast = Some(Toast {
            message: tr(model.language, "config_updated").to_string(),
            style: ToastStyle::Info,
            duration: std::time::Duration::from_secs(2),
        });
    }

    super::refresh_preflight(model);
    Task::none()
}
