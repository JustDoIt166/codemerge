use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};

use crate::app::message::{Message, UiMessage};
use crate::app::model::{Model, StatsDetailType};
use crate::app::theme;
use crate::utils::i18n::tr;

pub fn view(model: &Model) -> Element<'_, Message> {
    let lang = model.language;
    let Some(result) = &model.result else {
        return container(text(tr(lang, "no_stats"))).into();
    };

    let stats = &result.stats;
    let top = row![
        button(
            column![
                text(tr(lang, "files")).size(12),
                text(format!("{}", stats.processed_files)).size(24)
            ]
            .spacing(2)
        )
        .style(theme::button_secondary)
        .on_press(Message::Ui(UiMessage::ExpandStats(Some(
            StatsDetailType::Files
        )))),
        button(
            column![
                text(tr(lang, "skipped")).size(12),
                text(format!("{}", stats.skipped_files)).size(24)
            ]
            .spacing(2)
        )
        .style(theme::button_secondary),
        button(
            column![
                text(tr(lang, "chars")).size(12),
                text(format!("{}", stats.total_chars)).size(24)
            ]
            .spacing(2)
        )
        .style(theme::button_secondary)
        .on_press(Message::Ui(UiMessage::ExpandStats(Some(
            StatsDetailType::Chars
        )))),
        button(
            column![
                text(tr(lang, "tokens")).size(12),
                text(format!("{}", stats.total_tokens)).size(24)
            ]
            .spacing(2)
        )
        .style(theme::button_secondary)
        .on_press(Message::Ui(UiMessage::ExpandStats(Some(
            StatsDetailType::Tokens
        )))),
    ]
    .spacing(8);

    let details: Element<'_, Message> = if let Some(kind) = model.ui.expanded_stats {
        let mut body = String::new();
        for d in &result.file_details {
            let s = match kind {
                StatsDetailType::Files => {
                    format!("{} | chars={} tokens={}", d.path, d.chars, d.tokens)
                }
                StatsDetailType::Chars => format!("{} | chars={}", d.path, d.chars),
                StatsDetailType::Tokens => format!("{} | tokens={}", d.path, d.tokens),
            };
            body.push_str(&s);
            body.push('\n');
        }
        scrollable(text(body)).height(Length::Fixed(120.0)).into()
    } else {
        container(text(tr(lang, "click_for_details"))).into()
    };

    let close: Element<'_, Message> = if model.ui.expanded_stats.is_some() {
        container(text(tr(lang, "switch_details"))).into()
    } else {
        container(text("")).into()
    };

    container(column![top, close, details].spacing(8))
        .width(Length::Fill)
        .into()
}
