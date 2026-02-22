use iced::widget::{checkbox, column, container, pick_list, radio, text};
use iced::{Element, Length};

use crate::app::message::{ConfigMessage, Message};
use crate::app::model::{Model, OutputFormat, ProcessingMode};
use crate::app::theme;
use crate::utils::i18n::tr;

const FORMATS: [OutputFormat; 4] = [
    OutputFormat::Default,
    OutputFormat::Xml,
    OutputFormat::PlainText,
    OutputFormat::Markdown,
];

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Default => write!(
                f,
                "{}",
                tr(crate::app::model::Language::Zh, "format_default")
            ),
            OutputFormat::Xml => write!(f, "{}", tr(crate::app::model::Language::Zh, "format_xml")),
            OutputFormat::PlainText => {
                write!(f, "{}", tr(crate::app::model::Language::Zh, "format_plain"))
            }
            OutputFormat::Markdown => write!(
                f,
                "{}",
                tr(crate::app::model::Language::Zh, "format_markdown")
            ),
        }
    }
}

pub fn view(model: &Model) -> Element<'_, Message> {
    let lang = model.language;
    let content = column![
        container(text(tr(lang, "section_options")).size(16))
            .padding([theme::SPACE_XS, 10])
            .style(theme::strip_neutral),
        container(
            checkbox(model.options.compress)
                .label(tr(lang, "compress"))
                .on_toggle(|v| Message::Config(ConfigMessage::ToggleCompress(v))),
        )
        .width(Length::Fill),
        container(
            checkbox(model.options.use_gitignore)
                .label(tr(lang, "use_gitignore"))
                .on_toggle(|v| Message::Config(ConfigMessage::ToggleUseGitignore(v))),
        )
        .width(Length::Fill),
        container(
            checkbox(model.options.ignore_git)
                .label(tr(lang, "ignore_git"))
                .on_toggle(|v| Message::Config(ConfigMessage::ToggleIgnoreGit(v))),
        )
        .width(Length::Fill),
        container(
            checkbox(model.dedupe_exact_path)
                .label(tr(lang, "dedupe_exact_path"))
                .on_toggle(|v| Message::Config(ConfigMessage::ToggleDedupe(v))),
        )
        .width(Length::Fill),
        column![
            text(format!("{}:", tr(lang, "format"))),
            pick_list(FORMATS, Some(model.options.output_format), |v| {
                Message::Config(ConfigMessage::SetOutputFormat(v))
            })
            .width(Length::Fill),
        ]
        .spacing(theme::SPACE_XS as u32)
        .width(Length::Fill),
        column![
            text(format!("{}:", tr(lang, "mode"))),
            container(radio(
                tr(lang, "mode_full"),
                ProcessingMode::Full,
                Some(model.options.mode),
                |v| Message::Config(ConfigMessage::SetMode(v))
            ),)
            .width(Length::Fill),
            container(radio(
                tr(lang, "mode_tree_only"),
                ProcessingMode::TreeOnly,
                Some(model.options.mode),
                |v| Message::Config(ConfigMessage::SetMode(v))
            ),)
            .width(Length::Fill),
        ]
        .spacing(theme::SPACE_XS as u32)
        .width(Length::Fill),
    ]
    .spacing(theme::SPACE_XS as u32);

    container(content).width(Length::Fill).into()
}
