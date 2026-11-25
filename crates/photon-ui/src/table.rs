//! Reusable sortable Details table header and rows for Browser views.

use iced::alignment::Horizontal;
use iced::widget::{button, column, container, row, rule, text};
use iced::{Alignment, Border, Color, Element, Length};

use crate::button::{self as ui_button, ButtonVariant};
use crate::icons::{self, Icon};
use crate::theme::{PaletteTokens, RADIUS_SM, hairline_rule_style};

/// Column descriptor for `details_table_header`.
#[derive(Debug, Clone)]
pub struct TableColumn<M> {
    pub label: String,
    pub width: Option<f32>,
    pub align_right: bool,
    pub sorted: Option<bool>,
    pub on_sort: M,
}

fn cell_width(width: Option<f32>) -> Length {
    match width {
        Some(w) => Length::Fixed(w),
        None => Length::Fill,
    }
}

/// Sortable table header bar matching Photon's studio palette.
pub fn details_table_header<'a, M: Clone + 'a>(columns: Vec<TableColumn<M>>) -> Element<'a, M> {
    let mut r = row![].spacing(8).align_y(Alignment::Center);
    for col in columns {
        let mut label_row = row![].spacing(4).align_y(Alignment::Center);
        if col.align_right {
            label_row = label_row.push(iced::widget::space::horizontal());
        }
        let is_sorted = col.sorted.is_some();
        label_row = label_row.push(
            text(col.label)
                .size(11)
                .wrapping(text::Wrapping::None)
                .style(move |theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(if is_sorted {
                            t.text_primary
                        } else {
                            t.text_muted
                        }),
                    }
                }),
        );
        if let Some(asc) = col.sorted {
            let ic = if asc { Icon::SortAsc } else { Icon::SortDesc };
            label_row = label_row.push(icons::render(ic, 12.0, None));
        }
        if !col.align_right {
            label_row = label_row.push(iced::widget::space::horizontal());
        }

        let btn = button(label_row)
            .width(cell_width(col.width))
            .padding([6, 6])
            .style(|theme, status| {
                let t = PaletteTokens::for_theme(theme);
                let bg = match status {
                    button::Status::Hovered => t.bg_hover,
                    button::Status::Pressed => t.bg_pressed,
                    _ => Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: t.text_secondary,
                    border: Border {
                        radius: RADIUS_SM.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(col.on_sort);
        r = r.push(btn);
    }

    column![
        container(r)
            .padding([2, 8])
            .width(Length::Fill)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                container::Style {
                    background: Some(t.bg_surface.into()),
                    text_color: Some(t.text_primary),
                    ..Default::default()
                }
            }),
        rule::horizontal(1).style(hairline_rule_style),
    ]
    .width(Length::Fill)
    .into()
}

/// Clickable Details table row (`cells`: `(text, fixed_width_or_fill, align_right)`).
pub fn details_table_row<'a, M: Clone + 'a>(
    icon: Icon,
    cells: Vec<(String, Option<f32>, bool)>,
    row_height: f32,
    selected: bool,
    on_press: M,
) -> Element<'a, M> {
    let mut r = row![].spacing(8).align_y(Alignment::Center);
    for (idx, (val, width, align_right)) in cells.into_iter().enumerate() {
        let mut cell_text = text(val).size(12).wrapping(text::Wrapping::None);
        if idx > 0 {
            cell_text = cell_text.style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_secondary),
                }
            });
        }

        let cell_el: Element<'a, M> = if idx == 0 {
            row![icons::render(icon, 16.0, None), cell_text]
                .spacing(8)
                .align_y(Alignment::Center)
                .width(cell_width(width))
                .into()
        } else {
            container(cell_text)
                .width(cell_width(width))
                .align_x(if align_right {
                    Horizontal::Right
                } else {
                    Horizontal::Left
                })
                .align_y(Alignment::Center)
                .into()
        };
        r = r.push(cell_el);
    }

    button(
        container(r)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(row_height))
    .padding([0, 14])
    .style(ui_button::style_for(ButtonVariant::ListRow {
        active: selected,
    }))
    .on_press(on_press)
    .into()
}
