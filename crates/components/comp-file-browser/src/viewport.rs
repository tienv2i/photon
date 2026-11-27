//! Virtualized viewport range math for grid and linear list file views.

pub const GRID_CONTENT_TOP: f32 = 12.0;
pub const GRID_CARD_META_HEIGHT: f32 = 56.0;
pub const LIST_ROW_HEIGHT: f32 = 35.0;
pub const LIST_CONTENT_TOP: f32 = 8.0;

/// Map a vertical scroll offset + viewport height to a half-open range of
/// entry indices for a fixed-height linear list.
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

/// Map a vertical scroll offset + viewport height to a half-open range of
/// entry indices for a responsive grid with `cols` cards per row.
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

/// Map a vertical scroll offset + viewport height to a half-open range of
/// entry indices for the standard list view.
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
