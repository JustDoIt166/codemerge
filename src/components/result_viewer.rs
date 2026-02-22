use iced::widget::{container, scrollable, text};
use iced::{Color, Element, Font, Length};

use crate::app::message::Message;
use crate::app::model::Model;
use crate::app::theme;

pub fn view(model: &Model) -> Element<'_, Message> {
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
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
