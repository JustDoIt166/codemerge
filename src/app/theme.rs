use iced::widget::{button, container};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

pub fn theme() -> Theme {
    Theme::Light
}

pub const SPACE_XS: u16 = 6;
pub const SPACE_SM: u16 = 8;
pub const SPACE_MD: u16 = 12;
pub const SPACE_LG: u16 = 16;
pub const CARD_PADDING: u16 = 12;
pub const APP_RADIUS: f32 = 18.0;
pub const PANEL_RADIUS: f32 = 16.0;
pub const CARD_RADIUS: f32 = PANEL_RADIUS - SPACE_SM as f32;
pub const STRIP_RADIUS: f32 = CARD_RADIUS - 2.0;
pub const TOAST_RADIUS: f32 = 12.0;
pub const BUTTON_RADIUS: f32 = STRIP_RADIUS;
pub const BUTTON_COMPACT_RADIUS: f32 = BUTTON_RADIUS - 2.0;

const BG_APP: Color = Color::from_rgb(0.94, 0.95, 0.97);
const BG_PANEL: Color = Color::from_rgb(0.90, 0.92, 0.95);
const BG_CARD: Color = Color::from_rgb(0.98, 0.99, 1.0);
const BG_ACCENT_SOFT: Color = Color::from_rgb(0.88, 0.94, 1.0);
const BG_CODE_SURFACE: Color = Color::from_rgb(0.10, 0.13, 0.18);
const BORDER_SOFT: Color = Color::from_rgb(0.80, 0.84, 0.90);
const SHADOW_SOFT: Color = Color::from_rgba(0.16, 0.21, 0.30, 0.18);

const PRIMARY: Color = Color::from_rgb(0.09, 0.39, 0.89);
const PRIMARY_HOVER: Color = Color::from_rgb(0.08, 0.35, 0.80);
const SECONDARY: Color = Color::from_rgb(0.82, 0.86, 0.93);
const SECONDARY_HOVER: Color = Color::from_rgb(0.76, 0.81, 0.90);
const DANGER: Color = Color::from_rgb(0.78, 0.24, 0.25);
const DANGER_HOVER: Color = Color::from_rgb(0.70, 0.21, 0.22);
const LANG_TOGGLE: Color = Color::from_rgb(0.24, 0.45, 0.86);
const LANG_TOGGLE_HOVER: Color = Color::from_rgb(0.20, 0.40, 0.78);
const LANG_TOGGLE_PRESSED: Color = Color::from_rgb(0.16, 0.34, 0.69);

const SUCCESS_BG: Color = Color::from_rgb(0.86, 0.95, 0.89);
const INFO_BG: Color = Color::from_rgb(0.86, 0.93, 1.0);
const ERROR_BG: Color = Color::from_rgb(0.98, 0.87, 0.88);
const STRIP_PROGRESS: Color = Color::from_rgb(0.86, 0.92, 1.0);
const STRIP_STATS: Color = Color::from_rgb(0.90, 0.97, 0.90);
const STRIP_TREE: Color = Color::from_rgb(0.99, 0.94, 0.86);
const STRIP_RESULT: Color = Color::from_rgb(0.92, 0.90, 0.99);
const STRIP_NEUTRAL: Color = Color::from_rgb(0.91, 0.95, 0.99);

pub fn app_background(_: &Theme) -> container::Style {
    container::Style::default().background(BG_APP).border(
        Border::default()
            .rounded(APP_RADIUS)
            .width(1.0)
            .color(BORDER_SOFT),
    )
}

pub fn panel_background(_: &Theme) -> container::Style {
    container::Style::default().background(BG_PANEL).border(
        Border::default()
            .rounded(PANEL_RADIUS)
            .width(1.0)
            .color(BORDER_SOFT),
    )
}

pub fn card_background(_: &Theme) -> container::Style {
    container::Style::default()
        .background(BG_CARD)
        .border(
            Border::default()
                .rounded(CARD_RADIUS)
                .width(1.0)
                .color(BORDER_SOFT),
        )
        .shadow(Shadow {
            color: SHADOW_SOFT,
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        })
}

pub fn accent_tile(_: &Theme) -> container::Style {
    container::Style::default()
        .background(BG_ACCENT_SOFT)
        .border(
            Border::default()
                .rounded(STRIP_RADIUS)
                .width(1.0)
                .color(BORDER_SOFT),
        )
}

pub fn code_surface(_: &Theme) -> container::Style {
    container::Style::default()
        .background(BG_CODE_SURFACE)
        .border(
            Border::default()
                .rounded(STRIP_RADIUS)
                .width(1.0)
                .color(BORDER_SOFT),
        )
}

pub fn tooltip_bubble(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from_rgb(0.16, 0.21, 0.30))
        .border(Border::default().rounded(8.0).width(1.0).color(BORDER_SOFT))
}

pub fn toast_success(_: &Theme) -> container::Style {
    toast_style(SUCCESS_BG)
}

pub fn toast_info(_: &Theme) -> container::Style {
    toast_style(INFO_BG)
}

pub fn toast_error(_: &Theme) -> container::Style {
    toast_style(ERROR_BG)
}

fn toast_style(bg: Color) -> container::Style {
    container::Style::default()
        .background(bg)
        .border(
            Border::default()
                .rounded(TOAST_RADIUS)
                .width(1.0)
                .color(BORDER_SOFT),
        )
        .shadow(Shadow {
            color: SHADOW_SOFT,
            offset: Vector::new(0.0, 1.0),
            blur_radius: 6.0,
        })
}

pub fn strip_progress(_: &Theme) -> container::Style {
    strip_style(STRIP_PROGRESS)
}

pub fn strip_progress_pulse(phase: f32) -> container::Style {
    let t = (phase * std::f32::consts::TAU).sin() * 0.5 + 0.5;
    let bg = lerp_color(STRIP_PROGRESS, Color::from_rgb(0.74, 0.86, 1.0), t);
    strip_style(bg)
}

pub fn strip_stats(_: &Theme) -> container::Style {
    strip_style(STRIP_STATS)
}

pub fn strip_tree(_: &Theme) -> container::Style {
    strip_style(STRIP_TREE)
}

pub fn strip_result(_: &Theme) -> container::Style {
    strip_style(STRIP_RESULT)
}

pub fn strip_neutral(_: &Theme) -> container::Style {
    strip_style(STRIP_NEUTRAL)
}

fn strip_style(bg: Color) -> container::Style {
    container::Style::default().background(bg).border(
        Border::default()
            .rounded(STRIP_RADIUS)
            .width(1.0)
            .color(BORDER_SOFT),
    )
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let clamp_t = t.clamp(0.0, 1.0);
    Color::from_rgba(
        a.r + (b.r - a.r) * clamp_t,
        a.g + (b.g - a.g) * clamp_t,
        a.b + (b.b - a.b) * clamp_t,
        a.a + (b.a - a.a) * clamp_t,
    )
}

pub fn button_primary(_: &Theme, status: button::Status) -> button::Style {
    button_style(
        match status {
            button::Status::Hovered => PRIMARY_HOVER,
            button::Status::Pressed => PRIMARY_HOVER,
            button::Status::Disabled => Color::from_rgb(0.65, 0.71, 0.82),
            button::Status::Active => PRIMARY,
        },
        Color::WHITE,
    )
}

pub fn button_secondary(_: &Theme, status: button::Status) -> button::Style {
    button_style(
        match status {
            button::Status::Hovered => SECONDARY_HOVER,
            button::Status::Pressed => SECONDARY_HOVER,
            button::Status::Disabled => Color::from_rgb(0.86, 0.89, 0.94),
            button::Status::Active => SECONDARY,
        },
        Color::from_rgb(0.13, 0.17, 0.24),
    )
}

pub fn button_danger(_: &Theme, status: button::Status) -> button::Style {
    button_style(
        match status {
            button::Status::Hovered => DANGER_HOVER,
            button::Status::Pressed => DANGER_HOVER,
            button::Status::Disabled => Color::from_rgb(0.81, 0.63, 0.64),
            button::Status::Active => DANGER,
        },
        Color::WHITE,
    )
}

pub fn button_tab_active(_: &Theme, status: button::Status) -> button::Style {
    button_style(
        match status {
            button::Status::Hovered => PRIMARY_HOVER,
            button::Status::Pressed => PRIMARY_HOVER,
            button::Status::Disabled => Color::from_rgb(0.65, 0.71, 0.82),
            button::Status::Active => PRIMARY,
        },
        Color::WHITE,
    )
}

pub fn button_tab_inactive(_: &Theme, status: button::Status) -> button::Style {
    button_style(
        match status {
            button::Status::Hovered => Color::from_rgb(0.72, 0.77, 0.87),
            button::Status::Pressed => Color::from_rgb(0.68, 0.73, 0.84),
            button::Status::Disabled => Color::from_rgb(0.84, 0.87, 0.93),
            button::Status::Active => Color::from_rgb(0.78, 0.82, 0.90),
        },
        Color::from_rgb(0.12, 0.16, 0.22),
    )
}

pub fn button_icon(_: &Theme, status: button::Status) -> button::Style {
    let (bg, fg, border, shadow_offset, shadow_blur) = match status {
        button::Status::Active => (
            Color::from_rgb(0.78, 0.82, 0.90),
            Color::from_rgb(0.10, 0.13, 0.18),
            Color::from_rgb(0.67, 0.73, 0.83),
            1.0,
            4.0,
        ),
        button::Status::Hovered => (
            Color::from_rgb(0.74, 0.79, 0.89),
            Color::from_rgb(0.07, 0.10, 0.16),
            Color::from_rgb(0.55, 0.64, 0.79),
            2.0,
            7.0,
        ),
        button::Status::Pressed => (
            Color::from_rgb(0.66, 0.71, 0.82),
            Color::from_rgb(0.06, 0.09, 0.14),
            Color::from_rgb(0.50, 0.58, 0.72),
            0.0,
            2.0,
        ),
        button::Status::Disabled => (
            Color::from_rgb(0.86, 0.89, 0.94),
            Color::from_rgb(0.50, 0.54, 0.62),
            Color::from_rgb(0.79, 0.83, 0.90),
            0.0,
            0.0,
        ),
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: Border::default()
            .rounded(BUTTON_RADIUS)
            .width(1.0)
            .color(border),
        shadow: Shadow {
            color: SHADOW_SOFT,
            offset: Vector::new(0.0, shadow_offset),
            blur_radius: shadow_blur,
        },
        ..button::Style::default()
    }
}

pub fn button_compact(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => SECONDARY_HOVER,
        button::Status::Pressed => SECONDARY_HOVER,
        button::Status::Disabled => Color::from_rgb(0.86, 0.89, 0.94),
        button::Status::Active => SECONDARY,
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.13, 0.17, 0.24),
        border: Border::default()
            .rounded(BUTTON_COMPACT_RADIUS)
            .width(1.0)
            .color(BORDER_SOFT),
        shadow: Shadow {
            color: SHADOW_SOFT,
            offset: Vector::new(0.0, 1.0),
            blur_radius: 3.0,
        },
        ..button::Style::default()
    }
}

pub fn button_language(_: &Theme, status: button::Status) -> button::Style {
    let (bg, border, shadow_offset, shadow_blur) = match status {
        button::Status::Active => (LANG_TOGGLE, Color::from_rgb(0.12, 0.29, 0.60), 1.0, 5.0),
        button::Status::Hovered => (
            LANG_TOGGLE_HOVER,
            Color::from_rgb(0.10, 0.25, 0.54),
            2.0,
            8.0,
        ),
        button::Status::Pressed => (
            LANG_TOGGLE_PRESSED,
            Color::from_rgb(0.08, 0.21, 0.47),
            0.0,
            2.0,
        ),
        button::Status::Disabled => (Color::from_rgb(0.65, 0.71, 0.82), BORDER_SOFT, 0.0, 0.0),
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border::default()
            .rounded(BUTTON_COMPACT_RADIUS)
            .width(1.0)
            .color(border),
        shadow: Shadow {
            color: SHADOW_SOFT,
            offset: Vector::new(0.0, shadow_offset),
            blur_radius: shadow_blur,
        },
        ..button::Style::default()
    }
}

fn button_style(bg: Color, text_color: Color) -> button::Style {
    button::Style {
        background: Some(Background::Color(bg)),
        text_color,
        border: Border::default()
            .rounded(BUTTON_RADIUS)
            .width(1.0)
            .color(BORDER_SOFT),
        shadow: Shadow {
            color: SHADOW_SOFT,
            offset: Vector::new(0.0, 1.0),
            blur_radius: 4.0,
        },
        ..button::Style::default()
    }
}
