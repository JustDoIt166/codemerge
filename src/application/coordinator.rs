use std::sync::Arc;

use crate::application::task::{JobKind, TaskStream, TaskSupervisor};
use crate::error::AppResult;
use crate::processor::walker::WalkerOptions;
use crate::services::preflight::{
    PreflightEvent, PreflightEventSink, PreflightRequest, execute as execute_preflight,
};
use crate::services::preview::{PreviewEvent, PreviewRequest, execute as execute_preview};
use crate::services::process::{
    ProcessEvent, ProcessEventSink, ProcessRequest, execute as execute_process,
};

pub struct WorkspaceCoordinator {
    tasks: TaskSupervisor,
}

impl Default for WorkspaceCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceCoordinator {
    pub fn new() -> Self {
        Self {
            tasks: TaskSupervisor::new(),
        }
    }

    pub fn cancel_all(&self) {
        self.tasks.cancel_all();
    }

    pub fn request_cancel(&self, kind: JobKind) -> bool {
        self.tasks.request_cancel(kind)
    }

    pub fn start_process(&self, request: ProcessRequest) -> AppResult<TaskStream<ProcessEvent>> {
        self.tasks
            .start_latest(JobKind::Process, move |context| async move {
                let progress_context = context.clone();
                let emit: ProcessEventSink = Arc::new(move |event| {
                    let _ = progress_context.emit(event);
                });
                let job_id = context.job_id;
                let cancel = context.cancel.clone();
                let outcome = execute_process(request, job_id, cancel.clone(), emit).await;
                match outcome {
                    Ok(_) if cancel.is_cancelled() => {
                        let _ = context.emit(ProcessEvent::cancelled(job_id));
                    }
                    Ok(result) => {
                        let _ = context.emit(ProcessEvent::completed(job_id, result));
                    }
                    Err(_) if cancel.is_cancelled() => {
                        let _ = context.emit(ProcessEvent::cancelled(job_id));
                    }
                    Err(error) => {
                        let _ = context.emit(ProcessEvent::failed(job_id, error));
                    }
                }
                Ok(())
            })
    }

    pub fn start_preflight(
        &self,
        request: PreflightRequest,
        options: WalkerOptions,
    ) -> AppResult<TaskStream<PreflightEvent>> {
        self.tasks
            .start_latest(JobKind::Preflight, move |context| async move {
                let event_context = context.clone();
                let emit: PreflightEventSink = Arc::new(move |event| {
                    let _ = event_context.emit(event);
                });
                tokio::task::spawn_blocking(move || execute_preflight(request, options, emit))
                    .await
                    .map_err(|error| {
                        crate::error::AppError::from_source(
                            crate::error::ErrorCode::TaskPanicked,
                            "preflight-task",
                            "preflight task failed",
                            error,
                        )
                    })??;
                Ok(())
            })
    }

    pub fn start_preview(&self, request: PreviewRequest) -> AppResult<TaskStream<PreviewEvent>> {
        self.tasks
            .start_latest(JobKind::Preview, move |context| async move {
                let event = tokio::task::spawn_blocking(move || execute_preview(request))
                    .await
                    .map_err(|error| {
                        crate::error::AppError::from_source(
                            crate::error::ErrorCode::TaskPanicked,
                            "preview-task",
                            "preview task failed",
                            error,
                        )
                    })??;
                let _ = context.emit(event);
                Ok(())
            })
    }
}
