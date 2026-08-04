//! Theme palette and button styles for the iced GUI.

use iced::widget::button;
use iced::{Background, Border, Color, Theme};

/// Rounded corner radius shared by all app buttons.
pub const BUTTON_RADIUS: f32 = 8.0;

/// Palette for custom-painted dashboard chrome (follows light/dark).
#[derive(Clone)]
pub struct ThemePaint {
    pub dark: bool,
    #[allow(dead_code)]
    pub panel: Color,
    pub card: Color,
    pub card_stroke: Color,
    pub chart_bg: Color,
    pub chart_grid: Color,
    pub chart_axis: Color,
    pub text_strong: Color,
    pub text_muted: Color,
    pub track: Color,
}

impl ThemePaint {
    pub fn get(dark: bool) -> Self {
        if dark {
            Self {
                dark: true,
                panel: Color::from_rgb(0.11, 0.125, 0.15),
                card: Color::from_rgb(0.14, 0.165, 0.196),
                card_stroke: Color::from_rgb(0.204, 0.235, 0.275),
                chart_bg: Color::from_rgb(0.125, 0.141, 0.165),
                chart_grid: Color::from_rgb(0.188, 0.212, 0.243),
                chart_axis: Color::from_rgb(0.216, 0.243, 0.282),
                text_strong: Color::from_rgb(0.86, 0.882, 0.902),
                text_muted: Color::from_rgb(0.51, 0.549, 0.588),
                track: Color::from_rgb(0.165, 0.188, 0.22),
            }
        } else {
            Self {
                dark: false,
                panel: Color::from_rgb(0.961, 0.969, 0.98),
                card: Color::WHITE,
                card_stroke: Color::from_rgb(0.824, 0.847, 0.878),
                chart_bg: Color::from_rgb(0.925, 0.941, 0.961),
                chart_grid: Color::from_rgb(0.824, 0.847, 0.878),
                chart_axis: Color::from_rgb(0.745, 0.776, 0.816),
                text_strong: Color::from_rgb(0.118, 0.141, 0.173),
                text_muted: Color::from_rgb(0.392, 0.431, 0.478),
                track: Color::from_rgb(0.863, 0.886, 0.918),
            }
        }
    }
}

pub fn status_ok_color(dark: bool) -> Color {
    if dark {
        Color::from_rgb(0.706, 0.784, 0.706)
    } else {
        Color::from_rgb(0.55, 0.706, 0.55)
    }
}

pub fn status_err_color() -> Color {
    Color::from_rgb(0.863, 0.549, 0.471)
}

pub fn disclaimer_color() -> Color {
    Color::from_rgb(0.627, 0.549, 0.392)
}

pub fn elevated_ok() -> Color {
    Color::from_rgb(0.314, 0.706, 0.471)
}

pub fn elevated_warn() -> Color {
    Color::from_rgb(0.863, 0.471, 0.314)
}

pub fn on_color(dark: bool) -> Color {
    if dark {
        Color::from_rgb(0.863, 0.549, 0.275)
    } else {
        Color::from_rgb(0.784, 0.353, 0.275)
    }
}

pub fn off_color(dark: bool) -> Color {
    if dark {
        Color::from_rgb(0.353, 0.745, 0.569)
    } else {
        Color::from_rgb(0.275, 0.588, 0.431)
    }
}

fn is_dark(theme: &Theme) -> bool {
    matches!(theme, Theme::Dark)
}

fn rounded(bg: Color, text: Color) -> button::Style {
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: text,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: BUTTON_RADIUS.into(),
        },
        ..button::Style::default()
    }
}

fn with_status(active: button::Style, hover_bg: Color, status: button::Status) -> button::Style {
    match status {
        button::Status::Active | button::Status::Pressed => active,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(hover_bg)),
            ..active
        },
        button::Status::Disabled => button::Style {
            background: active
                .background
                .map(|b| b.scale_alpha(0.45)),
            text_color: active.text_color.scale_alpha(0.45),
            ..active
        },
    }
}

/// Main actions — greenish fill, white label, rounded.
pub fn btn_primary(theme: &Theme, status: button::Status) -> button::Style {
    let (bg, hover) = if is_dark(theme) {
        (
            Color::from_rgb(0.27, 0.47, 0.35), // ~70,120,90
            Color::from_rgb(0.33, 0.55, 0.42),
        )
    } else {
        (
            Color::from_rgb(0.32, 0.52, 0.40),
            Color::from_rgb(0.26, 0.45, 0.34),
        )
    };
    with_status(rounded(bg, Color::WHITE), hover, status)
}

/// Complementary actions — muted egui-like chrome, rounded.
pub fn btn_secondary(theme: &Theme, status: button::Status) -> button::Style {
    let (bg, hover, text, border) = if is_dark(theme) {
        (
            Color::from_rgb(0.22, 0.24, 0.28),
            Color::from_rgb(0.28, 0.31, 0.36),
            Color::from_rgb(0.88, 0.90, 0.92),
            Color::from_rgb(0.32, 0.35, 0.40),
        )
    } else {
        (
            Color::from_rgb(0.90, 0.91, 0.93),
            Color::from_rgb(0.84, 0.86, 0.89),
            Color::from_rgb(0.16, 0.18, 0.22),
            Color::from_rgb(0.78, 0.80, 0.84),
        )
    };
    let mut base = rounded(bg, text);
    base.border = Border {
        color: border,
        width: 1.0,
        radius: BUTTON_RADIUS.into(),
    };
    with_status(base, hover, status)
}

/// Destructive / armed confirm — warm red, rounded.
pub fn btn_danger(theme: &Theme, status: button::Status) -> button::Style {
    let (bg, hover) = if is_dark(theme) {
        (
            Color::from_rgb(0.55, 0.27, 0.22), // ~140,70,55
            Color::from_rgb(0.65, 0.32, 0.26),
        )
    } else {
        (
            Color::from_rgb(0.72, 0.35, 0.28),
            Color::from_rgb(0.62, 0.28, 0.22),
        )
    };
    with_status(rounded(bg, Color::WHITE), hover, status)
}
