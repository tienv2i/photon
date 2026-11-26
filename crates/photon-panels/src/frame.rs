//! Unified sidebar panel frames, drop-target wrappers, dock containers, cards, and status bar.

use iced::widget::{column, container, mouse_area, row, rule};
use iced::{Alignment, Element, Length, Padding};
use photon_ui::typography::text;

use photon_ui::button::{self as ui_button, ButtonVariant};
use photon_ui::icons::{self, Icon};
use photon_ui::theme::{PaletteTokens, card_style, hairline_rule_style, surface_panel_style};
use photon_ui::tooltip::{TooltipPosition, with_optional_tooltip};

use crate::header::{DraggablePanelHeaderSpec, draggable_panel_header, drop_indicator_line};
use crate::order::DropIndicator;

/// Configuration for [`sidebar_panel`], unifying header, collapsible body, height fill,
/// and drag-and-drop drop-target behavior across all workspace sidebars.
pub struct PanelFrameSpec<'a, M> {
    pub header: DraggablePanelHeaderSpec<M>,
    pub body: Option<Element<'a, M>>,
    pub fill_height: bool,
    pub body_padding: Padding,
    pub drop_indicator: DropIndicator,
    pub on_drop_hover: Option<M>,
    pub on_drop_release: Option<M>,
}

impl<'a, M> PanelFrameSpec<'a, M> {
    /// Start a new [`PanelFrameSpec`] with the given header specification.
    pub fn new(header: DraggablePanelHeaderSpec<M>) -> Self {
        Self {
            header,
            body: None,
            fill_height: false,
            body_padding: Padding::ZERO,
            drop_indicator: DropIndicator::None,
            on_drop_hover: None,
            on_drop_release: None,
        }
    }

    /// Set the panel body element (ignored when `header.collapsed` is `true`).
    pub fn body(mut self, body: impl Into<Element<'a, M>>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Conditionally set the panel body element.
    pub fn maybe_body(mut self, body: Option<Element<'a, M>>) -> Self {
        self.body = body;
        self
    }

    /// Expand the panel body to fill available vertical height when uncollapsed.
    pub fn fill_height(mut self, fill: bool) -> Self {
        self.fill_height = fill;
        self
    }

    /// Apply inner padding around the body content.
    pub fn body_padding(mut self, padding: impl Into<Padding>) -> Self {
        self.body_padding = padding.into();
        self
    }

    /// Set the outer [`DropIndicator`] bar shown before or after the panel card.
    pub fn drop_indicator(mut self, indicator: DropIndicator) -> Self {
        self.drop_indicator = indicator;
        self
    }

    /// Set the message emitted when a dragged panel hovers over this panel.
    pub fn on_drop_hover(mut self, msg: Option<M>) -> Self {
        self.on_drop_hover = msg;
        self
    }

    /// Set the message emitted when a dragged panel is released onto this panel.
    pub fn on_drop_release(mut self, msg: Option<M>) -> Self {
        self.on_drop_release = msg;
        self
    }

    /// Set the optional per-panel options toggle callback on the header.
    pub fn on_options(mut self, msg: Option<M>) -> Self {
        self.header.on_options = msg;
        self
    }

    /// Set whether the panel's options toggle button is currently active.
    pub fn options_active(mut self, active: bool) -> Self {
        self.header.options_active = active;
        self
    }

    /// Set the tooltip for the panel's options toggle button.
    pub fn options_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.header.options_tooltip = Some(tooltip.into());
        self
    }
}

/// Render a unified collapsible, optional-closable, drag-reorderable sidebar panel section.
///
/// Matches the Editor sidebar chrome so Quick Access, Explorer, Detail/Info, View Adjustment,
/// Tool Options/History, and Layers share identical headers, collapse behavior, and drop targets.
pub fn sidebar_panel<'a, M: Clone + 'a>(spec: PanelFrameSpec<'a, M>) -> Element<'a, M> {
    let collapsed = spec.header.collapsed || spec.body.is_none();
    let fill = !collapsed && spec.fill_height;
    let header_el = draggable_panel_header(spec.header);

    let mut content_col = column![header_el].spacing(6).width(Length::Fill);
    if !collapsed && let Some(body) = spec.body {
        let mut body_box = container(body)
            .padding(spec.body_padding)
            .width(Length::Fill);
        if spec.fill_height {
            body_box = body_box.height(Length::Fill);
            content_col = content_col.height(Length::Fill);
        }
        content_col = content_col.push(body_box);
    }

    drop_target_panel_fill(
        content_col,
        spec.drop_indicator,
        fill,
        spec.on_drop_hover,
        spec.on_drop_release,
    )
}

/// Wrap any sidebar column content in the standard padded, surface-styled fixed-width dock shell.
pub fn sidebar_dock<'a, M: 'a>(width: f32, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content.into())
        .padding(8)
        .width(Length::Fixed(width))
        .height(Length::Fill)
        .style(surface_panel_style)
        .into()
}

/// Wrap an entire panel card with drop-target hover/release handling and top/bottom drop
/// indicator bars during sidebar drag-and-drop reordering.
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

/// Configuration for [`render_dock_panel`].
pub struct DockPanelSpec<'a, M> {
    pub title: String,
    pub icon: Option<Icon>,
    pub width: f32,
    pub body: Element<'a, M>,
    pub close_tooltip: Option<String>,
    pub on_close: Option<M>,
}

/// Render a standalone fixed-width dock panel with 13px header, hairline divider, and optional
/// close `X` button.
pub fn render_dock_panel<'a, M: Clone + 'a>(spec: DockPanelSpec<'a, M>) -> Element<'a, M> {
    let mut header_row = row![].spacing(6).align_y(Alignment::Center);
    if let Some(ic) = spec.icon {
        header_row = header_row.push(icons::render(ic, 14.0, None));
    }
    header_row = header_row.push(
        text(spec.title)
            .size(13)
            .wrapping(text::Wrapping::None)
            .width(Length::Fill),
    );
    if let Some(close_msg) = spec.on_close {
        let close_btn = ui_button::compact_icon(Icon::Close, ButtonVariant::Ghost, Some(close_msg));
        header_row = header_row.push(with_optional_tooltip(
            close_btn,
            spec.close_tooltip,
            TooltipPosition::Left,
        ));
    }

    container(
        column![
            container(header_row).padding([10, 12]),
            rule::horizontal(1).style(hairline_rule_style),
            container(spec.body).padding(12).height(Length::Fill),
        ]
        .height(Length::Fill),
    )
    .width(Length::Fixed(spec.width))
    .height(Length::Fill)
    .style(surface_panel_style)
    .into()
}

/// Standalone fixed-width dock panel without a close button.
pub fn dock_panel<'a, M: Clone + 'a>(
    title: impl Into<String>,
    icon: Option<Icon>,
    width: f32,
    body: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    render_dock_panel(DockPanelSpec {
        title: title.into(),
        icon,
        width,
        body: body.into(),
        close_tooltip: None,
        on_close: None,
    })
}

/// Standalone fixed-width dock panel with a close `X` button in the header.
pub fn dock_panel_closable<'a, M: Clone + 'a>(
    title: impl Into<String>,
    icon: Option<Icon>,
    width: f32,
    close_tooltip: Option<String>,
    on_close: M,
    body: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    render_dock_panel(DockPanelSpec {
        title: title.into(),
        icon,
        width,
        body: body.into(),
        close_tooltip,
        on_close: Some(on_close),
    })
}

/// Elevated section card with muted 12px heading.
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

/// Key-value inspector row with 12px typography.
pub fn kv_row<'a, M: 'a>(key: impl Into<String>, value: impl Into<String>) -> Element<'a, M> {
    row![
        text(key.into())
            .size(12)
            .width(Length::Shrink)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
        text(value.into())
            .size(12)
            .width(Length::Fill)
            .align_x(Alignment::End)
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_primary),
                }
            }),
    ]
    .width(Length::Fill)
    .spacing(8)
    .align_y(Alignment::Start)
    .into()
}

/// Workspace bottom status bar with 12px left status and right metadata.
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
    use crate::tabs::{PanelTabItem, panel_tab_bar};

    #[derive(Debug, Clone)]
    enum Msg {
        Toggle,
        DragStart,
        Hover,
        Drop,
        Swap,
        Close,
        TabOptions,
        TabHistory,
    }

    #[test]
    fn panel_builders_construct_without_panicking() {
        let header = DraggablePanelHeaderSpec::new("Layers")
            .icon(Icon::Layers)
            .badge("3")
            .collapsed(false)
            .on_drag_start(Msg::DragStart)
            .on_swap(Some(Msg::Swap))
            .on_toggle_collapse(Msg::Toggle)
            .on_close(Some(Msg::Close))
            .drag_tooltip("Drag")
            .swap_tooltip("Swap")
            .collapse_tooltip("Collapse")
            .close_tooltip("Close");

        let _expanded: Element<'_, Msg> = sidebar_panel(
            PanelFrameSpec::new(header.clone())
                .body(kv_row("Blend", "Normal"))
                .fill_height(true)
                .drop_indicator(DropIndicator::After)
                .on_drop_hover(Some(Msg::Hover))
                .on_drop_release(Some(Msg::Drop)),
        );

        let _collapsed: Element<'_, Msg> = sidebar_panel(
            PanelFrameSpec::new(header.collapsed(true)).body(kv_row("Hidden", "Body")),
        );

        let _tabs: Element<'_, Msg> = panel_tab_bar(
            vec![
                PanelTabItem::new("options", "Tool Options", true, Msg::TabOptions)
                    .icon(Icon::Sliders),
                PanelTabItem::new("history", "History", false, Msg::TabHistory).icon(Icon::History),
            ],
            false,
        );

        let _inline_tabbed_panel: Element<'_, Msg> = sidebar_panel(
            PanelFrameSpec::new(
                DraggablePanelHeaderSpec::with_tabs(vec![
                    PanelTabItem::new("options", "Tool Options", true, Msg::TabOptions)
                        .icon(Icon::Sliders)
                        .tooltip("Tool Options"),
                    PanelTabItem::new("history", "History", false, Msg::TabHistory)
                        .icon(Icon::History)
                        .tooltip("History"),
                ])
                .on_drag_start(Msg::DragStart)
                .on_swap(Some(Msg::Swap))
                .on_toggle_collapse(Msg::Toggle),
            )
            .body(kv_row("Radius", "12 px"))
            .fill_height(true),
        );

        let _inline_compact_panel: Element<'_, Msg> = sidebar_panel(
            PanelFrameSpec::new(
                DraggablePanelHeaderSpec::with_tabs(vec![
                    PanelTabItem::new("options", "Tool Options", true, Msg::TabOptions)
                        .icon(Icon::Sliders),
                    PanelTabItem::new("history", "History", false, Msg::TabHistory)
                        .icon(Icon::History),
                ])
                .compact_tabs(true)
                .collapsed(true)
                .on_drag_start(Msg::DragStart)
                .on_toggle_collapse(Msg::Toggle),
            )
            .body(kv_row("Hidden", "Collapsed")),
        );

        let _closable: Element<'_, Msg> = dock_panel_closable(
            "Info",
            Some(Icon::Info),
            280.0,
            Some("Close panel".into()),
            Msg::Close,
            section_card("Meta", kv_row("Size", "1920×1080")),
        );

        let _dock: Element<'_, Msg> = sidebar_dock(280.0, status_bar("Ready", "photon-panels"));
    }
}
