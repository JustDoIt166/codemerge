use std::collections::HashMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::AbortHandle;
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult, ErrorCode};
use crate::services::runtime::RUNTIME;

pub type JobId = u64;

static ACTIVE_TASKS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_TASK_PEAK: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TaskMetrics {
    pub active: usize,
    pub peak: usize,
}

pub fn task_metrics() -> TaskMetrics {
    TaskMetrics {
        active: ACTIVE_TASKS.load(Ordering::Relaxed),
        peak: ACTIVE_TASK_PEAK.load(Ordering::Relaxed),
    }
}

pub(crate) fn reset_task_metrics() {
    ACTIVE_TASKS.store(0, Ordering::Relaxed);
    ACTIVE_TASK_PEAK.store(0, Ordering::Relaxed);
}

fn record_active_tasks(active: usize) {
    ACTIVE_TASKS.store(active, Ordering::Relaxed);
    ACTIVE_TASK_PEAK.fetch_max(active, Ordering::Relaxed);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobKind {
    Preflight,
    Process,
    Preview,
    Settings,
    Export,
    Cleanup,
}

#[derive(Debug)]
pub struct TaskEnvelope<E> {
    pub job_id: JobId,
    pub payload: TaskPayload<E>,
}

#[derive(Debug)]
pub enum TaskPayload<E> {
    Event(E),
    Finished,
    Failed(AppError),
}

pub struct TaskContext<E> {
    pub job_id: JobId,
    pub cancel: CancellationToken,
    sender: UnboundedSender<TaskEnvelope<E>>,
}

impl<E> Clone for TaskContext<E> {
    fn clone(&self) -> Self {
        Self {
            job_id: self.job_id,
            cancel: self.cancel.clone(),
            sender: self.sender.clone(),
        }
    }
}

impl<E> TaskContext<E> {
    pub fn emit(&self, event: E) -> bool {
        self.sender
            .send(TaskEnvelope {
                job_id: self.job_id,
                payload: TaskPayload::Event(event),
            })
            .is_ok()
    }
}

pub struct TaskStream<E> {
    pub job_id: JobId,
    pub kind: JobKind,
    pub cancel: CancellationToken,
    pub events: UnboundedReceiver<TaskEnvelope<E>>,
}

impl<E> TaskStream<E> {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

#[derive(Clone)]
struct ActiveTask {
    job_id: JobId,
    cancel: CancellationToken,
    abort: AbortHandle,
}

struct SupervisorInner {
    next_job_id: AtomicU64,
    active: Mutex<HashMap<JobKind, ActiveTask>>,
}

pub struct TaskSupervisor {
    inner: Arc<SupervisorInner>,
}

impl Default for TaskSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskSupervisor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(SupervisorInner {
                next_job_id: AtomicU64::new(1),
                active: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn start_latest<E, F, Fut>(&self, kind: JobKind, task: F) -> AppResult<TaskStream<E>>
    where
        E: Send + 'static,
        F: FnOnce(TaskContext<E>) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        let runtime = RUNTIME.as_ref().map_err(Clone::clone)?;
        self.cancel(kind);

        let job_id = self.inner.next_job_id.fetch_add(1, Ordering::Relaxed);
        let cancel = CancellationToken::new();
        let (sender, events) = mpsc::unbounded_channel();
        let task_sender = sender.clone();
        let task_cancel = cancel.clone();
        let inner = Arc::clone(&self.inner);
        let (start_sender, start_receiver) = tokio::sync::oneshot::channel();
        let join = runtime.spawn(async move {
            if start_receiver.await.is_err() {
                return;
            }
            let context = TaskContext {
                job_id,
                cancel: task_cancel,
                sender: task_sender,
            };
            let outcome = AssertUnwindSafe(task(context)).catch_unwind().await;
            let payload = match outcome {
                Ok(Ok(())) => TaskPayload::Finished,
                Ok(Err(error)) => TaskPayload::Failed(error),
                Err(_) => TaskPayload::Failed(AppError::with_code(
                    ErrorCode::TaskPanicked,
                    "background-task",
                    "background task panicked",
                )),
            };
            if let Ok(mut active) = inner.active.lock()
                && active.get(&kind).is_some_and(|task| task.job_id == job_id)
            {
                active.remove(&kind);
                record_active_tasks(active.len());
            }
            let _ = sender.send(TaskEnvelope { job_id, payload });
        });

        let active_task = ActiveTask {
            job_id,
            cancel: cancel.clone(),
            abort: join.abort_handle(),
        };
        let active = self.inner.active.lock().map_err(|_| {
            AppError::with_code(
                ErrorCode::TaskPanicked,
                "task-supervisor",
                "task registry lock poisoned",
            )
        })?;
        let mut active = active;
        active.insert(kind, active_task);
        record_active_tasks(active.len());
        drop(active);
        let _ = start_sender.send(());

        Ok(TaskStream {
            job_id,
            kind,
            cancel,
            events,
        })
    }

    pub fn cancel(&self, kind: JobKind) -> bool {
        let task = self
            .inner
            .active
            .lock()
            .ok()
            .and_then(|mut active| active.remove(&kind));
        if let Some(task) = task {
            task.cancel.cancel();
            task.abort.abort();
            record_active_tasks(self.active_count());
            true
        } else {
            false
        }
    }

    pub fn request_cancel(&self, kind: JobKind) -> bool {
        let task = self
            .inner
            .active
            .lock()
            .ok()
            .and_then(|active| active.get(&kind).cloned());
        if let Some(task) = task {
            task.cancel.cancel();
            true
        } else {
            false
        }
    }

    pub fn cancel_all(&self) {
        let tasks = self
            .inner
            .active
            .lock()
            .map(|mut active| active.drain().map(|(_, task)| task).collect::<Vec<_>>())
            .unwrap_or_default();
        for task in tasks {
            task.cancel.cancel();
            task.abort.abort();
        }
        record_active_tasks(self.active_count());
    }

    pub fn active_count(&self) -> usize {
        self.inner
            .active
            .lock()
            .map(|active| active.len())
            .unwrap_or_default()
    }
}

impl Drop for TaskSupervisor {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

#[cfg(test)]
mod tests {
    use super::{JobKind, TaskPayload, TaskSupervisor};
    use crate::error::{AppError, ErrorCode};

    #[test]
    fn latest_task_cancels_previous_task_of_same_kind() {
        let supervisor = TaskSupervisor::new();
        let first = supervisor
            .start_latest::<(), _, _>(JobKind::Preview, |context| async move {
                context.cancel.cancelled().await;
                Ok(())
            })
            .expect("first task");
        let second = supervisor
            .start_latest::<(), _, _>(JobKind::Preview, |_| async move { Ok(()) })
            .expect("second task");

        assert!(first.cancel.is_cancelled());
        assert_ne!(first.job_id, second.job_id);
    }

    #[test]
    fn task_failure_preserves_error_code() {
        let supervisor = TaskSupervisor::new();
        let mut stream = supervisor
            .start_latest::<(), _, _>(JobKind::Settings, |_| async move {
                Err(AppError::with_code(
                    ErrorCode::Config,
                    "settings",
                    "save failed",
                ))
            })
            .expect("task stream");
        let runtime = crate::services::runtime::RUNTIME
            .as_ref()
            .expect("runtime available");
        let event = runtime
            .block_on(stream.events.recv())
            .expect("terminal event");

        match event.payload {
            TaskPayload::Failed(error) => assert_eq!(error.code(), ErrorCode::Config),
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn successful_task_emits_finished_and_leaves_registry() {
        let supervisor = TaskSupervisor::new();
        let mut stream = supervisor
            .start_latest::<u8, _, _>(JobKind::Export, |context| async move {
                assert!(context.emit(7));
                Ok(())
            })
            .expect("task stream");
        let runtime = crate::services::runtime::RUNTIME
            .as_ref()
            .expect("runtime available");

        let payloads = runtime.block_on(async {
            let first = stream.events.recv().await.expect("event").payload;
            let second = stream.events.recv().await.expect("finished").payload;
            (first, second)
        });

        assert!(matches!(payloads.0, TaskPayload::Event(7)));
        assert!(matches!(payloads.1, TaskPayload::Finished));
        assert_eq!(supervisor.active_count(), 0);
    }

    #[test]
    fn panicking_task_is_reported_with_stable_code() {
        let supervisor = TaskSupervisor::new();
        let mut stream = supervisor
            .start_latest::<(), _, _>(JobKind::Cleanup, |_| async move {
                panic!("boom");
                #[allow(unreachable_code)]
                Ok(())
            })
            .expect("task stream");
        let runtime = crate::services::runtime::RUNTIME
            .as_ref()
            .expect("runtime available");
        let event = runtime.block_on(stream.events.recv()).expect("panic event");

        match event.payload {
            TaskPayload::Failed(error) => assert_eq!(error.code(), ErrorCode::TaskPanicked),
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn cancel_and_drop_cleanup_active_tasks() {
        let supervisor = TaskSupervisor::new();
        let stream = supervisor
            .start_latest::<(), _, _>(JobKind::Process, |context| async move {
                context.cancel.cancelled().await;
                Ok(())
            })
            .expect("task stream");
        assert_eq!(supervisor.active_count(), 1);
        assert!(supervisor.cancel(JobKind::Process));
        assert!(stream.cancel.is_cancelled());
        assert_eq!(supervisor.active_count(), 0);

        let dropped_stream = {
            let supervisor = TaskSupervisor::new();
            let stream = supervisor
                .start_latest::<(), _, _>(JobKind::Preview, |context| async move {
                    context.cancel.cancelled().await;
                    Ok(())
                })
                .expect("task stream");
            assert_eq!(supervisor.active_count(), 1);
            stream
        };
        assert!(dropped_stream.cancel.is_cancelled());
    }
}
