use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PerfSnapshot {
    pub workspace_view_notifies: usize,
    pub preview_range_requests: usize,
    pub preview_visible_syncs: usize,
    pub preview_render_cache_rebuilds: usize,
    pub preview_render_cache_partial_updates: usize,
    pub preview_table_syncs: usize,
    pub tree_syncs: usize,
    pub tree_set_items: usize,
}

static WORKSPACE_VIEW_NOTIFIES: AtomicUsize = AtomicUsize::new(0);
static PREVIEW_RANGE_REQUESTS: AtomicUsize = AtomicUsize::new(0);
static PREVIEW_VISIBLE_SYNCS: AtomicUsize = AtomicUsize::new(0);
static PREVIEW_RENDER_CACHE_REBUILDS: AtomicUsize = AtomicUsize::new(0);
static PREVIEW_RENDER_CACHE_PARTIAL_UPDATES: AtomicUsize = AtomicUsize::new(0);
static PREVIEW_TABLE_SYNCS: AtomicUsize = AtomicUsize::new(0);
static TREE_SYNCS: AtomicUsize = AtomicUsize::new(0);
static TREE_SET_ITEMS: AtomicUsize = AtomicUsize::new(0);

#[inline]
pub fn record_workspace_view_notify() {
    WORKSPACE_VIEW_NOTIFIES.fetch_add(1, Ordering::Relaxed);
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

pub fn snapshot() -> PerfSnapshot {
    PerfSnapshot {
        workspace_view_notifies: WORKSPACE_VIEW_NOTIFIES.load(Ordering::Relaxed),
        preview_range_requests: PREVIEW_RANGE_REQUESTS.load(Ordering::Relaxed),
        preview_visible_syncs: PREVIEW_VISIBLE_SYNCS.load(Ordering::Relaxed),
        preview_render_cache_rebuilds: PREVIEW_RENDER_CACHE_REBUILDS.load(Ordering::Relaxed),
        preview_render_cache_partial_updates: PREVIEW_RENDER_CACHE_PARTIAL_UPDATES
            .load(Ordering::Relaxed),
        preview_table_syncs: PREVIEW_TABLE_SYNCS.load(Ordering::Relaxed),
        tree_syncs: TREE_SYNCS.load(Ordering::Relaxed),
        tree_set_items: TREE_SET_ITEMS.load(Ordering::Relaxed),
    }
}

pub fn reset() {
    WORKSPACE_VIEW_NOTIFIES.store(0, Ordering::Relaxed);
    PREVIEW_RANGE_REQUESTS.store(0, Ordering::Relaxed);
    PREVIEW_VISIBLE_SYNCS.store(0, Ordering::Relaxed);
    PREVIEW_RENDER_CACHE_REBUILDS.store(0, Ordering::Relaxed);
    PREVIEW_RENDER_CACHE_PARTIAL_UPDATES.store(0, Ordering::Relaxed);
    PREVIEW_TABLE_SYNCS.store(0, Ordering::Relaxed);
    TREE_SYNCS.store(0, Ordering::Relaxed);
    TREE_SET_ITEMS.store(0, Ordering::Relaxed);
}
