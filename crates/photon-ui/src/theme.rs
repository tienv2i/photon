//! Studio color tokens, typography helpers, and `iced` widget styles.

use iced::widget::overlay::menu as iced_menu;
use iced::widget::{container, pick_list, rule, slider, text_input};
use iced::{Border, Color, Shadow, Theme, Vector, color};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UiMode {
    Light,
    #[default]
    Dark,
}

impl UiMode {
    pub fn from_iced(theme: &Theme) -> Self {
        match theme {
            Theme::Light => Self::Light,
            _ => Self::Dark,
        }
    }
}

/// Neutral studio palette designed for photo/visual applications (zero color cast).
#[derive(Debug, Clone, Copy)]
pub struct PaletteTokens {
    pub bg_app: Color,
    pub bg_canvas: Color,
    pub bg_surface: Color,
    pub bg_elevated: Color,
    pub bg_hover: Color,
    pub bg_pressed: Color,
    pub bg_selected: Color,
    pub border: Color,
    pub border_strong: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_text: Color,
    pub danger: Color,
    pub success: Color,
    pub warning: Color,
    pub backdrop: Color,
}

impl PaletteTokens {
    pub const DARK: Self = Self {
        bg_app: color!(0x141619),
        bg_canvas: color!(0x101214),
        bg_surface: color!(0x1b1e23),
        bg_elevated: color!(0x242830),
        bg_hover: color!(0x2c313b),
        bg_pressed: color!(0x353b47),
        bg_selected: color!(0x22364d),
        border: color!(0x2a2e36),
        border_strong: color!(0x3a404c),
        text_primary: color!(0xe8ecf2),
        text_secondary: color!(0xb4bcc8),
        text_muted: color!(0x7a8290),
        accent: color!(0x3b82f6),
        accent_hover: color!(0x60a5fa),
        accent_text: color!(0xffffff),
        danger: color!(0xef4444),
        success: color!(0x22c55e),
        warning: color!(0xf59e0b),
        backdrop: Color::from_rgba(0.0, 0.0, 0.0, 0.58),
    };

    pub const LIGHT: Self = Self {
        bg_app: color!(0xf3f4f6),
        bg_canvas: color!(0xe9ecf0),
        bg_surface: color!(0xffffff),
        bg_elevated: color!(0xf8f9fb),
        bg_hover: color!(0xedeff3),
        bg_pressed: color!(0xe2e5eb),
        bg_selected: color!(0xdbeafe),
        border: color!(0xdbe0e8),
        border_strong: color!(0xc2c9d6),
        text_primary: color!(0x181c22),
        text_secondary: color!(0x4b5563),
        text_muted: color!(0x6b7280),
        accent: color!(0x2563eb),
        accent_hover: color!(0x1d4ed8),
        accent_text: color!(0xffffff),
        danger: color!(0xdc2626),
        success: color!(0x16a34a),
        warning: color!(0xd97706),
        backdrop: Color::from_rgba(0.06, 0.08, 0.12, 0.42),
    };

    pub fn for_theme(theme: &Theme) -> Self {
        match UiMode::from_iced(theme) {
            UiMode::Dark => Self::DARK,
            UiMode::Light => Self::LIGHT,
        }
    }
}

pub const RADIUS_SM: f32 = 5.0;
pub const RADIUS_MD: f32 = 8.0;
pub const RADIUS_LG: f32 = 12.0;

/// Standard dialog and panel title font size (`14px`).
pub const TEXT_SIZE_DIALOG_TITLE: f32 = 14.0;
/// Standard control, body, input, button, checkbox, radio, and pick_list font size (`12px`).
pub const TEXT_SIZE_CONTROL: f32 = 12.0;
/// Standard caption, shortcut, and metadata font size (`11px`).
pub const TEXT_SIZE_CAPTION: f32 = 11.0;

pub fn app_shell_style(theme: &Theme) -> container::Style {
    let t = PaletteTokens::for_theme(theme);
    container::Style {
        background: Some(t.bg_app.into()),
        text_color: Some(t.text_primary),
        ..Default::default()
    }
}

pub fn canvas_backdrop_style(theme: &Theme) -> container::Style {
    let t = PaletteTokens::for_theme(theme);
    container::Style {
        background: Some(t.bg_canvas.into()),
        text_color: Some(t.text_primary),
        ..Default::default()
    }
}

pub fn surface_panel_style(theme: &Theme) -> container::Style {
    let t = PaletteTokens::for_theme(theme);
    container::Style {
        background: Some(t.bg_surface.into()),
        text_color: Some(t.text_primary),
        border: Border {
            color: t.border,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn card_style(theme: &Theme) -> container::Style {
    let t = PaletteTokens::for_theme(theme);
    container::Style {
        background: Some(t.bg_elevated.into()),
        text_color: Some(t.text_primary),
        border: Border {
            color: t.border,
            width: 1.0,
            radius: RADIUS_MD.into(),
        },
        ..Default::default()
    }
}

pub fn popover_style(theme: &Theme) -> container::Style {
    let t = PaletteTokens::for_theme(theme);
    container::Style {
        background: Some(t.bg_surface.into()),
        text_color: Some(t.text_primary),
        border: Border {
            color: t.border_strong,
            width: 1.0,
            radius: RADIUS_MD.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..Default::default()
    }
}

pub fn tooltip_style(theme: &Theme) -> container::Style {
    let t = PaletteTokens::for_theme(theme);
    container::Style {
        background: Some(t.bg_elevated.into()),
        text_color: Some(t.text_primary),
        border: Border {
            color: t.border_strong,
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.30),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 10.0,
        },
        ..Default::default()
    }
}

pub fn dialog_frame_style(theme: &Theme) -> container::Style {
    let t = PaletteTokens::for_theme(theme);
    container::Style {
        background: Some(t.bg_surface.into()),
        text_color: Some(t.text_primary),
        border: Border {
            color: t.border_strong,
            width: 1.0,
            radius: RADIUS_LG.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 28.0,
        },
        ..Default::default()
    }
}

pub fn modal_backdrop_style(theme: &Theme) -> container::Style {
    let t = PaletteTokens::for_theme(theme);
    container::Style {
        background: Some(t.backdrop.into()),
        ..Default::default()
    }
}

pub fn input_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let t = PaletteTokens::for_theme(theme);
    let border_color = match status {
        text_input::Status::Focused { .. } => t.accent,
        text_input::Status::Hovered => t.border_strong,
        _ => t.border,
    };
    text_input::Style {
        background: t.bg_elevated.into(),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
        icon: t.text_muted,
        placeholder: t.text_muted,
        value: t.text_primary,
        selection: t.bg_selected,
    }
}

pub fn slider_style(theme: &Theme, status: slider::Status) -> slider::Style {
    let t = PaletteTokens::for_theme(theme);
    let active_color = match status {
        slider::Status::Hovered | slider::Status::Dragged => t.accent_hover,
        slider::Status::Active => t.accent,
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (active_color.into(), t.border_strong.into()),
            width: 4.0,
            border: Border {
                radius: 2.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 6.5 },
            background: active_color.into(),
            border_width: 2.0,
            border_color: t.bg_surface,
        },
    }
}

pub fn hairline_rule_style(theme: &Theme) -> rule::Style {
    let t = PaletteTokens::for_theme(theme);
    rule::Style {
        color: t.border,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

pub fn pick_list_style(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let t = PaletteTokens::for_theme(theme);
    let active = matches!(
        status,
        pick_list::Status::Hovered | pick_list::Status::Opened { .. }
    );
    pick_list::Style {
        text_color: t.text_primary,
        placeholder_color: t.text_muted,
        handle_color: t.text_secondary,
        background: if active { t.bg_hover } else { t.bg_surface }.into(),
        border: Border {
            color: if active { t.accent } else { t.border_strong },
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
    }
}

pub fn pick_list_menu_style(theme: &Theme) -> iced_menu::Style {
    let t = PaletteTokens::for_theme(theme);
    iced_menu::Style {
        background: t.bg_elevated.into(),
        border: Border {
            color: t.border_strong,
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
        text_color: t.text_primary,
        selected_text_color: Color::WHITE,
        selected_background: t.accent.into(),
        shadow: Shadow::default(),
    }
}
