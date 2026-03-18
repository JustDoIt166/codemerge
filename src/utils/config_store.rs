use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::AppConfigV1;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigLoadIssue {
    ConfigDirUnavailable,
    MissingFile,
    ReadFailed(String),
    ParseFailed(String),
}

#[derive(Debug, Clone)]
pub struct ConfigLoadReport {
    pub config: AppConfigV1,
    pub issue: Option<ConfigLoadIssue>,
    pub path: Option<PathBuf>,
}

pub fn config_path() -> Option<PathBuf> {
    let base = dirs::config_dir()?;
    Some(base.join("codemerge").join("config.json"))
}

pub fn load_config() -> AppConfigV1 {
    load_config_report().config
}

pub fn load_config_report() -> ConfigLoadReport {
    let Some(path) = config_path() else {
        return ConfigLoadReport {
            config: AppConfigV1::default(),
            issue: Some(ConfigLoadIssue::ConfigDirUnavailable),
            path: None,
        };
    };

    load_config_report_from_path(&path)
}

pub fn load_config_report_from_path(path: &Path) -> ConfigLoadReport {
    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(config) => ConfigLoadReport {
                config,
                issue: None,
                path: Some(path.to_path_buf()),
            },
            Err(err) => ConfigLoadReport {
                config: AppConfigV1::default(),
                issue: Some(ConfigLoadIssue::ParseFailed(err.to_string())),
                path: Some(path.to_path_buf()),
            },
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ConfigLoadReport {
            config: AppConfigV1::default(),
            issue: Some(ConfigLoadIssue::MissingFile),
            path: Some(path.to_path_buf()),
        },
        Err(err) => ConfigLoadReport {
            config: AppConfigV1::default(),
            issue: Some(ConfigLoadIssue::ReadFailed(err.to_string())),
            path: Some(path.to_path_buf()),
        },
    }
}

pub fn save_config(cfg: &AppConfigV1) -> AppResult<()> {
    let Some(path) = config_path() else {
        return Err(AppError::new("config dir unavailable"));
    };

    save_config_to_path(cfg, &path)
}

pub fn save_config_to_path(cfg: &AppConfigV1, path: &Path) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("invalid config path"))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| AppError::new(format!("create config dir failed: {e}")))?;

    let body = serde_json::to_string_pretty(cfg)
        .map_err(|e| AppError::new(format!("serialize config failed: {e}")))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| AppError::new(format!("create temp config file failed: {e}")))?;
    temp.as_file_mut()
        .write_all(body.as_bytes())
        .map_err(|e| AppError::new(format!("write temp config failed: {e}")))?;
    temp.as_file_mut()
        .flush()
        .map_err(|e| AppError::new(format!("flush temp config failed: {e}")))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| AppError::new(format!("sync temp config failed: {e}")))?;

    temp.persist(path)
        .map_err(|err| AppError::new(format!("persist config failed: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{ConfigLoadIssue, load_config_report_from_path, save_config_to_path};
    use crate::domain::{AppConfigV1, Language, OutputFormat, ProcessingMode, ProcessingOptions};

    #[test]
    fn missing_file_returns_default_with_issue() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.json");

        let report = load_config_report_from_path(&path);

        assert_eq!(report.config.language, AppConfigV1::default().language);
        assert_eq!(report.issue, Some(ConfigLoadIssue::MissingFile));
    }

    #[test]
    fn invalid_json_returns_default_with_parse_issue() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{invalid").expect("write invalid json");

        let report = load_config_report_from_path(&path);

        assert!(matches!(
            report.issue,
            Some(ConfigLoadIssue::ParseFailed(_))
        ));
    }

    #[test]
    fn save_config_errors_when_parent_is_not_directory() {
        let dir = tempdir().expect("tempdir");
        let blocker = dir.path().join("blocker");
        let path = blocker.join("config.json");
        std::fs::write(&blocker, "x").expect("write blocker file");

        let result = save_config_to_path(&AppConfigV1::default(), &path);

        assert!(result.is_err());
    }

    #[test]
    fn save_then_load_roundtrip_replaces_existing_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{broken").expect("seed broken file");

        let config = AppConfigV1 {
            language: Language::En,
            options: ProcessingOptions {
                compress: true,
                use_gitignore: false,
                ignore_git: false,
                output_format: OutputFormat::Markdown,
                mode: ProcessingMode::TreeOnly,
            },
            folder_blacklist: vec!["src".to_string(), "build".to_string()],
            ext_blacklist: vec![".log".to_string(), ".tmp".to_string()],
        };

        save_config_to_path(&config, &path).expect("save config");

        let report = load_config_report_from_path(&path);
        assert_eq!(report.issue, None);
        assert_eq!(report.config.language, config.language);
        assert_eq!(report.config.options.compress, config.options.compress);
        assert_eq!(
            report.config.options.use_gitignore,
            config.options.use_gitignore
        );
        assert_eq!(report.config.options.ignore_git, config.options.ignore_git);
        assert_eq!(
            report.config.options.output_format,
            config.options.output_format
        );
        assert_eq!(report.config.options.mode, config.options.mode);
        assert_eq!(report.config.folder_blacklist, config.folder_blacklist);
        assert_eq!(report.config.ext_blacklist, config.ext_blacklist);
    }

    #[test]
    fn unreadable_path_returns_read_issue() {
        let dir = tempdir().expect("tempdir");

        let report = load_config_report_from_path(dir.path());

        assert!(matches!(report.issue, Some(ConfigLoadIssue::ReadFailed(_))));
    }
}
