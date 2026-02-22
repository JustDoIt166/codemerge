use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::model::{
    Language, ProcessingOptions, default_ext_blacklist, default_folder_blacklist,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigV1 {
    pub language: Language,
    pub options: ProcessingOptions,
    pub folder_blacklist: Vec<String>,
    pub ext_blacklist: Vec<String>,
}

impl Default for AppConfigV1 {
    fn default() -> Self {
        Self {
            language: Language::Zh,
            options: ProcessingOptions::default(),
            folder_blacklist: default_folder_blacklist(),
            ext_blacklist: default_ext_blacklist(),
        }
    }
}

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

pub fn save_config(cfg: &AppConfigV1) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Err("config dir unavailable".to_string());
    };

    let parent = path
        .parent()
        .ok_or_else(|| "invalid config path".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("create config dir failed: {e}"))?;

    let body =
        serde_json::to_string_pretty(cfg).map_err(|e| format!("serialize config failed: {e}"))?;
    std::fs::write(path, body).map_err(|e| format!("write config failed: {e}"))
}
