use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Color, Element, Font, Length};

use crate::app::message::{Message, UiMessage};
use crate::app::model::Model;
use crate::app::theme;
use crate::utils::i18n::tr;

pub fn view(model: &Model) -> Element<'_, Message> {
    let lang = model.language;
    let selected_id = model.ui.selected_preview_file_id;
    let filter = model.ui.preview_filter_input.trim().to_lowercase();

    let mut file_rows = column![]
        .spacing(theme::SPACE_XS as u32)
        .width(Length::Fill)
        .height(Length::Shrink);

    if let Some(result) = &model.result {
        for entry in &result.preview_files {
            if !filter.is_empty() && !entry.display_path.to_lowercase().contains(&filter) {
                continue;
            }

            let mut file_btn = button(
                row![
                    text(&entry.display_path).size(12),
                    text(format!("{}c / {}t", entry.chars, entry.tokens)).size(11),
                ]
                .spacing(theme::SPACE_SM as u32)
                .align_y(iced::Alignment::Center),
            )
            .width(Length::Fill)
            .padding([6, 8]);

            file_btn = if selected_id == Some(entry.id) {
                file_btn.style(theme::button_tab_active)
            } else {
                file_btn.style(theme::button_tab_inactive)
            };

            file_rows = file_rows.push(
                file_btn.on_press(Message::Ui(UiMessage::SelectPreviewFile(entry.id))),
            );
        }
    }

    let prev_enabled = model.ui.selected_preview_file_id.is_some() && model.ui.preview_offset > 0;
    let next_enabled = model.ui.selected_preview_file_id.is_some()
        && model.ui.preview_loaded_bytes > 0
        && model.ui.preview_offset.saturating_add(model.ui.preview_loaded_bytes)
            < model.ui.preview_total_bytes;

    let mut prev_btn = button(tr(lang, "prev_page"))
        .padding([6, 10])
        .style(theme::button_secondary);
    if prev_enabled {
        prev_btn = prev_btn.on_press(Message::Ui(UiMessage::PreviewPrevPage));
    }

    let mut next_btn = button(tr(lang, "next_page"))
        .padding([6, 10])
        .style(theme::button_secondary);
    if next_enabled {
        next_btn = next_btn.on_press(Message::Ui(UiMessage::PreviewNextPage));
    }

    let range = format!(
        "{}..{} / {} bytes",
        model.ui.preview_offset,
        model.ui
            .preview_offset
            .saturating_add(model.ui.preview_loaded_bytes),
        model.ui.preview_total_bytes
    );

    let status = if model.ui.preview_loading {
        tr(lang, "preview_loading")
    } else {
        ""
    };

    container(
        column![
            text_input(tr(lang, "file_filter"), &model.ui.preview_filter_input)
                .on_input(|v| Message::Ui(UiMessage::PreviewFilterChanged(v)))
                .padding([6, 8]),
            container(scrollable(file_rows).height(Length::Fixed(140.0)))
                .style(theme::card_background)
                .padding(6),
            row![
                prev_btn,
                next_btn,
                text(range).size(12),
                text(status).size(12),
            ]
            .spacing(theme::SPACE_SM as u32)
            .align_y(iced::Alignment::Center),
            container(
                scrollable(
                    text(&model.ui.preview_content)
                        .font(Font::MONOSPACE)
                        .color(Color::from_rgb(0.85, 0.89, 0.97)),
                )
                .height(Length::Fill),
            )
            .padding(theme::SPACE_SM)
            .style(theme::code_surface)
            .height(Length::Fill),
        ]
        .spacing(theme::SPACE_SM as u32),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
