use iced::widget::{container, scrollable, text};
use iced::{Color, Element, Font, Length};

use crate::app::message::Message;
use crate::app::model::Model;
use crate::app::theme;

pub fn view(model: &Model) -> Element<'_, Message> {
    let tree = model
        .result
        .as_ref()
        .and_then(|r| r.tree_string.clone())
        .unwrap_or_default();

    container(
        scrollable(
            text(tree)
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
