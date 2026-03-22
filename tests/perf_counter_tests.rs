use codemerge::ui::perf;

/// All perf counter tests run in a single function to avoid global state
/// interference between parallel test threads. Run with:
///   cargo test --test perf_counter_tests
#[test]
fn perf_counter_correctness() {
    // --- Section 1: snapshot starts at zero after reset ---
    perf::reset();
    let snap = perf::snapshot();
    assert_eq!(snap.workspace_view_notifies, 0);
    assert_eq!(snap.preview_range_requests, 0);
    assert_eq!(snap.preview_visible_syncs, 0);
    assert_eq!(snap.preview_render_cache_rebuilds, 0);
    assert_eq!(snap.preview_render_cache_partial_updates, 0);
    assert_eq!(snap.preview_table_syncs, 0);
    assert_eq!(snap.tree_syncs, 0);
    assert_eq!(snap.tree_set_items, 0);

    // --- Section 2: each record function increments exactly its counter ---
    perf::reset();
    perf::record_workspace_view_notify();
    perf::record_workspace_view_notify();
    perf::record_preview_range_request();
    perf::record_preview_visible_sync();
    perf::record_preview_visible_sync();
    perf::record_preview_visible_sync();
    perf::record_preview_render_cache_rebuild();
    perf::record_preview_render_cache_partial_update();
    perf::record_preview_table_sync();
    perf::record_tree_sync();
    perf::record_tree_set_items();
    perf::record_tree_set_items();

    let snap = perf::snapshot();
    assert_eq!(snap.workspace_view_notifies, 2);
    assert_eq!(snap.preview_range_requests, 1);
    assert_eq!(snap.preview_visible_syncs, 3);
    assert_eq!(snap.preview_render_cache_rebuilds, 1);
    assert_eq!(snap.preview_render_cache_partial_updates, 1);
    assert_eq!(snap.preview_table_syncs, 1);
    assert_eq!(snap.tree_syncs, 1);
    assert_eq!(snap.tree_set_items, 2);

    // --- Section 3: reset clears all counters ---
    // (counters are non-zero from section 2)
    perf::reset();
    let snap = perf::snapshot();
    assert_eq!(snap, perf::PerfSnapshot::default());

    // --- Section 4: high-frequency increments are consistent ---
    perf::reset();
    let n = 10_000usize;
    for _ in 0..n {
        perf::record_workspace_view_notify();
    }
    let snap = perf::snapshot();
    assert_eq!(snap.workspace_view_notifies, n);
    assert_eq!(snap.preview_range_requests, 0);
    assert_eq!(snap.tree_syncs, 0);

    // --- Section 5: concurrent increments are consistent ---
    perf::reset();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                for _ in 0..1_000 {
                    perf::record_workspace_view_notify();
                    perf::record_preview_range_request();
                }
            })
        })
        .collect();
    for t in threads {
        t.join().expect("thread join");
    }
    let snap = perf::snapshot();
    assert_eq!(snap.workspace_view_notifies, 8_000);
    assert_eq!(snap.preview_range_requests, 8_000);
    // Other counters untouched
    assert_eq!(snap.preview_visible_syncs, 0);
    assert_eq!(snap.tree_syncs, 0);
}
