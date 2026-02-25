use iced::Task;

use crate::app::message::{ConfigMessage, Message};
use crate::app::model::{Model, Toast, ToastStyle};
use crate::utils::i18n::tr;

pub fn update_config(model: &mut Model, msg: ConfigMessage) -> Task<Message> {
    let mut needs_preflight_refresh = false;
    match msg {
        ConfigMessage::ToggleCompress(v) => model.options.compress = v,
        ConfigMessage::ToggleUseGitignore(v) => model.options.use_gitignore = v,
        ConfigMessage::ToggleIgnoreGit(v) => {
            model.options.ignore_git = v;
            if v {
                if !model.folder_blacklist.contains(&".git".to_string()) {
                    model.folder_blacklist.push(".git".to_string());
                    needs_preflight_refresh = true;
                }
            } else {
                let before = model.folder_blacklist.len();
                model.folder_blacklist.retain(|x| x != ".git");
                needs_preflight_refresh = model.folder_blacklist.len() != before;
            }
        }
        ConfigMessage::SetOutputFormat(v) => model.options.output_format = v,
        ConfigMessage::SetMode(v) => model.options.mode = v,
        ConfigMessage::ToggleDedupe(v) => model.dedupe_exact_path = v,
    }

    super::queue_config_save(
        model,
        super::CONFIG_SAVE_DEBOUNCE_MS,
        Some(Toast {
            message: tr(model.language, "config_updated").to_string(),
            style: ToastStyle::Info,
            duration: std::time::Duration::from_secs(2),
        }),
    );

    if needs_preflight_refresh {
        super::refresh_preflight(model)
    } else {
        Task::none()
    }
}
