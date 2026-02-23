use iced::widget::{Space, button, column, container, row, scrollable, text, tooltip};
use iced::{Element, Length};
use iced_aw::iced_fonts::bootstrap;

use crate::app::message::{I18nMessage, Message, ProcessMessage, UiMessage};
use crate::app::model::{Model, OutputTab, ProcessingState};
use crate::app::theme;
use crate::components;
use crate::utils::i18n::tr;

pub fn view(model: &Model) -> Element<'_, Message> {
    let lang = model.language;
    let is_processing = matches!(model.processing_state, ProcessingState::InProgress { .. });

    let header = card(
        row![
            text(tr(lang, "title")).size(28),
            Space::new().width(Length::Fill),
            button(
                row![
                    text("🌐").size(14),
                    text(match model.language {
                        crate::app::model::Language::Zh => "EN",
                        crate::app::model::Language::En => "中文",
                    })
                    .size(14),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center)
            )
            .padding([6, 12])
            .style(theme::button_language)
            .on_press(Message::I18n(I18nMessage::ToggleLanguage)),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(theme::SPACE_SM as u32)
        .into(),
    );

    let start_label = if is_processing {
        tr(lang, "processing_ellipsis")
    } else {
        tr(lang, "start")
    };
    let start_btn_content = container(text(start_label).size(16))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);
    let mut start_btn = button(start_btn_content)
        .width(Length::FillPortion(2))
        .height(Length::Fixed(44.0))
        .padding([0, 16])
        .style(theme::button_primary);
    if !is_processing {
        start_btn = start_btn.on_press(Message::Process(ProcessMessage::Start));
    }

    let cancel_btn_content = container(text(tr(lang, "cancel")).size(16))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);
    let mut cancel_btn = button(cancel_btn_content)
        .width(Length::FillPortion(1))
        .height(Length::Fixed(44.0))
        .padding([0, 16])
        .style(theme::button_secondary);
    if is_processing {
        cancel_btn = cancel_btn.on_press(Message::Ui(UiMessage::RequestCancel));
    }

    let action_bar: Element<'_, Message> = if model.window_size.0 < 720.0 {
        column![start_btn, cancel_btn]
            .spacing(theme::SPACE_SM as u32)
            .into()
    } else {
        row![start_btn, cancel_btn]
            .spacing(theme::SPACE_SM as u32)
            .into()
    };

    let reset_btn = button(tr(lang, "reset"))
        .width(Length::Fill)
        .padding([8, 10])
        .style(theme::button_secondary)
        .on_press(Message::Ui(UiMessage::Reset));

    let quick_config_scroll = scrollable(components::config_panel::view(model))
        .direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .width(4.0)
                .scroller_width(4.0)
                .margin(1.0),
        ))
        .height(Length::Fill);

    let quick_config_panel = if model.ui.config_expanded {
        collapsible_fill_card(
            tr(lang, "panel_quick_config"),
            model.ui.config_expanded,
            UiMessage::ToggleConfigExpanded,
            container(quick_config_scroll).height(Length::Fill).into(),
        )
    } else {
        collapsible_card(
            tr(lang, "panel_quick_config"),
            model.ui.config_expanded,
            UiMessage::ToggleConfigExpanded,
            container(quick_config_scroll).height(Length::Fill).into(),
        )
    };

    let left_top = column![
        header,
        container(card(components::file_selector::view(model))).height(Length::Fixed(180.0)),
        quick_config_panel,
    ]
    .spacing(theme::SPACE_MD as u32);

    let blacklist_panel = if model.ui.blacklist_expanded {
        collapsible_fill_card(
            tr(lang, "panel_blacklist"),
            model.ui.blacklist_expanded,
            UiMessage::ToggleBlacklistExpanded,
            components::blacklist_editor::view(model),
        )
    } else {
        collapsible_card(
            tr(lang, "panel_blacklist"),
            model.ui.blacklist_expanded,
            UiMessage::ToggleBlacklistExpanded,
            components::blacklist_editor::view(model),
        )
    };

    let middle = column![
        card(action_bar),
        section_card(
            tr(lang, "section_summary"),
            theme::strip_stats,
            components::summary_panel::view(model)
        ),
        container(section_card(
            tr(lang, "section_progress"),
            move |th| {
                if is_processing {
                    let _ = th;
                    theme::strip_progress_pulse(model.ui.pulse_phase)
                } else {
                    theme::strip_progress(th)
                }
            },
            components::progress_area::view(model)
        ))
        .height(Length::Fill),
        card(reset_btn.into()),
    ]
    .spacing(theme::SPACE_MD as u32)
    .height(Length::Fill);

    let tab_tree_style = if model.ui.active_output_tab == OutputTab::Tree {
        theme::button_tab_active
    } else {
        theme::button_tab_inactive
    };
    let tab_merged_style = if model.ui.active_output_tab == OutputTab::MergedContent {
        theme::button_tab_active
    } else {
        theme::button_tab_inactive
    };

    let tabs = row![
        button(tr(lang, "tab_tree_preview"))
            .style(tab_tree_style)
            .on_press(Message::Ui(UiMessage::SwitchOutputTab(OutputTab::Tree))),
        button(tr(lang, "tab_merged_content"))
            .style(tab_merged_style)
            .on_press(Message::Ui(UiMessage::SwitchOutputTab(
                OutputTab::MergedContent
            ))),
    ]
    .spacing(theme::SPACE_SM as u32);

    let in_content_tab = model.ui.active_output_tab == OutputTab::MergedContent;
    let output_tools: Element<'_, Message> = row![
        icon_tool_button(
            bootstrap::clipboard().size(14).into(),
            if in_content_tab {
                tr(lang, "copy_current_page")
            } else {
                tr(lang, "copy_tree")
            },
            Some(Message::Ui(if in_content_tab {
                UiMessage::CopyContent
            } else {
                UiMessage::CopyTree
            })),
        ),
        icon_tool_button(
            bootstrap::download().size(14).into(),
            tr(lang, "download"),
            if in_content_tab {
                Some(Message::Ui(UiMessage::DownloadContent))
            } else {
                None
            },
        ),
    ]
    .spacing(theme::SPACE_SM as u32)
    .into();

    let output_title = if model.ui.active_output_tab == OutputTab::Tree {
        tr(lang, "section_tree")
    } else {
        tr(lang, "section_result")
    };

    let output_content: Element<'_, Message> = if model.ui.active_output_tab == OutputTab::Tree {
        components::tree_view::view(model)
    } else {
        components::result_viewer::view(model)
    };

    let mut right = card(
        column![
            row![tabs, Space::new().width(Length::Fill), output_tools]
                .spacing(theme::SPACE_SM as u32),
            container(text(output_title).size(14))
                .padding([theme::SPACE_XS, 10])
                .style(theme::strip_result),
            container(output_content).height(Length::Fill),
        ]
        .spacing(theme::SPACE_SM as u32)
        .height(Length::Fill)
        .into(),
    );

    if let Some(toast) = components::toast::view(model) {
        right = column![right, toast].spacing(theme::SPACE_SM as u32).into();
    }

    let body: Element<'_, Message> = if model.window_size.0 >= 1200.0 {
        let left_middle = column![
            row![
                container(left_top).width(Length::Fixed(380.0)).height(
                    if model.ui.config_expanded {
                        Length::Fill
                    } else {
                        Length::Shrink
                    }
                ),
                container(middle).width(Length::Fixed(320.0)),
            ]
            .spacing(theme::SPACE_MD as u32),
            container(blacklist_panel).height(if model.ui.blacklist_expanded {
                Length::Fill
            } else {
                Length::Shrink
            }),
        ]
        .spacing(theme::SPACE_MD as u32)
        .height(Length::Fill);

        row![
            container(left_middle)
                .width(Length::Fixed(720.0))
                .height(Length::Fill),
            container(right).width(Length::Fill).height(Length::Fill),
        ]
        .spacing(theme::SPACE_MD as u32)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if model.window_size.0 >= 900.0 {
        let left_middle = column![
            row![
                container(left_top).width(Length::FillPortion(2)).height(
                    if model.ui.config_expanded {
                        Length::Fill
                    } else {
                        Length::Shrink
                    }
                ),
                container(middle).width(Length::FillPortion(1)),
            ]
            .spacing(theme::SPACE_MD as u32),
            container(blacklist_panel).height(if model.ui.blacklist_expanded {
                Length::Fill
            } else {
                Length::Shrink
            }),
        ]
        .spacing(theme::SPACE_MD as u32)
        .height(Length::Fill);

        row![
            container(left_middle)
                .width(Length::FillPortion(2))
                .height(Length::Fill),
            container(right)
                .width(Length::FillPortion(3))
                .height(Length::Fill),
        ]
        .spacing(theme::SPACE_MD as u32)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if model.window_size.0 >= 600.0 {
        column![
            container(left_top).width(Length::Fill),
            container(blacklist_panel).height(if model.ui.blacklist_expanded {
                Length::FillPortion(1)
            } else {
                Length::Shrink
            }),
            row![
                container(middle).width(Length::FillPortion(1)),
                container(right).width(Length::FillPortion(2)),
            ]
            .height(Length::FillPortion(2))
            .spacing(theme::SPACE_MD as u32),
        ]
        .spacing(theme::SPACE_MD as u32)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        column![right, middle, left_top, blacklist_panel]
            .spacing(theme::SPACE_MD as u32)
            .width(Length::Fill)
            .into()
    };

    let mut root = column![
        container(body)
            .padding(theme::CARD_PADDING)
            .style(theme::panel_background)
            .width(Length::Fill)
            .height(Length::Fill)
    ]
    .spacing(theme::SPACE_SM as u32)
    .width(Length::Fill)
    .height(Length::Fill);

    if model.ui.show_reset_confirmation {
        root = root.push(card(
            row![
                text(tr(lang, "confirm_reset")),
                button(tr(lang, "yes"))
                    .style(theme::button_primary)
                    .on_press(Message::Ui(UiMessage::ConfirmReset)),
                button(tr(lang, "no"))
                    .style(theme::button_secondary)
                    .on_press(Message::Ui(UiMessage::CancelReset)),
            ]
            .spacing(8)
            .into(),
        ));
    }

    if model.ui.show_cancel_confirmation {
        root = root.push(card(
            row![
                text(tr(lang, "confirm_cancel")),
                button(tr(lang, "yes"))
                    .style(theme::button_primary)
                    .on_press(Message::Ui(UiMessage::ConfirmCancel)),
                button(tr(lang, "no"))
                    .style(theme::button_secondary)
                    .on_press(Message::Ui(UiMessage::CancelCancel)),
            ]
            .spacing(theme::SPACE_SM as u32)
            .into(),
        ));
    }

    container(root)
        .padding([theme::SPACE_SM, theme::SPACE_MD])
        .style(theme::app_background)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn card<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    container(content)
        .padding(theme::CARD_PADDING)
        .style(theme::card_background)
        .into()
}

fn section_card<'a, F>(
    title: &'a str,
    strip_style: F,
    content: Element<'a, Message>,
) -> Element<'a, Message>
where
    F: Fn(&iced::Theme) -> iced::widget::container::Style + 'a,
{
    card(
        column![
            container(text(title).size(14))
                .padding([theme::SPACE_XS, 10])
                .style(strip_style),
            content,
        ]
        .spacing(theme::SPACE_SM as u32)
        .into(),
    )
}

fn icon_tool_button<'a>(
    icon: Element<'a, Message>,
    tip: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let icon = container(icon)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    let mut btn = button(icon)
        .width(Length::Fixed(40.0))
        .height(Length::Fixed(34.0))
        .padding([4, 4])
        .style(theme::button_icon);
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }

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

fn collapsible_card<'a>(
    title: &'a str,
    expanded: bool,
    toggle: UiMessage,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let indicator = if expanded { "-" } else { "+" };
    let body: Element<'a, Message> = if expanded {
        content
    } else {
        container(text("")).height(Length::Shrink).into()
    };

    card(
        column![
            row![
                text(title).size(14),
                Space::new().width(Length::Fill),
                button(indicator)
                    .style(theme::button_secondary)
                    .on_press(Message::Ui(toggle)),
            ]
            .spacing(theme::SPACE_SM as u32),
            body,
        ]
        .spacing(theme::SPACE_SM as u32)
        .into(),
    )
}

fn collapsible_fill_card<'a>(
    title: &'a str,
    expanded: bool,
    toggle: UiMessage,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let indicator = if expanded { "-" } else { "+" };
    let body: Element<'a, Message> = if expanded {
        container(content).height(Length::Fill).into()
    } else {
        container(text("")).height(Length::Shrink).into()
    };

    card(
        container(
            column![
                row![
                    text(title).size(14),
                    Space::new().width(Length::Fill),
                    button(indicator)
                        .style(theme::button_secondary)
                        .on_press(Message::Ui(toggle)),
                ]
                .spacing(theme::SPACE_SM as u32),
                body,
            ]
            .spacing(theme::SPACE_SM as u32)
            .height(if expanded {
                Length::Fill
            } else {
                Length::Shrink
            }),
        )
        .height(if expanded {
            Length::Fill
        } else {
            Length::Shrink
        })
        .into(),
    )
}
