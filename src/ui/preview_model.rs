use std::ops::Range;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use gpui::SharedString;

use crate::services::preview::{PreviewDocument, PreviewEvent, PreviewRequest};
use crate::ui::state::PreviewPanelState;

pub struct PreviewModel {
    state: PreviewPanelState,
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
            } => {
                if revision != self.state.preview_revision
                    || self.state.selected_preview_file_id != Some(file_id)
                {
                    return PreviewEventEffect::Ignored;
                }
                self.state.preview_document = Some(document);
                self.state.preview_error = None;
                self.state.preview_requested_range = None;
                self.state.queued_preview_range = None;
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

    pub fn open_preview(&mut self, file_id: u32, path: PathBuf) -> PreviewRequest {
        self.state.preview_revision += 1;
        self.state.selected_preview_file_id = Some(file_id);
        self.state.preview_error = None;
        self.state.preview_requested_range =
            Some(0..crate::ui::state::PreviewPanelState::VISIBLE_BUCKET_LINES * 2);
        self.state.queued_preview_range = None;
        self.state.preview_document = None;
        self.state.clear_loaded_chunks();
        self.state.bump_render_revision();

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

    pub fn render_revision(&self) -> u64 {
        self.state.render_revision
    }

    pub fn build_render_lines(&self, range: Range<usize>) -> Vec<PreviewRenderLine> {
        range
            .map(|ix| {
                let loaded = self.line_at(ix);
                let text = loaded.clone().unwrap_or_default();
                PreviewRenderLine {
                    line_number: SharedString::from((ix + 1).to_string()),
                    missing: loaded.is_none(),
                    text,
                }
            })
            .collect()
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
        let request = model.open_preview(7, path.clone());
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
        });

        assert!(matches!(
            model.load_preview_range_request(160..200),
            Some(PreviewRequest::LoadRange { .. })
        ));
        assert!(model.load_preview_range_request(420..460).is_none());
        assert!(model.take_queued_preview_range().is_some());
        let _ = fs::remove_dir_all(root);
    }
}
