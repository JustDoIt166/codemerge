pub mod state;
mod workspace;

use anyhow::Result;
use gpui::{AppContext, Application, WindowOptions};
use gpui_component::Root;

pub fn run() {
    let app = Application::new();
    app.run(move |cx| {
        gpui_component::init(cx);
        cx.activate(true);
        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = workspace::Workspace::view(window, cx);
                cx.new(|cx| Root::new(view, window, cx))
            })?;
            Result::<(), anyhow::Error>::Ok(())
        })
        .detach();
    });
}
