//! Standardized button styles, toolbar controls, and reusable button constructors.

use iced::widget::{Button, button, container, row, text};
use iced::{Alignment, Border, Color, Element, Length, Padding, Theme};

use crate::icons::{self, Icon};
use crate::theme::{PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL};
use crate::tooltip::{TooltipPosition, with_optional_tooltip, with_tooltip};

/// Uniform height (px) for all top-toolbar buttons and workspace switcher tabs.
pub const TOOLBAR_CONTROL_HEIGHT: f32 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Ghost,
    Danger,
    Tab { active: bool },
    ListRow { active: bool },
}

pub fn style_for(variant: ButtonVariant) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = PaletteTokens::for_theme(theme);
        let base = match variant {
            ButtonVariant::Primary => button::Style {
                background: Some(t.accent.into()),
                text_color: t.accent_text,
                border: Border {
                    color: t.accent,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            },
            ButtonVariant::Secondary => button::Style {
                background: Some(t.bg_elevated.into()),
                text_color: t.text_primary,
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            },
            ButtonVariant::Ghost => button::Style {
                background: None,
                text_color: t.text_secondary,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            },
            ButtonVariant::Danger => button::Style {
                background: Some(t.danger.into()),
                text_color: t.accent_text,
                border: Border {
                    color: t.danger,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            },
            ButtonVariant::Tab { active } => button::Style {
                background: Some(if active { t.accent } else { Color::TRANSPARENT }.into()),
                text_color: if active {
                    t.accent_text
                } else {
                    t.text_secondary
                },
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            },
            ButtonVariant::ListRow { active } => button::Style {
                background: Some(
                    if active {
                        t.bg_selected
                    } else {
                        Color::TRANSPARENT
                    }
                    .into(),
                ),
                text_color: t.text_primary,
                border: Border {
                    color: if active { t.accent } else { Color::TRANSPARENT },
                    width: if active { 1.0 } else { 0.0 },
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            },
        };

        match status {
            button::Status::Active => base,
            button::Status::Hovered => match variant {
                ButtonVariant::Primary => button::Style {
                    background: Some(t.accent_hover.into()),
                    ..base
                },
                ButtonVariant::Tab { active: true } => base,
                ButtonVariant::Ghost | ButtonVariant::Tab { active: false } => button::Style {
                    background: Some(t.bg_hover.into()),
                    text_color: t.text_primary,
                    ..base
                },
                ButtonVariant::Secondary | ButtonVariant::ListRow { .. } => button::Style {
                    background: Some(t.bg_hover.into()),
                    border: Border {
                        color: t.border_strong,
                        ..base.border
                    },
                    ..base
                },
                ButtonVariant::Danger => button::Style {
                    background: Some(t.danger.scale_alpha(0.88).into()),
                    ..base
                },
            },
            button::Status::Pressed => button::Style {
                background: Some(t.bg_pressed.into()),
                ..base
            },
            button::Status::Disabled => match variant {
                ButtonVariant::Tab { active: true } => base,
                _ => button::Style {
                    background: base.background.map(|b| b.scale_alpha(0.45)),
                    text_color: base.text_color.scale_alpha(0.45),
                    ..base
                },
            },
        }
    }
}

pub fn labeled<'a, M: Clone + 'a>(
    label: impl Into<String>,
    icon: Option<Icon>,
    variant: ButtonVariant,
    on_press: Option<M>,
) -> Button<'a, M> {
    let label_str = label.into();
    let content: Element<'a, M> = if let Some(ic) = icon {
        let icon_color = match variant {
            ButtonVariant::Primary
            | ButtonVariant::Danger
            | ButtonVariant::Tab { active: true } => Some(Color::WHITE),
            _ => None,
        };
        row![
            icons::render(ic, 14.0, icon_color),
            text(label_str).size(TEXT_SIZE_CONTROL)
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else {
        text(label_str).size(TEXT_SIZE_CONTROL).into()
    };

    let mut b = button(content).padding([6, 10]).style(style_for(variant));
    if let Some(msg) = on_press {
        b = b.on_press(msg);
    }
    b
}

pub fn icon_only<'a, M: Clone + 'a>(
    icon: Icon,
    variant: ButtonVariant,
    on_press: Option<M>,
) -> Button<'a, M> {
    let icon_color = match (variant, on_press.is_some()) {
        (_, false) => None,
        (
            ButtonVariant::Primary | ButtonVariant::Danger | ButtonVariant::Tab { active: true },
            _,
        ) => Some(Color::WHITE),
        _ => None,
    };
    let rendered = if on_press.is_some() {
        icons::render(icon, 16.0, icon_color)
    } else {
        icons::render_disabled(icon, 16.0)
    };
    let mut b = button(rendered).padding([6, 8]).style(style_for(variant));
    if let Some(msg) = on_press {
        b = b.on_press(msg);
    }
    b
}

pub fn compact_icon<'a, M: Clone + 'a>(
    icon: Icon,
    variant: ButtonVariant,
    on_press: Option<M>,
) -> Button<'a, M> {
    let icon_color = match (variant, on_press.is_some()) {
        (_, false) => None,
        (
            ButtonVariant::Primary | ButtonVariant::Danger | ButtonVariant::Tab { active: true },
            _,
        ) => Some(Color::WHITE),
        _ => None,
    };
    let rendered = if on_press.is_some() {
        icons::render(icon, 12.0, icon_color)
    } else {
        icons::render_disabled(icon, 12.0)
    };
    let mut b = button(rendered).padding([4, 4]).style(style_for(variant));
    if let Some(msg) = on_press {
        b = b.on_press(msg);
    }
    b
}

/// Builder for an icon button with uniform sizing, variant styling, and optional tooltip.
pub struct IconButton<'a, M> {
    icon: Icon,
    variant: ButtonVariant,
    icon_size: f32,
    padding: Padding,
    fixed_width: Option<f32>,
    fixed_height: Option<f32>,
    on_press: Option<M>,
    tooltip: Option<String>,
    tooltip_position: TooltipPosition,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a, M: Clone + 'a> IconButton<'a, M> {
    pub fn new(icon: Icon) -> Self {
        Self {
            icon,
            variant: ButtonVariant::Ghost,
            icon_size: 16.0,
            padding: Padding::from([6, 8]),
            fixed_width: None,
            fixed_height: None,
            on_press: None,
            tooltip: None,
            tooltip_position: TooltipPosition::Bottom,
            _marker: std::marker::PhantomData,
        }
    }

    /// Configure as a compact 12px icon button (`padding: [4, 4]`).
    pub fn compact(mut self) -> Self {
        self.icon_size = 12.0;
        self.padding = Padding::from([4, 4]);
        self
    }

    /// Configure as a uniform toolbar icon button (`height: 30px`, centered).
    pub fn toolbar(mut self) -> Self {
        self.icon_size = 16.0;
        self.padding = Padding::from([0, 8]);
        self.fixed_height = Some(TOOLBAR_CONTROL_HEIGHT);
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, icon_size: f32) -> Self {
        self.icon_size = icon_size;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.fixed_width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.fixed_height = Some(height);
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn on_press(mut self, msg: M) -> Self {
        self.on_press = Some(msg);
        self
    }

    pub fn maybe_on_press(mut self, msg: Option<M>) -> Self {
        self.on_press = msg;
        self
    }

    pub fn tooltip(mut self, tip: impl Into<String>) -> Self {
        self.tooltip = Some(tip.into());
        self
    }

    pub fn maybe_tooltip(mut self, tip: Option<String>) -> Self {
        self.tooltip = tip;
        self
    }

    pub fn tooltip_position(mut self, position: TooltipPosition) -> Self {
        self.tooltip_position = position;
        self
    }

    pub fn into_element(self) -> Element<'a, M> {
        let icon_color = match (self.variant, self.on_press.is_some()) {
            (_, false) => None,
            (
                ButtonVariant::Primary
                | ButtonVariant::Danger
                | ButtonVariant::Tab { active: true },
                _,
            ) => Some(Color::WHITE),
            _ => None,
        };
        let rendered = if self.on_press.is_some() {
            icons::render(self.icon, self.icon_size, icon_color)
        } else {
            icons::render_disabled(self.icon, self.icon_size)
        };
        let content: Element<'a, M> = if self.fixed_height.is_some() || self.fixed_width.is_some() {
            container(rendered)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        } else {
            rendered.into()
        };

        let mut b = button(content)
            .padding(self.padding)
            .style(style_for(self.variant));
        if let Some(w) = self.fixed_width {
            b = b.width(Length::Fixed(w));
        }
        if let Some(h) = self.fixed_height {
            b = b.height(Length::Fixed(h));
        }
        if let Some(msg) = self.on_press {
            b = b.on_press(msg);
        }
        with_optional_tooltip(b, self.tooltip, self.tooltip_position)
    }
}

impl<'a, M: Clone + 'a> From<IconButton<'a, M>> for Element<'a, M> {
    fn from(value: IconButton<'a, M>) -> Self {
        value.into_element()
    }
}

/// Icon-only button wrapped with a styled tooltip below the button.
pub fn icon_only_tipped<'a, M: Clone + 'a>(
    icon: Icon,
    variant: ButtonVariant,
    on_press: Option<M>,
    tip: impl Into<String>,
) -> Element<'a, M> {
    with_tooltip(icon_only(icon, variant, on_press), tip)
}

/// Compact icon button wrapped with a styled tooltip.
pub fn compact_icon_tipped<'a, M: Clone + 'a>(
    icon: Icon,
    variant: ButtonVariant,
    on_press: Option<M>,
    tip: impl Into<String>,
) -> Element<'a, M> {
    with_tooltip(compact_icon(icon, variant, on_press), tip)
}

/// Labeled button wrapped with a styled tooltip.
pub fn labeled_tipped<'a, M: Clone + 'a>(
    label: impl Into<String>,
    icon: Option<Icon>,
    variant: ButtonVariant,
    on_press: Option<M>,
    tip: impl Into<String>,
) -> Element<'a, M> {
    with_tooltip(labeled(label, icon, variant, on_press), tip)
}

/// Toolbar icon button with uniform `TOOLBAR_CONTROL_HEIGHT`, consistent active toggle styling,
/// and optional tooltip.
pub fn toolbar_icon<'a, M: Clone + 'a>(
    icon: Icon,
    active: bool,
    on_press: Option<M>,
    tip: Option<String>,
) -> Element<'a, M> {
    let variant = if active {
        ButtonVariant::Tab { active: true }
    } else {
        ButtonVariant::Ghost
    };
    IconButton::new(icon)
        .toolbar()
        .variant(variant)
        .maybe_on_press(on_press)
        .maybe_tooltip(tip)
        .into_element()
}

/// Toolbar labeled button with uniform `TOOLBAR_CONTROL_HEIGHT` and optional tooltip.
pub fn toolbar_labeled<'a, M: Clone + 'a>(
    label: impl Into<String>,
    icon: Option<Icon>,
    variant: ButtonVariant,
    on_press: Option<M>,
    tip: Option<String>,
) -> Element<'a, M> {
    let label_str = label.into();
    let content: Element<'a, M> = if let Some(ic) = icon {
        let icon_color = match variant {
            ButtonVariant::Primary
            | ButtonVariant::Danger
            | ButtonVariant::Tab { active: true } => Some(Color::WHITE),
            _ => None,
        };
        row![
            icons::render(ic, 14.0, icon_color),
            text(label_str).size(TEXT_SIZE_CONTROL)
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else {
        text(label_str).size(TEXT_SIZE_CONTROL).into()
    };

    let mut b = button(
        container(content)
            .height(Length::Fill)
            .align_y(Alignment::Center),
    )
    .height(Length::Fixed(TOOLBAR_CONTROL_HEIGHT))
    .padding([0, 10])
    .style(style_for(variant));
    if let Some(msg) = on_press {
        b = b.on_press(msg);
    }
    with_optional_tooltip(b, tip, TooltipPosition::Bottom)
}

pub fn list_row<'a, M: Clone + 'a>(
    icon: Icon,
    label: impl Into<String>,
    trailing: Option<String>,
    active: bool,
    on_press: Option<M>,
) -> Button<'a, M> {
    let mut r = row![
        icons::render(icon, 14.0, None),
        container(
            text(label.into())
                .size(TEXT_SIZE_CONTROL)
                .width(Length::Fill)
                .wrapping(iced::widget::text::Wrapping::None)
        )
        .width(Length::Fill)
        .clip(true)
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .width(Length::Fill);
    if let Some(meta) = trailing {
        r = r
            .push(iced::widget::space::horizontal())
            .push(text(meta).size(TEXT_SIZE_CAPTION));
    }
    let mut b = button(r)
        .padding([7, 10])
        .width(Length::Fill)
        .style(style_for(ButtonVariant::ListRow { active }));
    if let Some(msg) = on_press {
        b = b.on_press(msg);
    }
    b
}
