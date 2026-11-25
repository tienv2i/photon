//! Reusable responsive thumbnail grid and clickable breadcrumb bar.

use iced::widget::{button, column, container, image, responsive, row, scrollable, text};
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
        text(spec.label)
            .size(12)
            .wrapping(text::Wrapping::None)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_primary),
                }
            }),
    ]
    .spacing(2)
    .width(Length::Fill);

    if let Some(sub) = spec.subtitle {
        meta_col = meta_col.push(text(sub).size(11).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.text_muted),
            }
        }));
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

pub fn filmstrip_bar<'a, M: Clone + 'a>(items: Vec<FilmstripItem<M>>) -> Element<'a, M> {
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

    container(scrollable(container(r).padding([8, 12])).direction(
        scrollable::Direction::Horizontal(scrollable::Scrollbar::default()),
    ))
    .width(Length::Fill)
    .style(crate::theme::surface_panel_style)
    .into()
}
