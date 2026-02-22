use iced::widget::{column, container, progress_bar, scrollable, text};
use iced::{Element, Length};

use crate::app::message::Message;
use crate::app::model::{Model, ProcessStatus, ProcessingState};
use crate::utils::i18n::tr;

pub fn view(model: &Model) -> Element<'_, Message> {
    let lang = model.language;
    let content = match &model.processing_state {
        ProcessingState::Idle => column![text(tr(lang, "status_ready"))],
        ProcessingState::InProgress {
            total,
            processed,
            skipped,
            current_file,
            records,
        } => {
            let denom = (*total).max(1) as f32;
            let done = *processed + *skipped;
            let pct = done as f32 / denom;
            let elapsed_ms = model.ui.processing_elapsed_ms;
            let eta_ms = estimate_eta_ms(done, *total, elapsed_ms);
            let mut rows = column![].spacing(4);
            for r in records.iter().rev().take(120) {
                let symbol = match r.status {
                    ProcessStatus::Success => "✓",
                    ProcessStatus::Skipped => "-",
                    ProcessStatus::Failed => "✗",
                };
                rows = rows.push(text(format!("{symbol} {}", r.file_name)).size(13));
            }
            column![
                text(format!("{}{}", tr(lang, "processing"), current_file)),
                progress_bar(0.0..=1.0, pct),
                text(format!("{} {done}/{}", tr(lang, "progress_count"), total)),
                text(format!(
                    "{} {} | {} {}",
                    tr(lang, "elapsed"),
                    format_duration(elapsed_ms),
                    tr(lang, "eta"),
                    format_duration(eta_ms)
                ))
                .size(13),
                text(format!(
                    "{}{}, {}{}",
                    tr(lang, "processed"),
                    processed,
                    tr(lang, "skipped_label"),
                    skipped
                )),
                scrollable(rows).height(Length::Fixed(140.0)),
            ]
            .spacing(6)
        }
        ProcessingState::Completed { processed, skipped } => {
            column![text(format!(
                "{}{}, {}{}",
                tr(lang, "completed"),
                processed,
                tr(lang, "skipped_label"),
                skipped
            ))]
        }
        ProcessingState::Failed(err) => column![text(format!("{}{}", tr(lang, "failed"), err))],
    };

    container(content).width(Length::Fill).into()
}

fn estimate_eta_ms(done: usize, total: usize, elapsed_ms: u64) -> u64 {
    if done == 0 || done >= total || elapsed_ms == 0 {
        return 0;
    }
    let remain = total - done;
    ((elapsed_ms as f64 / done as f64) * remain as f64) as u64
}

fn format_duration(ms: u64) -> String {
    if ms == 0 {
        return "--:--".to_string();
    }
    let total_s = ms / 1000;
    let h = total_s / 3600;
    let m = (total_s % 3600) / 60;
    let s = total_s % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
