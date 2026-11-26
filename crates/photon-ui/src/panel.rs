//! Core dock panel, card, key-value row, and status bar primitives.
//!
//! Note: The unified collapsible, closable, and drag-reorderable sidebar panel system
//! (`SidebarOrderState`, `PanelFrameSpec`, `sidebar_panel`, `draggable_panel_header`,
//! `panel_tab_bar`) lives in the `photon-panels` crate. Compatibility exports remain
//! here so `photon-ui` stays cycle-free while downstream crates migrate.

use std::rc::Rc;

use iced::widget::{Space, button, column, container, mouse_area, row, rule, slider, text};
use iced::{Alignment, Border, Color, Element, Length};

use crate::button::{self as ui_button, ButtonVariant};
use crate::icons::{self, Icon};
use crate::theme::{
    PaletteTokens, RADIUS_MD, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, card_style,
    hairline_rule_style, slider_style, surface_panel_style,
};
use crate::tooltip::{TooltipPosition, with_optional_tooltip};

pub fn dock_panel<'a, M: 'a>(
    title: impl Into<String>,
    icon: Option<Icon>,
    width: f32,
    body: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let header_row = if let Some(ic) = icon {
        row![icons::render(ic, 14.0, None), text(title.into()).size(13)]
            .spacing(6)
            .align_y(Alignment::Center)
    } else {
        row![text(title.into()).size(13)].align_y(Alignment::Center)
    };

    container(
        column![
            container(header_row).padding([10, 12]),
            rule::horizontal(1).style(hairline_rule_style),
            container(body.into()).padding(12).height(Length::Fill),
        ]
        .height(Length::Fill),
    )
    .width(Length::Fixed(width))
    .height(Length::Fill)
    .style(surface_panel_style)
    .into()
}

pub fn section_card<'a, M: 'a>(
    title: impl Into<String>,
    body: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    container(
        column![
            text(title.into()).size(12).style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
            body.into(),
        ]
        .spacing(8),
    )
    .padding(10)
    .width(Length::Fill)
    .style(card_style)
    .into()
}

/// Tab descriptor for [`panel_tab_bar`] (matching KogaView's `PanelTabs.qml`).
#[derive(Debug, Clone)]
pub struct PanelTabItem<M> {
    pub key: String,
    pub label: String,
    pub icon: Option<Icon>,
    pub active: bool,
    pub tooltip: Option<String>,
    pub on_select: M,
}

impl<M> PanelTabItem<M> {
    pub fn new(
        key: impl Into<String>,
        label: impl Into<String>,
        active: bool,
        on_select: M,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            icon: None,
            active,
            tooltip: None,
            on_select,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

/// KogaView-style dock tab strip (`Tool Options | History`, etc.).
///
/// When `compact` is `true`, labels are hidden and icons stay centered with automatic tooltip
/// fallback to the tab label.
pub fn panel_tab_bar<'a, M: Clone + 'a>(
    tabs: Vec<PanelTabItem<M>>,
    compact: bool,
) -> Element<'a, M> {
    let mut r = row![].spacing(4).align_y(Alignment::Center);

    for tab in tabs {
        let active = tab.active;
        let tip = if compact {
            Some(tab.tooltip.unwrap_or_else(|| tab.label.clone()))
        } else {
            tab.tooltip
        };

        let label_for_content = tab.label;
        let tab_icon = tab.icon;
        let content: Element<'a, M> = match (tab_icon, compact) {
            (Some(ic), true) => container(icons::render(ic, 15.0, None))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into(),
            (Some(ic), false) => container(
                row![
                    icons::render(ic, 14.0, None),
                    text(label_for_content)
                        .size(11)
                        .wrapping(text::Wrapping::None),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into(),
            (None, _) => container(
                text(label_for_content)
                    .size(11)
                    .wrapping(text::Wrapping::None),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into(),
        };

        let btn = button(content)
            .width(Length::Fill)
            .height(Length::Fixed(28.0))
            .padding([0, 8])
            .style(move |theme, status| {
                let t = PaletteTokens::for_theme(theme);
                let bg = if active {
                    t.bg_selected
                } else {
                    match status {
                        button::Status::Hovered => t.bg_hover,
                        button::Status::Pressed => t.bg_pressed,
                        _ => Color::TRANSPARENT,
                    }
                };
                let text_color = if active || matches!(status, button::Status::Hovered) {
                    t.text_primary
                } else {
                    t.text_secondary
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color,
                    border: Border {
                        color: if active { t.accent } else { Color::TRANSPARENT },
                        width: if active { 1.0 } else { 0.0 },
                        radius: RADIUS_SM.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(tab.on_select);

        r = r.push(with_optional_tooltip(btn, tip, TooltipPosition::Bottom));
    }

    container(r)
        .padding(3)
        .width(Length::Fill)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(t.bg_app.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: RADIUS_MD.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

/// Visual drop-target placement indicator for draggable right-sidebar panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DropIndicator {
    #[default]
    None,
    Before,
    After,
}

/// Configuration for [`draggable_panel_header`].
pub struct DraggablePanelHeaderSpec<M> {
    pub title: String,
    pub icon: Option<Icon>,
    pub badge: Option<String>,
    pub collapsed: bool,
    pub dragging: bool,
    pub drop_indicator: DropIndicator,
    pub drag_tooltip: Option<String>,
    pub collapse_tooltip: Option<String>,
    pub on_toggle_collapse: M,
    pub on_drag_start: M,
    pub on_drag_hover: Option<M>,
    pub on_drag_end: Option<M>,
}

fn drop_indicator_line<'a, M: 'a>(visible: bool) -> Element<'a, M> {
    if !visible {
        return Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(0.0))
            .into();
    }
    container(Space::new().width(Length::Fill).height(Length::Fixed(2.0)))
        .width(Length::Fill)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(t.accent.into()),
                border: Border {
                    radius: 1.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

/// Reusable collapsible & drag-reorderable panel header with grip handle, optional icon/badge,
/// collapse chevron, and top/bottom drop-target highlight.
pub fn draggable_panel_header<'a, M: Clone + 'a>(
    spec: DraggablePanelHeaderSpec<M>,
) -> Element<'a, M> {
    let grip_raw = container(icons::render_muted(Icon::Grip, 13.0))
        .padding([4, 3])
        .align_y(Alignment::Center);
    let mut grip_area = mouse_area(grip_raw).on_press(spec.on_drag_start);
    if let Some(end_msg) = spec.on_drag_end.clone() {
        grip_area = grip_area.on_release(end_msg);
    }
    let grip = with_optional_tooltip(grip_area, spec.drag_tooltip, TooltipPosition::Left);

    let mut header_row = row![grip].spacing(5).align_y(Alignment::Center);
    if let Some(ic) = spec.icon {
        header_row = header_row.push(icons::render(ic, 14.0, None));
    }
    header_row = header_row.push(
        text(spec.title)
            .size(12)
            .wrapping(text::Wrapping::None)
            .width(Length::Fill),
    );
    if let Some(badge) = spec.badge {
        header_row = header_row.push(text(badge).size(11).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.text_muted),
            }
        }));
    }

    let chevron = ui_button::compact_icon(
        if spec.collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        },
        ButtonVariant::Ghost,
        Some(spec.on_toggle_collapse),
    );
    header_row = header_row.push(with_optional_tooltip(
        chevron,
        spec.collapse_tooltip,
        TooltipPosition::Left,
    ));

    let dragging = spec.dragging;
    let is_target = spec.drop_indicator != DropIndicator::None;

    let bar = container(header_row)
        .padding([4, 6])
        .width(Length::Fill)
        .style(move |theme| {
            let t = PaletteTokens::for_theme(theme);
            let bg = if dragging {
                t.bg_selected
            } else if is_target {
                t.bg_hover
            } else {
                t.bg_surface
            };
            container::Style {
                background: Some(bg.into()),
                text_color: Some(t.text_primary),
                border: Border {
                    color: if dragging || is_target {
                        t.accent
                    } else {
                        t.border
                    },
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            }
        });

    let interactive_bar: Element<'a, M> =
        if spec.on_drag_hover.is_some() || spec.on_drag_end.is_some() {
            let mut ma = mouse_area(bar);
            if let Some(hover_msg) = spec.on_drag_hover {
                ma = ma.on_enter(hover_msg);
            }
            if let Some(end_msg) = spec.on_drag_end {
                ma = ma.on_release(end_msg);
            }
            ma.into()
        } else {
            bar.into()
        };

    column![
        drop_indicator_line(spec.drop_indicator == DropIndicator::Before),
        interactive_bar,
        drop_indicator_line(spec.drop_indicator == DropIndicator::After),
    ]
    .spacing(2)
    .width(Length::Fill)
    .into()
}

/// Wrap an entire panel card with drop-target hover/release handling and top/bottom drop
/// indicator bars during right-sidebar drag-and-drop reordering.
pub fn drop_target_panel<'a, M: Clone + 'a>(
    content: impl Into<Element<'a, M>>,
    indicator: DropIndicator,
    on_hover: Option<M>,
    on_release: Option<M>,
) -> Element<'a, M> {
    drop_target_panel_fill(content, indicator, false, on_hover, on_release)
}

/// Variant of [`drop_target_panel`] that optionally fills available vertical height so inner
/// scrollables shrink cleanly within split dock slots.
pub fn drop_target_panel_fill<'a, M: Clone + 'a>(
    content: impl Into<Element<'a, M>>,
    indicator: DropIndicator,
    fill_height: bool,
    on_hover: Option<M>,
    on_release: Option<M>,
) -> Element<'a, M> {
    let mut inner = column![
        drop_indicator_line(indicator == DropIndicator::Before),
        content.into(),
        drop_indicator_line(indicator == DropIndicator::After),
    ]
    .spacing(3)
    .width(Length::Fill);
    if fill_height {
        inner = inner.height(Length::Fill);
    }

    if on_hover.is_some() || on_release.is_some() {
        let mut ma = mouse_area(inner);
        if let Some(h) = on_hover {
            ma = ma.on_enter(h);
        }
        if let Some(r) = on_release {
            ma = ma.on_release(r);
        }
        ma.into()
    } else {
        inner.into()
    }
}

pub fn kv_row<'a, M: 'a>(key: impl Into<String>, value: impl Into<String>) -> Element<'a, M> {
    row![
        text(key.into())
            .size(12)
            .width(Length::Fill)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
        text(value.into()).size(12).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.text_primary),
            }
        }),
    ]
    .width(Length::Fill)
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

const STEPPER_DEFAULT_VALUE_WIDTH: f32 = 54.0;
const STEPPER_COMPACT_CHAR_LIMIT: usize = 7;

fn step_f32_value(value: f32, delta: f32, min_v: f32, max_v: f32) -> f32 {
    let next = (value + delta).clamp(min_v, max_v);
    if min_v <= 0.0 && max_v >= 0.0 && next.abs() < 1e-5 {
        0.0
    } else {
        next
    }
}

fn render_stepper_pill<'a, M: Clone + 'a>(
    value_text: String,
    value_width: Option<f32>,
    on_decrement: Option<M>,
    on_increment: Option<M>,
) -> Element<'a, M> {
    let step_btn = |sym: &'static str, on_press: Option<M>| {
        let mut btn = button(text(sym).size(TEXT_SIZE_CAPTION))
            .padding([2, 6])
            .style(|theme, status| {
                let t = PaletteTokens::for_theme(theme);
                let (bg, fg, border_color) = match status {
                    button::Status::Disabled => (
                        t.bg_surface.scale_alpha(0.55),
                        t.text_muted.scale_alpha(0.45),
                        t.border.scale_alpha(0.6),
                    ),
                    button::Status::Hovered => (t.bg_hover, t.text_primary, t.border_strong),
                    button::Status::Pressed => (t.bg_pressed, t.text_primary, t.accent),
                    button::Status::Active => (t.bg_elevated, t.text_secondary, t.border),
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: fg,
                    border: Border {
                        color: border_color,
                        width: 1.0,
                        radius: RADIUS_SM.into(),
                    },
                    ..Default::default()
                }
            });
        if let Some(msg) = on_press {
            btn = btn.on_press(msg);
        }
        btn
    };

    let auto_fixed = value_width.or_else(|| {
        (value_text.chars().count() <= STEPPER_COMPACT_CHAR_LIMIT)
            .then_some(STEPPER_DEFAULT_VALUE_WIDTH)
    });

    let mut pill = container(
        text(value_text)
            .size(TEXT_SIZE_CAPTION)
            .wrapping(text::Wrapping::None)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_primary),
                }
            }),
    )
    .padding([2, 6])
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(|theme| {
        let t = PaletteTokens::for_theme(theme);
        container::Style {
            background: Some(t.bg_app.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: RADIUS_SM.into(),
            },
            ..Default::default()
        }
    });
    if let Some(w) = auto_fixed {
        pill = pill.width(Length::Fixed(w));
    }

    row![
        step_btn("<", on_decrement),
        pill,
        step_btn(">", on_increment),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .into()
}

/// Reusable compact numeric stepper control: `< [value] >`.
///
/// Pass `None` for `on_decrement` / `on_increment` to visually disable the corresponding button
/// at the minimum or maximum bound.
pub fn numeric_stepper<'a, M: Clone + 'a>(
    value_text: impl Into<String>,
    on_decrement: Option<M>,
    on_increment: Option<M>,
) -> Element<'a, M> {
    render_stepper_pill(value_text.into(), None, on_decrement, on_increment)
}

/// Specification for [`slider_stepper`]: a two-line slider + numeric stepper template.
///
/// - **Line 1**: `<Label>` on the left, `< [number] >` stepper pill on the right.
/// - **Line 2**: Full-width slider (`Length::Fill`).
pub struct SliderStepperSpec<'a, M> {
    pub label: String,
    pub value: f32,
    pub range: std::ops::RangeInclusive<f32>,
    pub step: f32,
    pub value_text: String,
    pub value_width: Option<f32>,
    pub on_change: Box<dyn Fn(f32) -> M + 'a>,
}

/// Two-line slider + numeric stepper built from a [`SliderStepperSpec`].
pub fn slider_stepper<'a, M: Clone + 'a>(spec: SliderStepperSpec<'a, M>) -> Element<'a, M> {
    let min_v = *spec.range.start();
    let max_v = *spec.range.end();
    let safe_step = if spec.step.is_finite() && spec.step > 0.0 {
        spec.step
    } else {
        1.0
    };
    let clamped_val = spec.value.clamp(min_v, max_v);
    let cb = Rc::new(spec.on_change);
    let cb_slider = Rc::clone(&cb);

    let can_dec = clamped_val > min_v + 1e-6;
    let can_inc = clamped_val < max_v - 1e-6;
    let prev_val = step_f32_value(clamped_val, -safe_step, min_v, max_v);
    let next_val = step_f32_value(clamped_val, safe_step, min_v, max_v);

    let header = row![
        text(spec.label)
            .size(TEXT_SIZE_CONTROL)
            .width(Length::Fill)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_secondary),
                }
            }),
        render_stepper_pill(
            spec.value_text,
            spec.value_width,
            can_dec.then(|| cb(prev_val)),
            can_inc.then(|| cb(next_val)),
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let slider_el = slider(spec.range, clamped_val, move |v| cb_slider(v))
        .step(safe_step)
        .style(slider_style)
        .width(Length::Fill);

    column![header, slider_el]
        .spacing(4)
        .width(Length::Fill)
        .into()
}

/// Two-line slider + stepper row:
/// - **Line 1**: `<Label>` on the left, `< [value] >` stepper on the right.
/// - **Line 2**: Full-width slider (`Length::Fill`).
pub fn slider_control_row<'a, M: Clone + 'a>(
    label: impl Into<String>,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    value_text: impl Into<String>,
    on_change: impl Fn(f32) -> M + 'a,
) -> Element<'a, M> {
    slider_stepper(SliderStepperSpec {
        label: label.into(),
        value,
        range,
        step,
        value_text: value_text.into(),
        value_width: None,
        on_change: Box::new(on_change),
    })
}

/// Semantic alias for [`slider_control_row`]: two-line `<Label>` + `< [number] >` stepper
/// above a full-width `Length::Fill` slider.
pub fn slider_stepper_row<'a, M: Clone + 'a>(
    label: impl Into<String>,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    value_text: impl Into<String>,
    on_change: impl Fn(f32) -> M + 'a,
) -> Element<'a, M> {
    slider_control_row(label, value, range, step, value_text, on_change)
}

/// Integer (`i32`) variant of [`slider_stepper_row`] for discrete controls such as RGB channels,
/// export quality, brush/padding pixel counts, and percentages.
pub fn int_slider_stepper_row<'a, M: Clone + 'a>(
    label: impl Into<String>,
    value: i32,
    range: std::ops::RangeInclusive<i32>,
    step: i32,
    value_text: impl Into<String>,
    on_change: impl Fn(i32) -> M + 'a,
) -> Element<'a, M> {
    let min_v = *range.start();
    let max_v = *range.end();
    let safe_step = step.max(1);
    let clamped_val = value.clamp(min_v, max_v);
    let cb = Rc::new(on_change);
    let cb_slider = Rc::clone(&cb);

    let can_dec = clamped_val > min_v;
    let can_inc = clamped_val < max_v;
    let prev_val = clamped_val.saturating_sub(safe_step).clamp(min_v, max_v);
    let next_val = clamped_val.saturating_add(safe_step).clamp(min_v, max_v);

    let header = row![
        text(label.into())
            .size(TEXT_SIZE_CONTROL)
            .width(Length::Fill)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_secondary),
                }
            }),
        render_stepper_pill(
            value_text.into(),
            None,
            can_dec.then(|| cb(prev_val)),
            can_inc.then(|| cb(next_val)),
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let slider_el = slider(range, clamped_val, move |v| cb_slider(v))
        .step(safe_step)
        .style(slider_style)
        .width(Length::Fill);

    column![header, slider_el]
        .spacing(4)
        .width(Length::Fill)
        .into()
}

pub fn status_bar<'a, M: 'a>(
    left_status: impl Into<String>,
    right_meta: impl Into<String>,
) -> Element<'a, M> {
    column![
        rule::horizontal(1).style(hairline_rule_style),
        container(
            row![
                text(left_status.into())
                    .size(12)
                    .width(Length::Fill)
                    .style(|theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(t.text_secondary),
                        }
                    }),
                text(right_meta.into()).size(12).style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
            ]
            .width(Length::Fill)
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([7, 12])
        .width(Length::Fill)
        .style(surface_panel_style),
    ]
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_f32_clamps_bounds_and_snaps_negative_zero() {
        assert_eq!(step_f32_value(0.05, -0.05, -1.0, 1.0), 0.0);
        assert_eq!(step_f32_value(-0.05, 0.05, -1.0, 1.0), 0.0);
        assert_eq!(step_f32_value(0.0, -0.05, 0.0, 1.0), 0.0);
        assert_eq!(step_f32_value(1.0, 0.05, 0.0, 1.0), 1.0);
        assert!((step_f32_value(0.5, 0.1, 0.0, 1.0) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn two_line_slider_stepper_helpers_build_elements() {
        let _f32_row: Element<'_, f32> =
            slider_control_row("Exposure", 0.0, -2.0..=2.0, 0.05, "0.00", |v| v);
        let _alias_row: Element<'_, f32> =
            slider_stepper_row("Opacity", 1.0, 0.0..=1.0, 0.05, "100%", |v| v);
        let _int_row: Element<'_, i32> =
            int_slider_stepper_row("Quality", 90, 10..=100, 5, "90%", |v| v);
        let _stepper: Element<'_, i32> = numeric_stepper("100%", Some(95), None);
        let _long_label_stepper: Element<'_, i32> =
            numeric_stepper("Auto (100%)", Some(95), Some(105));
    }
}
