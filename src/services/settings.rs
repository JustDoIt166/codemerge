use crate::domain::{AppConfigV1, SettingsCommand};
use crate::error::AppResult;
use crate::utils::config_store;

pub use crate::utils::config_store::{ConfigLoadIssue, ConfigLoadReport};

pub fn load() -> AppConfigV1 {
    config_store::load_config()
}

pub fn load_report() -> ConfigLoadReport {
    config_store::load_config_report()
}

pub fn execute(command: SettingsCommand) -> AppResult<AppConfigV1> {
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
