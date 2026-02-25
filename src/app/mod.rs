pub mod message;
pub mod model;
pub mod theme;
pub mod update;
pub mod view;

use iced::{Element, Event, Subscription, Task, Theme, window};

use crate::app::message::{Message, UiMessage};
use crate::app::model::{Model, ProcessingState};

pub struct App;

impl App {
    pub fn new() -> (Model, Task<Message>) {
        let cfg = crate::utils::config_store::load_config();
        let mut model = Model::default();
        model.language = cfg.language;
        model.options = cfg.options;
        model.folder_blacklist = cfg.folder_blacklist;
        model.ext_blacklist = cfg.ext_blacklist;
        let task = update::refresh_preflight(&mut model);
        (model, task)
    }

    pub fn update(model: &mut Model, message: Message) -> Task<Message> {
        update::update(model, message)
    }

    pub fn view(model: &Model) -> Element<'_, Message> {
        view::view(model)
    }

    pub fn theme(_: &Model) -> Theme {
        theme::theme()
    }

    pub fn title(_: &Model) -> String {
        "CodeMerge".to_string()
    }

    pub fn subscription(model: &Model) -> Subscription<Message> {
        let needs_tick = matches!(model.processing_state, ProcessingState::InProgress { .. })
            || model.ui.toast.is_some()
            || model.ui.config_save_due_ms.is_some();
        let resize = iced::event::listen().filter_map(|event| match event {
            Event::Window(window::Event::Resized(size)) => {
                Some(Message::Ui(UiMessage::Resize(size.width, size.height)))
            }
            _ => None,
        });

        if needs_tick {
            let tick = iced::time::every(std::time::Duration::from_millis(update::TICK_MS))
                .map(|_| Message::Tick);
            Subscription::batch([tick, resize])
        } else {
            resize
        }
    }
}
