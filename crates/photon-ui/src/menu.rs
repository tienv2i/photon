//! Reusable popover menus, shortcut-hinted items, separators, and compact menubars.

use iced::widget::{
    Space, button, column, container, mouse_area, opaque, responsive, row, rule, scrollable, stack,
    text,
};
use iced::{Alignment, Border, Color, Element, Length, Padding, Size};

use crate::icons::{self, Icon};
use crate::theme::{
    PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, hairline_rule_style,
    popover_style,
};

pub enum MenuEntry<'a, M> {
    Header(String),
    Item(MenuItem<'a, M>),
    IconRow(Vec<MenuIconButton<'a, M>>),
    Separator,
}

impl<'a, M> MenuEntry<'a, M> {
    pub fn map<N>(self, mut f: impl FnMut(M) -> N) -> MenuEntry<'a, N> {
        match self {
            Self::Header(h) => MenuEntry::Header(h),
            Self::Item(item) => MenuEntry::Item(item.map(f)),
            Self::IconRow(buttons) => {
                MenuEntry::IconRow(buttons.into_iter().map(|b| b.map(&mut f)).collect())
            }
            Self::Separator => MenuEntry::Separator,
        }
    }
}

pub struct MenuIconButton<'a, M> {
    pub icon: Icon,
    pub tooltip: String,
    pub active: bool,
    pub enabled: bool,
    pub on_press: M,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a, M> MenuIconButton<'a, M> {
    pub fn new(icon: Icon, tooltip: impl Into<String>, on_press: M) -> Self {
        Self {
            icon,
            tooltip: tooltip.into(),
            active: false,
            enabled: true,
            on_press,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn map<N>(self, f: impl FnOnce(M) -> N) -> MenuIconButton<'a, N> {
        MenuIconButton {
            icon: self.icon,
            tooltip: self.tooltip,
            active: self.active,
            enabled: self.enabled,
            on_press: f(self.on_press),
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct MenuItem<'a, M> {
    pub icon: Option<Icon>,
    pub label: String,
    pub shortcut: Option<String>,
    pub checked: bool,
    pub enabled: bool,
    pub submenu: bool,
    pub on_hover: Option<M>,
    pub on_press: M,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a, M> MenuItem<'a, M> {
    pub fn new(label: impl Into<String>, on_press: M) -> Self {
        Self {
            icon: None,
            label: label.into(),
            shortcut: None,
            checked: false,
            enabled: true,
            submenu: false,
            on_hover: None,
            on_press,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn submenu(mut self, submenu: bool) -> Self {
        self.submenu = submenu;
        self
    }

    pub fn on_hover(mut self, on_hover: M) -> Self {
        self.on_hover = Some(on_hover);
        self
    }

    pub fn map<N>(self, mut f: impl FnMut(M) -> N) -> MenuItem<'a, N> {
        MenuItem {
            icon: self.icon,
            label: self.label,
            shortcut: self.shortcut,
            checked: self.checked,
            enabled: self.enabled,
            submenu: self.submenu,
            on_hover: self.on_hover.map(&mut f),
            on_press: f(self.on_press),
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct MenubarTrigger<M> {
    pub label: String,
    pub active: bool,
    pub on_press: M,
}

fn render_menubar_trigger<'a, M: Clone + 'a>(
    trigger: MenubarTrigger<M>,
    on_hover: Option<M>,
) -> Element<'a, M> {
    let active = trigger.active;
    let btn = button(text(trigger.label).size(TEXT_SIZE_CONTROL))
        .padding([5, 9])
        .on_press(trigger.on_press)
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
            button::Style {
                background: Some(bg.into()),
                text_color: if active {
                    t.text_primary
                } else {
                    t.text_secondary
                },
                border: Border {
                    radius: RADIUS_SM.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        });

    if let Some(hover_msg) = on_hover {
        mouse_area(btn).on_enter(hover_msg).into()
    } else {
        btn.into()
    }
}

pub fn menubar_row<'a, M: Clone + 'a>(triggers: Vec<MenubarTrigger<M>>) -> Element<'a, M> {
    let mut r = row![].spacing(2).align_y(Alignment::Center);
    for trigger in triggers {
        r = r.push(render_menubar_trigger(trigger, None));
    }
    r.into()
}

/// Menubar row supporting hover-to-switch (`on_hover: Some(msg)`) while a menu is already open.
pub fn menubar_row_hoverable<'a, M: Clone + 'a>(
    triggers: Vec<(MenubarTrigger<M>, Option<M>)>,
) -> Element<'a, M> {
    let mut r = row![].spacing(2).align_y(Alignment::Center);
    for (trigger, on_hover) in triggers {
        r = r.push(render_menubar_trigger(trigger, on_hover));
    }
    r.into()
}

/// Estimate the vertical height (px) of a menu built from `entries`.
pub fn estimate_menu_height<M>(entries: &[MenuEntry<'_, M>]) -> f32 {
    let mut h = 12.0_f32;
    for entry in entries {
        match entry {
            MenuEntry::Header(_) => h += 22.0,
            MenuEntry::Item(_) => h += 31.0,
            MenuEntry::IconRow(_) => h += 34.0,
            MenuEntry::Separator => h += 9.0,
        }
    }
    h
}

/// Estimate the rendered height (px) of a menu including `column![].spacing(2)` gaps.
pub fn estimate_menu_height_with_spacing<M>(entries: &[MenuEntry<'_, M>]) -> f32 {
    estimate_menu_height(entries) + entries.len().saturating_sub(1) as f32 * 2.0 + 4.0
}

pub fn menu_panel<'a, M: Clone + 'a>(items: Vec<MenuItem<'a, M>>, width: f32) -> Element<'a, M> {
    menu_panel_entries(items.into_iter().map(MenuEntry::Item).collect(), width)
}

pub fn menu_panel_entries<'a, M: Clone + 'a>(
    entries: Vec<MenuEntry<'a, M>>,
    width: f32,
) -> Element<'a, M> {
    let mut col = column![].spacing(2);
    for entry in entries {
        match entry {
            MenuEntry::Header(label) => {
                col = col.push(
                    container(text(label).size(TEXT_SIZE_CAPTION).style(|theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(t.text_muted),
                        }
                    }))
                    .padding([4, 10]),
                );
            }
            MenuEntry::Separator => {
                col = col.push(
                    container(rule::horizontal(1).style(hairline_rule_style)).padding([3, 4]),
                );
            }
            MenuEntry::IconRow(buttons) => {
                let mut r = row![].spacing(4).align_y(Alignment::Center);
                for btn in buttons {
                    let el = crate::button::toolbar_icon(
                        btn.icon,
                        btn.active,
                        btn.enabled.then_some(btn.on_press),
                        Some(btn.tooltip),
                    );
                    r = r.push(
                        container(el)
                            .width(Length::FillPortion(1))
                            .align_x(iced::alignment::Horizontal::Center),
                    );
                }
                col = col.push(
                    container(r)
                        .padding([4, 6])
                        .width(Length::Fill)
                        .style(|theme| {
                            let t = PaletteTokens::for_theme(theme);
                            container::Style {
                                background: Some(t.bg_surface.into()),
                                border: Border {
                                    radius: RADIUS_SM.into(),
                                    width: 1.0,
                                    color: t.border,
                                },
                                ..Default::default()
                            }
                        }),
                );
            }
            MenuEntry::Item(item) => {
                let enabled = item.enabled;
                let mut r = row![].spacing(8).align_y(Alignment::Center);
                if let Some(ic) = item.icon {
                    let icon_el = if enabled {
                        icons::render(ic, 14.0, None)
                    } else {
                        icons::render_disabled(ic, 14.0)
                    };
                    r = r.push(icon_el);
                }
                r = r.push(
                    text(item.label)
                        .size(TEXT_SIZE_CONTROL)
                        .style(move |theme| {
                            let t = PaletteTokens::for_theme(theme);
                            text::Style {
                                color: Some(if enabled {
                                    t.text_primary
                                } else {
                                    t.text_muted.scale_alpha(0.75)
                                }),
                            }
                        }),
                );
                r = r.push(iced::widget::space::horizontal());
                if let Some(sc) = item.shortcut {
                    r = r.push(text(sc).size(TEXT_SIZE_CAPTION).style(move |theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(if enabled {
                                t.text_muted
                            } else {
                                t.text_muted.scale_alpha(0.60)
                            }),
                        }
                    }));
                }
                if item.checked {
                    let check_el = if enabled {
                        icons::render(Icon::Check, 13.0, None)
                    } else {
                        icons::render_disabled(Icon::Check, 13.0)
                    };
                    r = r.push(check_el);
                }
                if item.submenu {
                    let arrow_el = if enabled {
                        icons::render(Icon::ChevronRight, 13.0, None)
                    } else {
                        icons::render_disabled(Icon::ChevronRight, 13.0)
                    };
                    r = r.push(arrow_el);
                }

                let mut btn =
                    button(r)
                        .width(Length::Fill)
                        .padding([6, 10])
                        .style(|theme, status| {
                            let t = PaletteTokens::for_theme(theme);
                            let (bg, fg) = match status {
                                button::Status::Disabled => {
                                    (Color::TRANSPARENT, t.text_muted.scale_alpha(0.75))
                                }
                                button::Status::Hovered => (t.bg_hover, t.text_primary),
                                button::Status::Pressed => (t.bg_pressed, t.text_primary),
                                _ => (Color::TRANSPARENT, t.text_primary),
                            };
                            button::Style {
                                background: Some(bg.into()),
                                text_color: fg,
                                border: Border {
                                    radius: RADIUS_SM.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        });
                if enabled {
                    btn = btn.on_press(item.on_press);
                }
                let entry_el: Element<'a, M> = if enabled && let Some(hover_msg) = item.on_hover {
                    mouse_area(btn).on_enter(hover_msg).into()
                } else {
                    btn.into()
                };
                col = col.push(entry_el);
            }
        }
    }

    container(col)
        .padding(6)
        .width(Length::Fixed(width))
        .style(popover_style)
        .into()
}

/// Overlay a top-right anchored menu over `base` content; clicking outside triggers `on_dismiss`.
pub fn with_top_right_menu<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    open: bool,
    menu: impl Into<Element<'a, M>>,
    on_dismiss: M,
) -> Element<'a, M> {
    with_top_right_menu_at(base, open, 12.0, 48.0, menu, on_dismiss)
}

/// Overlay a top-right anchored menu at `(right_offset, top_offset)` over `base` content,
/// leaving the top bar strip `0..top_offset` uncovered so toolbar triggers remain clickable.
pub fn with_top_right_menu_at<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    open: bool,
    right_offset: f32,
    top_offset: f32,
    menu: impl Into<Element<'a, M>>,
    on_dismiss: M,
) -> Element<'a, M> {
    if !open {
        return base.into();
    }
    let dismiss_below_bar = mouse_area(
        container(iced::widget::space::horizontal())
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(on_dismiss.clone())
    .on_right_press(on_dismiss);
    let dismiss_layer = column![
        Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(top_offset.max(0.0))),
        dismiss_below_bar,
    ]
    .width(Length::Fill)
    .height(Length::Fill);
    let clamped_menu = opaque(scrollable(menu.into()).height(Length::Shrink));
    let positioned = container(clamped_menu)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Top)
        .padding(Padding {
            top: top_offset,
            right: right_offset.max(8.0),
            bottom: 12.0,
            left: 12.0,
        });

    stack![base.into(), dismiss_layer, positioned]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Overlay a top-left anchored menu (for a menubar dropdown) over `base` content.
pub fn with_anchored_menu<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    open: bool,
    left_offset: f32,
    top_offset: f32,
    menu: impl Into<Element<'a, M>>,
    on_dismiss: M,
) -> Element<'a, M> {
    if !open {
        return base.into();
    }
    // Leave the top menubar band (`0..top_offset`) uncovered so clicks and hover events
    // reach menubar triggers directly without being swallowed by the dismiss backdrop.
    let dismiss_below_bar = mouse_area(
        container(iced::widget::space::horizontal())
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(on_dismiss.clone())
    .on_right_press(on_dismiss);
    let dismiss_layer = column![
        Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(top_offset.max(0.0))),
        dismiss_below_bar,
    ]
    .width(Length::Fill)
    .height(Length::Fill);
    let clamped_menu = opaque(scrollable(menu.into()).height(Length::Shrink));
    let positioned = container(clamped_menu)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Left)
        .align_y(iced::alignment::Vertical::Top)
        .padding(Padding {
            top: top_offset,
            right: 12.0,
            bottom: 12.0,
            left: left_offset,
        });

    stack![base.into(), dismiss_layer, positioned]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Overlay a bottom-right anchored menu at `(right_offset, bottom_offset)` over `base` content.
pub fn with_bottom_right_menu_at<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    open: bool,
    right_offset: f32,
    bottom_offset: f32,
    menu: impl Into<Element<'a, M>>,
    on_dismiss: M,
) -> Element<'a, M> {
    if !open {
        return base.into();
    }
    let dismiss_layer = mouse_area(
        container(iced::widget::space::horizontal())
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(on_dismiss.clone())
    .on_right_press(on_dismiss);
    let clamped_menu = opaque(scrollable(menu.into()).height(Length::Shrink));
    let positioned = container(clamped_menu)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .padding(Padding {
            top: 12.0,
            right: right_offset.max(8.0),
            bottom: bottom_offset.max(8.0),
            left: 12.0,
        });

    stack![base.into(), dismiss_layer, positioned]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn clamp_context_menu_origin(
    anchor_x: f32,
    anchor_y: f32,
    viewport_size: Size,
    menu_width: f32,
    estimated_height: f32,
) -> (f32, f32) {
    let max_left = (viewport_size.width - menu_width - 8.0).max(8.0);
    let clamped_h = estimated_height.min((viewport_size.height - 16.0).max(60.0));
    let max_top = (viewport_size.height - clamped_h - 8.0).max(8.0);
    let left = anchor_x.clamp(8.0, max_left);
    let top = anchor_y.clamp(8.0, max_top);
    (left, top)
}

/// Overlay a cursor-anchored context menu clamped inside `viewport_size`; clicking or
/// right-clicking outside dismisses via `on_dismiss`.
#[allow(clippy::too_many_arguments)]
pub fn with_context_menu<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    open: bool,
    anchor_x: f32,
    anchor_y: f32,
    viewport_size: Size,
    menu_width: f32,
    estimated_height: f32,
    menu: impl Into<Element<'a, M>>,
    on_dismiss: M,
) -> Element<'a, M> {
    with_context_menu_and_submenu(
        base,
        open,
        anchor_x,
        anchor_y,
        viewport_size,
        menu_width,
        estimated_height,
        menu,
        None,
        on_dismiss,
    )
}

/// Overlay a cursor-anchored context menu with an optional secondary submenu panel
/// positioned beside the primary menu and clamped inside `viewport_size`.
#[allow(clippy::too_many_arguments)]
pub fn with_context_menu_and_submenu<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    open: bool,
    anchor_x: f32,
    anchor_y: f32,
    viewport_size: Size,
    menu_width: f32,
    estimated_height: f32,
    menu: impl Into<Element<'a, M>>,
    submenu: Option<(f32, f32, Element<'a, M>)>,
    on_dismiss: M,
) -> Element<'a, M> {
    with_context_menu_and_submenus(
        base,
        open,
        anchor_x,
        anchor_y,
        viewport_size,
        menu_width,
        estimated_height,
        menu,
        submenu.into_iter().collect(),
        on_dismiss,
    )
}

/// Overlay a cursor-anchored context menu with zero or more cascading submenu panels,
/// each positioned beside its parent menu and clamped inside `viewport_size`.
#[allow(clippy::too_many_arguments)]
pub fn with_context_menu_and_submenus<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    open: bool,
    anchor_x: f32,
    anchor_y: f32,
    viewport_size: Size,
    menu_width: f32,
    estimated_height: f32,
    menu: impl Into<Element<'a, M>>,
    submenus: Vec<(f32, f32, Element<'a, M>)>,
    on_dismiss: M,
) -> Element<'a, M> {
    if !open {
        return base.into();
    }
    let max_h = (viewport_size.height - 16.0).max(80.0);
    let (left, top) = clamp_context_menu_origin(
        anchor_x,
        anchor_y,
        viewport_size,
        menu_width,
        estimated_height,
    );
    let dismiss_layer = mouse_area(
        container(iced::widget::space::horizontal())
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(on_dismiss.clone())
    .on_right_press(on_dismiss);

    let clamped_menu: Element<'a, M> = if estimated_height > max_h {
        scrollable(menu.into()).height(Length::Fixed(max_h)).into()
    } else {
        container(menu.into()).max_height(max_h).into()
    };

    let positioned_menu = container(opaque(clamped_menu))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Left)
        .align_y(iced::alignment::Vertical::Top)
        .padding(Padding {
            top,
            right: 8.0,
            bottom: 8.0,
            left,
        });

    let mut layers = vec![base.into(), dismiss_layer.into(), positioned_menu.into()];
    let mut prev_left = left;
    let mut prev_top = top;
    let mut prev_w = menu_width;
    let mut prev_clamped_h = estimated_height.min(max_h);
    for (sub_w, sub_est_h, sub_el) in submenus {
        let sub_left = if prev_left + prev_w + 4.0 + sub_w <= viewport_size.width - 8.0 {
            prev_left + prev_w + 4.0
        } else {
            (prev_left - sub_w - 4.0).max(8.0)
        };
        let clamped_sub_h = sub_est_h.min(max_h);
        let sub_top = (prev_top + prev_clamped_h - clamped_sub_h)
            .clamp(8.0, (viewport_size.height - clamped_sub_h - 8.0).max(8.0));
        let clamped_sub: Element<'a, M> = if sub_est_h > max_h {
            scrollable(sub_el).height(Length::Fixed(max_h)).into()
        } else {
            container(sub_el).max_height(max_h).into()
        };
        let positioned_sub = container(opaque(clamped_sub))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(iced::alignment::Vertical::Top)
            .padding(Padding {
                top: sub_top,
                right: 8.0,
                bottom: 8.0,
                left: sub_left,
            });
        layers.push(positioned_sub.into());
        prev_left = sub_left;
        prev_top = sub_top;
        prev_w = sub_w;
        prev_clamped_h = clamped_sub_h;
    }

    stack(layers)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Overlay a cursor-anchored context menu whose origin clamps dynamically against the live
/// `responsive` layout bounds (preventing stale window-size assumptions).
#[allow(clippy::too_many_arguments)]
pub fn with_responsive_context_menu<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    open: bool,
    anchor_x: f32,
    anchor_y: f32,
    menu_width: f32,
    estimated_height: f32,
    build_menu: impl Fn(Size) -> Element<'a, M> + 'a,
    on_dismiss: M,
) -> Element<'a, M> {
    if !open {
        return base.into();
    }
    let dismiss_layer = mouse_area(
        container(iced::widget::space::horizontal())
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(on_dismiss.clone())
    .on_right_press(on_dismiss);

    let positioned = responsive(move |bounds| {
        let max_h = (bounds.height - 16.0).max(80.0);
        let (left, top) =
            clamp_context_menu_origin(anchor_x, anchor_y, bounds, menu_width, estimated_height);
        let menu_el = build_menu(bounds);
        let clamped_menu: Element<'a, M> = if estimated_height > max_h {
            scrollable(menu_el).height(Length::Fixed(max_h)).into()
        } else {
            container(menu_el).max_height(max_h).into()
        };
        container(opaque(clamped_menu))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(iced::alignment::Vertical::Top)
            .padding(Padding {
                top,
                right: 8.0,
                bottom: 8.0,
                left,
            })
            .into()
    })
    .width(Length::Fill)
    .height(Length::Fill);

    stack![base.into(), dismiss_layer, positioned]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_origin_clamps_within_viewport() {
        let vp = Size::new(800.0, 600.0);
        assert_eq!(
            clamp_context_menu_origin(100.0, 120.0, vp, 220.0, 300.0),
            (100.0, 120.0)
        );
        assert_eq!(
            clamp_context_menu_origin(750.0, 550.0, vp, 220.0, 300.0),
            (572.0, 292.0)
        );
        assert_eq!(
            clamp_context_menu_origin(-10.0, 0.0, vp, 220.0, 300.0),
            (8.0, 8.0)
        );
    }

    #[test]
    fn context_menu_clamps_tall_menus_and_tiny_viewports() {
        let vp = Size::new(300.0, 240.0);
        assert_eq!(
            clamp_context_menu_origin(260.0, 220.0, vp, 220.0, 480.0),
            (72.0, 8.0)
        );
        let tiny = Size::new(100.0, 100.0);
        assert_eq!(
            clamp_context_menu_origin(50.0, 50.0, tiny, 220.0, 300.0),
            (8.0, 8.0)
        );
    }

    #[test]
    fn estimate_menu_height_counts_items_and_separators() {
        let entries = vec![
            MenuEntry::Item(MenuItem::new("Open", ())),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new("Disabled", ()).enabled(false)),
        ];
        assert_eq!(estimate_menu_height(&entries), 12.0 + 31.0 * 2.0 + 9.0);
    }

    #[test]
    fn anchored_and_corner_menus_build_scrollable_opaque_overlays() {
        let items: Vec<_> = (0..20)
            .map(|i| MenuEntry::Item(MenuItem::new(format!("Item {i}"), ())))
            .collect();
        let _tr: Element<'_, ()> = with_top_right_menu_at(
            Space::new(),
            true,
            16.0,
            44.0,
            menu_panel_entries(items, 220.0),
            (),
        );
        let _tl: Element<'_, ()> =
            with_anchored_menu(Space::new(), true, 12.0, 44.0, text("menu"), ());
        let _br: Element<'_, ()> =
            with_bottom_right_menu_at(Space::new(), true, 14.0, 60.0, text("menu"), ());
    }

    #[test]
    fn icon_row_maps_estimates_height_and_renders() {
        let entry: MenuEntry<'_, i32> = MenuEntry::IconRow(vec![
            MenuIconButton::new(Icon::Check, "Single", 1).active(true),
            MenuIconButton::new(Icon::Check, "Double", 2),
            MenuIconButton::new(Icon::Check, "Manhwa", 3).enabled(false),
        ]);

        let mapped = entry.map(|v| format!("msg-{v}"));
        match &mapped {
            MenuEntry::IconRow(buttons) => {
                assert_eq!(buttons.len(), 3);
                assert_eq!(buttons[0].tooltip, "Single");
                assert!(buttons[0].active);
                assert!(buttons[0].enabled);
                assert_eq!(buttons[0].on_press, "msg-1");
                assert!(!buttons[1].active);
                assert_eq!(buttons[1].on_press, "msg-2");
                assert!(!buttons[2].enabled);
                assert_eq!(buttons[2].on_press, "msg-3");
            }
            _ => panic!("expected MenuEntry::IconRow"),
        }

        let entries = vec![
            MenuEntry::Header("Reader Mode".to_owned()),
            mapped,
            MenuEntry::Separator,
        ];
        assert_eq!(estimate_menu_height(&entries), 12.0 + 22.0 + 34.0 + 9.0);
        let _panel: Element<'_, String> = menu_panel_entries(entries, 220.0);
    }
}
