use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use crate::domain::{AppConfigV1, SettingsCommand};
use crate::error::AppError;
use crate::error::AppResult;
use crate::utils::config_store;

pub use crate::utils::config_store::{ConfigLoadIssue, ConfigLoadReport};

struct SaveCoordinator {
    revision: AtomicU64,
    lock: Mutex<()>,
}

impl SaveCoordinator {
    fn new() -> Self {
        Self {
            revision: AtomicU64::new(0),
            lock: Mutex::new(()),
        }
    }

    fn begin(&self) -> u64 {
        self.revision.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn is_latest(&self, revision: u64) -> bool {
        self.revision.load(Ordering::Acquire) == revision
    }

    fn save_if_latest_with<F>(
        &self,
        revision: u64,
        config: AppConfigV1,
        save: F,
    ) -> AppResult<Option<AppConfigV1>>
    where
        F: FnOnce(&AppConfigV1) -> AppResult<()>,
    {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::new("settings save lock poisoned"))?;
        if !self.is_latest(revision) {
            return Ok(None);
        }
        save(&config)?;
        Ok(Some(config))
    }
}

static SAVE_COORDINATOR: LazyLock<SaveCoordinator> = LazyLock::new(SaveCoordinator::new);

pub fn begin_save() -> u64 {
    SAVE_COORDINATOR.begin()
}

pub fn is_latest_save(revision: u64) -> bool {
    SAVE_COORDINATOR.is_latest(revision)
}

pub fn save_if_latest(revision: u64, config: AppConfigV1) -> AppResult<Option<AppConfigV1>> {
    SAVE_COORDINATOR.save_if_latest_with(revision, config, config_store::save_config)
}

pub fn load() -> AppConfigV1 {
    config_store::load_config()
}

pub fn load_report() -> ConfigLoadReport {
    config_store::load_config_report()
}

pub fn execute(command: SettingsCommand) -> AppResult<AppConfigV1> {
    let config = match command {
        SettingsCommand::Save(config) => config,
        SettingsCommand::ResetToDefault => AppConfigV1::default(),
    };
    let revision = begin_save();
    let saved = save_if_latest(revision, config.clone())?;
    Ok(saved.unwrap_or(config))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::SaveCoordinator;
    use crate::domain::AppConfigV1;

    #[test]
    fn stale_settings_save_is_skipped_after_newer_revision() {
        let coordinator = SaveCoordinator::new();
        let stale = coordinator.begin();
        let latest = coordinator.begin();
        let writes = AtomicUsize::new(0);

        let stale_result = coordinator
            .save_if_latest_with(stale, AppConfigV1::default(), |_| {
                writes.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
            .expect("stale save result");
        let latest_result = coordinator
            .save_if_latest_with(latest, AppConfigV1::default(), |_| {
                writes.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
            .expect("latest save result");

        assert!(stale_result.is_none());
        assert!(latest_result.is_some());
        assert_eq!(writes.load(Ordering::Relaxed), 1);
    }
}
