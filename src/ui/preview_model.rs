use std::ops::Range;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use gpui::SharedString;

use crate::services::preview::{PreviewDocument, PreviewEvent, PreviewRequest};
use crate::ui::state::PreviewPanelState;

pub struct PreviewModel {
    state: PreviewPanelState,
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

    pub fn state_mut(&mut self) -> &mut PreviewPanelState {
        &mut self.state
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
            } => {
                if revision != self.state.preview_revision
                    || self.state.selected_preview_file_id != Some(file_id)
                {
                    return PreviewEventEffect::Ignored;
                }
                self.state.preview_document = Some(document);
                self.state.preview_error = None;
                self.state.preview_requested_range = None;
                self.state.clear_loaded_chunks();
                self.state.store_chunk(
                    loaded_range,
                    lines.into_iter().map(SharedString::from).collect(),
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
                self.state.store_chunk(
                    loaded_range,
                    lines.into_iter().map(SharedString::from).collect(),
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
                PreviewEventEffect::Updated
            }
        }
    }

    pub fn clear_request(&mut self) {
        self.state.preview_requested_range = None;
    }

    pub fn take_preview_rx(&mut self) -> Option<Receiver<PreviewEvent>> {
        self.state.preview_rx.take()
    }

    pub fn open_preview(&mut self, file_id: u32, path: PathBuf) -> PreviewRequest {
        self.state.preview_revision += 1;
        self.state.selected_preview_file_id = Some(file_id);
        self.state.preview_error = None;
        self.state.preview_requested_range =
            Some(0..crate::ui::state::PreviewPanelState::VISIBLE_BUCKET_LINES * 2);
        self.state.preview_document = None;
        self.state.preview_last_visible_range = 0..0;
        self.state.clear_loaded_chunks();

        PreviewRequest::Open {
            revision: self.state.preview_revision,
            file_id,
            path,
            initial_range: 0..crate::ui::state::PreviewPanelState::VISIBLE_BUCKET_LINES * 2,
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

    pub fn line_at(&self, ix: usize) -> Option<SharedString> {
        self.state.line_at(ix)
    }

    pub fn preview_request_range(&self, range: Range<usize>, line_count: usize) -> Range<usize> {
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

        bucket_start.saturating_sub(bucket)..(bucket_end + bucket * 2).min(line_count)
    }

    pub fn load_preview_range_request(&mut self, range: Range<usize>) -> Option<PreviewRequest> {
        let document = self.state.preview_document.as_ref()?.clone();
        let file_id = self.state.selected_preview_file_id?;
        let padded = self.preview_request_range(range, document.line_count());
        if padded.start >= padded.end || self.state.has_loaded_range(&padded) {
            return None;
        }
        if self.state.preview_requested_range.as_ref() == Some(&padded) {
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
    use super::{PreviewEventEffect, PreviewModel};
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
        let request = model.open_preview(7, path.clone());
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
        });
        assert!(matches!(effect, PreviewEventEffect::ScrollTop));
        assert!(model.preview_document().is_some());
        assert_eq!(
            model.line_at(1).map(|line| line.to_string()),
            Some("b".into())
        );
        let _ = fs::remove_dir_all(root);
    }
}
