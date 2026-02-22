use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use crate::app::message::Message;
use crate::app::model::{Model, ProcessingMode};
use crate::app::theme;
use crate::utils::i18n::tr;

pub fn view(model: &Model) -> Element<'_, Message> {
    let lang = model.language;
    let mode_hint = match model.options.mode {
        ProcessingMode::Full => tr(lang, "mode_full_desc"),
        ProcessingMode::TreeOnly => tr(lang, "mode_tree_only_desc"),
    };

    container(
        column![
            container(text(tr(lang, "section_preflight")).size(16))
                .padding([theme::SPACE_XS, 10])
                .style(theme::strip_neutral),
            row![
                stat_tile(tr(lang, "total"), model.preflight.total_files),
                stat_tile(tr(lang, "skip"), model.preflight.skipped_files),
                stat_tile(tr(lang, "process"), model.preflight.to_process_files),
            ]
            .width(Length::Fill)
            .spacing(theme::SPACE_MD as u32),
            text(mode_hint).size(13),
        ]
        .spacing(theme::SPACE_SM as u32),
    )
    .width(Length::Fill)
    .into()
}

fn stat_tile<'a>(label: &'a str, value: usize) -> Element<'a, Message> {
    container(
        column![text(label).size(12), text(format!("{value}")).size(22)]
            .spacing(4)
            .width(Length::Fill),
    )
    .padding(10)
    .width(Length::Fill)
    .style(theme::accent_tile)
    .into()
}
