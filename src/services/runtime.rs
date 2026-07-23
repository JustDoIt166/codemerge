use once_cell::sync::Lazy;
use tokio::runtime::{Builder, Runtime};

use crate::error::{AppError, AppResult, ErrorCode};

pub static RUNTIME: Lazy<AppResult<Runtime>> = Lazy::new(|| {
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            let message = format!("tokio runtime init failed: {error}");
            AppError::from_source(ErrorCode::TaskPanicked, "task-runtime", message, error)
        })
});
