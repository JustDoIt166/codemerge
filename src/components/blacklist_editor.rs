use iced::widget::{
    Space, button, checkbox, column, container, row, scrollable, text, text_input, tooltip,
};
use iced::{Element, Length};
use iced_aw::iced_fonts::bootstrap;

use crate::app::message::{BlacklistMessage, Message};
use crate::app::model::Model;
use crate::app::theme;
use crate::utils::i18n::tr;

pub fn view(model: &Model) -> Element<'_, Message> {
    let lang = model.language;
    let filter = model.ui.blacklist_filter_input.trim().to_ascii_lowercase();
    let mut items: Vec<BlacklistItem> = Vec::new();
    let mut matches = 0usize;
    for v in &model.folder_blacklist {
        if !filter.is_empty() && !v.to_ascii_lowercase().contains(&filter) {
            continue;
        }
        let key = format!("folder:{v}");
        matches += 1;
        items.push(BlacklistItem {
            icon: "📁",
            value: v.to_string(),
            selected: model.ui.blacklist_selected.contains(&key),
            on_toggle: Message::Blacklist(BlacklistMessage::ToggleSelect(key)),
            on_remove: Message::Blacklist(BlacklistMessage::RemoveFolder(v.clone())),
        });
    }

    for v in &model.ext_blacklist {
        if !filter.is_empty() && !v.to_ascii_lowercase().contains(&filter) {
            continue;
        }
        let key = format!("ext:{v}");
        matches += 1;
        items.push(BlacklistItem {
            icon: "📄",
            value: v.to_string(),
            selected: model.ui.blacklist_selected.contains(&key),
            on_toggle: Message::Blacklist(BlacklistMessage::ToggleSelect(key)),
            on_remove: Message::Blacklist(BlacklistMessage::RemoveExt(v.clone())),
        });
    }

    let mut entries = column![].spacing(theme::SPACE_XS as u32);
    if matches == 0 {
        entries = entries.push(
            container(text(tr(lang, "blacklist_no_match")).size(13))
                .padding([theme::SPACE_XS, theme::SPACE_SM])
                .style(theme::strip_neutral),
        );
    } else {
        let per_row = if model.window_size.0 >= 1400.0 {
            4
        } else if model.window_size.0 >= 1100.0 {
            3
        } else if model.window_size.0 >= 800.0 {
            2
        } else {
            1
        };
        while !items.is_empty() {
            let take = per_row.min(items.len());
            let row_items = items.drain(0..take).collect::<Vec<_>>();
            let mut line = row![].spacing(4);
            for item in row_items {
                line = line.push(tag_chip(item).width(Length::FillPortion(1)));
            }
            if take < per_row {
                for _ in 0..(per_row - take) {
                    line = line.push(Space::new().width(Length::FillPortion(1)));
                }
            }
            entries = entries.push(line);
        }
    }

    let mut delete_selected_btn =
        button(tr(lang, "blacklist_delete_selected")).style(theme::button_secondary);
    if !model.ui.blacklist_selected.is_empty() {
        delete_selected_btn =
            delete_selected_btn.on_press(Message::Blacklist(BlacklistMessage::DeleteSelected));
    }

    let content = column![
        row![
            text_input(
                tr(lang, "blacklist_filter"),
                &model.ui.blacklist_filter_input
            )
            .on_input(|v| Message::Blacklist(BlacklistMessage::FilterInputChanged(v)))
            .width(Length::Fill),
            button("✕")
                .style(theme::button_icon)
                .on_press(Message::Blacklist(BlacklistMessage::FilterInputChanged(
                    String::new()
                ))),
            text(format!("{} {matches}", tr(lang, "blacklist_match_count"))).size(12),
        ]
        .spacing(theme::SPACE_XS as u32)
        .align_y(iced::Alignment::Center),
        row![
            text_input(
                tr(lang, "blacklist_unified_hint"),
                &model.ui.folder_blacklist_input
            )
            .on_input(|v| Message::Blacklist(BlacklistMessage::SharedInputChanged(v)))
            .width(Length::Fill),
            button(tr(lang, "add_folder"))
                .style(theme::button_secondary)
                .on_press(Message::Blacklist(BlacklistMessage::AddFolder)),
            button(tr(lang, "add_ext"))
                .style(theme::button_secondary)
                .on_press(Message::Blacklist(BlacklistMessage::AddExt)),
        ]
        .spacing(theme::SPACE_XS as u32),
        row![
            button(if model.ui.blacklist_selected_all {
                tr(lang, "blacklist_unselect_all")
            } else {
                tr(lang, "blacklist_select_all")
            })
            .style(theme::button_secondary)
            .on_press(Message::Blacklist(BlacklistMessage::ToggleSelectAll)),
            button(tr(lang, "blacklist_invert_selection"))
                .style(theme::button_secondary)
                .on_press(Message::Blacklist(BlacklistMessage::ToggleInvertSelection)),
            delete_selected_btn,
            icon_toolbar_btn(
                "⤒",
                tr(lang, "blacklist_import_append"),
                Message::Blacklist(BlacklistMessage::ImportAppend)
            ),
            icon_toolbar_btn(
                "⤓",
                tr(lang, "blacklist_export"),
                Message::Blacklist(BlacklistMessage::Export)
            ),
            icon_toolbar_btn(
                "↺",
                tr(lang, "blacklist_reset_default"),
                Message::Blacklist(BlacklistMessage::ResetToDefault)
            ),
            icon_toolbar_btn(
                "🗑",
                tr(lang, "blacklist_clear_all"),
                Message::Blacklist(BlacklistMessage::ClearAll)
            ),
        ]
        .spacing(theme::SPACE_XS as u32)
        .align_y(iced::Alignment::Center),
        scrollable(entries)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(3.0)
                    .scroller_width(3.0)
                    .margin(1.0),
            ))
            .height(Length::Fill),
        row![
            Space::new().width(Length::Fill),
            button(tr(lang, "save_settings"))
                .style(theme::button_primary)
                .on_press(Message::Blacklist(BlacklistMessage::SaveSettings)),
        ]
        .align_y(iced::Alignment::Center),
    ]
    .spacing(theme::SPACE_SM as u32)
    .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

#[derive(Clone)]
struct BlacklistItem {
    icon: &'static str,
    value: String,
    selected: bool,
    on_toggle: Message,
    on_remove: Message,
}

fn tag_chip(item: BlacklistItem) -> iced::widget::Container<'static, Message> {
    let on_toggle = item.on_toggle.clone();
    let on_remove = item.on_remove.clone();
    container(
        row![
            checkbox(item.selected)
                .size(14)
                .on_toggle(move |_| on_toggle.clone()),
            text(item.icon).size(12),
            text(item.value).size(12),
            Space::new().width(Length::Fill),
            button(
                container(bootstrap::x().size(12))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
                .width(Length::Fixed(20.0))
                .height(Length::Fixed(20.0))
                .padding(0)
                .style(theme::button_icon)
                .on_press(on_remove),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(3),
    )
    .padding([2, 5])
    .style(theme::accent_tile)
}

fn icon_toolbar_btn<'a>(icon: &'a str, tip: &'a str, on_press: Message) -> Element<'a, Message> {
    let btn = button(text(icon).size(14))
        .width(Length::Fixed(34.0))
        .height(Length::Fixed(30.0))
        .style(theme::button_icon)
        .on_press(on_press);

    tooltip(
        btn,
        container(
            text(tip)
                .size(12)
                .color(iced::Color::from_rgb(0.96, 0.98, 1.0)),
        )
            .padding([4, 8])
            .style(theme::tooltip_bubble),
        tooltip::Position::Bottom,
    )
    .into()
}
