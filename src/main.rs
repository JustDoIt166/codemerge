#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use codemerge::app;

fn main() -> iced::Result {
    iced::application(app::App::new, app::App::update, app::App::view)
        .font(iced_aw::iced_fonts::BOOTSTRAP_FONT_BYTES)
        .title(app::App::title)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .run()
}
