//! Reusable popover menus, shortcut-hinted items, separators, and compact menubars.

use crate::typography::text;
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, responsive, row, rule, scrollable, stack,
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

const VIEWPORT_MARGIN: f32 = 8.0;
const SUBMENU_GAP: f32 = 4.0;

const MENU_CHROME_VERTICAL: f32 = 12.0; // container(col).padding(6) => 6 top + 6 bottom
const MENU_ROW_SPACING: f32 = 2.0; // column![].spacing(2)
const MENU_HEADER_HEIGHT: f32 = 22.0; // 11px text (~14.3) + [4, 10] padding (8)
const MENU_ITEM_HEIGHT: f32 = 28.0; // 12px text (~15.6) + [6, 10] padding (12)
const MENU_ICON_ROW_HEIGHT: f32 = 38.0; // 30px toolbar_icon + [4, 6] padding (8)
const MENU_SEPARATOR_HEIGHT: f32 = 7.0; // 1px rule + [3, 4] padding (6)

fn menu_entry_height<M>(entry: &MenuEntry<'_, M>) -> f32 {
    match entry {
        MenuEntry::Header(_) => MENU_HEADER_HEIGHT,
        MenuEntry::Item(_) => MENU_ITEM_HEIGHT,
        MenuEntry::IconRow(_) => MENU_ICON_ROW_HEIGHT,
        MenuEntry::Separator => MENU_SEPARATOR_HEIGHT,
    }
}

/// Estimate the vertical height (px) of a menu built from `entries` (excluding inter-row gaps).
pub fn estimate_menu_height<M>(entries: &[MenuEntry<'_, M>]) -> f32 {
    let mut h = MENU_CHROME_VERTICAL;
    for entry in entries {
        h += menu_entry_height(entry);
    }
    h
}

/// Estimate the rendered height (px) of a menu including `column![].spacing(2)` gaps.
pub fn estimate_menu_height_with_spacing<M>(entries: &[MenuEntry<'_, M>]) -> f32 {
    estimate_menu_height(entries) + entries.len().saturating_sub(1) as f32 * MENU_ROW_SPACING
}

/// Estimate the vertical offset (px) from the top of the menu content area to `entries[index]`.
///
/// Pass this to [`CascadingSubmenu::parent_row_offset_y`] so a cascading submenu aligns its first
/// item with the active triggering row in its parent panel.
pub fn estimate_entry_offset_y<M>(entries: &[MenuEntry<'_, M>], index: usize) -> f32 {
    let mut y = 0.0_f32;
    for entry in entries.iter().take(index) {
        y += menu_entry_height(entry) + MENU_ROW_SPACING;
    }
    y
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
        return stack![base.into()]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
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
        return stack![base.into()]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
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
        return stack![base.into()]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum VerticalAnchor {
    /// Menu hangs downward from `top` (`align_y(Vertical::Top)`, `padding.top = top`).
    Top(f32),
    /// Menu grows upward from `bottom` (`align_y(Vertical::Bottom)`, `padding.bottom = bottom`).
    Bottom(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MenuPlacement {
    left: f32,
    width: f32,
    /// Post-clamp / post-flip top coordinate in viewport space.
    top: f32,
    /// Clamped panel height (`estimated_height.min(max_h)`).
    height: f32,
    /// Post-clamp / post-flip bottom coordinate in viewport space.
    bottom: f32,
    vertical: VerticalAnchor,
    /// Whether cascading child submenus should prefer opening to the left.
    cascading_left: bool,
}

impl MenuPlacement {
    fn bottom_anchored(self) -> bool {
        matches!(self.vertical, VerticalAnchor::Bottom(_))
    }

    fn available_height(self, viewport_height: f32) -> f32 {
        match self.vertical {
            VerticalAnchor::Top(top) => (viewport_height - top - VIEWPORT_MARGIN).max(60.0),
            VerticalAnchor::Bottom(bottom) => {
                (viewport_height - bottom - VIEWPORT_MARGIN).max(60.0)
            }
        }
    }
}

fn place_context_menu(
    anchor_x: f32,
    anchor_y: f32,
    viewport_size: Size,
    menu_width: f32,
    estimated_height: f32,
) -> MenuPlacement {
    let max_left = (viewport_size.width - menu_width - VIEWPORT_MARGIN).max(VIEWPORT_MARGIN);
    let left = anchor_x.clamp(VIEWPORT_MARGIN, max_left);

    let max_h = (viewport_size.height - VIEWPORT_MARGIN * 2.0).max(60.0);
    let clamped_h = estimated_height.min(max_h).max(0.0);
    let max_anchor_y = (viewport_size.height - VIEWPORT_MARGIN).max(VIEWPORT_MARGIN);
    let clamped_anchor_y = anchor_y.clamp(VIEWPORT_MARGIN, max_anchor_y);

    let (vertical, top, bottom) =
        if clamped_anchor_y + clamped_h <= viewport_size.height - VIEWPORT_MARGIN {
            let top = clamped_anchor_y;
            (VerticalAnchor::Top(top), top, top + clamped_h)
        } else if clamped_anchor_y - clamped_h >= VIEWPORT_MARGIN {
            // Flipped upward above the cursor: anchor using Vertical::Bottom so the menu's
            // actual rendered bottom sits directly against `anchor_y`.
            let bottom_pad = (viewport_size.height - clamped_anchor_y).max(VIEWPORT_MARGIN);
            let bottom = (viewport_size.height - bottom_pad).max(VIEWPORT_MARGIN);
            let top = (bottom - clamped_h).max(VIEWPORT_MARGIN);
            (VerticalAnchor::Bottom(bottom_pad), top, bottom)
        } else {
            // Clamped near the bottom viewport edge: align with Vertical::Bottom (padding.bottom = 8.0)
            // so the menu hugs the bottom margin instead of floating above the cursor.
            let bottom_pad = VIEWPORT_MARGIN;
            let bottom = (viewport_size.height - bottom_pad).max(VIEWPORT_MARGIN);
            let top = (bottom - clamped_h).max(VIEWPORT_MARGIN);
            (VerticalAnchor::Bottom(bottom_pad), top, bottom)
        };

    MenuPlacement {
        left,
        width: menu_width,
        top,
        height: clamped_h,
        bottom,
        vertical,
        cascading_left: false,
    }
}

fn place_submenu(
    parent: MenuPlacement,
    sub_w: f32,
    sub_est_h: f32,
    parent_row_offset_y: Option<f32>,
    parent_row_height: f32,
    level_idx: usize,
    viewport_size: Size,
) -> MenuPlacement {
    let max_left = (viewport_size.width - sub_w - VIEWPORT_MARGIN).max(VIEWPORT_MARGIN);
    let right_candidate = parent.left + parent.width + SUBMENU_GAP;
    let left_candidate = parent.left - sub_w - SUBMENU_GAP;
    let fits_right = right_candidate + sub_w <= viewport_size.width - VIEWPORT_MARGIN;
    let fits_left = left_candidate >= VIEWPORT_MARGIN;

    let (left, cascading_left) = if parent.cascading_left {
        if fits_left {
            (left_candidate, true)
        } else if fits_right {
            (right_candidate, false)
        } else {
            (left_candidate.clamp(VIEWPORT_MARGIN, max_left), true)
        }
    } else if fits_right {
        (right_candidate, false)
    } else if fits_left {
        (left_candidate, true)
    } else {
        let space_right = viewport_size.width - VIEWPORT_MARGIN - right_candidate;
        let space_left = parent.left - SUBMENU_GAP - VIEWPORT_MARGIN;
        if space_left > space_right {
            (left_candidate.clamp(VIEWPORT_MARGIN, max_left), true)
        } else {
            (right_candidate.clamp(VIEWPORT_MARGIN, max_left), false)
        }
    };

    let max_h = (viewport_size.height - VIEWPORT_MARGIN * 2.0).max(60.0);
    let clamped_h = sub_est_h.min(max_h).max(0.0);
    let row_h = parent_row_height.max(1.0);
    let max_y = (viewport_size.height - VIEWPORT_MARGIN).max(VIEWPORT_MARGIN);

    let bottom_aligned = |target_bottom: f32| -> (VerticalAnchor, f32, f32) {
        let bottom_pad = (viewport_size.height - target_bottom).max(VIEWPORT_MARGIN);
        let bottom = (viewport_size.height - bottom_pad).max(VIEWPORT_MARGIN);
        let top = (bottom - clamped_h).max(VIEWPORT_MARGIN);
        (VerticalAnchor::Bottom(bottom_pad), top, bottom)
    };

    let (vertical, top, bottom) = if let Some(offset_y) = parent_row_offset_y {
        let clamped_offset =
            offset_y.clamp(0.0, (parent.height - row_h - MENU_CHROME_VERTICAL).max(0.0));
        let preferred_top = (parent.top + clamped_offset).clamp(VIEWPORT_MARGIN, max_y);
        let preferred_bottom = (preferred_top + row_h + MENU_CHROME_VERTICAL)
            .min(parent.bottom)
            .clamp(VIEWPORT_MARGIN, max_y);
        if preferred_top + clamped_h <= viewport_size.height - VIEWPORT_MARGIN {
            (
                VerticalAnchor::Top(preferred_top),
                preferred_top,
                preferred_top + clamped_h,
            )
        } else if preferred_bottom - clamped_h >= VIEWPORT_MARGIN {
            bottom_aligned(preferred_bottom)
        } else {
            bottom_aligned(max_y)
        }
    } else if level_idx == 0 {
        if parent.bottom_anchored() {
            if parent.bottom - clamped_h >= VIEWPORT_MARGIN {
                bottom_aligned(parent.bottom)
            } else {
                bottom_aligned(max_y)
            }
        } else {
            let row_top = (parent.bottom - row_h - MENU_CHROME_VERTICAL)
                .max(parent.top)
                .clamp(VIEWPORT_MARGIN, max_y);
            if row_top + clamped_h <= viewport_size.height - VIEWPORT_MARGIN {
                (VerticalAnchor::Top(row_top), row_top, row_top + clamped_h)
            } else if parent.bottom - clamped_h >= VIEWPORT_MARGIN {
                bottom_aligned(parent.bottom)
            } else {
                bottom_aligned(max_y)
            }
        }
    } else {
        // Level-3+ cascading submenu anchored to the actual post-clamp/post-flip top of its
        // level-2+ parent panel, flipping upward if near the bottom viewport edge.
        let preferred_top = parent.top.clamp(VIEWPORT_MARGIN, max_y);
        if preferred_top + clamped_h <= viewport_size.height - VIEWPORT_MARGIN {
            (
                VerticalAnchor::Top(preferred_top),
                preferred_top,
                preferred_top + clamped_h,
            )
        } else if parent.bottom - clamped_h >= VIEWPORT_MARGIN {
            bottom_aligned(parent.bottom)
        } else {
            bottom_aligned(max_y)
        }
    };

    MenuPlacement {
        left,
        width: sub_w,
        top,
        height: clamped_h,
        bottom,
        vertical,
        cascading_left,
    }
}

fn clamped_menu_element<'a, M: Clone + 'a>(
    menu: Element<'a, M>,
    estimated_height: f32,
    max_h: f32,
    available_h: f32,
) -> Element<'a, M> {
    let limit = max_h.min(available_h).max(60.0);
    if estimated_height > limit {
        scrollable(menu).height(Length::Fixed(limit)).into()
    } else {
        container(menu).max_height(limit).into()
    }
}

fn position_menu_layer<'a, M: Clone + 'a>(
    clamped_menu: Element<'a, M>,
    placement: MenuPlacement,
) -> Element<'a, M> {
    let base = container(opaque(clamped_menu))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Left);

    match placement.vertical {
        VerticalAnchor::Top(top) => base
            .align_y(iced::alignment::Vertical::Top)
            .padding(Padding {
                top,
                right: VIEWPORT_MARGIN,
                bottom: VIEWPORT_MARGIN,
                left: placement.left,
            })
            .into(),
        VerticalAnchor::Bottom(bottom) => base
            .align_y(iced::alignment::Vertical::Bottom)
            .padding(Padding {
                top: VIEWPORT_MARGIN,
                right: VIEWPORT_MARGIN,
                bottom,
                left: placement.left,
            })
            .into(),
    }
}

/// Specification for a cascading submenu panel (level-2, level-3, ...).
pub struct CascadingSubmenu<'a, M> {
    /// Fixed panel width in logical pixels.
    pub width: f32,
    /// Estimated panel height in logical pixels (from [`estimate_menu_height_with_spacing`]).
    pub estimated_height: f32,
    /// Optional Y offset (px) from the top of the parent panel's content area to the active
    /// triggering row (see [`estimate_entry_offset_y`]).
    pub parent_row_offset_y: Option<f32>,
    /// Height (px) of the triggering row in the parent panel (defaults to `28.0`).
    pub parent_row_height: f32,
    /// Rendered submenu element.
    pub content: Element<'a, M>,
}

impl<'a, M> CascadingSubmenu<'a, M> {
    pub fn new(width: f32, estimated_height: f32, content: impl Into<Element<'a, M>>) -> Self {
        Self {
            width,
            estimated_height,
            parent_row_offset_y: None,
            parent_row_height: MENU_ITEM_HEIGHT,
            content: content.into(),
        }
    }

    pub fn parent_row_offset_y(mut self, offset_y: f32) -> Self {
        self.parent_row_offset_y = Some(offset_y);
        self
    }

    pub fn parent_row_height(mut self, height: f32) -> Self {
        self.parent_row_height = height;
        self
    }
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
/// each positioned beside its parent menu's actual post-clamp/post-flip coordinates
/// and clamped inside `viewport_size`.
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
    let cascading = submenus
        .into_iter()
        .map(|(w, h, el)| CascadingSubmenu::new(w, h, el))
        .collect();
    with_context_menu_and_cascading_submenus(
        base,
        open,
        anchor_x,
        anchor_y,
        viewport_size,
        menu_width,
        estimated_height,
        menu,
        cascading,
        on_dismiss,
    )
}

/// Overlay a cursor-anchored context menu with cascading [`CascadingSubmenu`] panels that can
/// optionally specify their triggering `parent_row_offset_y` for exact row alignment.
#[allow(clippy::too_many_arguments)]
pub fn with_context_menu_and_cascading_submenus<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    open: bool,
    anchor_x: f32,
    anchor_y: f32,
    viewport_size: Size,
    menu_width: f32,
    estimated_height: f32,
    menu: impl Into<Element<'a, M>>,
    submenus: Vec<CascadingSubmenu<'a, M>>,
    on_dismiss: M,
) -> Element<'a, M> {
    if !open {
        return stack![base.into()]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
    let max_h = (viewport_size.height - VIEWPORT_MARGIN * 2.0).max(60.0);
    let primary = place_context_menu(
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

    let clamped_menu = clamped_menu_element(
        menu.into(),
        estimated_height,
        max_h,
        primary.available_height(viewport_size.height),
    );
    let positioned_menu = position_menu_layer(clamped_menu, primary);

    let mut layers = vec![base.into(), dismiss_layer.into(), positioned_menu];
    let mut prev = primary;
    for (level_idx, sub) in submenus.into_iter().enumerate() {
        let placement = place_submenu(
            prev,
            sub.width,
            sub.estimated_height,
            sub.parent_row_offset_y,
            sub.parent_row_height,
            level_idx,
            viewport_size,
        );
        let clamped_sub = clamped_menu_element(
            sub.content,
            sub.estimated_height,
            max_h,
            placement.available_height(viewport_size.height),
        );
        layers.push(position_menu_layer(clamped_sub, placement));
        prev = placement;
    }

    stack(layers)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Overlay a menubar-anchored dropdown with zero or more cascading [`CascadingSubmenu`] panels.
#[allow(clippy::too_many_arguments)]
pub fn with_anchored_menu_and_submenus<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    open: bool,
    left_offset: f32,
    top_offset: f32,
    viewport_size: Size,
    menu_width: f32,
    estimated_height: f32,
    menu: impl Into<Element<'a, M>>,
    submenus: Vec<CascadingSubmenu<'a, M>>,
    on_dismiss: M,
) -> Element<'a, M> {
    if !open {
        return stack![base.into()]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
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

    let max_h = (viewport_size.height - top_offset - VIEWPORT_MARGIN).max(60.0);
    let x = left_offset.clamp(
        VIEWPORT_MARGIN,
        (viewport_size.width - menu_width - VIEWPORT_MARGIN).max(VIEWPORT_MARGIN),
    );
    let primary = place_context_menu(
        x,
        top_offset.max(0.0),
        viewport_size,
        menu_width,
        estimated_height.min(max_h),
    );
    let clamped_menu = clamped_menu_element(menu.into(), estimated_height, max_h, max_h);
    let positioned_menu = position_menu_layer(clamped_menu, primary);

    let mut layers = vec![base.into(), dismiss_layer.into(), positioned_menu];
    let mut prev = primary;
    for (level_idx, sub) in submenus.into_iter().enumerate() {
        let placement = place_submenu(
            prev,
            sub.width,
            sub.estimated_height,
            sub.parent_row_offset_y,
            sub.parent_row_height,
            level_idx,
            viewport_size,
        );
        let clamped_sub = clamped_menu_element(
            sub.content,
            sub.estimated_height,
            max_h,
            placement.available_height(viewport_size.height),
        );
        layers.push(position_menu_layer(clamped_sub, placement));
        prev = placement;
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
        return stack![base.into()]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
    let dismiss_layer = mouse_area(
        container(iced::widget::space::horizontal())
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(on_dismiss.clone())
    .on_right_press(on_dismiss);

    let positioned = responsive(move |bounds| {
        let max_h = (bounds.height - VIEWPORT_MARGIN * 2.0).max(60.0);
        let placement =
            place_context_menu(anchor_x, anchor_y, bounds, menu_width, estimated_height);
        let menu_el = build_menu(bounds);
        let clamped_menu = clamped_menu_element(
            menu_el,
            estimated_height,
            max_h,
            placement.available_height(bounds.height),
        );
        position_menu_layer(clamped_menu, placement)
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
    fn context_menu_anchors_top_when_fitting_below_cursor() {
        let vp = Size::new(800.0, 600.0);
        let p = place_context_menu(100.0, 120.0, vp, 220.0, 300.0);
        assert_eq!(p.left, 100.0);
        assert_eq!(p.top, 120.0);
        assert_eq!(p.bottom, 420.0);
        assert_eq!(p.vertical, VerticalAnchor::Top(120.0));

        let clamped_top_left = place_context_menu(-10.0, 0.0, vp, 220.0, 300.0);
        assert_eq!(clamped_top_left.left, 8.0);
        assert_eq!(clamped_top_left.top, 8.0);
        assert_eq!(clamped_top_left.vertical, VerticalAnchor::Top(8.0));
    }

    #[test]
    fn context_menu_flips_upward_using_vertical_bottom_at_cursor_y() {
        let vp = Size::new(800.0, 600.0);
        let p = place_context_menu(750.0, 550.0, vp, 220.0, 300.0);
        assert_eq!(p.left, 572.0);
        // Bottom edge sits directly at anchor_y (550.0): padding.bottom = 600 - 550 = 50.0.
        assert_eq!(p.vertical, VerticalAnchor::Bottom(50.0));
        assert_eq!(p.bottom, 550.0);
        assert_eq!(p.top, 250.0);
    }

    #[test]
    fn context_menu_clamps_tall_menus_to_bottom_edge_and_handles_tiny_viewports() {
        let vp = Size::new(800.0, 600.0);
        // Does not fit below (350 + 400 > 592) and does not fit above (350 - 400 < 8).
        let clamped = place_context_menu(200.0, 350.0, vp, 220.0, 400.0);
        assert_eq!(clamped.left, 200.0);
        assert_eq!(clamped.vertical, VerticalAnchor::Bottom(8.0));
        assert_eq!(clamped.bottom, 592.0);
        assert_eq!(clamped.top, 192.0);

        let small_vp = Size::new(300.0, 240.0);
        let tall = place_context_menu(260.0, 220.0, small_vp, 220.0, 480.0);
        assert_eq!(tall.left, 72.0);
        assert_eq!(tall.vertical, VerticalAnchor::Bottom(8.0));
        assert_eq!(tall.top, 8.0);
        assert_eq!(tall.bottom, 232.0);

        let tiny = Size::new(100.0, 100.0);
        let tiny_p = place_context_menu(50.0, 50.0, tiny, 220.0, 300.0);
        assert_eq!(tiny_p.left, 8.0);
        assert_eq!(tiny_p.vertical, VerticalAnchor::Bottom(8.0));
        assert_eq!(tiny_p.top, 8.0);
    }

    #[test]
    fn cascading_level_3_submenus_continue_left_when_level_2_flipped_left() {
        let vp = Size::new(1280.0, 720.0);
        // Right-click near bottom-right corner.
        let lvl1 = place_context_menu(1180.0, 680.0, vp, 225.0, 380.0);
        assert_eq!(lvl1.left, 1047.0);
        assert_eq!(lvl1.vertical, VerticalAnchor::Bottom(40.0));
        assert_eq!(lvl1.top, 300.0);
        assert_eq!(lvl1.bottom, 680.0);

        // Level-2 does not fit right of level-1, so it flips left and stays bottom-anchored to level-1.
        let lvl2 = place_submenu(lvl1, 220.0, 200.0, None, MENU_ITEM_HEIGHT, 0, vp);
        assert_eq!(lvl2.left, 823.0);
        assert!(lvl2.cascading_left);
        assert_eq!(lvl2.vertical, VerticalAnchor::Bottom(40.0));
        assert_eq!(lvl2.top, 480.0);
        assert_eq!(lvl2.bottom, 680.0);

        // Level-3 MUST continue cascading left (599.0) instead of jumping back onto level-1 (1047.0).
        let lvl3 = place_submenu(lvl2, 220.0, 120.0, None, MENU_ITEM_HEIGHT, 1, vp);
        assert_eq!(lvl3.left, 599.0);
        assert!(lvl3.cascading_left);
        // Anchored to level-2's actual post-flip top (480.0) and fits downward (480 + 120 <= 712).
        assert_eq!(lvl3.vertical, VerticalAnchor::Top(480.0));
        assert_eq!(lvl3.top, 480.0);
        assert_eq!(lvl3.bottom, 600.0);
    }

    #[test]
    fn cascading_level_3_anchors_to_explicit_row_and_flips_up_near_bottom() {
        let vp = Size::new(1280.0, 720.0);
        let lvl1 = place_context_menu(100.0, 120.0, vp, 225.0, 360.0);
        let lvl2 = place_submenu(lvl1, 220.0, 240.0, Some(300.0), MENU_ITEM_HEIGHT, 0, vp);
        // lvl1.top = 120, row_offset = 300 -> preferred_top = 420; 420 + 240 = 660 <= 712 -> Top(420)
        assert_eq!(lvl2.left, 329.0);
        assert_eq!(lvl2.vertical, VerticalAnchor::Top(420.0));

        // Active row near bottom of lvl2 (offset 180 -> row top 600); a 200px level-3 overflows
        // bottom (600 + 200 > 712), so it flips upward aligned with that row's bottom (640).
        let lvl3 = place_submenu(lvl2, 220.0, 200.0, Some(180.0), MENU_ITEM_HEIGHT, 1, vp);
        assert_eq!(lvl3.left, 553.0);
        assert_eq!(lvl3.vertical, VerticalAnchor::Bottom(80.0));
        assert_eq!(lvl3.bottom, 640.0);
        assert_eq!(lvl3.top, 440.0);
    }

    #[test]
    fn cascading_level_3_flips_left_when_hitting_right_edge() {
        let vp = Size::new(700.0, 600.0);
        let lvl1 = place_context_menu(100.0, 80.0, vp, 225.0, 200.0);
        assert_eq!(lvl1.left, 100.0);
        let lvl2 = place_submenu(lvl1, 220.0, 140.0, None, MENU_ITEM_HEIGHT, 0, vp);
        assert_eq!(lvl2.left, 329.0);
        assert!(!lvl2.cascading_left);
        // 329 + 220 + 4 + 220 = 773 > 692 -> flips left of level-2 (329 - 220 - 4 = 105).
        let lvl3 = place_submenu(lvl2, 220.0, 100.0, None, MENU_ITEM_HEIGHT, 1, vp);
        assert_eq!(lvl3.left, 105.0);
        assert!(lvl3.cascading_left);
    }

    #[test]
    fn estimate_menu_height_and_entry_offsets_are_tight() {
        let entries = vec![
            MenuEntry::Item(MenuItem::new("Open", ())),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new("Disabled", ()).enabled(false)),
        ];
        assert_eq!(estimate_menu_height(&entries), 12.0 + 28.0 * 2.0 + 7.0);
        assert_eq!(
            estimate_menu_height_with_spacing(&entries),
            12.0 + 28.0 * 2.0 + 7.0 + 2.0 * 2.0
        );
        assert_eq!(estimate_entry_offset_y(&entries, 0), 0.0);
        assert_eq!(estimate_entry_offset_y(&entries, 1), 28.0 + 2.0);
        assert_eq!(
            estimate_entry_offset_y(&entries, 2),
            (28.0 + 2.0) + (7.0 + 2.0)
        );
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
        let _cascading: Element<'_, ()> = with_context_menu_and_cascading_submenus(
            Space::new(),
            true,
            700.0,
            560.0,
            Size::new(800.0, 600.0),
            220.0,
            240.0,
            text("lvl1"),
            vec![
                CascadingSubmenu::new(220.0, 180.0, text("lvl2")).parent_row_offset_y(160.0),
                CascadingSubmenu::new(220.0, 120.0, text("lvl3")).parent_row_offset_y(60.0),
            ],
            (),
        );
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
        assert_eq!(estimate_menu_height(&entries), 12.0 + 22.0 + 38.0 + 7.0);
        let _panel: Element<'_, String> = menu_panel_entries(entries, 220.0);
    }
}
