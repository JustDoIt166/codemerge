use gpui::SharedString;

use crate::domain::{Language, PreviewRowViewModel, ProcessResult, ResultTab};
use crate::ui::state::{DeferredPreviewState, NarrowContentTab};
use crate::utils::i18n::tr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct EmptyStateViewModel {
    pub title: SharedString,
    pub hint: SharedString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum ResultsCopyAction {
    Tree,
    Preview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum ResultsPanelBodyViewModel {
    Tree,
    Content,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct ResultsPanelViewModel {
    pub selected_tab: usize,
    pub has_content_result: bool,
    pub copy_label: SharedString,
    pub copy_action: ResultsCopyAction,
    pub body: ResultsPanelBodyViewModel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct ContentFileListViewModel {
    pub visible_row_count: usize,
    pub filter_active: bool,
    pub file_list_collapsed: bool,
    pub toggle_label: SharedString,
    pub empty_state: Option<EmptyStateViewModel>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct ContentBodyViewModel {
    pub file_list: ContentFileListViewModel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum ContentPanelBodyViewModel {
    TreeOnly(EmptyStateViewModel),
    Split(ContentBodyViewModel),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct ContentPanelViewModel {
    pub body: ContentPanelBodyViewModel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum CompactContentBodyViewModel {
    Status,
    Results,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct CompactContentPanelViewModel {
    pub selected_tab: usize,
    pub body: CompactContentBodyViewModel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct PreviewDocumentViewModel {
    pub line_count: usize,
    pub byte_len: u64,
    pub document_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct PreviewDeferredViewModel {
    pub title: SharedString,
    pub detail: SharedString,
    pub source_byte_size: SharedString,
    pub excerpt_byte_size: SharedString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct PreviewExcerptBannerViewModel {
    pub title: SharedString,
    pub detail: SharedString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct PreviewContentViewModel {
    pub file_path: String,
    pub archive_paths: Option<(String, String)>,
    pub line_count: usize,
    pub byte_len: u64,
    pub excerpt_banner: Option<PreviewExcerptBannerViewModel>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) enum PreviewPaneBodyViewModel {
    DeferredMerged(PreviewDeferredViewModel),
    Error {
        title: SharedString,
        detail: SharedString,
    },
    Placeholder {
        title: SharedString,
        detail: SharedString,
    },
    Content(PreviewContentViewModel),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct PreviewPaneViewModel {
    pub body: PreviewPaneBodyViewModel,
}

#[derive(Clone)]
pub(in crate::ui::workspace) struct PreviewTableModel {
    pub rows: Vec<PreviewRowViewModel>,
    pub selected_row_ix: Option<usize>,
    pub next_selected_file_id: Option<u32>,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::ui::workspace) enum PreviewTableSort {
    #[default]
    None,
    CharsDesc,
    CharsAsc,
}

impl PreviewTableSort {
    pub fn toggle_chars(self) -> Self {
        match self {
            Self::CharsDesc => Self::CharsAsc,
            Self::CharsAsc | Self::None => Self::CharsDesc,
        }
    }
}

pub(in crate::ui::workspace) fn build_results_panel_view_model(
    active_tab: ResultTab,
    has_content_result: bool,
    language: Language,
) -> ResultsPanelViewModel {
    let body = if active_tab == ResultTab::Content && has_content_result {
        ResultsPanelBodyViewModel::Content
    } else {
        ResultsPanelBodyViewModel::Tree
    };
    let (copy_label, copy_action, selected_tab) = match body {
        ResultsPanelBodyViewModel::Tree => (
            SharedString::from(tr(language, "copy_tree")),
            ResultsCopyAction::Tree,
            0,
        ),
        ResultsPanelBodyViewModel::Content => (
            SharedString::from(tr(language, "copy_current_page")),
            ResultsCopyAction::Preview,
            1,
        ),
    };

    ResultsPanelViewModel {
        selected_tab,
        has_content_result,
        copy_label,
        copy_action,
        body,
    }
}

pub(in crate::ui::workspace) fn build_content_panel_view_model(
    tree_only: bool,
    preview_rows_len: usize,
    filter_active: bool,
    file_list_collapsed: bool,
    language: Language,
) -> ContentPanelViewModel {
    if tree_only {
        return ContentPanelViewModel {
            body: ContentPanelBodyViewModel::TreeOnly(empty_state(
                tr(language, "mode_tree_only"),
                tr(language, "mode_tree_only_desc"),
            )),
        };
    }

    let empty_state = (preview_rows_len == 0).then(|| {
        empty_state(
            if filter_active {
                tr(language, "content_no_match")
            } else {
                tr(language, "content_empty")
            },
            if filter_active {
                tr(language, "content_no_match_hint")
            } else {
                tr(language, "content_empty_hint")
            },
        )
    });

    ContentPanelViewModel {
        body: ContentPanelBodyViewModel::Split(ContentBodyViewModel {
            file_list: ContentFileListViewModel {
                visible_row_count: preview_rows_len,
                filter_active,
                file_list_collapsed,
                toggle_label: SharedString::from(if file_list_collapsed {
                    tr(language, "content_files_expand")
                } else {
                    tr(language, "content_files_collapse")
                }),
                empty_state,
            },
        }),
    }
}

pub(in crate::ui::workspace) fn build_compact_content_panel_view_model(
    narrow_content_tab: NarrowContentTab,
) -> CompactContentPanelViewModel {
    match narrow_content_tab {
        NarrowContentTab::Status => CompactContentPanelViewModel {
            selected_tab: 0,
            body: CompactContentBodyViewModel::Status,
        },
        NarrowContentTab::Results => CompactContentPanelViewModel {
            selected_tab: 1,
            body: CompactContentBodyViewModel::Results,
        },
    }
}

fn empty_state(title: impl Into<String>, hint: impl Into<String>) -> EmptyStateViewModel {
    EmptyStateViewModel {
        title: SharedString::from(title.into()),
        hint: SharedString::from(hint.into()),
    }
}

pub(in crate::ui::workspace) fn build_preview_table_model(
    result: Option<&ProcessResult>,
    filter: &str,
    current_selected_id: Option<u32>,
    sort: PreviewTableSort,
) -> PreviewTableModel {
    let mut rows = result
        .map(|result| {
            result
                .preview_files
                .iter()
                .filter(|entry| {
                    filter.is_empty() || entry.display_path.to_ascii_lowercase().contains(filter)
                })
                .map(|entry| PreviewRowViewModel {
                    id: entry.id,
                    display_path: entry.display_path.clone(),
                    chars: entry.chars,
                    tokens: entry.tokens,
                    archive: entry.archive.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sort_preview_rows(&mut rows, sort);

    let selected_row_ix = rows
        .iter()
        .position(|row| Some(row.id) == current_selected_id);
    let next_selected_file_id = selected_row_ix
        .and_then(|ix| rows.get(ix))
        .map(|row| row.id)
        .or_else(|| rows.first().map(|row| row.id));

    PreviewTableModel {
        rows,
        selected_row_ix,
        next_selected_file_id,
    }
}

pub(in crate::ui::workspace) fn sort_preview_rows(
    rows: &mut [PreviewRowViewModel],
    sort: PreviewTableSort,
) {
    match sort {
        PreviewTableSort::None => {}
        PreviewTableSort::CharsDesc => rows.sort_by(|left, right| {
            right
                .chars
                .cmp(&left.chars)
                .then_with(|| left.display_path.cmp(&right.display_path))
                .then_with(|| left.id.cmp(&right.id))
        }),
        PreviewTableSort::CharsAsc => rows.sort_by(|left, right| {
            left.chars
                .cmp(&right.chars)
                .then_with(|| left.display_path.cmp(&right.display_path))
                .then_with(|| left.id.cmp(&right.id))
        }),
    }
}

pub(in crate::ui::workspace) fn preview_file_row(
    result: Option<&ProcessResult>,
    file_id: u32,
) -> Option<PreviewRowViewModel> {
    result?
        .preview_files
        .iter()
        .find(|entry| entry.id == file_id)
        .map(|entry| PreviewRowViewModel {
            id: entry.id,
            display_path: entry.display_path.clone(),
            chars: entry.chars,
            tokens: entry.tokens,
            archive: entry.archive.clone(),
        })
}

pub(in crate::ui::workspace) fn build_preview_pane_view_model(
    result: Option<&ProcessResult>,
    selected_preview_id: Option<u32>,
    preview_loading: bool,
    preview_error: Option<&str>,
    deferred_preview: Option<&DeferredPreviewState>,
    preview_document: Option<PreviewDocumentViewModel>,
    language: Language,
) -> PreviewPaneViewModel {
    let merged_preview_selected =
        selected_preview_id == Some(super::super::MERGED_CONTENT_PREVIEW_FILE_ID);
    let selected_preview = if merged_preview_selected {
        None
    } else {
        selected_preview_id.and_then(|file_id| preview_file_row(result, file_id))
    };
    let selected_preview_label = if merged_preview_selected {
        tr(language, "tab_merged_content").to_string()
    } else {
        selected_preview
            .as_ref()
            .map(|row| row.display_path.clone())
            .unwrap_or_else(|| tr(language, "preview_unknown_path").to_string())
    };

    if let Some(deferred) =
        deferred_preview.filter(|_| merged_preview_selected && preview_document.is_none())
    {
        return PreviewPaneViewModel {
            body: PreviewPaneBodyViewModel::DeferredMerged(PreviewDeferredViewModel {
                title: SharedString::from(tr(language, "tab_merged_content")),
                detail: SharedString::from(
                    preview_error
                        .unwrap_or(tr(language, "large_preview_hint"))
                        .to_string(),
                ),
                source_byte_size: SharedString::from(super::super::view::format_size(
                    deferred.source_byte_len,
                )),
                excerpt_byte_size: SharedString::from(super::super::view::format_size(
                    deferred.excerpt_byte_len,
                )),
            }),
        };
    }

    if let Some(error) = preview_error {
        return PreviewPaneViewModel {
            body: PreviewPaneBodyViewModel::Error {
                title: SharedString::from(format!(
                    "{}: {}",
                    tr(language, "status_error"),
                    selected_preview_label
                )),
                detail: SharedString::from(error.to_string()),
            },
        };
    }

    let Some(preview_document) = preview_document else {
        return PreviewPaneViewModel {
            body: PreviewPaneBodyViewModel::Placeholder {
                title: SharedString::from(if preview_loading && selected_preview_id.is_some() {
                    tr(language, "preview_loading")
                } else {
                    tr(language, "preview_empty")
                }),
                detail: SharedString::from(if preview_loading && selected_preview_id.is_some() {
                    selected_preview_label
                } else {
                    tr(language, "preview_empty_hint").to_string()
                }),
            },
        };
    };

    let file_path = if merged_preview_selected {
        tr(language, "tab_merged_content").to_string()
    } else {
        selected_preview
            .as_ref()
            .map(|row| row.display_path.clone())
            .unwrap_or(preview_document.document_path)
    };
    let archive_paths = selected_preview.as_ref().and_then(|row| {
        row.archive
            .as_ref()
            .map(|archive| (archive.archive_path.clone(), archive.entry_path.clone()))
    });
    let excerpt_banner = deferred_preview
        .filter(|state| merged_preview_selected && state.is_excerpt_loaded())
        .map(|state| PreviewExcerptBannerViewModel {
            title: SharedString::from(tr(language, "preview_loaded")),
            detail: SharedString::from(format!(
                "{} {}",
                tr(language, "large_preview_excerpt_hint"),
                super::super::view::format_size(state.source_byte_len)
            )),
        });

    PreviewPaneViewModel {
        body: PreviewPaneBodyViewModel::Content(PreviewContentViewModel {
            file_path,
            archive_paths,
            line_count: preview_document.line_count,
            byte_len: preview_document.byte_len,
            excerpt_banner,
        }),
    }
}

pub(in crate::ui::workspace) fn preview_file_node_id(
    result: Option<&ProcessResult>,
    file_id: u32,
) -> Option<String> {
    result?
        .preview_files
        .iter()
        .find(|entry| entry.id == file_id)
        .map(|entry| entry.display_path.clone())
}
