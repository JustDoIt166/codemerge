mod assets;
mod models;
pub mod perf;
mod preview_model;
mod result_model;
mod selection_model;
pub mod state;
mod workspace;

use anyhow::Result;
use gpui::{AppContext, Application, WindowDecorations, WindowOptions};
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
    build_main_window_options(
        cfg!(target_os = "windows"),
        cfg!(target_os = "linux"),
        cfg!(target_os = "macos"),
        custom_titlebar_enabled(),
    )
}

pub(crate) fn custom_titlebar_enabled() -> bool {
    !std::env::var("CODEMERGE_SYSTEM_TITLEBAR")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn build_main_window_options(
    is_windows: bool,
    is_linux: bool,
    is_macos: bool,
    prefer_custom_titlebar: bool,
) -> WindowOptions {
    let mut options = WindowOptions::default();

    if prefer_custom_titlebar && (is_windows || is_macos) {
        options.titlebar = Some(gpui_component::TitleBar::title_bar_options());
    }

    if prefer_custom_titlebar && is_linux {
        options.window_decorations = Some(WindowDecorations::Client);
    }

    options
}

#[cfg(test)]
mod tests {
    use super::build_main_window_options;
    use gpui::WindowDecorations;

    #[test]
    fn main_window_options_enable_custom_titlebar_on_windows_and_macos() {
        let windows = build_main_window_options(true, false, false, true);
        let macos = build_main_window_options(false, false, true, true);

        let windows_titlebar = windows.titlebar.expect("windows titlebar");
        assert!(windows_titlebar.appears_transparent);
        assert!(windows_titlebar.traffic_light_position.is_some());

        let macos_titlebar = macos.titlebar.expect("macos titlebar");
        assert!(macos_titlebar.appears_transparent);
        assert!(macos_titlebar.traffic_light_position.is_some());
    }

    #[test]
    fn main_window_options_prefer_client_decorations_on_linux() {
        let linux = build_main_window_options(false, true, false, true);

        assert_eq!(linux.window_decorations, Some(WindowDecorations::Client));
        assert!(
            !linux.titlebar.expect("linux titlebar").appears_transparent,
            "linux should keep the default system titlebar configuration"
        );
    }

    #[test]
    fn main_window_options_keep_default_decorations_on_other_platforms() {
        let other = build_main_window_options(false, false, false, false);

        assert!(other.window_decorations.is_none());
        assert!(
            !other
                .titlebar
                .expect("default titlebar")
                .appears_transparent,
            "unknown platforms should keep default titlebar settings"
        );
    }

    #[test]
    fn main_window_options_can_disable_custom_titlebar_on_macos() {
        let macos = build_main_window_options(false, false, true, false);
        assert!(
            !macos
                .titlebar
                .expect("default macos titlebar")
                .appears_transparent,
            "macOS fallback should keep the default system titlebar configuration"
        );
    }
}
