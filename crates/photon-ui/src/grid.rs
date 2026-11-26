//! Reusable responsive thumbnail grid and clickable breadcrumb bar.

use iced::widget::{
    button, column, container, image, mouse_area, responsive, row, scrollable, text,
};
use iced::{Alignment, Border, ContentFit, Element, Length};

use crate::button::{self as ui_button, ButtonVariant};
use crate::icons::{self, Icon};
use crate::theme::{PaletteTokens, RADIUS_MD, RADIUS_SM};

#[derive(Debug, Clone)]
pub struct GridCardSpec<M> {
    pub label: String,
    pub subtitle: Option<String>,
    pub thumbnail: Option<image::Handle>,
    pub fallback_icon: Icon,
    pub selected: bool,
    pub on_press: M,
}

#[derive(Debug, Clone)]
pub struct BreadcrumbItem<M> {
    pub label: String,
    pub active: bool,
    pub on_press: M,
}

#[derive(Debug, Clone)]
pub struct FilmstripItem<M> {
    pub thumbnail: Option<image::Handle>,
    pub active: bool,
    pub on_press: M,
}

pub fn thumbnail_card<'a, M: Clone + 'a>(spec: GridCardSpec<M>, card_width: f32) -> Element<'a, M> {
    let preview_height = (card_width * 0.72).clamp(96.0, 170.0);
    let selected = spec.selected;

    let preview: Element<'a, M> = if let Some(handle) = spec.thumbnail {
        container(
            image(handle)
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(ContentFit::Contain),
        )
        .width(Length::Fill)
        .height(Length::Fixed(preview_height))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(t.bg_app.into()),
                border: Border {
                    radius: RADIUS_SM.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
    } else {
        container(icons::render(spec.fallback_icon, 36.0, None))
            .width(Length::Fill)
            .height(Length::Fixed(preview_height))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                container::Style {
                    background: Some(t.bg_app.into()),
                    border: Border {
                        radius: RADIUS_SM.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .into()
    };

    let mut meta_col = column![
        container(
            text(spec.label)
                .size(12)
                .width(Length::Fill)
                .wrapping(text::Wrapping::None)
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_primary),
                    }
                })
        )
        .width(Length::Fill)
        .clip(true),
    ]
    .spacing(2)
    .width(Length::Fill);

    if let Some(sub) = spec.subtitle {
        meta_col = meta_col.push(
            container(
                text(sub)
                    .size(11)
                    .width(Length::Fill)
                    .wrapping(text::Wrapping::None)
                    .style(|theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(t.text_muted),
                        }
                    }),
            )
            .width(Length::Fill)
            .clip(true),
        );
    }

    button(column![preview, meta_col].spacing(8).width(Length::Fill))
        .padding(8)
        .width(Length::Fixed(card_width))
        .style(move |theme, status| {
            let t = PaletteTokens::for_theme(theme);
            let bg = match (selected, status) {
                (true, _) => t.bg_selected,
                (false, button::Status::Hovered) => t.bg_hover,
                (false, button::Status::Pressed) => t.bg_pressed,
                (false, _) => t.bg_surface,
            };
            let border_color = if selected {
                t.accent
            } else if matches!(status, button::Status::Hovered) {
                t.border_strong
            } else {
                t.border
            };
            button::Style {
                background: Some(bg.into()),
                text_color: t.text_primary,
                border: Border {
                    color: border_color,
                    width: if selected { 1.5 } else { 1.0 },
                    radius: RADIUS_MD.into(),
                },
                ..Default::default()
            }
        })
        .on_press(spec.on_press)
        .into()
}

pub fn responsive_grid<'a, M: Clone + 'a>(
    items: Vec<GridCardSpec<M>>,
    min_card_width: f32,
    gap: f32,
) -> Element<'a, M> {
    responsive(move |bounds| {
        let usable = (bounds.width - 24.0).max(min_card_width);
        let cols = ((usable + gap) / (min_card_width + gap)).floor().max(1.0) as usize;
        let card_width = ((usable - gap * (cols.saturating_sub(1) as f32)) / cols as f32)
            .clamp(min_card_width, 300.0);

        let mut grid_col = column![].spacing(gap).padding([12, 12]);
        for chunk in items.chunks(cols) {
            let mut r = row![].spacing(gap).align_y(Alignment::Start);
            for item in chunk {
                r = r.push(thumbnail_card(item.clone(), card_width));
            }
            grid_col = grid_col.push(r);
        }
        scrollable(grid_col)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub fn breadcrumbs_bar<'a, M: Clone + 'a>(items: Vec<BreadcrumbItem<M>>) -> Element<'a, M> {
    let mut r = row![].spacing(2).align_y(Alignment::Center);
    let len = items.len();
    for (idx, item) in items.into_iter().enumerate() {
        if idx > 0 {
            r = r.push(icons::render(Icon::ChevronRight, 12.0, None));
        }
        let variant = if item.active {
            ButtonVariant::Secondary
        } else {
            ButtonVariant::Ghost
        };
        let on_press = (idx + 1 < len).then_some(item.on_press);
        r = r.push(ui_button::labeled(item.label, None, variant, on_press));
    }
    scrollable(r)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ))
        .width(Length::Fill)
        .into()
}

pub fn filmstrip_bar<'a, M: Clone + 'a>(
    items: Vec<FilmstripItem<M>>,
    scroll_id: iced::widget::Id,
    on_scroll: Option<Box<dyn Fn(scrollable::Viewport) -> M + 'a>>,
    on_wheel: Option<Box<dyn Fn(iced::mouse::ScrollDelta) -> M + 'a>>,
) -> Element<'a, M> {
    let mut r = row![].spacing(6).align_y(Alignment::Center);
    for item in items {
        let active = item.active;
        let thumb: Element<'a, M> = if let Some(handle) = item.thumbnail {
            image(handle)
                .width(Length::Fixed(58.0))
                .height(Length::Fixed(46.0))
                .content_fit(ContentFit::Cover)
                .into()
        } else {
            container(icons::render(Icon::Image, 18.0, None))
                .width(Length::Fixed(58.0))
                .height(Length::Fixed(46.0))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        };

        r = r.push(
            button(thumb)
                .padding(3)
                .style(move |theme, status| {
                    let t = PaletteTokens::for_theme(theme);
                    let border_color = if active {
                        t.accent
                    } else if matches!(status, button::Status::Hovered) {
                        t.border_strong
                    } else {
                        t.border
                    };
                    button::Style {
                        background: Some(t.bg_app.into()),
                        text_color: t.text_primary,
                        border: Border {
                            color: border_color,
                            width: if active { 2.0 } else { 1.0 },
                            radius: RADIUS_SM.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(item.on_press),
        );
    }

    let strip = scrollable(container(r).padding([8, 12]))
        .id(scroll_id)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ));
    let strip: Element<'a, M> = match on_scroll {
        Some(f) => strip.on_scroll(f).into(),
        None => strip.into(),
    };
    let strip: Element<'a, M> = match on_wheel {
        Some(f) => mouse_area(strip).on_scroll(f).into(),
        None => strip,
    };
    container(strip)
        .width(Length::Fill)
        .style(crate::theme::surface_panel_style)
        .into()
}

/// Horizontal pitch of one filmstrip thumbnail, including its padding and gap.
pub const FILMSTRIP_ITEM_PITCH: f32 = 58.0 + 3.0 * 2.0 + 6.0;
const FILMSTRIP_CONTENT_PADDING: f32 = 12.0;

/// Extra filmstrip items (both sides) kept warm around the visible range.
pub const FILMSTRIP_VISIBLE_OVERSCAN: usize = 12;

/// Half-open visible filmstrip range (plus `overscan` items on both sides).
pub fn filmstrip_visible_range(
    offset_x: f32,
    viewport_w: f32,
    total: usize,
    overscan: usize,
) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let first =
        ((offset_x - FILMSTRIP_CONTENT_PADDING).max(0.0) / FILMSTRIP_ITEM_PITCH).floor() as usize;
    let last = (((offset_x + viewport_w - FILMSTRIP_CONTENT_PADDING).max(0.0)
        / FILMSTRIP_ITEM_PITCH)
        .ceil()) as usize;
    let start = first.saturating_sub(overscan).min(total);
    let end = last.saturating_add(overscan).min(total);
    (start, end.max(start))
}

/// Scroll offset that centers `index` in a filmstrip viewport.
pub fn filmstrip_offset_for_index(index: usize, viewport_w: f32) -> f32 {
    (index as f32 * FILMSTRIP_ITEM_PITCH + FILMSTRIP_ITEM_PITCH * 0.5 - viewport_w * 0.5).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filmstrip_visible_range_covers_viewport_plus_overscan() {
        // At the start: 10 items fit in 700 px; overscan adds 4 on the right.
        assert_eq!(filmstrip_visible_range(0.0, 700.0, 40, 4), (0, 14));
        // Scrolled one viewport: first visible item is 9, worst case 19.
        assert_eq!(filmstrip_visible_range(700.0, 700.0, 40, 4), (5, 24));
        // End clamp at the maximum realistic scroll offset.
        assert_eq!(filmstrip_visible_range(2100.0, 700.0, 40, 4), (25, 40));
        assert_eq!(filmstrip_visible_range(10_000.0, 700.0, 40, 4), (40, 40));
        assert_eq!(filmstrip_visible_range(0.0, 700.0, 0, 4), (0, 0));
    }

    #[test]
    fn filmstrip_offset_centers_active_item() {
        assert_eq!(filmstrip_offset_for_index(0, 700.0), 0.0);
        let offset = filmstrip_offset_for_index(10, 700.0);
        assert!(offset > 0.0);
        // Item 10 sits roughly at the middle of the viewport after scrolling.
        assert!(
            (offset + 700.0 * 0.5 - (10.0 * FILMSTRIP_ITEM_PITCH + FILMSTRIP_ITEM_PITCH * 0.5))
                .abs()
                < 0.01
        );
    }
}
