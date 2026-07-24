use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PerfSnapshot {
    pub store_dispatches: usize,
    pub progress_batches: usize,
    pub progress_events: usize,
    pub processing_queue_peak: usize,
    pub active_tasks: usize,
    pub active_task_peak: usize,
    pub workspace_view_notifies: usize,
    pub preview_range_requests: usize,
    pub preview_visible_syncs: usize,
    pub preview_render_cache_rebuilds: usize,
    pub preview_render_cache_partial_updates: usize,
    pub preview_table_syncs: usize,
    pub tree_syncs: usize,
    pub tree_set_items: usize,
    pub input_cache_rebuilds: usize,
    pub tree_filter_rebuilds: usize,
    pub copy_jobs: usize,
    pub copy_progress_batches: usize,
}

static STORE_DISPATCHES: AtomicUsize = AtomicUsize::new(0);
static PROGRESS_BATCHES: AtomicUsize = AtomicUsize::new(0);
static PROGRESS_EVENTS: AtomicUsize = AtomicUsize::new(0);
static PROCESSING_QUEUE_PEAK: AtomicUsize = AtomicUsize::new(0);
static WORKSPACE_VIEW_NOTIFIES: AtomicUsize = AtomicUsize::new(0);
static PREVIEW_RANGE_REQUESTS: AtomicUsize = AtomicUsize::new(0);
static PREVIEW_VISIBLE_SYNCS: AtomicUsize = AtomicUsize::new(0);
static PREVIEW_RENDER_CACHE_REBUILDS: AtomicUsize = AtomicUsize::new(0);
static PREVIEW_RENDER_CACHE_PARTIAL_UPDATES: AtomicUsize = AtomicUsize::new(0);
static PREVIEW_TABLE_SYNCS: AtomicUsize = AtomicUsize::new(0);
static TREE_SYNCS: AtomicUsize = AtomicUsize::new(0);
static TREE_SET_ITEMS: AtomicUsize = AtomicUsize::new(0);
static INPUT_CACHE_REBUILDS: AtomicUsize = AtomicUsize::new(0);
static TREE_FILTER_REBUILDS: AtomicUsize = AtomicUsize::new(0);
static COPY_JOBS: AtomicUsize = AtomicUsize::new(0);
static COPY_PROGRESS_BATCHES: AtomicUsize = AtomicUsize::new(0);

#[inline]
pub fn record_workspace_view_notify() {
    WORKSPACE_VIEW_NOTIFIES.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_store_dispatch() {
    STORE_DISPATCHES.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_progress_batch(events: usize) {
    PROGRESS_BATCHES.fetch_add(1, Ordering::Relaxed);
    PROGRESS_EVENTS.fetch_add(events, Ordering::Relaxed);
}

#[inline]
pub fn record_processing_queue_len(len: usize) {
    PROCESSING_QUEUE_PEAK.fetch_max(len, Ordering::Relaxed);
}

#[inline]
pub fn record_preview_range_request() {
    PREVIEW_RANGE_REQUESTS.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_preview_visible_sync() {
    PREVIEW_VISIBLE_SYNCS.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_preview_render_cache_rebuild() {
    PREVIEW_RENDER_CACHE_REBUILDS.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_preview_render_cache_partial_update() {
    PREVIEW_RENDER_CACHE_PARTIAL_UPDATES.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_preview_table_sync() {
    PREVIEW_TABLE_SYNCS.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_tree_sync() {
    TREE_SYNCS.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_tree_set_items() {
    TREE_SET_ITEMS.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_input_cache_rebuild() {
    INPUT_CACHE_REBUILDS.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_tree_filter_rebuild() {
    TREE_FILTER_REBUILDS.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_copy_job() {
    COPY_JOBS.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn record_copy_progress_batch() {
    COPY_PROGRESS_BATCHES.fetch_add(1, Ordering::Relaxed);
}

pub fn snapshot() -> PerfSnapshot {
    let task_metrics = crate::application::task::task_metrics();
    PerfSnapshot {
        store_dispatches: STORE_DISPATCHES.load(Ordering::Relaxed),
        progress_batches: PROGRESS_BATCHES.load(Ordering::Relaxed),
        progress_events: PROGRESS_EVENTS.load(Ordering::Relaxed),
        processing_queue_peak: PROCESSING_QUEUE_PEAK.load(Ordering::Relaxed),
        active_tasks: task_metrics.active,
        active_task_peak: task_metrics.peak,
        workspace_view_notifies: WORKSPACE_VIEW_NOTIFIES.load(Ordering::Relaxed),
        preview_range_requests: PREVIEW_RANGE_REQUESTS.load(Ordering::Relaxed),
        preview_visible_syncs: PREVIEW_VISIBLE_SYNCS.load(Ordering::Relaxed),
        preview_render_cache_rebuilds: PREVIEW_RENDER_CACHE_REBUILDS.load(Ordering::Relaxed),
        preview_render_cache_partial_updates: PREVIEW_RENDER_CACHE_PARTIAL_UPDATES
            .load(Ordering::Relaxed),
        preview_table_syncs: PREVIEW_TABLE_SYNCS.load(Ordering::Relaxed),
        tree_syncs: TREE_SYNCS.load(Ordering::Relaxed),
        tree_set_items: TREE_SET_ITEMS.load(Ordering::Relaxed),
        input_cache_rebuilds: INPUT_CACHE_REBUILDS.load(Ordering::Relaxed),
        tree_filter_rebuilds: TREE_FILTER_REBUILDS.load(Ordering::Relaxed),
        copy_jobs: COPY_JOBS.load(Ordering::Relaxed),
        copy_progress_batches: COPY_PROGRESS_BATCHES.load(Ordering::Relaxed),
    }
}

pub fn reset() {
    STORE_DISPATCHES.store(0, Ordering::Relaxed);
    PROGRESS_BATCHES.store(0, Ordering::Relaxed);
    PROGRESS_EVENTS.store(0, Ordering::Relaxed);
    PROCESSING_QUEUE_PEAK.store(0, Ordering::Relaxed);
    crate::application::task::reset_task_metrics();
    WORKSPACE_VIEW_NOTIFIES.store(0, Ordering::Relaxed);
    PREVIEW_RANGE_REQUESTS.store(0, Ordering::Relaxed);
    PREVIEW_VISIBLE_SYNCS.store(0, Ordering::Relaxed);
    PREVIEW_RENDER_CACHE_REBUILDS.store(0, Ordering::Relaxed);
    PREVIEW_RENDER_CACHE_PARTIAL_UPDATES.store(0, Ordering::Relaxed);
    PREVIEW_TABLE_SYNCS.store(0, Ordering::Relaxed);
    TREE_SYNCS.store(0, Ordering::Relaxed);
    TREE_SET_ITEMS.store(0, Ordering::Relaxed);
    INPUT_CACHE_REBUILDS.store(0, Ordering::Relaxed);
    TREE_FILTER_REBUILDS.store(0, Ordering::Relaxed);
    COPY_JOBS.store(0, Ordering::Relaxed);
    COPY_PROGRESS_BATCHES.store(0, Ordering::Relaxed);
}
