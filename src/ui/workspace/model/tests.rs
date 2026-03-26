use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{Decorations, SharedString};

use super::{
    CompactContentBodyViewModel, ContentPanelBodyViewModel, PreviewDocumentViewModel,
    PreviewPaneBodyViewModel, PreviewTableSort, ResultsCopyAction, ResultsPanelBodyViewModel,
    TreeCountSummary, TreeIconKind, TreeInteractionSnapshot, TreePaneBodyViewModel,
    TreePanelEffect, TreeRenderState, WindowChromeMode, WindowZoomAction, WorkspaceChromeTone,
    ancestor_node_ids, apply_preflight_event, apply_tree_interaction, build_blacklist_sections,
    build_compact_content_panel_view_model, build_content_panel_view_model,
    build_preview_pane_view_model, build_preview_table_model, build_results_panel_view_model,
    build_status_panel_view_model, build_tree_pane_view_model, build_tree_panel_data,
    build_tree_projection, build_tree_render_state, build_workspace_chrome_view_model,
    icon_kind_for_extension, preview_file_row, process_status_message, process_status_title,
    resolve_window_chrome_mode, resolve_window_zoom_action, summarize_archive_entries,
};
use crate::domain::{
    ArchiveEntrySource, Language, PreflightStats, PreviewFileEntry, ProcessRecord, ProcessResult,
    ProcessStatus, ResultTab, TreeNode,
};
use crate::processor::stats::ProcessingStats;
use crate::services::preflight::PreflightEvent;
use crate::ui::state::{
    DeferredPreviewState, NarrowContentTab, ProcessState, ProcessUiStatus, TreePanelState,
};
use crate::utils::app_metadata;
use crate::utils::i18n::tr;

#[test]
fn stale_preflight_event_does_not_override_current_state() {
    let mut process = ProcessState {
        preflight_revision: 2,
        ui_status: ProcessUiStatus::Idle,
        processing_current_file: "ready".to_string(),
        ..ProcessState::default()
    };

    apply_preflight_event(
        &mut process,
        PreflightEvent::Completed {
            revision: 1,
            stats: PreflightStats {
                total_files: 99,
                ..PreflightStats::default()
            },
        },
        false,
        "ready",
    );

    assert_eq!(process.preflight.total_files, 0);
    assert_eq!(process.ui_status, ProcessUiStatus::Idle);
    assert_eq!(process.processing_current_file, "ready");
}

#[test]
fn preview_table_keeps_selected_row_when_filter_matches() {
    let result = sample_archive_result();
    let model = build_preview_table_model(Some(&result), "lib", Some(2), PreviewTableSort::None);

    assert_eq!(model.rows.len(), 1);
    assert_eq!(model.selected_row_ix, Some(0));
    assert_eq!(model.next_selected_file_id, Some(2));
    assert_eq!(
        model.rows[0].archive,
        Some(ArchiveEntrySource {
            archive_path: "bundle.zip".to_string(),
            entry_path: "src/lib.rs".to_string(),
        })
    );
}

#[test]
fn preview_table_selects_first_row_when_current_selection_disappears() {
    let result = ProcessResult {
        stats: ProcessingStats::default(),
        tree_string: String::new(),
        tree_nodes: Vec::new(),
        merged_content_path: None,
        suggested_result_name: "workspace-20260319.txt".to_string(),
        file_details: Vec::new(),
        preview_files: vec![PreviewFileEntry {
            id: 7,
            display_path: "src/lib.rs".to_string(),
            chars: 12,
            tokens: 4,
            preview_blob_path: PathBuf::from("b"),
            byte_len: 12,
            archive: None,
        }],
        preview_blob_dir: None,
    };

    let model = build_preview_table_model(Some(&result), "lib", Some(2), PreviewTableSort::None);

    assert_eq!(model.selected_row_ix, None);
    assert_eq!(model.next_selected_file_id, Some(7));
}

#[test]
fn preview_table_sort_toggle_switches_between_desc_and_asc() {
    assert_eq!(
        PreviewTableSort::None.toggle_chars(),
        PreviewTableSort::CharsDesc
    );
    assert_eq!(
        PreviewTableSort::CharsDesc.toggle_chars(),
        PreviewTableSort::CharsAsc
    );
    assert_eq!(
        PreviewTableSort::CharsAsc.toggle_chars(),
        PreviewTableSort::CharsDesc
    );
}

#[test]
fn preview_table_sorts_by_chars_descending_and_keeps_selection() {
    let result = sample_sort_result();

    let model = build_preview_table_model(Some(&result), "", Some(3), PreviewTableSort::CharsDesc);

    assert_eq!(
        model.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![2, 3, 1]
    );
    assert_eq!(model.selected_row_ix, Some(1));
    assert_eq!(model.next_selected_file_id, Some(3));
}

#[test]
fn preview_table_sorts_by_chars_ascending_and_falls_back_to_first_row() {
    let result = sample_sort_result();

    let model = build_preview_table_model(Some(&result), "", Some(99), PreviewTableSort::CharsAsc);

    assert_eq!(
        model.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![1, 3, 2]
    );
    assert_eq!(model.selected_row_ix, None);
    assert_eq!(model.next_selected_file_id, Some(1));
}

#[test]
fn preview_file_row_uses_full_result_metadata() {
    let result = sample_archive_result();

    let row = preview_file_row(Some(&result), 2).expect("preview row");

    assert_eq!(row.display_path, "bundle.zip/src/lib.rs");
    assert_eq!(
        row.archive,
        Some(ArchiveEntrySource {
            archive_path: "bundle.zip".to_string(),
            entry_path: "src/lib.rs".to_string(),
        })
    );
}

#[test]
fn ancestor_node_ids_collect_parent_chain() {
    assert_eq!(
        ancestor_node_ids("bundle.zip/src/lib.rs"),
        BTreeSet::from(["bundle.zip".to_string(), "bundle.zip/src".to_string()])
    );
    assert!(ancestor_node_ids("README.md").is_empty());
}

#[test]
fn blacklist_sections_keep_group_order_and_item_order_without_filter() {
    let sections = build_blacklist_sections(
        &["target".into(), "dist".into()],
        &[".log".into(), ".tmp".into()],
        "",
        Language::En,
    );

    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].title.as_ref(), "Folders");
    assert_eq!(
        sections[0]
            .items
            .iter()
            .map(|item| item.value.as_str())
            .collect::<Vec<_>>(),
        vec!["target", "dist"]
    );
    assert_eq!(sections[1].title.as_ref(), "Extensions");
    assert_eq!(
        sections[1]
            .items
            .iter()
            .map(|item| item.value.as_str())
            .collect::<Vec<_>>(),
        vec![".log", ".tmp"]
    );
}

#[test]
fn blacklist_sections_filter_case_insensitively() {
    let sections = build_blacklist_sections(
        &["Node_Modules".into(), "target".into()],
        &[".PNG".into(), ".tmp".into()],
        "png",
        Language::En,
    );

    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].title.as_ref(), "Extensions");
    assert_eq!(sections[0].count, 1);
    assert_eq!(sections[0].items[0].value, ".PNG");
}

#[test]
fn blacklist_sections_return_empty_when_filter_matches_nothing() {
    let sections = build_blacklist_sections(
        &["target".into()],
        &[".tmp".into()],
        "missing",
        Language::En,
    );

    assert!(sections.is_empty());
}

#[test]
fn tree_panel_projection_links_preview_file_and_icons() {
    let result = sample_archive_result();
    let data = build_tree_panel_data(Some(&result));
    let expanded = data
        .as_ref()
        .map(|data| data.index.default_expanded_ids.clone())
        .unwrap_or_default();

    let projection = build_tree_projection(data.as_ref(), "");
    let render =
        build_tree_render_state(&projection, false, &expanded, Some("bundle.zip/src/lib.rs"));

    let lib = render
        .rows
        .iter()
        .find(|row| row.node_id.as_ref() == "bundle.zip/src/lib.rs")
        .expect("lib row");
    assert_eq!(lib.preview_file_id, Some(2));
    assert_eq!(lib.preview_chars, Some(12));
    assert_eq!(lib.preview_tokens, Some(4));
    assert_eq!(
        lib.archive,
        Some(ArchiveEntrySource {
            archive_path: "bundle.zip".to_string(),
            entry_path: "src/lib.rs".to_string(),
        })
    );
    assert_eq!(lib.icon_kind, TreeIconKind::Rust);
    assert_eq!(render.selected_row_ix, Some(2));
}

#[test]
fn archive_summary_counts_distinct_archives_and_entries() {
    let summary = summarize_archive_entries(Some(&sample_archive_result()));

    assert_eq!(summary.archives, 1);
    assert_eq!(summary.entries, 1);
}

#[test]
fn results_panel_view_model_normalizes_tab_and_copy_action() {
    let tree_vm = build_results_panel_view_model(ResultTab::Tree, true, Language::En);
    assert_eq!(tree_vm.selected_tab, 0);
    assert!(tree_vm.has_content_result);
    assert_eq!(tree_vm.copy_label.as_ref(), tr(Language::En, "copy_tree"));
    assert_eq!(tree_vm.copy_action, ResultsCopyAction::Tree);
    assert_eq!(tree_vm.body, ResultsPanelBodyViewModel::Tree);

    let content_vm = build_results_panel_view_model(ResultTab::Content, true, Language::En);
    assert_eq!(content_vm.selected_tab, 1);
    assert_eq!(
        content_vm.copy_label.as_ref(),
        tr(Language::En, "copy_current_page")
    );
    assert_eq!(content_vm.copy_action, ResultsCopyAction::Preview);
    assert_eq!(content_vm.body, ResultsPanelBodyViewModel::Content);

    let fallback_vm = build_results_panel_view_model(ResultTab::Content, false, Language::En);
    assert_eq!(fallback_vm.selected_tab, 0);
    assert!(!fallback_vm.has_content_result);
    assert_eq!(fallback_vm.copy_action, ResultsCopyAction::Tree);
    assert_eq!(fallback_vm.body, ResultsPanelBodyViewModel::Tree);
}

#[test]
fn content_panel_view_model_builds_tree_only_and_empty_states() {
    let tree_only_vm = build_content_panel_view_model(true, 0, false, false, Language::En);
    assert_eq!(
        tree_only_vm.body,
        ContentPanelBodyViewModel::TreeOnly(super::EmptyStateViewModel {
            title: SharedString::from(tr(Language::En, "mode_tree_only")),
            hint: SharedString::from(tr(Language::En, "mode_tree_only_desc")),
        })
    );

    let no_match_vm = build_content_panel_view_model(false, 0, true, false, Language::En);
    match no_match_vm.body {
        ContentPanelBodyViewModel::Split(content) => {
            assert!(!content.file_list.file_list_collapsed);
            assert_eq!(
                content.file_list.toggle_label.as_ref(),
                tr(Language::En, "content_files_collapse")
            );
            let empty = content.file_list.empty_state.expect("empty state");
            assert_eq!(empty.title.as_ref(), tr(Language::En, "content_no_match"));
            assert_eq!(
                empty.hint.as_ref(),
                tr(Language::En, "content_no_match_hint")
            );
        }
        body => panic!("expected split content body, got {body:?}"),
    }

    let empty_vm = build_content_panel_view_model(false, 0, false, true, Language::En);
    match empty_vm.body {
        ContentPanelBodyViewModel::Split(content) => {
            assert!(content.file_list.file_list_collapsed);
            assert_eq!(
                content.file_list.toggle_label.as_ref(),
                tr(Language::En, "content_files_expand")
            );
            let empty = content.file_list.empty_state.expect("empty state");
            assert_eq!(empty.title.as_ref(), tr(Language::En, "content_empty"));
            assert_eq!(empty.hint.as_ref(), tr(Language::En, "content_empty_hint"));
        }
        body => panic!("expected split content body, got {body:?}"),
    }
}

#[test]
fn content_and_compact_view_models_preserve_visible_rows_and_selected_tab() {
    let content_vm = build_content_panel_view_model(false, 12, false, false, Language::En);
    match content_vm.body {
        ContentPanelBodyViewModel::Split(content) => {
            assert_eq!(content.file_list.visible_row_count, 12);
            assert!(content.file_list.empty_state.is_none());
        }
        body => panic!("expected split content body, got {body:?}"),
    }

    let status_vm = build_compact_content_panel_view_model(NarrowContentTab::Status);
    assert_eq!(status_vm.selected_tab, 0);
    assert_eq!(status_vm.body, CompactContentBodyViewModel::Status);

    let results_vm = build_compact_content_panel_view_model(NarrowContentTab::Results);
    assert_eq!(results_vm.selected_tab, 1);
    assert_eq!(results_vm.body, CompactContentBodyViewModel::Results);
}

#[test]
fn status_panel_view_model_derives_metrics_progress_and_recent_activity() {
    let mut process = ProcessState {
        ui_status: ProcessUiStatus::Running,
        preflight: PreflightStats {
            total_files: 20,
            skipped_files: 6,
            to_process_files: 14,
            ..PreflightStats::default()
        },
        processing_candidates: 20,
        processing_current_file: "src/current.rs".to_string(),
        processing_started_at: Some(Instant::now() - Duration::from_secs(125)),
        ..ProcessState::default()
    };
    process.processing_records = (0..18)
        .map(|ix| ProcessRecord {
            file_name: format!("file-{ix}.rs"),
            status: if ix == 4 {
                ProcessStatus::Failed
            } else {
                ProcessStatus::Success
            },
            chars: Some(ix + 1),
            tokens: Some(ix + 2),
            error: (ix == 4).then(|| "boom".to_string()),
        })
        .collect();

    let mut result = sample_archive_result();
    result.stats = ProcessingStats {
        total_chars: 123,
        total_tokens: 45,
        ..ProcessingStats::default()
    };

    let vm = build_status_panel_view_model(&process, Some(&result), Language::En, None);

    assert_eq!(vm.summary_metrics[0].value.as_ref(), "20");
    assert_eq!(vm.summary_metrics[1].value.as_ref(), "14");
    assert_eq!(vm.summary_metrics[2].value.as_ref(), "6");
    assert_eq!(vm.result_metrics[0].value.as_ref(), "123");
    assert_eq!(vm.result_metrics[1].value.as_ref(), "45");
    assert_eq!(vm.result_metrics[2].value.as_ref(), "1");
    assert_eq!(vm.status_title.as_ref(), tr(Language::En, "status_running"));
    assert_eq!(vm.status_message.as_ref(), "src/current.rs");
    assert_eq!(vm.progress.value_text.as_ref(), "18/20");
    assert!((vm.progress.fill_ratio - 0.9).abs() < 0.0001);
    assert_eq!(vm.progress.current_file.as_ref(), "src/current.rs");
    assert_ne!(vm.progress.elapsed_value.as_ref(), "--:--");
    assert_eq!(vm.activity_rows.len(), 16);
    assert_eq!(vm.activity_rows[0].file_name, "file-17.rs");
    assert_eq!(vm.activity_rows[15].file_name, "file-2.rs");
    assert_eq!(
        vm.archive_summary
            .as_ref()
            .expect("archive summary")
            .value
            .as_ref(),
        format!(
            "1 {} · 1 {}",
            tr(Language::En, "archive_files"),
            tr(Language::En, "archive_entries")
        )
    );
}

#[test]
fn status_panel_view_model_handles_missing_result_and_idle_progress() {
    let process = ProcessState {
        processing_current_file: "ready".to_string(),
        ..ProcessState::default()
    };

    let vm = build_status_panel_view_model(&process, None, Language::En, Some("1.2 MB".into()));

    assert_eq!(vm.result_metrics[0].value.as_ref(), "--");
    assert_eq!(vm.result_metrics[1].value.as_ref(), "--");
    assert_eq!(vm.result_metrics[2].value.as_ref(), "0");
    assert!(vm.archive_summary.is_none());
    assert_eq!(vm.progress.value_text.as_ref(), "0/1");
    assert_eq!(vm.progress.elapsed_value.as_ref(), "--:--");
    assert_eq!(vm.status_title.as_ref(), tr(Language::En, "status_idle"));
    assert_eq!(
        vm.status_message.as_ref(),
        tr(Language::En, "status_idle_hint")
    );
}

#[test]
fn tree_pane_view_model_uses_tree_body_when_rows_are_visible() {
    let result = sample_result();
    let data = build_tree_panel_data(Some(&result));
    let expanded = data
        .as_ref()
        .map(|data| data.index.default_expanded_ids.clone())
        .unwrap_or_default();
    let projection = build_tree_projection(data.as_ref(), "");
    let render = build_tree_render_state(&projection, false, &expanded, None);

    let vm = build_tree_pane_view_model(
        &render,
        projection.total_summary,
        "",
        Some(&result),
        Language::En,
        false,
    );

    assert_eq!(vm.body, TreePaneBodyViewModel::Tree);
    assert!(!vm.filter_active);
    assert!(!vm.disable_structure_actions);
    assert_eq!(
        vm.view_mode_label.as_ref(),
        tr(Language::En, "tree_view_text")
    );
    assert_eq!(vm.visible_summary, render.visible_summary);
    assert_eq!(vm.total_summary, projection.total_summary);
}

#[test]
fn tree_pane_view_model_uses_plain_text_lines_and_no_match_empty_state() {
    let mut result = sample_result();
    result.tree_string = "src/\n  lib.rs\r\n".to_string();

    let vm = build_tree_pane_view_model(
        &TreeRenderState::default(),
        TreeCountSummary::default(),
        "",
        Some(&result),
        Language::En,
        true,
    );

    assert!(vm.disable_structure_actions);
    assert_eq!(
        vm.view_mode_label.as_ref(),
        tr(Language::En, "tree_view_tree")
    );
    match &vm.body {
        TreePaneBodyViewModel::PlainText { lines } => {
            assert_eq!(lines.len(), 3);
            assert_eq!(lines[0].as_ref(), "src/");
            assert_eq!(lines[1].as_ref(), "\u{00A0}\u{00A0}lib.rs");
            assert_eq!(lines[2].as_ref(), "");
        }
        body => panic!("expected plain text body, got {body:?}"),
    }

    let no_match = build_tree_pane_view_model(
        &TreeRenderState::default(),
        TreeCountSummary::default(),
        "lib",
        Some(&result),
        Language::En,
        false,
    );

    assert!(no_match.filter_active);
    assert!(no_match.disable_structure_actions);
    assert_eq!(
        no_match.body,
        TreePaneBodyViewModel::Empty {
            title: SharedString::from(tr(Language::En, "tree_no_match")),
            hint: SharedString::from(tr(Language::En, "tree_no_match_hint")),
        }
    );
}

#[test]
fn preview_pane_view_model_prefers_deferred_merged_state_before_error() {
    let deferred = DeferredPreviewState {
        source_path: PathBuf::from("merged.txt"),
        source_byte_len: 2_048,
        excerpt_byte_len: 1_024,
        excerpt_path: None,
    };

    let vm = build_preview_pane_view_model(
        Some(&sample_result()),
        Some(super::super::MERGED_CONTENT_PREVIEW_FILE_ID),
        false,
        Some("deferred failed"),
        Some(&deferred),
        None,
        Language::En,
    );

    match vm.body {
        PreviewPaneBodyViewModel::DeferredMerged(deferred_vm) => {
            assert_eq!(
                deferred_vm.title.as_ref(),
                tr(Language::En, "tab_merged_content")
            );
            assert_eq!(deferred_vm.detail.as_ref(), "deferred failed");
            assert_eq!(
                deferred_vm.source_byte_size.as_ref(),
                super::super::view::format_size(2_048)
            );
            assert_eq!(
                deferred_vm.excerpt_byte_size.as_ref(),
                super::super::view::format_size(1_024)
            );
        }
        body => panic!("expected deferred merged body, got {body:?}"),
    }
}

#[test]
fn preview_pane_view_model_builds_error_and_placeholder_states() {
    let error_vm = build_preview_pane_view_model(
        Some(&sample_archive_result()),
        Some(2),
        false,
        Some("boom"),
        None,
        None,
        Language::En,
    );
    assert_eq!(
        error_vm.body,
        PreviewPaneBodyViewModel::Error {
            title: SharedString::from(format!(
                "{}: bundle.zip/src/lib.rs",
                tr(Language::En, "status_error")
            )),
            detail: SharedString::from("boom"),
        }
    );

    let loading_vm = build_preview_pane_view_model(
        Some(&sample_result()),
        Some(1),
        true,
        None,
        None,
        None,
        Language::En,
    );
    assert_eq!(
        loading_vm.body,
        PreviewPaneBodyViewModel::Placeholder {
            title: SharedString::from(tr(Language::En, "preview_loading")),
            detail: SharedString::from("src/main.rs"),
        }
    );

    let empty_vm = build_preview_pane_view_model(
        Some(&sample_result()),
        None,
        false,
        None,
        None,
        None,
        Language::En,
    );
    assert_eq!(
        empty_vm.body,
        PreviewPaneBodyViewModel::Placeholder {
            title: SharedString::from(tr(Language::En, "preview_empty")),
            detail: SharedString::from(tr(Language::En, "preview_empty_hint")),
        }
    );
}

#[test]
fn preview_pane_view_model_builds_content_for_archive_and_merged_preview() {
    let archive_vm = build_preview_pane_view_model(
        Some(&sample_archive_result()),
        Some(2),
        false,
        None,
        None,
        Some(PreviewDocumentViewModel {
            line_count: 12,
            byte_len: 345,
            document_path: "tmp/preview/lib.rs".to_string(),
        }),
        Language::En,
    );

    match archive_vm.body {
        PreviewPaneBodyViewModel::Content(content) => {
            assert_eq!(content.file_path, "bundle.zip/src/lib.rs");
            assert_eq!(
                content.archive_paths,
                Some(("bundle.zip".to_string(), "src/lib.rs".to_string()))
            );
            assert_eq!(content.line_count, 12);
            assert_eq!(content.byte_len, 345);
            assert!(content.excerpt_banner.is_none());
        }
        body => panic!("expected content body, got {body:?}"),
    }

    let deferred = DeferredPreviewState {
        source_path: PathBuf::from("merged.txt"),
        source_byte_len: 4_096,
        excerpt_byte_len: 1_024,
        excerpt_path: Some(PathBuf::from("merged_excerpt.txt")),
    };
    let merged_vm = build_preview_pane_view_model(
        Some(&sample_result()),
        Some(super::super::MERGED_CONTENT_PREVIEW_FILE_ID),
        false,
        None,
        Some(&deferred),
        Some(PreviewDocumentViewModel {
            line_count: 24,
            byte_len: 512,
            document_path: "tmp/merged.txt".to_string(),
        }),
        Language::En,
    );

    match merged_vm.body {
        PreviewPaneBodyViewModel::Content(content) => {
            assert_eq!(content.file_path, tr(Language::En, "tab_merged_content"));
            assert!(content.archive_paths.is_none());
            assert_eq!(content.line_count, 24);
            assert_eq!(content.byte_len, 512);
            let banner = content.excerpt_banner.expect("excerpt banner");
            assert_eq!(banner.title.as_ref(), tr(Language::En, "preview_loaded"));
            assert_eq!(
                banner.detail.as_ref(),
                format!(
                    "{} {}",
                    tr(Language::En, "large_preview_excerpt_hint"),
                    super::super::view::format_size(4_096)
                )
            );
        }
        body => panic!("expected content body, got {body:?}"),
    }
}

#[test]
fn tree_panel_filter_keeps_ancestor_context_and_counts_visible_nodes() {
    let result = sample_result();
    let data = build_tree_panel_data(Some(&result));
    let expanded = data
        .as_ref()
        .map(|data| data.index.default_expanded_ids.clone())
        .unwrap_or_default();

    let projection = build_tree_projection(data.as_ref(), "lib");
    let render = build_tree_render_state(&projection, true, &expanded, None);

    assert_eq!(
        render.visible_summary,
        TreeCountSummary {
            folders: 1,
            files: 1,
        }
    );
    assert_eq!(render.rows.len(), 2);
    assert_eq!(render.rows[0].node_id.as_ref(), "src");
    assert_eq!(render.rows[1].node_id.as_ref(), "src/lib.rs");
    assert_eq!(render.rows[1].match_range, Some(0..3));
}

#[test]
fn tree_structure_signature_only_changes_when_visible_structure_changes() {
    let result = sample_result();
    let data = build_tree_panel_data(Some(&result));
    let expanded = data
        .as_ref()
        .map(|data| data.index.default_expanded_ids.clone())
        .unwrap_or_default();

    let projection = build_tree_projection(data.as_ref(), "");
    let render_a = build_tree_render_state(&projection, false, &expanded, Some("src"));
    let render_b = build_tree_render_state(&projection, false, &expanded, Some("src/lib.rs"));

    assert_eq!(render_a.structure_signature, render_b.structure_signature);
    assert_ne!(render_a.selected_row_ix, render_b.selected_row_ix);
}

#[test]
fn window_chrome_mode_matches_platform_and_decorations() {
    assert_eq!(
        resolve_window_chrome_mode(Decorations::Server, true, false, false),
        WindowChromeMode::CustomTitleBar
    );
    assert_eq!(
        resolve_window_chrome_mode(
            Decorations::Client {
                tiling: Default::default(),
            },
            false,
            true,
            false,
        ),
        WindowChromeMode::CustomTitleBar
    );
    assert_eq!(
        resolve_window_chrome_mode(Decorations::Server, false, true, false),
        WindowChromeMode::CompactHeaderFallback
    );
    assert_eq!(
        resolve_window_chrome_mode(Decorations::Server, false, false, true),
        WindowChromeMode::CustomTitleBar
    );
}

#[test]
fn window_zoom_action_uses_restore_for_fullscreen_and_maximized() {
    assert_eq!(
        resolve_window_zoom_action(false, false),
        WindowZoomAction::Maximize
    );
    assert_eq!(
        resolve_window_zoom_action(true, false),
        WindowZoomAction::Restore
    );
    assert_eq!(
        resolve_window_zoom_action(false, true),
        WindowZoomAction::Restore
    );
}

#[test]
fn chrome_view_model_maps_all_statuses_to_labels_and_tones() {
    let cases = [
        (ProcessUiStatus::Idle, WorkspaceChromeTone::Neutral),
        (ProcessUiStatus::Preflight, WorkspaceChromeTone::Accent),
        (ProcessUiStatus::Running, WorkspaceChromeTone::Accent),
        (ProcessUiStatus::Completed, WorkspaceChromeTone::Success),
        (ProcessUiStatus::Cancelled, WorkspaceChromeTone::Warning),
        (ProcessUiStatus::Error, WorkspaceChromeTone::Danger),
    ];

    for (status, expected_tone) in cases {
        let mut process = ProcessState {
            ui_status: status,
            processing_current_file: "current".to_string(),
            ..ProcessState::default()
        };
        if status == ProcessUiStatus::Preflight {
            process.preflight.scanned_entries = 42;
        }
        if status == ProcessUiStatus::Error {
            process.last_error = Some("boom".to_string());
        }

        let vm = build_workspace_chrome_view_model(&process, Language::En, None);

        assert_eq!(
            vm.status_label.as_ref(),
            process_status_title(status, Language::En)
        );
        assert_eq!(
            vm.status_message.as_ref(),
            process_status_message(&process, Language::En, None)
        );
        assert_eq!(vm.status_tone, expected_tone);
    }
}

#[test]
fn chrome_view_model_localizes_language_button_and_completed_message() {
    let process = ProcessState {
        ui_status: ProcessUiStatus::Completed,
        ..ProcessState::default()
    };

    let zh = build_workspace_chrome_view_model(&process, Language::Zh, Some("1.2 MB".into()));
    let en = build_workspace_chrome_view_model(&process, Language::En, Some("1.2 MB".into()));

    assert_eq!(zh.title.as_ref(), "CodeMerge");
    assert_eq!(zh.version_label.as_ref(), app_metadata::version_label());
    assert_eq!(zh.language_button_label.as_ref(), "EN");
    assert_eq!(en.language_button_label.as_ref(), "中文");
    assert_eq!(
        zh.repository_tooltip.as_ref(),
        format!(
            "{}{}",
            tr(Language::Zh, "repository_tooltip"),
            app_metadata::repository_url()
        )
    );
    assert_eq!(
        en.repository_tooltip.as_ref(),
        format!(
            "{}{}",
            tr(Language::En, "repository_tooltip"),
            app_metadata::repository_url()
        )
    );
    assert_eq!(
        zh.status_label.as_ref(),
        tr(Language::Zh, "status_completed")
    );
    assert_eq!(
        zh.status_message.as_ref(),
        format!("{} (1.2 MB)", tr(Language::Zh, "status_completed_hint"))
    );
    assert_eq!(
        en.status_message.as_ref(),
        format!("{} (1.2 MB)", tr(Language::En, "status_completed_hint"))
    );
}

#[test]
fn folder_summaries_include_nested_children() {
    let result = nested_result();
    let data = build_tree_panel_data(Some(&result));
    let expanded = data
        .as_ref()
        .map(|data| data.index.default_expanded_ids.clone())
        .unwrap_or_default();

    let projection = build_tree_projection(data.as_ref(), "");
    let render = build_tree_render_state(&projection, false, &expanded, None);
    let src = render
        .rows
        .iter()
        .find(|row| row.node_id.as_ref() == "src")
        .expect("src row");
    let nested = render
        .rows
        .iter()
        .find(|row| row.node_id.as_ref() == "src/nested")
        .expect("nested row");
    let nested_file = render
        .rows
        .iter()
        .find(|row| row.node_id.as_ref() == "src/nested/lib.rs")
        .expect("nested file row");
    let main = render
        .rows
        .iter()
        .find(|row| row.node_id.as_ref() == "src/main.rs")
        .expect("main row");

    assert_eq!(src.child_folder_count, 1);
    assert_eq!(src.child_file_count, 2);
    assert_eq!(nested.depth, 1);
    assert_eq!(nested_file.depth, 2);
    assert_eq!(nested_file.preview_chars, Some(12));
    assert_eq!(main.preview_tokens, Some(3));
}

#[test]
fn collapsed_folders_do_not_emit_hidden_descendant_rows() {
    let data = build_tree_panel_data(Some(&nested_result()));
    let expanded = BTreeSet::from(["src".to_string()]);

    let projection = build_tree_projection(data.as_ref(), "");
    let render = build_tree_render_state(&projection, false, &expanded, None);

    assert_eq!(
        render
            .rows
            .iter()
            .map(|row| (row.node_id.as_ref(), row.depth))
            .collect::<Vec<_>>(),
        vec![
            ("src", 0),
            ("src/nested", 1),
            ("src/main.rs", 1),
            ("README.md", 0),
        ]
    );
    assert_eq!(
        render.visible_summary,
        TreeCountSummary {
            folders: 2,
            files: 2,
        }
    );
    assert_eq!(render.rows[1].depth, 1);
    assert_eq!(render.rows[2].depth, 1);
}

#[test]
fn tree_interaction_folder_selection_refreshes_once_and_updates_expansion() {
    let mut state = TreePanelState::default();
    let effect = apply_tree_interaction(
        &mut state,
        None,
        Some(TreeInteractionSnapshot {
            node_id: Some("src".to_string()),
            is_folder: true,
            is_expanded: true,
            preview_file_id: None,
        }),
    );

    assert_eq!(effect, TreePanelEffect::RefreshVisibleTree);
    assert_eq!(state.selected_node_id.as_deref(), Some("src"));
    assert!(state.expanded_ids.contains("src"));

    let same = apply_tree_interaction(
        &mut state,
        Some(&TreeInteractionSnapshot {
            node_id: Some("src".to_string()),
            is_folder: true,
            is_expanded: true,
            preview_file_id: None,
        }),
        Some(TreeInteractionSnapshot {
            node_id: Some("src".to_string()),
            is_folder: true,
            is_expanded: true,
            preview_file_id: None,
        }),
    );

    assert_eq!(same, TreePanelEffect::None);
}

#[test]
fn tree_interaction_folder_expansion_change_refreshes_visible_rows() {
    let mut state = TreePanelState {
        selected_node_id: Some("src".to_string()),
        expanded_ids: BTreeSet::from(["src".to_string()]),
    };
    let effect = apply_tree_interaction(
        &mut state,
        Some(&TreeInteractionSnapshot {
            node_id: Some("src".to_string()),
            is_folder: true,
            is_expanded: true,
            preview_file_id: None,
        }),
        Some(TreeInteractionSnapshot {
            node_id: Some("src".to_string()),
            is_folder: true,
            is_expanded: false,
            preview_file_id: None,
        }),
    );

    assert_eq!(effect, TreePanelEffect::RefreshVisibleTree);
    assert!(!state.expanded_ids.contains("src"));
}

#[test]
fn tree_interaction_file_selection_switches_to_preview_once() {
    let mut state = TreePanelState::default();
    let effect = apply_tree_interaction(
        &mut state,
        None,
        Some(TreeInteractionSnapshot {
            node_id: Some("src/lib.rs".to_string()),
            is_folder: false,
            is_expanded: false,
            preview_file_id: Some(2),
        }),
    );

    assert_eq!(effect, TreePanelEffect::SwitchToContentAndOpen(2));
    assert_eq!(state.selected_node_id.as_deref(), Some("src/lib.rs"));
}

#[test]
fn tree_icon_mapping_promotes_common_file_types_to_dedicated_kinds() {
    assert_eq!(
        icon_kind_for_extension(Some("rs".into())),
        TreeIconKind::Rust
    );
    assert_eq!(
        icon_kind_for_extension(Some("toml".into())),
        TreeIconKind::Toml
    );
    assert_eq!(
        icon_kind_for_extension(Some("json".into())),
        TreeIconKind::Json
    );
    assert_eq!(
        icon_kind_for_extension(Some("md".into())),
        TreeIconKind::Markdown
    );
    assert_eq!(
        icon_kind_for_extension(Some("ts".into())),
        TreeIconKind::Code
    );
    assert_eq!(
        icon_kind_for_extension(Some("txt".into())),
        TreeIconKind::Document
    );
}

fn sample_result() -> ProcessResult {
    ProcessResult {
        stats: ProcessingStats::default(),
        tree_string: String::new(),
        tree_nodes: vec![
            TreeNode {
                id: "src".to_string(),
                label: "src".to_string(),
                relative_path: "src".to_string(),
                is_folder: true,
                children: vec![
                    TreeNode {
                        id: "src/main.rs".to_string(),
                        label: "main.rs".to_string(),
                        relative_path: "src/main.rs".to_string(),
                        is_folder: false,
                        children: Vec::new(),
                    },
                    TreeNode {
                        id: "src/lib.rs".to_string(),
                        label: "lib.rs".to_string(),
                        relative_path: "src/lib.rs".to_string(),
                        is_folder: false,
                        children: Vec::new(),
                    },
                ],
            },
            TreeNode {
                id: "README.md".to_string(),
                label: "README.md".to_string(),
                relative_path: "README.md".to_string(),
                is_folder: false,
                children: Vec::new(),
            },
        ],
        merged_content_path: None,
        suggested_result_name: "workspace-20260319.txt".to_string(),
        file_details: Vec::new(),
        preview_files: vec![
            PreviewFileEntry {
                id: 1,
                display_path: "src/main.rs".to_string(),
                chars: 10,
                tokens: 3,
                preview_blob_path: PathBuf::from("a"),
                byte_len: 10,
                archive: None,
            },
            PreviewFileEntry {
                id: 2,
                display_path: "src/lib.rs".to_string(),
                chars: 12,
                tokens: 4,
                preview_blob_path: PathBuf::from("b"),
                byte_len: 12,
                archive: None,
            },
        ],
        preview_blob_dir: None,
    }
}

fn sample_archive_result() -> ProcessResult {
    ProcessResult {
        stats: ProcessingStats::default(),
        tree_string: String::new(),
        tree_nodes: vec![TreeNode {
            id: "bundle.zip".to_string(),
            label: "bundle.zip".to_string(),
            relative_path: "bundle.zip".to_string(),
            is_folder: true,
            children: vec![TreeNode {
                id: "bundle.zip/src".to_string(),
                label: "src".to_string(),
                relative_path: "bundle.zip/src".to_string(),
                is_folder: true,
                children: vec![TreeNode {
                    id: "bundle.zip/src/lib.rs".to_string(),
                    label: "lib.rs".to_string(),
                    relative_path: "bundle.zip/src/lib.rs".to_string(),
                    is_folder: false,
                    children: Vec::new(),
                }],
            }],
        }],
        merged_content_path: None,
        suggested_result_name: "workspace-20260319.txt".to_string(),
        file_details: Vec::new(),
        preview_files: vec![PreviewFileEntry {
            id: 2,
            display_path: "bundle.zip/src/lib.rs".to_string(),
            chars: 12,
            tokens: 4,
            preview_blob_path: PathBuf::from("b"),
            byte_len: 12,
            archive: Some(ArchiveEntrySource {
                archive_path: "bundle.zip".to_string(),
                entry_path: "src/lib.rs".to_string(),
            }),
        }],
        preview_blob_dir: None,
    }
}

fn sample_sort_result() -> ProcessResult {
    ProcessResult {
        stats: ProcessingStats::default(),
        tree_string: String::new(),
        tree_nodes: Vec::new(),
        merged_content_path: None,
        suggested_result_name: "workspace-20260319.txt".to_string(),
        file_details: Vec::new(),
        preview_files: vec![
            PreviewFileEntry {
                id: 1,
                display_path: "src/a.rs".to_string(),
                chars: 8,
                tokens: 2,
                preview_blob_path: PathBuf::from("a"),
                byte_len: 8,
                archive: None,
            },
            PreviewFileEntry {
                id: 2,
                display_path: "src/b.rs".to_string(),
                chars: 24,
                tokens: 6,
                preview_blob_path: PathBuf::from("b"),
                byte_len: 24,
                archive: None,
            },
            PreviewFileEntry {
                id: 3,
                display_path: "src/c.rs".to_string(),
                chars: 12,
                tokens: 3,
                preview_blob_path: PathBuf::from("c"),
                byte_len: 12,
                archive: None,
            },
        ],
        preview_blob_dir: None,
    }
}

fn nested_result() -> ProcessResult {
    ProcessResult {
        stats: ProcessingStats::default(),
        tree_string: String::new(),
        tree_nodes: vec![
            TreeNode {
                id: "src".to_string(),
                label: "src".to_string(),
                relative_path: "src".to_string(),
                is_folder: true,
                children: vec![
                    TreeNode {
                        id: "src/nested".to_string(),
                        label: "nested".to_string(),
                        relative_path: "src/nested".to_string(),
                        is_folder: true,
                        children: vec![TreeNode {
                            id: "src/nested/lib.rs".to_string(),
                            label: "lib.rs".to_string(),
                            relative_path: "src/nested/lib.rs".to_string(),
                            is_folder: false,
                            children: Vec::new(),
                        }],
                    },
                    TreeNode {
                        id: "src/main.rs".to_string(),
                        label: "main.rs".to_string(),
                        relative_path: "src/main.rs".to_string(),
                        is_folder: false,
                        children: Vec::new(),
                    },
                ],
            },
            TreeNode {
                id: "README.md".to_string(),
                label: "README.md".to_string(),
                relative_path: "README.md".to_string(),
                is_folder: false,
                children: Vec::new(),
            },
        ],
        merged_content_path: None,
        suggested_result_name: "workspace-20260319.txt".to_string(),
        file_details: Vec::new(),
        preview_files: vec![
            PreviewFileEntry {
                id: 1,
                display_path: "src/main.rs".to_string(),
                chars: 10,
                tokens: 3,
                preview_blob_path: PathBuf::from("a"),
                byte_len: 10,
                archive: None,
            },
            PreviewFileEntry {
                id: 2,
                display_path: "src/nested/lib.rs".to_string(),
                chars: 12,
                tokens: 4,
                preview_blob_path: PathBuf::from("b"),
                byte_len: 12,
                archive: None,
            },
        ],
        preview_blob_dir: None,
    }
}
