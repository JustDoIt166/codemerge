use std::ops::Range;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use gpui::SharedString;

use crate::services::preview::{PreviewDocument, PreviewEvent, PreviewRequest};
use crate::ui::state::PreviewPanelState;

pub struct PreviewModel {
    state: PreviewPanelState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewScrollDirection {
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewRenderLine {
    pub line_number: SharedString,
    pub text: SharedString,
    pub missing: bool,
}

impl PreviewModel {
    pub fn new() -> Self {
        Self {
            state: PreviewPanelState::default(),
        }
    }

    pub fn state(&self) -> &PreviewPanelState {
        &self.state
    }

    pub fn clear(&mut self) {
        self.state = PreviewPanelState::default();
    }

    pub fn apply_event(&mut self, event: PreviewEvent) -> PreviewEventEffect {
        match event {
            PreviewEvent::Opened {
                revision,
                file_id,
                document,
                loaded_range,
                lines,
                full_text,
            } => {
                if revision != self.state.preview_revision
                    || self.state.selected_preview_file_id != Some(file_id)
                {
                    return PreviewEventEffect::Ignored;
                }
                self.state.preview_document = Some(document);
                self.state.preview_text = full_text.map(SharedString::from);
                self.state.preview_error = None;
                self.state.preview_requested_range = None;
                self.state.queued_preview_range = None;
                self.state.clear_loaded_chunks();
                self.state.store_chunk_with_focus(
                    loaded_range,
                    lines.into_iter().map(SharedString::from).collect(),
                    &(0..crate::ui::state::PreviewPanelState::VISIBLE_BUCKET_LINES * 2),
                );
                PreviewEventEffect::ScrollTop
            }
            PreviewEvent::Loaded {
                revision,
                file_id,
                loaded_range,
                lines,
            } => {
                if revision != self.state.preview_revision
                    || self.state.selected_preview_file_id != Some(file_id)
                {
                    return PreviewEventEffect::Ignored;
                }
                self.state.preview_error = None;
                self.state.preview_requested_range = None;
                let focus_range = self
                    .state
                    .queued_preview_range
                    .clone()
                    .or_else(|| self.state.preview_requested_range.clone())
                    .unwrap_or_else(|| loaded_range.clone());
                self.state.store_chunk_with_focus(
                    loaded_range,
                    lines.into_iter().map(SharedString::from).collect(),
                    &focus_range,
                );
                PreviewEventEffect::Updated
            }
            PreviewEvent::Failed {
                revision,
                file_id,
                error,
            } => {
                if revision != self.state.preview_revision
                    || self.state.selected_preview_file_id != Some(file_id)
                {
                    return PreviewEventEffect::Ignored;
                }
                self.state.preview_error = Some(error.to_string());
                self.state.preview_requested_range = None;
                if self.state.preview_document.is_none() {
                    self.state.clear_loaded_chunks();
                }
                self.state.bump_render_revision();
                PreviewEventEffect::Updated
            }
        }
    }

    pub fn apply_events<I>(&mut self, events: I) -> PreviewEventEffect
    where
        I: IntoIterator<Item = PreviewEvent>,
    {
        let mut effect = PreviewEventEffect::Ignored;
        for event in events {
            let next = self.apply_event(event);
            effect = match (effect, next) {
                (PreviewEventEffect::ScrollTop, _) | (_, PreviewEventEffect::ScrollTop) => {
                    PreviewEventEffect::ScrollTop
                }
                (PreviewEventEffect::Updated, _) | (_, PreviewEventEffect::Updated) => {
                    PreviewEventEffect::Updated
                }
                _ => PreviewEventEffect::Ignored,
            };
        }
        effect
    }

    pub fn clear_request(&mut self) {
        self.state.preview_requested_range = None;
    }

    pub fn take_queued_preview_range(&mut self) -> Option<Range<usize>> {
        self.state.take_queued_preview_range()
    }

    pub fn take_preview_rx(&mut self) -> Option<Receiver<PreviewEvent>> {
        self.state.preview_rx.take()
    }

    pub fn open_preview(
        &mut self,
        file_id: u32,
        path: PathBuf,
        include_full_text: bool,
    ) -> PreviewRequest {
        self.state.preview_revision += 1;
        self.state.selected_preview_file_id = Some(file_id);
        self.state.preview_error = None;
        self.state.preview_requested_range =
            Some(0..crate::ui::state::PreviewPanelState::VISIBLE_BUCKET_LINES * 2);
        self.state.queued_preview_range = None;
        self.state.preview_document = None;
        self.state.preview_text = None;
        self.state.clear_loaded_chunks();
        self.state.bump_render_revision();

        PreviewRequest::Open {
            revision: self.state.preview_revision,
            file_id,
            path,
            initial_range: 0..crate::ui::state::PreviewPanelState::VISIBLE_BUCKET_LINES * 2,
            include_full_text,
        }
    }

    pub fn set_preview_rx(&mut self, rx: Option<Receiver<PreviewEvent>>) {
        self.state.preview_rx = rx;
    }

    pub fn selected_preview_file_id(&self) -> Option<u32> {
        self.state.selected_preview_file_id
    }

    pub fn preview_document(&self) -> Option<&PreviewDocument> {
        self.state.preview_document.as_ref()
    }

    pub fn preview_text(&self) -> Option<SharedString> {
        self.state.preview_text.clone()
    }

    pub fn preview_revision(&self) -> u64 {
        self.state.preview_revision
    }

    pub fn line_at(&self, ix: usize) -> Option<SharedString> {
        self.state.line_at(ix)
    }

    pub fn render_revision(&self) -> u64 {
        self.state.render_revision
    }

    pub fn build_render_lines(&self, range: Range<usize>) -> Vec<PreviewRenderLine> {
        self.build_render_lines_partial(range)
    }

    pub fn build_render_lines_partial(&self, range: Range<usize>) -> Vec<PreviewRenderLine> {
        range.map(|ix| self.build_render_line(ix)).collect()
    }

    pub fn build_render_line(&self, ix: usize) -> PreviewRenderLine {
        let loaded = self.line_at(ix);
        let text = loaded
            .clone()
            .unwrap_or_else(|| SharedString::from("\u{2026}"));
        PreviewRenderLine {
            line_number: SharedString::from((ix + 1).to_string()),
            missing: loaded.is_none(),
            text,
        }
    }

    pub fn preview_request_range(
        &self,
        range: Range<usize>,
        line_count: usize,
        direction: PreviewScrollDirection,
    ) -> Range<usize> {
        if line_count == 0 {
            return 0..0;
        }
        let bucket = PreviewPanelState::VISIBLE_BUCKET_LINES;
        let start = range.start.min(line_count.saturating_sub(1));
        let end = range.end.max(start + 1).min(line_count);
        let bucket_start = (start / bucket) * bucket;
        let bucket_end = end
            .saturating_sub(1)
            .checked_div(bucket)
            .map(|ix| (ix + 1) * bucket)
            .unwrap_or(bucket)
            .min(line_count);

        let leading_buckets = match direction {
            PreviewScrollDirection::Down => 1,
            PreviewScrollDirection::Up => 3,
        };
        let trailing_buckets = match direction {
            PreviewScrollDirection::Down => 3,
            PreviewScrollDirection::Up => 1,
        };

        bucket_start.saturating_sub(bucket * leading_buckets)
            ..(bucket_end + bucket * trailing_buckets).min(line_count)
    }

    pub fn load_preview_range_request(
        &mut self,
        range: Range<usize>,
        direction: PreviewScrollDirection,
    ) -> Option<PreviewRequest> {
        let document = self.state.preview_document.as_ref()?.clone();
        let file_id = self.state.selected_preview_file_id?;
        let padded = self.preview_request_range(range, document.line_count(), direction);
        if padded.start >= padded.end || self.state.has_loaded_range(&padded) {
            return None;
        }
        if self.state.preview_requested_range.as_ref() == Some(&padded) {
            return None;
        }
        if self.state.preview_requested_range.is_some() {
            self.state.queue_preview_range(padded);
            return None;
        }

        self.state.preview_requested_range = Some(padded.clone());
        Some(PreviewRequest::LoadRange {
            revision: self.state.preview_revision,
            file_id,
            document,
            range: padded,
        })
    }
}

pub enum PreviewEventEffect {
    Ignored,
    Updated,
    ScrollTop,
}

#[cfg(test)]
mod tests {
    use super::{PreviewEventEffect, PreviewModel, PreviewScrollDirection};
    use crate::services::preview::{PreviewEvent, PreviewRequest, index_document};
    use std::fs;

    #[test]
    fn opened_event_primes_document_and_chunks() {
        let root = std::env::temp_dir().join(format!(
            "codemerge_preview_model_tests_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        let path = root.join("preview.txt");
        fs::write(&path, "a\nb\nc").expect("write preview");
        let mut model = PreviewModel::new();
        let request = model.open_preview(7, path.clone(), false);
        let revision = match request {
            PreviewRequest::Open { revision, .. } => revision,
            _ => unreachable!(),
        };
        let document = index_document(&path).expect("index document");
        let effect = model.apply_event(PreviewEvent::Opened {
            revision,
            file_id: 7,
            document,
            loaded_range: 0..2,
            lines: vec!["a".into(), "b".into()],
            full_text: None,
        });
        assert!(matches!(effect, PreviewEventEffect::ScrollTop));
        assert!(model.preview_document().is_some());
        assert_eq!(
            model.line_at(1).map(|line| line.to_string()),
            Some("b".into())
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repeated_bucket_requests_are_coalesced_while_request_is_in_flight() {
        let root = std::env::temp_dir().join(format!(
            "codemerge_preview_model_queue_tests_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        let path = root.join("preview.txt");
        fs::write(
            &path,
            (0..512)
                .map(|ix| format!("line-{ix}\n"))
                .collect::<String>(),
        )
        .expect("write preview");
        let mut model = PreviewModel::new();
        let request = model.open_preview(7, path.clone(), false);
        let revision = match request {
            PreviewRequest::Open { revision, .. } => revision,
            _ => unreachable!(),
        };
        let document = index_document(&path).expect("index document");
        let _ = model.apply_event(PreviewEvent::Opened {
            revision,
            file_id: 7,
            document,
            loaded_range: 0..128,
            lines: (0..128).map(|ix| format!("line-{ix}")).collect(),
            full_text: None,
        });

        assert!(matches!(
            model.load_preview_range_request(160..200, PreviewScrollDirection::Down),
            Some(PreviewRequest::LoadRange { .. })
        ));
        assert!(
            model
                .load_preview_range_request(420..460, PreviewScrollDirection::Down)
                .is_none()
        );
        assert!(model.take_queued_preview_range().is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn preview_request_range_prefetches_in_scroll_direction() {
        let model = PreviewModel::new();

        assert_eq!(
            model.preview_request_range(384..448, 2_000, PreviewScrollDirection::Down),
            192..1152
        );
        assert_eq!(
            model.preview_request_range(384..448, 2_000, PreviewScrollDirection::Up),
            0..768
        );
    }

    #[test]
    fn repeated_requests_within_same_prefetch_window_do_not_refetch() {
        let root = std::env::temp_dir().join(format!(
            "codemerge_preview_model_prefetch_tests_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        let path = root.join("preview.txt");
        fs::write(
            &path,
            (0..1_024)
                .map(|ix| format!("line-{ix}\n"))
                .collect::<String>(),
        )
        .expect("write preview");

        let mut model = PreviewModel::new();
        let request = model.open_preview(9, path.clone(), false);
        let revision = match request {
            PreviewRequest::Open { revision, .. } => revision,
            _ => unreachable!(),
        };
        let document = index_document(&path).expect("index document");
        let _ = model.apply_event(PreviewEvent::Opened {
            revision,
            file_id: 9,
            document,
            loaded_range: 0..128,
            lines: (0..128).map(|ix| format!("line-{ix}")).collect(),
            full_text: None,
        });

        assert!(matches!(
            model.load_preview_range_request(192..256, PreviewScrollDirection::Down),
            Some(PreviewRequest::LoadRange { .. })
        ));
        assert!(
            model
                .load_preview_range_request(256..320, PreviewScrollDirection::Down)
                .is_none()
        );
        let _ = fs::remove_dir_all(root);
    }
}
