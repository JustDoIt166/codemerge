mod assets;
mod models;
pub mod perf;
mod preview_model;
mod result_model;
mod selection_model;
pub mod state;
mod workspace;

use anyhow::Result;
use gpui::{AppContext, Application, WindowOptions};
use gpui_component::Root;

pub fn run() {
    let app = Application::new().with_assets(assets::AppAssets);
    app.run(move |cx| {
        gpui_component::init(cx);
        cx.activate(true);
        cx.spawn(async move |cx| {
            cx.open_window(main_window_options(), |window, cx| {
                #[cfg(debug_assertions)]
                window.toggle_inspector(cx);

                let view = workspace::Workspace::view(window, cx);
                cx.new(|cx| Root::new(view, window, cx))
            })?;
            Result::<(), anyhow::Error>::Ok(())
        })
        .detach();
    });
}

fn main_window_options() -> WindowOptions {
    let mut options = WindowOptions::default();

    #[cfg(target_os = "windows")]
    {
        options.titlebar = Some(gpui_component::TitleBar::title_bar_options());
    }

    #[cfg(target_os = "linux")]
    {
        options.window_decorations = Some(gpui::WindowDecorations::Client);
    }

    options
}
