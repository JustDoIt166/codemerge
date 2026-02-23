use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use crate::app::message::Message;
use crate::app::model::Model;
use crate::app::theme;
use crate::utils::i18n::tr;

pub fn view(model: &Model) -> Element<'_, Message> {
    let lang = model.language;
    let preflight = row![
        stat_tile(
            tr(lang, "total"),
            format_num(model.preflight.total_files),
            tr(lang, "summary_before"),
        ),
        stat_tile(
            tr(lang, "skip"),
            format_num(model.preflight.skipped_files),
            tr(lang, "summary_before"),
        ),
        stat_tile(
            tr(lang, "process"),
            format_num(model.preflight.to_process_files),
            tr(lang, "summary_before"),
        ),
    ]
    .spacing(theme::SPACE_SM as u32)
    .width(Length::Fill);

    let stats_row = if let Some(result) = &model.result {
        row![
            stat_tile(
                tr(lang, "chars"),
                format_num(result.stats.total_chars),
                tr(lang, "summary_after"),
            ),
            stat_tile(
                tr(lang, "tokens"),
                format_num(result.stats.total_tokens),
                tr(lang, "summary_after"),
            ),
        ]
        .spacing(theme::SPACE_SM as u32)
        .width(Length::Fill)
    } else {
        row![
            container(text(tr(lang, "no_stats")).size(13))
                .padding(theme::SPACE_SM)
                .style(theme::accent_tile)
                .width(Length::Fill)
        ]
    };

    column![
        text(tr(lang, "summary_before")).size(12),
        preflight,
        text(tr(lang, "summary_after")).size(12),
        stats_row
    ]
    .spacing(theme::SPACE_SM as u32)
    .into()
}

fn stat_tile<'a>(label: &'a str, value: String, subtitle: &'a str) -> Element<'a, Message> {
    container(
        column![
            text(label).size(12),
            text(value).size(20),
            text(subtitle).size(11)
        ]
        .spacing(2)
        .width(Length::Fill),
    )
    .padding(theme::SPACE_SM)
    .width(Length::Fill)
    .style(theme::accent_tile)
    .into()
}

fn format_num(value: usize) -> String {
    let s = value.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i != 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}
