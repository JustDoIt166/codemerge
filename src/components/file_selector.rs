use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};

use crate::app::message::{FileMessage, Message};
use crate::app::model::Model;
use crate::app::theme;
use crate::utils::i18n::tr;

pub fn view(model: &Model) -> Element<'_, Message> {
    let lang = model.language;
    let mut list = column![].spacing(6);
    for (idx, f) in model.selected_files.iter().enumerate() {
        let icon = file_icon(&f.name);
        list = list.push(
            row![
                text(format!("{icon} {} ({})", f.name, human_size(f.size))).size(14),
                button(tr(lang, "remove_tag"))
                    .style(theme::button_compact)
                    .on_press(Message::File(FileMessage::RemoveFile(idx)))
            ]
            .spacing(8),
        );
    }

    let content = column![
        container(text(tr(lang, "section_files")).size(16))
            .padding([theme::SPACE_XS, 10])
            .style(theme::strip_neutral),
        row![
            button(tr(lang, "folder"))
                .style(theme::button_compact)
                .on_press(Message::File(FileMessage::SelectFolder)),
            button(tr(lang, "files"))
                .style(theme::button_compact)
                .on_press(Message::File(FileMessage::SelectFiles)),
            button(tr(lang, "gitignore"))
                .style(theme::button_compact)
                .on_press(Message::File(FileMessage::SelectGitignore)),
            button(tr(lang, "apply"))
                .style(theme::button_compact)
                .on_press(Message::File(FileMessage::ApplyGitignore)),
            button(tr(lang, "clear"))
                .style(theme::button_compact)
                .on_press(Message::File(FileMessage::ClearAllFiles)),
        ]
        .spacing(8),
        text(match &model.selected_folder {
            Some(p) => format!("{}{}", tr(lang, "folder_label"), p.display()),
            None => format!("{}{}", tr(lang, "folder_label"), tr(lang, "none")),
        }),
        text(format!(
            "{}{}",
            tr(lang, "files_label"),
            model.selected_files.len()
        )),
        scrollable(list).height(Length::Fixed(88.0)),
    ]
    .spacing(8);

    container(content).width(Length::Fill).into()
}

fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let v = bytes as f64;
    if v >= GB {
        format!("{:.2} GB", v / GB)
    } else if v >= MB {
        format!("{:.1} MB", v / MB)
    } else if v >= KB {
        format!("{:.1} KB", v / KB)
    } else {
        format!("{bytes} B")
    }
}

fn file_icon(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".rs")
        || lower.ends_with(".ts")
        || lower.ends_with(".js")
        || lower.ends_with(".py")
        || lower.ends_with(".go")
        || lower.ends_with(".java")
    {
        "[code]"
    } else if lower.ends_with(".md") || lower.ends_with(".txt") {
        "[text]"
    } else if lower.ends_with(".json") || lower.ends_with(".yaml") || lower.ends_with(".toml") {
        "[cfg]"
    } else {
        "[file]"
    }
}
