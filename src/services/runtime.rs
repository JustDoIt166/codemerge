use once_cell::sync::Lazy;
use tokio::runtime::{Builder, Runtime};

pub static RUNTIME: Lazy<Result<Runtime, String>> = Lazy::new(|| {
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("tokio runtime init failed: {err}"))
});
