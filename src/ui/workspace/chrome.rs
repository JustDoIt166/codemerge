use gpui::{
    AnyElement, App, Context, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement as _, Styled, Window, WindowControlArea, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable, Size, StyledExt as _, TITLE_BAR_HEIGHT, TitleBar,
    button::Button, h_flex,
};

use super::Workspace;
use super::model::{self, WorkspaceChromeTone, WorkspaceChromeViewModel};

impl Workspace {
    pub(super) fn render_window_chrome(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chrome_mode = model::resolve_window_chrome_mode(
            window.window_decorations(),
            cfg!(target_os = "windows"),
            cfg!(target_os = "linux"),
            cfg!(target_os = "macos"),
        );
        let chrome = self.build_workspace_chrome_view_model(cx);

        match chrome_mode {
            model::WindowChromeMode::CustomTitleBar => {
                self.render_custom_title_bar(window, &chrome, cx)
            }
            model::WindowChromeMode::CompactHeaderFallback => {
                self.render_compact_header(&chrome, cx).into_any_element()
            }
        }
    }

    fn build_workspace_chrome_view_model(&self, cx: &App) -> WorkspaceChromeViewModel {
        let language = self.language(cx);
        let merged_file_size_hint = self.merged_file_size_hint(cx);
        let process = self.process.read(cx);
        model::build_workspace_chrome_view_model(process.state(), language, merged_file_size_hint)
    }

    fn merged_file_size_hint(&self, cx: &App) -> Option<String> {
        self.result
            .read(cx)
            .state()
            .result
            .as_ref()
            .and_then(|result| result.merged_content_path.as_ref())
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|metadata| super::view::format_size(metadata.len()))
    }

    fn render_custom_title_bar(
        &mut self,
        window: &mut Window,
        chrome: &WorkspaceChromeViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if cfg!(target_os = "windows") {
            return self.render_windows_title_bar(window, chrome, cx);
        }

        TitleBar::new()
            .bg(cx.theme().background)
            .border_color(cx.theme().border)
            .child(self.render_chrome_content(chrome, true, cx))
            .into_any_element()
    }

    fn render_compact_header(
        &mut self,
        chrome: &WorkspaceChromeViewModel,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div().px_4().pt_4().child(
            div()
                .rounded(px(14.))
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .px_4()
                .py_3()
                .child(self.render_chrome_content(chrome, false, cx)),
        )
    }

    fn render_chrome_content(
        &mut self,
        chrome: &WorkspaceChromeViewModel,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .min_w(px(0.))
            .justify_between()
            .items_center()
            .gap_3()
            .child(self.render_chrome_leading_content(chrome, compact, cx))
            .child(self.render_chrome_actions(chrome, cx))
    }

    fn render_chrome_leading_content(
        &self,
        chrome: &WorkspaceChromeViewModel,
        compact: bool,
        cx: &App,
    ) -> impl IntoElement {
        h_flex()
            .flex_1()
            .min_w(px(0.))
            .items_center()
            .gap_3()
            .child(self.render_brand_mark(cx))
            .child(
                div()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .whitespace_nowrap()
                    .child(chrome.title.clone()),
            )
            .child(self.render_status_capsule(chrome, compact, cx))
    }

    fn render_language_button(
        &self,
        chrome: &WorkspaceChromeViewModel,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Button::new("toggle-language-chrome")
            .outline()
            .compact()
            .with_size(Size::Small)
            .icon(IconName::Globe)
            .label(chrome.language_button_label.clone())
            .on_click(cx.listener(Self::toggle_language))
    }

    fn render_repository_button(
        &self,
        chrome: &WorkspaceChromeViewModel,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Button::new("open-repository-chrome")
            .outline()
            .compact()
            .with_size(Size::Small)
            .icon(IconName::GitHub)
            .tooltip(chrome.repository_tooltip.clone())
            .on_click(cx.listener(Self::open_repository))
    }

    fn render_version_badge(
        &self,
        chrome: &WorkspaceChromeViewModel,
        cx: &App,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .px_2()
            .py_1()
            .rounded(px(999.))
            .border_1()
            .border_color(cx.theme().border.opacity(0.82))
            .bg(cx.theme().secondary.opacity(0.48))
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .whitespace_nowrap()
                    .text_color(cx.theme().muted_foreground)
                    .child(chrome.version_label.clone()),
            )
    }

    fn render_chrome_actions(
        &mut self,
        chrome: &WorkspaceChromeViewModel,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .child(self.render_version_badge(chrome, cx))
            .child(self.render_repository_button(chrome, cx))
            .child(self.render_language_button(chrome, cx))
    }

    fn render_windows_title_bar(
        &mut self,
        window: &mut Window,
        chrome: &WorkspaceChromeViewModel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .flex_shrink_0()
            .h(TITLE_BAR_HEIGHT)
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .id("windows-title-bar-drag")
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .px_3()
                    .items_center()
                    .gap_3()
                    // GPUI 0.2.2 does not implement `start_window_move` on Windows, so
                    // dragging must use native non-client hit testing instead.
                    .window_control_area(WindowControlArea::Drag)
                    .child(self.render_chrome_leading_content(chrome, true, cx)),
            )
            .child(
                div()
                    .h_full()
                    .px_2()
                    .flex()
                    .items_center()
                    .child(self.render_chrome_actions(chrome, cx)),
            )
            .child(self.render_windows_window_controls(window, cx))
            .into_any_element()
    }

    fn render_windows_window_controls(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .h_full()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .px_2()
            .child(self.render_windows_window_button(
                "window-minimize",
                IconName::WindowMinimize,
                false,
                Self::minimize_window_chrome,
                cx,
            ))
            .child(self.render_windows_window_button(
                "window-maximize",
                match model::resolve_window_zoom_action(
                    window.is_maximized(),
                    window.is_fullscreen(),
                ) {
                    model::WindowZoomAction::Restore => IconName::WindowRestore,
                    model::WindowZoomAction::Maximize => IconName::WindowMaximize,
                },
                false,
                Self::toggle_zoom_window_chrome,
                cx,
            ))
            .child(self.render_windows_window_button(
                "window-close",
                IconName::WindowClose,
                true,
                Self::close_window_chrome,
                cx,
            ))
    }

    fn render_windows_window_button(
        &self,
        id: &'static str,
        icon: IconName,
        danger: bool,
        on_click: fn(&mut Self, &gpui::ClickEvent, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover_fg = if danger {
            cx.theme().danger_foreground
        } else {
            cx.theme().secondary_foreground
        };
        let hover_bg = if danger {
            cx.theme().danger
        } else {
            cx.theme().secondary_hover
        };
        let button_bg = cx.theme().secondary.opacity(0.24);
        let button_border = cx.theme().border.opacity(0.82);
        let hover_border = if danger {
            cx.theme().danger.opacity(0.34)
        } else {
            cx.theme().accent.opacity(0.22)
        };
        let active_bg = if danger {
            cx.theme().danger_active
        } else {
            cx.theme().secondary_active
        };
        let active_border = if danger {
            cx.theme().danger.opacity(0.42)
        } else {
            cx.theme().accent.opacity(0.28)
        };

        div()
            .id(id)
            .flex()
            .w(px(28.))
            .h(px(28.))
            .flex_shrink_0()
            .justify_center()
            .content_center()
            .items_center()
            .rounded(px(10.))
            .border_1()
            .border_color(button_border)
            .bg(button_bg)
            .text_color(cx.theme().foreground.opacity(0.92))
            .when(cx.theme().shadow, |this| this.shadow_xs())
            .hover(|style| {
                style
                    .bg(hover_bg)
                    .text_color(hover_fg)
                    .border_color(hover_border)
            })
            .active(|style| {
                style
                    .bg(active_bg)
                    .text_color(hover_fg)
                    .border_color(active_border)
            })
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(cx.listener(on_click))
            .child(Icon::new(icon).with_size(Size::Small))
            .into_any_element()
    }

    fn render_brand_mark(&self, cx: &App) -> impl IntoElement {
        div()
            .flex()
            .w(px(24.))
            .h(px(24.))
            .rounded(px(8.))
            .bg(cx.theme().primary)
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::GalleryVerticalEnd)
                    .text_color(cx.theme().primary_foreground)
                    .with_size(Size::Small),
            )
    }

    fn render_status_capsule(
        &self,
        chrome: &WorkspaceChromeViewModel,
        compact: bool,
        cx: &App,
    ) -> impl IntoElement {
        let (bg, border, label_fg) = chrome_tone_palette(chrome.status_tone, cx);
        let max_message_width = if compact { px(260.) } else { px(420.) };

        h_flex()
            .min_w(px(0.))
            .max_w(px(520.))
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .rounded(px(999.))
            .border_1()
            .border_color(border)
            .bg(bg)
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .whitespace_nowrap()
                    .text_color(label_fg)
                    .child(chrome.status_label.clone()),
            )
            .child(
                div()
                    .min_w(px(0.))
                    .max_w(max_message_width)
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(chrome.status_message.clone()),
            )
    }
}

fn chrome_tone_palette(tone: WorkspaceChromeTone, cx: &App) -> (Hsla, Hsla, Hsla) {
    match tone {
        WorkspaceChromeTone::Neutral => (
            cx.theme().secondary.opacity(0.75),
            cx.theme().border,
            cx.theme().foreground.opacity(0.85),
        ),
        WorkspaceChromeTone::Accent => (
            cx.theme().accent.opacity(0.14),
            cx.theme().accent.opacity(0.28),
            cx.theme().accent,
        ),
        WorkspaceChromeTone::Success => (
            cx.theme().primary.opacity(0.16),
            cx.theme().primary.opacity(0.28),
            cx.theme().primary,
        ),
        WorkspaceChromeTone::Warning => (
            cx.theme().warning.opacity(0.18),
            cx.theme().warning.opacity(0.34),
            cx.theme().warning,
        ),
        WorkspaceChromeTone::Danger => (
            cx.theme().danger.opacity(0.16),
            cx.theme().danger.opacity(0.3),
            cx.theme().danger,
        ),
    }
}
