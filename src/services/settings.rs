use crate::domain::{AppConfigV1, SettingsCommand};
use crate::utils::config_store;

pub fn load() -> AppConfigV1 {
    config_store::load_config()
}

pub fn execute(command: SettingsCommand) -> Result<AppConfigV1, String> {
    match command {
        SettingsCommand::Save(config) => {
            config_store::save_config(&config)?;
            Ok(config)
        }
        SettingsCommand::ResetToDefault => {
            let config = AppConfigV1::default();
            config_store::save_config(&config)?;
            Ok(config)
        }
    }
}
