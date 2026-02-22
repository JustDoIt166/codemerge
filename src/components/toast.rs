use iced::Element;
use iced::widget::{button, column, container, progress_bar, row, text};

use crate::app::message::{Message, UiMessage};
use crate::app::model::Model;
use crate::app::theme;
use crate::utils::i18n::tr;

pub fn view(model: &Model) -> Option<Element<'_, Message>> {
    let lang = model.language;
    let toast = model.ui.toast.as_ref()?;
    let prefix = match toast.style {
        crate::app::model::ToastStyle::Success => "[OK]",
        crate::app::model::ToastStyle::Info => "[INFO]",
        crate::app::model::ToastStyle::Error => "[ERR]",
    };
    let ttl_ms = toast.duration.as_millis() as u64;
    let elapsed = model.ui.toast_elapsed_ms.min(ttl_ms.max(1));
    let ratio = if ttl_ms == 0 {
        1.0
    } else {
        1.0 - (elapsed as f32 / ttl_ms as f32)
    };

    Some(
        container(
            column![
                row![
                    text(format!(
                        "{} {}",
                        prefix,
                        tr_or_message(lang, &toast.message)
                    )),
                    button(tr(lang, "close_toast"))
                        .style(theme::button_secondary)
                        .on_press(Message::Ui(UiMessage::DismissToast)),
                ]
                .spacing(8),
                progress_bar(0.0..=1.0, ratio),
            ]
            .spacing(6),
        )
        .padding(10)
        .style(match toast.style {
            crate::app::model::ToastStyle::Success => theme::toast_success,
            crate::app::model::ToastStyle::Info => theme::toast_info,
            crate::app::model::ToastStyle::Error => theme::toast_error,
        })
        .into(),
    )
}

fn tr_or_message(lang: crate::app::model::Language, message: &str) -> String {
    let translated = tr(lang, message);
    if translated.is_empty() {
        message.to_string()
    } else {
        translated.to_string()
    }
}
