//! Tab strip primitives for multi-tab sidebar panels (`Tool Options | History`, etc.).

use iced::widget::{button, container, responsive, row, scrollable, text};
use iced::{Alignment, Border, Color, Element, Length};

use photon_ui::button::{self as ui_button, ButtonVariant};
use photon_ui::icons::{self, Icon};
use photon_ui::theme::{PaletteTokens, RADIUS_MD, RADIUS_SM};
use photon_ui::tooltip::{TooltipPosition, with_optional_tooltip};

/// Tab descriptor for [`panel_tab_bar`] and inline [`crate::draggable_panel_header`] tabs.
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
    /// Construct a tab item with required key, label, active flag, and selection message.
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

    /// Attach a leading vector icon to this tab.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Attach a hover tooltip to this tab.
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

#[derive(Clone, Copy)]
struct TabStripMetrics {
    button_height: f32,
    track_padding: u16,
    horizontal_pad: u16,
    font_size: u32,
    icon_size: f32,
    compact_icon_size: f32,
    item_gap: f32,
}

impl TabStripMetrics {
    const STANDALONE: Self = Self {
        button_height: 28.0,
        track_padding: 3,
        horizontal_pad: 8,
        font_size: 12,
        icon_size: 14.0,
        compact_icon_size: 15.0,
        item_gap: 5.0,
    };

    const INLINE: Self = Self {
        button_height: 22.0,
        track_padding: 2,
        horizontal_pad: 5,
        font_size: 11,
        icon_size: 12.0,
        compact_icon_size: 13.0,
        item_gap: 3.0,
    };
}

fn render_tab_strip<'a, M: Clone + 'a>(
    tabs: Vec<PanelTabItem<M>>,
    compact: bool,
    m: TabStripMetrics,
) -> Element<'a, M> {
    let mut r = row![].spacing(3).align_y(Alignment::Center);

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
            (Some(ic), true) => container(icons::render(ic, m.compact_icon_size, None))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into(),
            (Some(ic), false) => container(
                row![
                    icons::render(ic, m.icon_size, None),
                    text(label_for_content)
                        .size(m.font_size)
                        .wrapping(text::Wrapping::None),
                ]
                .spacing(m.item_gap)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into(),
            (None, _) => container(
                text(label_for_content)
                    .size(m.font_size)
                    .wrapping(text::Wrapping::None),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into(),
        };

        let btn = button(content)
            .width(Length::Fill)
            .height(Length::Fixed(m.button_height))
            .padding([0, m.horizontal_pad])
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
        .padding(m.track_padding)
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

fn estimated_inline_label_width<M>(tabs: &[PanelTabItem<M>]) -> f32 {
    if tabs.is_empty() {
        return 0.0;
    }
    let m = TabStripMetrics::INLINE;
    let tabs_w: f32 = tabs
        .iter()
        .map(|tab| {
            let text_w = tab.label.chars().count() as f32 * 5.5;
            let icon_w = if tab.icon.is_some() {
                m.icon_size + m.item_gap
            } else {
                0.0
            };
            text_w + icon_w + f32::from(m.horizontal_pad) * 2.0 + 4.0
        })
        .sum();
    let gaps = tabs.len().saturating_sub(1) as f32 * 3.0;
    tabs_w + gaps + f32::from(m.track_padding) * 2.0
}

/// Dock tab strip (`Tool Options | History`, etc.) with standardized 12px typography.
///
/// When `compact` is `true`, labels are hidden and icons stay centered with automatic tooltip
/// fallback to the tab label.
pub fn panel_tab_bar<'a, M: Clone + 'a>(
    tabs: Vec<PanelTabItem<M>>,
    compact: bool,
) -> Element<'a, M> {
    render_tab_strip(tabs, compact, TabStripMetrics::STANDALONE)
}

/// Single-row compact tab strip designed for inline placement inside [`crate::draggable_panel_header`].
///
/// Automatically falls back to icon-only compact mode (with tooltips) if `compact` is `true` or
/// if the measured header slot width is too narrow for labels (e.g. ~240px sidebars).
pub fn inline_panel_tab_bar<'a, M: Clone + 'a>(
    tabs: Vec<PanelTabItem<M>>,
    compact: bool,
) -> Element<'a, M> {
    let m = TabStripMetrics::INLINE;
    let fixed_height = m.button_height + f32::from(m.track_padding) * 2.0 + 2.0;
    let all_have_icons = !tabs.is_empty() && tabs.iter().all(|t| t.icon.is_some());
    if compact || !all_have_icons {
        return render_tab_strip(tabs, compact, m);
    }

    let min_w = estimated_inline_label_width(&tabs);
    responsive(move |size| {
        let auto_compact = size.width < min_w;
        render_tab_strip(tabs.clone(), auto_compact, m)
    })
    .width(Length::Fill)
    .height(Length::Fixed(fixed_height))
    .into()
}

/// Document tab descriptor for [`document_tab_bar`] (multi-tab Editor bar below toolbar).
#[derive(Debug, Clone)]
pub struct DocumentTabItem<M> {
    pub id: u64,
    pub label: String,
    pub tooltip: Option<String>,
    pub active: bool,
    pub modified: bool,
    pub on_select: M,
    pub on_close: M,
}

impl<M> DocumentTabItem<M> {
    /// Construct a document tab item with ID, label, active flag, and select/close messages.
    pub fn new(id: u64, label: impl Into<String>, active: bool, on_select: M, on_close: M) -> Self {
        Self {
            id,
            label: label.into(),
            tooltip: None,
            active,
            modified: false,
            on_select,
            on_close,
        }
    }

    /// Mark whether this document tab has unsaved modifications (`•` indicator).
    pub fn modified(mut self, modified: bool) -> Self {
        self.modified = modified;
        self
    }

    /// Attach a hover tooltip (e.g. full file path and dimensions).
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

/// Horizontal scrollable document tab bar placed right below the top toolbar.
///
/// Renders bounded-width (`120..220px`) tabs with a dirty-dot (`•`) indicator,
/// per-tab close (`×`) button, and an optional trailing `+` open-file button.
pub fn document_tab_bar<'a, M: Clone + 'a>(
    tabs: Vec<DocumentTabItem<M>>,
    on_new_tab: Option<M>,
) -> Element<'a, M> {
    let mut tab_row = row![].spacing(4).align_y(Alignment::Center);

    for tab in tabs {
        let active = tab.active;
        let modified = tab.modified;

        let mut label_row = row![].spacing(5).align_y(Alignment::Center);
        if modified {
            label_row = label_row.push(text("•").size(13).style(move |theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(if active { t.accent } else { t.warning }),
                }
            }));
        }
        label_row = label_row.push(text(tab.label).size(12).wrapping(text::Wrapping::None));

        let select_btn = button(
            container(label_row)
                .width(Length::Shrink)
                .height(Length::Fill)
                .clip(true)
                .align_y(Alignment::Center),
        )
        .width(Length::Shrink)
        .height(Length::Fill)
        .padding([0, 10])
        .style(move |theme, status| {
            let t = PaletteTokens::for_theme(theme);
            let bg = match status {
                button::Status::Hovered if !active => t.bg_hover,
                button::Status::Pressed if !active => t.bg_pressed,
                _ => Color::TRANSPARENT,
            };
            let text_color = if active || matches!(status, button::Status::Hovered) {
                t.text_primary
            } else {
                t.text_secondary
            };
            button::Style {
                background: Some(bg.into()),
                text_color,
                border: Border::default(),
                ..Default::default()
            }
        })
        .on_press(tab.on_select);

        let close_btn = button(
            container(icons::render(Icon::Close, 11.0, None))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(Length::Fixed(20.0))
        .height(Length::Fixed(20.0))
        .padding(0)
        .style(ui_button::style_for(ButtonVariant::Ghost))
        .on_press(tab.on_close);

        let pill = container(
            row![select_btn, close_btn]
                .spacing(2)
                .align_y(Alignment::Center)
                .padding([0, 4]),
        )
        .width(Length::Shrink)
        .max_width(220)
        .height(Length::Fixed(28.0))
        .style(move |theme| {
            let t = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(if active { t.bg_selected } else { t.bg_elevated }.into()),
                border: Border {
                    color: if active { t.accent } else { t.border },
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            }
        });

        let tab_el: Element<'a, M> = pill.into();
        tab_row = tab_row.push(with_optional_tooltip(
            tab_el,
            tab.tooltip,
            TooltipPosition::Bottom,
        ));
    }

    let scroll = scrollable(tab_row)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .width(Length::Fill);

    let mut bar = row![scroll].spacing(6).align_y(Alignment::Center);
    if let Some(new_msg) = on_new_tab {
        bar = bar.push(ui_button::compact_icon(
            Icon::Plus,
            ButtonVariant::Ghost,
            Some(new_msg),
        ));
    }

    container(bar)
        .padding([4, 8])
        .width(Length::Fill)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(t.bg_app.into()),
                ..Default::default()
            }
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TabMsg {
        Select(u64),
        Close(u64),
        New,
    }

    #[test]
    fn document_tab_item_builders_set_fields() {
        let item = DocumentTabItem::new(7, "photo.png", true, TabMsg::Select(7), TabMsg::Close(7))
            .modified(true)
            .tooltip("/tmp/photo.png (1920×1080)");

        assert_eq!(item.id, 7);
        assert_eq!(item.label, "photo.png");
        assert!(item.active);
        assert!(item.modified);
        assert_eq!(item.tooltip.as_deref(), Some("/tmp/photo.png (1920×1080)"));
        assert_eq!(item.on_select, TabMsg::Select(7));
        assert_eq!(item.on_close, TabMsg::Close(7));
    }

    #[test]
    fn document_tab_bar_constructs_with_tabs_and_new_button() {
        let tabs = vec![
            DocumentTabItem::new(1, "a.png", false, TabMsg::Select(1), TabMsg::Close(1)),
            DocumentTabItem::new(
                2,
                "very-long-document-filename-for-clipping.png",
                true,
                TabMsg::Select(2),
                TabMsg::Close(2),
            )
            .modified(true)
            .tooltip("/tmp/very-long.png"),
        ];
        let _bar: Element<'_, TabMsg> = document_tab_bar(tabs, Some(TabMsg::New));
        let _empty: Element<'_, TabMsg> = document_tab_bar(Vec::new(), None);
    }
}
