//! Unified draggable, collapsible, swappable, closable, and inline-tabbed sidebar panel header.

use iced::widget::{Space, column, container, mouse_area, row, text};
use iced::{Alignment, Border, Element, Length};

use photon_ui::button::{self as ui_button, ButtonVariant};
use photon_ui::icons::{self, Icon};
use photon_ui::theme::{PaletteTokens, RADIUS_SM};
use photon_ui::tooltip::{TooltipPosition, with_optional_tooltip};

use crate::order::DropIndicator;
use crate::tabs::{PanelTabItem, inline_panel_tab_bar};

/// Configuration for [`draggable_panel_header`].
///
/// Supports either a standard `[icon] title [badge]` header or an inline single-row tab strip
/// when [`Self::tabs`] is non-empty (`[grip] [Tab 1 | Tab 2] [swap] [chevron] [close X]`).
/// All optional controls appear only when configured, ensuring identical chrome across Browser,
/// Viewer, and Editor sidebars.
#[derive(Debug, Clone)]
pub struct DraggablePanelHeaderSpec<M> {
    pub title: String,
    pub icon: Option<Icon>,
    pub badge: Option<String>,
    pub tabs: Vec<PanelTabItem<M>>,
    pub compact_tabs: bool,
    pub collapsed: bool,
    pub dragging: bool,
    pub drop_indicator: DropIndicator,
    pub drag_tooltip: Option<String>,
    pub swap_tooltip: Option<String>,
    pub collapse_tooltip: Option<String>,
    pub close_tooltip: Option<String>,
    pub on_toggle_collapse: Option<M>,
    pub on_drag_start: Option<M>,
    pub on_drag_hover: Option<M>,
    pub on_drag_end: Option<M>,
    pub on_swap: Option<M>,
    pub on_close: Option<M>,
}

impl<M> Default for DraggablePanelHeaderSpec<M> {
    fn default() -> Self {
        Self {
            title: String::new(),
            icon: None,
            badge: None,
            tabs: Vec::new(),
            compact_tabs: false,
            collapsed: false,
            dragging: false,
            drop_indicator: DropIndicator::None,
            drag_tooltip: None,
            swap_tooltip: None,
            collapse_tooltip: None,
            close_tooltip: None,
            on_toggle_collapse: None,
            on_drag_start: None,
            on_drag_hover: None,
            on_drag_end: None,
            on_swap: None,
            on_close: None,
        }
    }
}

impl<M> DraggablePanelHeaderSpec<M> {
    /// Construct a new header spec with the given title and default options.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    /// Construct a new header spec with inline tabs replacing the static title row.
    pub fn with_tabs(tabs: Vec<PanelTabItem<M>>) -> Self {
        Self {
            tabs,
            ..Default::default()
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn tabs(mut self, tabs: Vec<PanelTabItem<M>>) -> Self {
        self.tabs = tabs;
        self
    }

    pub fn compact_tabs(mut self, compact: bool) -> Self {
        self.compact_tabs = compact;
        self
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn dragging(mut self, dragging: bool) -> Self {
        self.dragging = dragging;
        self
    }

    pub fn drop_indicator(mut self, indicator: DropIndicator) -> Self {
        self.drop_indicator = indicator;
        self
    }

    pub fn drag_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.drag_tooltip = Some(tooltip.into());
        self
    }

    pub fn swap_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.swap_tooltip = Some(tooltip.into());
        self
    }

    pub fn collapse_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.collapse_tooltip = Some(tooltip.into());
        self
    }

    pub fn close_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.close_tooltip = Some(tooltip.into());
        self
    }

    pub fn on_toggle_collapse(mut self, msg: M) -> Self {
        self.on_toggle_collapse = Some(msg);
        self
    }

    pub fn on_drag_start(mut self, msg: M) -> Self {
        self.on_drag_start = Some(msg);
        self
    }

    pub fn on_drag_hover(mut self, msg: Option<M>) -> Self {
        self.on_drag_hover = msg;
        self
    }

    pub fn on_drag_end(mut self, msg: Option<M>) -> Self {
        self.on_drag_end = msg;
        self
    }

    pub fn on_swap(mut self, msg: Option<M>) -> Self {
        self.on_swap = msg;
        self
    }

    pub fn on_close(mut self, msg: Option<M>) -> Self {
        self.on_close = msg;
        self
    }
}

pub(crate) fn drop_indicator_line<'a, M: 'a>(visible: bool) -> Element<'a, M> {
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

/// Reusable collapsible, swappable, closable, inline-tabbed, and drag-reorderable panel header.
///
/// Layout order:
/// - Standard: `[grip] [icon] title [badge] [swap] [chevron] [close X]`
/// - Inline tabs: `[grip] [Tab 1 | Tab 2] [badge] [swap] [chevron] [close X]`
pub fn draggable_panel_header<'a, M: Clone + 'a>(
    spec: DraggablePanelHeaderSpec<M>,
) -> Element<'a, M> {
    let mut header_row = row![].spacing(4).align_y(Alignment::Center);

    if let Some(drag_start) = spec.on_drag_start {
        let grip_raw = container(icons::render_muted(Icon::Grip, 13.0))
            .padding([4, 2])
            .align_y(Alignment::Center);
        let mut grip_area = mouse_area(grip_raw).on_press(drag_start);
        if let Some(end_msg) = spec.on_drag_end.clone() {
            grip_area = grip_area.on_release(end_msg);
        }
        header_row = header_row.push(with_optional_tooltip(
            grip_area,
            spec.drag_tooltip,
            TooltipPosition::Left,
        ));
    }

    if !spec.tabs.is_empty() {
        header_row = header_row.push(inline_panel_tab_bar(spec.tabs, spec.compact_tabs));
    } else {
        if let Some(ic) = spec.icon {
            header_row = header_row.push(icons::render(ic, 14.0, None));
        }
        header_row = header_row.push(
            text(spec.title)
                .size(12)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill),
        );
    }

    if let Some(badge) = spec.badge {
        header_row = header_row.push(text(badge).size(12).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.text_muted),
            }
        }));
    }

    if let Some(swap_msg) = spec.on_swap {
        let swap_btn = ui_button::compact_icon(Icon::Swap, ButtonVariant::Ghost, Some(swap_msg));
        header_row = header_row.push(with_optional_tooltip(
            swap_btn,
            spec.swap_tooltip,
            TooltipPosition::Left,
        ));
    }

    if let Some(collapse_msg) = spec.on_toggle_collapse {
        let chevron = ui_button::compact_icon(
            if spec.collapsed {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            },
            ButtonVariant::Ghost,
            Some(collapse_msg),
        );
        header_row = header_row.push(with_optional_tooltip(
            chevron,
            spec.collapse_tooltip,
            TooltipPosition::Left,
        ));
    }

    if let Some(close_msg) = spec.on_close {
        let close_btn = ui_button::compact_icon(Icon::Close, ButtonVariant::Ghost, Some(close_msg));
        header_row = header_row.push(with_optional_tooltip(
            close_btn,
            spec.close_tooltip,
            TooltipPosition::Left,
        ));
    }

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
