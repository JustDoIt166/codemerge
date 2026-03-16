use std::path::PathBuf;

use crate::domain::AppConfigV1;
use crate::error::{AppError, AppResult};

pub fn config_path() -> Option<PathBuf> {
    let base = dirs::config_dir()?;
    Some(base.join("codemerge").join("config.json"))
}

pub fn load_config() -> AppConfigV1 {
    let Some(path) = config_path() else {
        return AppConfigV1::default();
    };

    let content = std::fs::read_to_string(path);
    match content {
        Ok(v) => serde_json::from_str(&v).unwrap_or_default(),
        Err(_) => AppConfigV1::default(),
    }
}

pub fn save_config(cfg: &AppConfigV1) -> AppResult<()> {
    let Some(path) = config_path() else {
        return Err(AppError::new("config dir unavailable"));
    };

    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("invalid config path"))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| AppError::new(format!("create config dir failed: {e}")))?;

    let body = serde_json::to_string_pretty(cfg)
        .map_err(|e| AppError::new(format!("serialize config failed: {e}")))?;
    std::fs::write(path, body).map_err(|e| AppError::new(format!("write config failed: {e}")))
}
