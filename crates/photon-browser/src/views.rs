use iced::widget::{Id, column, container, mouse_area, responsive, row, rule, scrollable};
use iced::{Alignment, Element, Fill, Length};
use photon_i18n::tr;
use photon_protocol::{BrowserRightPanel, BrowserSortField, BrowserViewMode, Entry, SortOrder};
use photon_ui::button;
use photon_ui::grid::{GridCardSpec, thumbnail_card};
use photon_ui::icons::Icon;
use photon_ui::splitter::panel_splitter;
use photon_ui::table::{TableColumn, details_table_header, details_table_row};
use photon_ui::theme::{
    PaletteTokens, TEXT_SIZE_CONTROL, canvas_backdrop_style, hairline_rule_style,
};
use photon_ui::typography::text;

use crate::model::{
    entry_icon, entry_kind_label, format_bytes, format_modified_ms,
    is_browser_filter_regex_invalid, sorted_filtered_entries, thumb_card_min_width,
};
use crate::sidebars::{
    browser_right_panel_is_open, build_browser_left_sidebar, build_browser_right_sidebar,
};
use crate::state::{BrowserMessage, BrowserViewState};
use crate::toolbar::{build_batch_banner, build_browser_filter_bar, build_browser_header};

pub fn build_browser_view<'a>(state: BrowserViewState<'a>) -> Element<'a, BrowserMessage> {
    let lang = state.snapshot.meta.language;

    let left_w = state
        .browser_left_width_drag
        .unwrap_or(state.snapshot.meta.layout.left_width as f32)
        .clamp(160.0, 520.0);

    let header_bar = build_browser_header(state);
    let filtered = sorted_filtered_entries(state);
    let regex_err = is_browser_filter_regex_invalid(state);

    let content: Element<'_, BrowserMessage> = if state.snapshot.entries.is_empty() {
        container(
            column![
                photon_ui::icons::render(Icon::FolderOpen, 36.0, None),
                text(tr(lang, "label-no-folder"))
                    .size(TEXT_SIZE_CONTROL)
                    .style(|theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(t.text_muted),
                        }
                    }),
            ]
            .spacing(10)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(Fill)
        .into()
    } else if filtered.is_empty() {
        container(
            column![
                photon_ui::icons::render(Icon::Search, 30.0, None),
                text(tr(
                    lang,
                    if regex_err {
                        "browser-filter-invalid-regex"
                    } else {
                        "label-empty-filter"
                    }
                ))
                .size(TEXT_SIZE_CONTROL)
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
            ]
            .spacing(10)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(Fill)
        .into()
    } else {
        match state.snapshot.meta.layout.browser_view_mode {
            BrowserViewMode::Grid => build_browser_grid(state, &filtered),
            BrowserViewMode::List => build_browser_list(state, &filtered),
            BrowserViewMode::Details => build_browser_details(state, &filtered),
        }
    };

    let mut center_col = column![header_bar, rule::horizontal(1).style(hairline_rule_style)]
        .width(Fill)
        .height(Fill);

    if state.browser_filter_bar_open {
        center_col = center_col
            .push(build_browser_filter_bar(
                state,
                filtered.len(),
                state.snapshot.entries.len(),
                regex_err,
            ))
            .push(rule::horizontal(1).style(hairline_rule_style));
    }

    if let Some(batch) = &state.snapshot.meta.batch {
        center_col = center_col.push(build_batch_banner(lang, batch));
    }

    center_col = center_col.push(
        container(content)
            .width(Fill)
            .height(Fill)
            .style(canvas_backdrop_style),
    );

    let show_sidebar = state.snapshot.meta.layout.panel("browser.sidebar")
        && (state.browser_quick_access_open || state.browser_explorer_open);
    let open_right_panels: Vec<BrowserRightPanel> = state
        .browser_right_panels
        .order()
        .iter()
        .copied()
        .filter(|&p| browser_right_panel_is_open(state, p))
        .collect();

    let mut layout_row = row![].height(Fill).width(Fill);
    if show_sidebar {
        let left_sidebar = build_browser_left_sidebar(state, left_w);
        layout_row = layout_row.push(left_sidebar).push(panel_splitter(
            left_w,
            1.0,
            160.0,
            520.0,
            240.0,
            BrowserMessage::BrowserSplitterDrag,
            BrowserMessage::BrowserSplitterRelease,
        ));
    }
    layout_row = layout_row.push(center_col);
    if !open_right_panels.is_empty() {
        let right_w = state
            .browser_right_width_drag
            .unwrap_or(state.snapshot.meta.layout.right_width as f32)
            .clamp(200.0, 600.0);
        let right_dock = build_browser_right_sidebar(state, right_w, &open_right_panels);
        layout_row = layout_row
            .push(panel_splitter(
                right_w,
                -1.0,
                200.0,
                600.0,
                290.0,
                BrowserMessage::BrowserRightSplitterDrag,
                BrowserMessage::BrowserRightSplitterRelease,
            ))
            .push(right_dock);
    }
    layout_row.into()
}

/// Number of extra rows rendered outside the viewport when reporting the
const VISIBLE_OVERSCAN_ROWS: usize = 2;
/// Approximate extra height of a grid card below its preview (padding + label).
const GRID_CARD_META_HEIGHT: f32 = 56.0;
/// Grid content top padding used by `build_browser_grid`.
const GRID_CONTENT_TOP: f32 = 12.0;
/// Approximate list row height (row button + column spacing).
const LIST_ROW_HEIGHT: f32 = 35.0;
/// List content top padding used by `build_browser_list`.
const LIST_CONTENT_TOP: f32 = 8.0;

/// Pure geometry helper: map a vertical scroll offset + viewport height to a
/// half-open range of entry indices for a fixed-height linear list.
pub fn visible_linear_range(
    offset_y: f32,
    viewport_height: f32,
    row_height: f32,
    content_top: f32,
    len: usize,
    overscan_rows: usize,
) -> (usize, usize) {
    if len == 0 {
        return (0, 0);
    }
    let row_height = row_height.max(1.0);
    let content_top = content_top.max(0.0);
    let first_row = ((offset_y - content_top).max(0.0) / row_height).floor() as usize;
    let last_row = (((offset_y + viewport_height.max(0.0) - content_top).max(0.0)) / row_height)
        .ceil() as usize;
    let start_row = first_row.saturating_sub(overscan_rows);
    let end_row = (last_row + overscan_rows + 1).max(start_row + 1);
    let start = start_row.min(len);
    let end = end_row.min(len).max(start);
    (start, end)
}

/// Pure geometry helper for the responsive grid. `cols` is the number of cards
/// per row and `card_width` the resolved card width used by the layout.
pub fn grid_visible_range(
    offset_y: f32,
    viewport_height: f32,
    card_width: f32,
    cols: usize,
    gap: f32,
    len: usize,
    overscan_rows: usize,
) -> (usize, usize) {
    if len == 0 || cols == 0 {
        return (0, 0);
    }
    let preview_height = (card_width * 0.72).clamp(96.0, 170.0);
    let tile_height = preview_height + GRID_CARD_META_HEIGHT;
    let row_height = (tile_height + gap).max(1.0);
    let (start_row, end_row) = visible_linear_range(
        offset_y,
        viewport_height,
        row_height,
        GRID_CONTENT_TOP,
        len.div_ceil(cols),
        overscan_rows,
    );
    let start = (start_row * cols).min(len);
    let end = (end_row * cols).min(len).max(start);
    (start, end)
}

/// Pure geometry helper for the list view.
pub fn list_visible_range(
    offset_y: f32,
    viewport_height: f32,
    len: usize,
    overscan_rows: usize,
) -> (usize, usize) {
    visible_linear_range(
        offset_y,
        viewport_height,
        LIST_ROW_HEIGHT,
        LIST_CONTENT_TOP,
        len,
        overscan_rows,
    )
}

fn build_browser_grid<'a>(
    state: BrowserViewState<'_>,
    entries: &[&Entry],
) -> Element<'a, BrowserMessage> {
    let cursor_pt = state.last_cursor.unwrap_or(iced::Point::new(360.0, 260.0));
    let cards: Vec<(String, GridCardSpec<BrowserMessage>)> = entries
        .iter()
        .map(|entry| {
            let ic = entry_icon(entry);
            let subtitle = if entry.is_dir {
                None
            } else {
                Some(format_bytes(entry.size))
            };
            (
                entry.path.clone(),
                GridCardSpec {
                    label: entry.name.clone(),
                    subtitle,
                    thumbnail: state.thumbnails.get(&entry.path).cloned(),
                    fallback_icon: ic,
                    selected: state.snapshot.meta.selection.contains(&entry.path),
                    on_press: BrowserMessage::BrowserEntryActivated {
                        path: entry.path.clone(),
                        is_dir: entry.is_dir,
                        is_image: entry.is_image,
                    },
                },
            )
        })
        .collect();
    let min_w = thumb_card_min_width(state.snapshot.meta.layout.browser_thumb_size);
    let gap = 10.0_f32;
    let total = entries.len();

    responsive(move |bounds| {
        let usable = (bounds.width - 24.0).max(min_w);
        let cols = ((usable + gap) / (min_w + gap)).floor().max(1.0) as usize;
        let card_width =
            ((usable - gap * (cols.saturating_sub(1) as f32)) / cols as f32).clamp(min_w, 300.0);

        let mut grid_col = column![].spacing(gap).padding([12, 12]);
        for chunk in cards.chunks(cols) {
            let mut r = row![].spacing(gap).align_y(Alignment::Start);
            for (path, spec) in chunk {
                let card = thumbnail_card(spec.clone(), card_width);
                r = r.push(mouse_area(card).on_right_press(
                    BrowserMessage::OpenBrowserContextMenu {
                        path: path.clone(),
                        x: cursor_pt.x,
                        y: cursor_pt.y,
                    },
                ));
            }
            grid_col = grid_col.push(r);
        }
        scrollable(grid_col)
            .id(Id::new("browser-grid-scroll"))
            .width(Length::Fill)
            .height(Length::Fill)
            .on_scroll(move |viewport| {
                let (start, end) = grid_visible_range(
                    viewport.absolute_offset().y,
                    viewport.bounds().height,
                    card_width,
                    cols,
                    gap,
                    total,
                    VISIBLE_OVERSCAN_ROWS,
                );
                BrowserMessage::VisibleRangeChanged { start, end }
            })
            .into()
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn build_browser_list<'a>(
    state: BrowserViewState<'_>,
    entries: &[&Entry],
) -> Element<'a, BrowserMessage> {
    let cursor_pt = state.last_cursor.unwrap_or(iced::Point::new(360.0, 260.0));
    let mut col = column![].spacing(3).padding([8, 12]);
    for entry in entries {
        let ic = entry_icon(entry);
        let trailing = if entry.is_dir {
            None
        } else {
            Some(format_bytes(entry.size))
        };
        let active = state.snapshot.meta.selection.contains(&entry.path);
        let row_btn = button::list_row(
            ic,
            &entry.name,
            trailing,
            active,
            Some(BrowserMessage::BrowserEntryActivated {
                path: entry.path.clone(),
                is_dir: entry.is_dir,
                is_image: entry.is_image,
            }),
        );
        col = col.push(mouse_area(row_btn).on_right_press(
            BrowserMessage::OpenBrowserContextMenu {
                path: entry.path.clone(),
                x: cursor_pt.x,
                y: cursor_pt.y,
            },
        ));
    }
    let total = entries.len();
    scrollable(col)
        .id(Id::new("browser-list-scroll"))
        .height(Fill)
        .on_scroll(move |viewport| {
            let (start, end) = list_visible_range(
                viewport.absolute_offset().y,
                viewport.bounds().height,
                total,
                VISIBLE_OVERSCAN_ROWS,
            );
            BrowserMessage::VisibleRangeChanged { start, end }
        })
        .into()
}

fn build_browser_details<'a>(
    state: BrowserViewState<'_>,
    entries: &[&Entry],
) -> Element<'a, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let layout = &state.snapshot.meta.layout;
    let active_field = layout.browser_sort_field;
    let is_asc = layout.browser_sort_order == SortOrder::Asc;
    let cursor_pt = state.last_cursor.unwrap_or(iced::Point::new(360.0, 260.0));

    let sort_msg = |field: BrowserSortField| -> BrowserMessage {
        if active_field == field {
            BrowserMessage::SetBrowserSortOrder(if is_asc {
                SortOrder::Desc
            } else {
                SortOrder::Asc
            })
        } else {
            BrowserMessage::SetBrowserSortField(field)
        }
    };

    let header = details_table_header(vec![
        TableColumn {
            label: tr(lang, "col-name"),
            width: None,
            align_right: false,
            sorted: (active_field == BrowserSortField::Name).then_some(is_asc),
            on_sort: sort_msg(BrowserSortField::Name),
        },
        TableColumn {
            label: tr(lang, "col-type"),
            width: Some(90.0),
            align_right: false,
            sorted: (active_field == BrowserSortField::Kind).then_some(is_asc),
            on_sort: sort_msg(BrowserSortField::Kind),
        },
        TableColumn {
            label: tr(lang, "col-size"),
            width: Some(90.0),
            align_right: true,
            sorted: (active_field == BrowserSortField::Size).then_some(is_asc),
            on_sort: sort_msg(BrowserSortField::Size),
        },
        TableColumn {
            label: tr(lang, "col-modified"),
            width: Some(140.0),
            align_right: false,
            sorted: (active_field == BrowserSortField::Modified).then_some(is_asc),
            on_sort: sort_msg(BrowserSortField::Modified),
        },
    ]);

    let mut rows_col = column![].spacing(2).padding([4, 8]);
    for entry in entries {
        let ic = entry_icon(entry);
        let active = state.snapshot.meta.selection.contains(&entry.path);
        let size_str = if entry.is_dir {
            "—".to_owned()
        } else {
            format_bytes(entry.size)
        };
        let row_el = details_table_row(
            ic,
            vec![
                (entry.name.clone(), None, false),
                (entry_kind_label(entry), Some(90.0), false),
                (size_str, Some(90.0), true),
                (format_modified_ms(entry.modified_ms), Some(140.0), false),
            ],
            32.0,
            active,
            BrowserMessage::BrowserEntryActivated {
                path: entry.path.clone(),
                is_dir: entry.is_dir,
                is_image: entry.is_image,
            },
        );
        rows_col = rows_col.push(mouse_area(row_el).on_right_press(
            BrowserMessage::OpenBrowserContextMenu {
                path: entry.path.clone(),
                x: cursor_pt.x,
                y: cursor_pt.y,
            },
        ));
    }

    column![header, scrollable(rows_col).height(Fill).width(Fill)]
        .width(Fill)
        .height(Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_linear_range_clamps_and_overscans() {
        assert_eq!(visible_linear_range(0.0, 0.0, 35.0, 8.0, 0, 2), (0, 0));
        // First screen of 100 rows: ~10 visible + 2 overscan + 1 partial.
        let (start, end) = visible_linear_range(0.0, 350.0, 35.0, 8.0, 100, 2);
        assert_eq!(start, 0);
        assert!(end > 10 && end < 20, "unexpected end {end}");
        // Scrolled to the bottom: range clamps to len.
        let (start, end) = visible_linear_range(10_000.0, 350.0, 35.0, 8.0, 100, 2);
        assert!(start <= end);
        assert_eq!(end, 100);
    }

    #[test]
    fn grid_visible_range_covers_rows_and_clamps() {
        let (start, end) = grid_visible_range(0.0, 600.0, 180.0, 4, 10.0, 100, 2);
        assert_eq!(start, 0);
        assert!(end > 4 && end <= 100);
        assert_eq!(end % 4, 0);
        assert_eq!(
            grid_visible_range(0.0, 600.0, 180.0, 0, 10.0, 100, 2),
            (0, 0)
        );
        assert_eq!(grid_visible_range(0.0, 600.0, 180.0, 4, 10.0, 0, 2), (0, 0));
        let (start, end) = grid_visible_range(100_000.0, 600.0, 180.0, 4, 10.0, 7, 2);
        assert!(start <= end);
        assert_eq!(end, 7);
    }
}
