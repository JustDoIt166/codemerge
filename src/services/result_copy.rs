use std::io::Read as _;
use std::path::PathBuf;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult};

const COPY_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ResultCopyRequest {
    pub revision: u64,
    pub path: PathBuf,
    pub total: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResultCopyEvent {
    Progress {
        revision: u64,
        read: u64,
        total: u64,
    },
    Copied {
        revision: u64,
    },
    Failed {
        revision: u64,
        error: String,
    },
    Cancelled {
        revision: u64,
    },
}

pub type ResultCopyEventSink = Arc<dyn Fn(ResultCopyEvent) + Send + Sync>;

pub async fn execute(
    request: ResultCopyRequest,
    cancel: CancellationToken,
    emit: ResultCopyEventSink,
) -> AppResult<()> {
    tokio::task::spawn_blocking(move || copy_in_background(request, cancel, emit))
        .await
        .map_err(|error| AppError::new(format!("copy task failed: {error}")))?
}

fn copy_in_background(
    request: ResultCopyRequest,
    cancel: CancellationToken,
    emit: ResultCopyEventSink,
) -> AppResult<()> {
    let mut file = match std::fs::File::open(&request.path) {
        Ok(file) => file,
        Err(error) => {
            emit(ResultCopyEvent::Failed {
                revision: request.revision,
                error: error.to_string(),
            });
            return Ok(());
        }
    };
    let mut bytes = Vec::new();
    let mut buffer = vec![0; COPY_CHUNK_BYTES];
    let mut read = 0_u64;

    loop {
        if cancel.is_cancelled() {
            emit(ResultCopyEvent::Cancelled {
                revision: request.revision,
            });
            return Ok(());
        }
        let count = match file.read(&mut buffer) {
            Ok(count) => count,
            Err(error) => {
                emit(ResultCopyEvent::Failed {
                    revision: request.revision,
                    error: error.to_string(),
                });
                return Ok(());
            }
        };
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        read = read.saturating_add(count as u64);
        emit(ResultCopyEvent::Progress {
            revision: request.revision,
            read,
            total: request.total.max(read),
        });
    }

    if cancel.is_cancelled() {
        emit(ResultCopyEvent::Cancelled {
            revision: request.revision,
        });
        return Ok(());
    }

    let content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(error) => {
            emit(ResultCopyEvent::Failed {
                revision: request.revision,
                error: error.to_string(),
            });
            return Ok(());
        }
    };
    let clipboard_result = arboard::Clipboard::new().and_then(|mut clipboard| {
        clipboard.set_text(content)?;
        Ok(())
    });
    match clipboard_result {
        Ok(()) => emit(ResultCopyEvent::Copied {
            revision: request.revision,
        }),
        Err(error) => emit(ResultCopyEvent::Failed {
            revision: request.revision,
            error: error.to_string(),
        }),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tokio_util::sync::CancellationToken;

    use super::{ResultCopyEvent, ResultCopyEventSink, ResultCopyRequest, execute};

    #[test]
    fn large_copy_reports_progress_and_can_be_cancelled_before_clipboard_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("large.txt");
        let file = std::fs::File::create(&path).expect("create large file");
        file.set_len(64 * 1024 * 1024).expect("size large file");
        let cancel = CancellationToken::new();
        let cancel_after_progress = cancel.clone();
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorded_events = Arc::clone(&events);
        let emit: ResultCopyEventSink = Arc::new(move |event| {
            if matches!(event, ResultCopyEvent::Progress { .. }) {
                cancel_after_progress.cancel();
            }
            recorded_events.lock().expect("events lock").push(event);
        });

        crate::services::runtime::RUNTIME
            .as_ref()
            .expect("runtime")
            .block_on(execute(
                ResultCopyRequest {
                    revision: 7,
                    path,
                    total: 64 * 1024 * 1024,
                },
                cancel,
                emit,
            ))
            .expect("copy task");

        let events = events.lock().expect("events lock");
        assert!(matches!(
            events.first(),
            Some(ResultCopyEvent::Progress { revision: 7, .. })
        ));
        assert!(matches!(
            events.last(),
            Some(ResultCopyEvent::Cancelled { revision: 7 })
        ));
    }
}
