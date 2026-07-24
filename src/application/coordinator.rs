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
use crate::services::result_copy::{
    ResultCopyEvent, ResultCopyEventSink, ResultCopyRequest, execute as execute_result_copy,
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

    pub fn start_result_copy(
        &self,
        request: ResultCopyRequest,
    ) -> AppResult<TaskStream<ResultCopyEvent>> {
        self.tasks
            .start_latest(JobKind::Copy, move |context| async move {
                let event_context = context.clone();
                let emit: ResultCopyEventSink = Arc::new(move |event| {
                    let _ = event_context.emit(event);
                });
                execute_result_copy(request, context.cancel.clone(), emit).await
            })
    }

    pub fn start_maintenance_cleanup(&self) -> AppResult<TaskStream<usize>> {
        self.tasks
            .start_latest(JobKind::Maintenance, move |context| async move {
                let cleaned = tokio::task::spawn_blocking(move || {
                    crate::utils::temp_file::cleanup_stale_temp_entries(
                        std::time::Duration::from_secs(24 * 60 * 60),
                    )
                })
                .await
                .map_err(|error| {
                    crate::error::AppError::new(format!("maintenance task failed: {error}"))
                })??;
                let _ = context.emit(cleaned);
                Ok(())
            })
    }
}

#[cfg(test)]
mod tests {
    use crate::application::store::{
        ExecutionAction, WorkspaceAction, WorkspaceEffect, WorkspaceStore,
    };
    use crate::application::task::TaskPayload;
    use crate::domain::{
        AppConfigV1, Language, OutputFormat, ProcessingMode, ProcessingOptions,
        TemporaryWhitelistMode,
    };
    use crate::services::process::ProcessRequest;

    use super::WorkspaceCoordinator;

    #[test]
    fn process_task_events_flow_through_workspace_store() {
        let source = tempfile::tempdir().expect("source tempdir");
        let source_path = source.path().join("lib.rs");
        std::fs::write(&source_path, "pub fn merged() {}\n").expect("write source");
        let request = ProcessRequest {
            selected_folder: None,
            selected_files: vec![source_path],
            folder_blacklist: Vec::new(),
            ext_blacklist: Vec::new(),
            excluded_files: Vec::new(),
            folder_whitelist: Vec::new(),
            ext_whitelist: Vec::new(),
            whitelist_mode: TemporaryWhitelistMode::WhitelistThenBlacklist,
            options: ProcessingOptions {
                compress: false,
                use_gitignore: false,
                ignore_git: false,
                output_format: OutputFormat::Default,
                mode: ProcessingMode::Full,
            },
            language: Language::En,
        };
        let coordinator = WorkspaceCoordinator::new();
        let mut stream = coordinator.start_process(request).expect("start process");
        let job_id = stream.job_id;
        let mut store = WorkspaceStore::new(AppConfigV1::default(), "ready".into());
        let _ = store.dispatch(WorkspaceAction::Execution(ExecutionAction::StartRun {
            run_id: job_id,
            scanning_label: "scanning".into(),
        }));
        let initial_revision = store.revisions().execution;
        let runtime = crate::services::runtime::RUNTIME
            .as_ref()
            .expect("runtime available");
        let mut completion_requested_preflight = false;

        runtime.block_on(async {
            while let Some(envelope) = stream.events.recv().await {
                assert_eq!(envelope.job_id, job_id);
                match envelope.payload {
                    TaskPayload::Event(event) => {
                        let transition = store
                            .dispatch(WorkspaceAction::Execution(ExecutionAction::Process(event)));
                        completion_requested_preflight |= transition.effects.iter().any(|effect| {
                            matches!(
                                effect,
                                WorkspaceEffect::RestartPreflight {
                                    preserve_completed_status: true
                                }
                            )
                        });
                    }
                    TaskPayload::Finished => break,
                    TaskPayload::Failed(error) => panic!("process task failed: {error}"),
                }
            }
        });

        assert!(store.revisions().execution > initial_revision);
        assert!(completion_requested_preflight);
        let result = store.result().result.as_ref().expect("completed result");
        assert_eq!(result.stats.processed_files, 1);
        let process_dir = result.process_dir.as_ref().expect("process directory");
        assert!(process_dir.exists());
        crate::utils::temp_file::cleanup_temp_dir(process_dir).expect("cleanup process directory");
    }
}
