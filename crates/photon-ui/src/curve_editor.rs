//! Interactive tonal curve graph widget (`CurveGraphProgram` & `curve_graph`).
//!
//! Provides a 256×256 / responsive square Dark & Light studio tonal curve editor
//! with a 4×4 grid, diagonal identity reference, optional translucent 256-bin
//! histogram backdrop, Fritsch–Carlson monotone cubic Hermite spline (matching
//! `photon_image::pixels::build_curve_lut`), interactive control points, and
//! live `(In, Out)` `0..=255` readout.

use std::sync::Arc;
use std::time::Instant;

use iced::mouse;
use iced::widget::canvas::{self, Action, Frame, Geometry, Path, Stroke, Text};
use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};

use crate::button::{self as ui_button, ButtonVariant};
use crate::icons::Icon;
use crate::theme::{PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION};

/// Minimum horizontal separation between neighboring control points so X order never inverts.
pub const MIN_X_SEPARATION: f32 = 0.004;

const CURVE_PAD: f32 = 10.0;
const POINT_HIT_RADIUS: f32 = 10.0;
const DOUBLE_CLICK_WINDOW_MS: u128 = 350;
const DOUBLE_CLICK_DIST_SQ: f32 = 36.0;

/// Events emitted by [`CurveGraphProgram`] and [`curve_graph`].
#[derive(Debug, Clone, PartialEq)]
pub enum CurveGraphEvent {
    /// Emitted when control points are added, moved, or removed.
    PointsChanged(Vec<[f32; 2]>),
    /// Emitted when the user requests resetting the active curve channel to linear.
    ResetChannel,
}

/// Interactive `iced::widget::canvas::Program` for tonal curve editing.
pub struct CurveGraphProgram<'a, M> {
    pub points: Vec<[f32; 2]>,
    pub channel_color: Color,
    pub histogram: Option<Arc<[u32; 256]>>,
    pub on_change: Box<dyn Fn(CurveGraphEvent) -> M + 'a>,
}

impl<'a, M> CurveGraphProgram<'a, M> {
    /// Create a new curve graph canvas program.
    pub fn new(
        points: Vec<[f32; 2]>,
        channel_color: Color,
        on_change: impl Fn(CurveGraphEvent) -> M + 'a,
    ) -> Self {
        Self {
            points,
            channel_color,
            histogram: None,
            on_change: Box::new(on_change),
        }
    }

    /// Attach an optional 256-bin histogram backdrop.
    pub fn with_histogram(mut self, histogram: Option<Arc<[u32; 256]>>) -> Self {
        self.histogram = histogram;
        self
    }
}

/// Persistent interaction state managed by `iced` for [`CurveGraphProgram`].
#[derive(Debug, Default)]
pub struct CurveGraphState {
    dragging_idx: Option<usize>,
    selected_idx: Option<usize>,
    hovered_idx: Option<usize>,
    hover_pos: Option<[f32; 2]>,
    last_click: Option<(Instant, Point, usize)>,
}

/// Build the default 360×360 studio tonal curve widget wrapped in a themed frame.
pub fn curve_graph<'a, M: Clone + 'a>(
    points: Vec<[f32; 2]>,
    channel_color: Color,
    histogram: Option<Arc<[u32; 256]>>,
    on_event: impl Fn(CurveGraphEvent) -> M + 'a,
) -> Element<'a, M> {
    curve_graph_sized(360.0, points, channel_color, histogram, on_event)
}

/// Build a tonal curve widget with an explicit square plot side length in logical pixels.
pub fn curve_graph_sized<'a, M: Clone + 'a>(
    size: f32,
    points: Vec<[f32; 2]>,
    channel_color: Color,
    histogram: Option<Arc<[u32; 256]>>,
    on_event: impl Fn(CurveGraphEvent) -> M + 'a,
) -> Element<'a, M> {
    let side = size.max(160.0);
    let reset_msg = on_event(CurveGraphEvent::ResetChannel);
    let program = CurveGraphProgram::new(points, channel_color, on_event).with_histogram(histogram);

    let canvas_box = container(
        iced::widget::canvas(program)
            .width(Length::Fixed(side))
            .height(Length::Fixed(side)),
    )
    .padding(2)
    .style(|theme| {
        let t = PaletteTokens::for_theme(theme);
        container::Style {
            background: Some(t.bg_surface.into()),
            border: Border {
                color: t.border_strong,
                width: 1.0,
                radius: RADIUS_SM.into(),
            },
            ..Default::default()
        }
    });

    let footer = row![
        text("0..255")
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            }),
        iced::widget::space::horizontal(),
        ui_button::compact_icon(Icon::Reset, ButtonVariant::Ghost, Some(reset_msg)),
    ]
    .width(Length::Fixed(side + 4.0))
    .align_y(Alignment::Center);

    container(
        column![canvas_box, footer]
            .spacing(4)
            .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .align_x(Alignment::Center)
    .into()
}

/// Normalize, clamp to `[0.0, 1.0]`, and sort curve control points (guaranteeing `>= 2` points).
pub fn normalize_points(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut pts: Vec<[f32; 2]> = points
        .iter()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .map(|[x, y]| [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)])
        .collect();
    if pts.is_empty() {
        return vec![[0.0, 0.0], [1.0, 1.0]];
    }
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    if pts.len() == 1 {
        let p = pts[0];
        if p[0] < 0.5 {
            pts.push([1.0, 1.0]);
        } else {
            pts.insert(0, [0.0, 0.0]);
        }
    }
    pts
}

/// Insert a new control point while preserving `MIN_X_SEPARATION` from adjacent points.
pub fn insert_control_point(
    points: &[[f32; 2]],
    new_pt: [f32; 2],
) -> Option<(Vec<[f32; 2]>, usize)> {
    let mut updated = normalize_points(points);
    let nx = new_pt[0].clamp(0.0, 1.0);
    let ny = new_pt[1].clamp(0.0, 1.0);
    let insert_idx = updated
        .binary_search_by(|p| p[0].total_cmp(&nx))
        .unwrap_or_else(|i| i);

    let min_x = if insert_idx > 0 {
        updated[insert_idx - 1][0] + MIN_X_SEPARATION
    } else {
        0.0
    };
    let max_x = if insert_idx < updated.len() {
        updated[insert_idx][0] - MIN_X_SEPARATION
    } else {
        1.0
    };

    if min_x > max_x {
        return None;
    }

    updated.insert(insert_idx, [nx.clamp(min_x, max_x), ny]);
    Some((updated, insert_idx))
}

/// Move an existing control point, clamped to `[0.0, 1.0]` with neighbor X separation `>= 0.004`.
pub fn move_control_point(
    points: &[[f32; 2]],
    idx: usize,
    target: [f32; 2],
) -> Option<Vec<[f32; 2]>> {
    let mut updated = normalize_points(points);
    if idx >= updated.len() {
        return None;
    }
    let min_x = if idx == 0 {
        0.0
    } else {
        updated[idx - 1][0] + MIN_X_SEPARATION
    };
    let max_x = if idx + 1 >= updated.len() {
        1.0
    } else {
        updated[idx + 1][0] - MIN_X_SEPARATION
    };
    let clamped_x = target[0].clamp(0.0, 1.0).clamp(min_x, max_x.max(min_x));
    let clamped_y = target[1].clamp(0.0, 1.0);
    updated[idx] = [clamped_x, clamped_y];
    Some(updated)
}

/// Remove an interior control point (`0 < idx < len - 1`), never deleting below 2 endpoints.
pub fn remove_interior_point(points: &[[f32; 2]], idx: usize) -> Option<Vec<[f32; 2]>> {
    let mut updated = normalize_points(points);
    if updated.len() <= 2 || idx == 0 || idx + 1 >= updated.len() {
        return None;
    }
    updated.remove(idx);
    Some(updated)
}

/// Evaluate Fritsch–Carlson monotone cubic Hermite spline matching `photon_image::pixels::build_curve_lut`.
pub fn eval_monotone_spline(points: &[[f32; 2]], x: f32) -> f32 {
    let pts = normalize_points(points);
    let mut dedup: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for [px, py] in pts {
        let fx = f64::from(px);
        let fy = f64::from(py);
        if let Some(last) = dedup.last_mut()
            && (fx - last.0).abs() < 1e-4
        {
            last.1 = fy;
            continue;
        }
        dedup.push((fx, fy));
    }

    let n = dedup.len();
    if n < 2 {
        return x.clamp(0.0, 1.0);
    }

    let qx = f64::from(x.clamp(0.0, 1.0));
    if qx <= dedup[0].0 {
        return dedup[0].1.clamp(0.0, 1.0) as f32;
    }
    if qx >= dedup[n - 1].0 {
        return dedup[n - 1].1.clamp(0.0, 1.0) as f32;
    }

    let mut h = vec![0.0_f64; n - 1];
    let mut d = vec![0.0_f64; n - 1];
    for i in 0..(n - 1) {
        h[i] = dedup[i + 1].0 - dedup[i].0;
        d[i] = (dedup[i + 1].1 - dedup[i].1) / h[i].max(1e-6);
    }

    let mut m = vec![0.0_f64; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for i in 1..(n - 1) {
        if d[i - 1] * d[i] <= 0.0 {
            m[i] = 0.0;
        } else {
            let w1 = 2.0 * h[i] + h[i - 1];
            let w2 = h[i] + 2.0 * h[i - 1];
            m[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]);
        }
    }

    for i in 0..(n - 1) {
        if d[i].abs() < 1e-9 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
        } else {
            let alpha = m[i] / d[i];
            let beta = m[i + 1] / d[i];
            let s = alpha * alpha + beta * beta;
            if s > 9.0 {
                let tau = 3.0 / s.sqrt();
                m[i] = tau * alpha * d[i];
                m[i + 1] = tau * beta * d[i];
            }
        }
    }

    let seg = (0..(n - 1)).rfind(|&i| qx >= dedup[i].0).unwrap_or(0);
    let hi = h[seg].max(1e-6);
    let t = ((qx - dedup[seg].0) / hi).clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;

    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;

    let y = h00 * dedup[seg].1 + h10 * hi * m[seg] + h01 * dedup[seg + 1].1 + h11 * hi * m[seg + 1];
    y.clamp(0.0, 1.0) as f32
}

/// Sample a 256-entry `0..=255` lookup table from control points via [`eval_monotone_spline`].
pub fn sample_curve_lut(points: &[[f32; 2]]) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let y = eval_monotone_spline(points, i as f32 / 255.0);
        *slot = (y.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    lut
}

fn square_plot_rect(size: Size) -> Rectangle {
    let avail_w = (size.width - CURVE_PAD * 2.0).max(24.0);
    let avail_h = (size.height - CURVE_PAD * 2.0).max(24.0);
    let side = avail_w.min(avail_h);
    let x = (size.width - side) * 0.5;
    let y = (size.height - side) * 0.5;
    Rectangle::new(Point::new(x, y), Size::new(side, side))
}

fn to_screen(plot: Rectangle, pt: [f32; 2]) -> Point {
    Point::new(
        plot.x + pt[0].clamp(0.0, 1.0) * plot.width,
        plot.y + (1.0 - pt[1].clamp(0.0, 1.0)) * plot.height,
    )
}

fn from_screen(plot: Rectangle, pos: Point) -> [f32; 2] {
    let nx = ((pos.x - plot.x) / plot.width).clamp(0.0, 1.0);
    let ny = (1.0 - (pos.y - plot.y) / plot.height).clamp(0.0, 1.0);
    [nx, ny]
}

fn nearest_point(plot: Rectangle, pts: &[[f32; 2]], pos: Point) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (idx, &pt) in pts.iter().enumerate() {
        let p = to_screen(plot, pt);
        let dist = ((p.x - pos.x).powi(2) + (p.y - pos.y).powi(2)).sqrt();
        if dist <= POINT_HIT_RADIUS && best.is_none_or(|(_, d)| dist < d) {
            best = Some((idx, dist));
        }
    }
    best.map(|(idx, _)| idx)
}

impl<'a, M: Clone + 'a> canvas::Program<M> for CurveGraphProgram<'a, M> {
    type State = CurveGraphState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        let local_bounds = Rectangle::new(Point::ORIGIN, bounds.size());
        let plot = square_plot_rect(bounds.size());
        let pts = normalize_points(&self.points);

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let pos = cursor.position_in(bounds)?;
                if let Some(idx) = nearest_point(plot, &pts, pos) {
                    let is_double_click =
                        state.last_click.is_some_and(|(t, prev_pos, prev_idx)| {
                            prev_idx == idx
                                && t.elapsed().as_millis() <= DOUBLE_CLICK_WINDOW_MS
                                && ((prev_pos.x - pos.x).powi(2) + (prev_pos.y - pos.y).powi(2))
                                    <= DOUBLE_CLICK_DIST_SQ
                        });
                    if is_double_click {
                        state.last_click = None;
                        state.dragging_idx = None;
                        if let Some(updated) = remove_interior_point(&pts, idx) {
                            state.selected_idx = None;
                            state.hovered_idx = None;
                            return Some(
                                Action::publish((self.on_change)(CurveGraphEvent::PointsChanged(
                                    updated,
                                )))
                                .and_capture(),
                            );
                        }
                        return Some(Action::request_redraw().and_capture());
                    }

                    state.last_click = Some((Instant::now(), pos, idx));
                    state.dragging_idx = Some(idx);
                    state.selected_idx = Some(idx);
                    state.hovered_idx = Some(idx);
                    return Some(Action::request_redraw().and_capture());
                }

                state.last_click = None;
                let new_pt = from_screen(plot, pos);
                if let Some((updated, insert_idx)) = insert_control_point(&pts, new_pt) {
                    state.dragging_idx = Some(insert_idx);
                    state.selected_idx = Some(insert_idx);
                    state.hovered_idx = Some(insert_idx);
                    state.hover_pos = Some(updated[insert_idx]);
                    return Some(
                        Action::publish((self.on_change)(CurveGraphEvent::PointsChanged(updated)))
                            .and_capture(),
                    );
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let pos = cursor.position_in(bounds)?;
                if let Some(idx) = nearest_point(plot, &pts, pos)
                    && let Some(updated) = remove_interior_point(&pts, idx)
                {
                    state.dragging_idx = None;
                    state.selected_idx = None;
                    state.hovered_idx = None;
                    state.last_click = None;
                    return Some(
                        Action::publish((self.on_change)(CurveGraphEvent::PointsChanged(updated)))
                            .and_capture(),
                    );
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(idx) = state.dragging_idx
                    && let Some(pos) = cursor.position_from(bounds.position())
                {
                    let target = from_screen(plot, pos);
                    if let Some(updated) = move_control_point(&pts, idx, target) {
                        state.selected_idx = Some(idx);
                        state.hover_pos = Some(updated[idx]);
                        return Some(
                            Action::publish((self.on_change)(CurveGraphEvent::PointsChanged(
                                updated,
                            )))
                            .and_capture(),
                        );
                    }
                }

                let over = cursor
                    .position_in(bounds)
                    .filter(|p| local_bounds.contains(*p));
                let next_hover = over.and_then(|pos| nearest_point(plot, &pts, pos));
                let next_pos = over.map(|pos| from_screen(plot, pos));
                if next_hover != state.hovered_idx || next_pos != state.hover_pos {
                    state.hovered_idx = next_hover;
                    state.hover_pos = next_pos;
                    return Some(Action::request_redraw());
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging_idx.take().is_some() {
                    return Some(Action::request_redraw().and_capture());
                }
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let t = PaletteTokens::for_theme(theme);
        let mut frame = Frame::new(renderer, bounds.size());
        let plot = square_plot_rect(bounds.size());

        // Plot surface background
        frame.fill_rectangle(plot.position(), plot.size(), t.bg_canvas);

        // Optional translucent 256-bin histogram backdrop
        if let Some(hist) = &self.histogram {
            let max_bin = hist.iter().copied().max().unwrap_or(0);
            if max_bin > 0 {
                let max_f = max_bin as f32;
                let base_y = plot.y + plot.height;
                let hist_poly = Path::new(|builder| {
                    builder.move_to(Point::new(plot.x, base_y));
                    for (bin, &count) in hist.iter().enumerate() {
                        let frac_x = bin as f32 / 255.0;
                        let frac_h = (count as f32 / max_f).clamp(0.0, 1.0) * 0.88;
                        let x = plot.x + frac_x * plot.width;
                        let y = base_y - frac_h * plot.height;
                        builder.line_to(Point::new(x, y));
                    }
                    builder.line_to(Point::new(plot.x + plot.width, base_y));
                    builder.close();
                });
                frame.fill(&hist_poly, self.channel_color.scale_alpha(0.18));
            }
        }

        // 4x4 grid lines
        let grid_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(t.border.scale_alpha(0.75));
        let border_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(t.border_strong);
        for i in 0..=4 {
            let frac = i as f32 / 4.0;
            let x = plot.x + frac * plot.width;
            let y = plot.y + frac * plot.height;
            let stroke = if i == 0 || i == 4 {
                border_stroke
            } else {
                grid_stroke
            };
            frame.stroke(
                &Path::line(Point::new(x, plot.y), Point::new(x, plot.y + plot.height)),
                stroke,
            );
            frame.stroke(
                &Path::line(Point::new(plot.x, y), Point::new(plot.x + plot.width, y)),
                stroke,
            );
        }

        // Diagonal identity reference line
        let diag = Path::line(to_screen(plot, [0.0, 0.0]), to_screen(plot, [1.0, 1.0]));
        frame.stroke(
            &diag,
            Stroke::default()
                .with_width(1.0)
                .with_color(t.text_muted.scale_alpha(0.45)),
        );

        let pts = normalize_points(&self.points);

        // Smooth monotone cubic Hermite spline
        let curve = Path::new(|builder| {
            let steps = 128usize;
            for i in 0..=steps {
                let x = i as f32 / steps as f32;
                let y = eval_monotone_spline(&pts, x);
                let p = to_screen(plot, [x, y]);
                if i == 0 {
                    builder.move_to(p);
                } else {
                    builder.line_to(p);
                }
            }
        });
        frame.stroke(
            &curve,
            Stroke::default()
                .with_width(2.0)
                .with_color(self.channel_color),
        );

        // Control point handles
        let active_idx = state
            .dragging_idx
            .or(state.hovered_idx)
            .or(state.selected_idx)
            .filter(|&i| i < pts.len());

        for (idx, &pt) in pts.iter().enumerate() {
            let center = to_screen(plot, pt);
            let active = active_idx == Some(idx);
            let radius = if active { 5.5 } else { 4.2 };
            let circle = Path::circle(center, radius);
            frame.fill(
                &circle,
                if active {
                    self.channel_color
                } else {
                    t.bg_surface
                },
            );
            frame.stroke(
                &circle,
                Stroke::default().with_width(1.7).with_color(if active {
                    t.text_primary
                } else {
                    self.channel_color
                }),
            );
        }

        // Live (In, Out) 0..=255 readout for active point or hovered curve sample
        let readout_pt = active_idx.map(|i| pts[i]).or_else(|| {
            state
                .hover_pos
                .map(|[hx, _]| [hx, eval_monotone_spline(&pts, hx)])
        });
        if let Some([rx, ry]) = readout_pt {
            let in_255 = (rx.clamp(0.0, 1.0) * 255.0).round() as u8;
            let out_255 = (ry.clamp(0.0, 1.0) * 255.0).round() as u8;
            let label = format!("In {in_255} · Out {out_255}");

            // Avoid covering a point in the top-left quadrant by moving readout to bottom-right
            let place_bottom_right = rx < 0.45 && ry > 0.65;
            let pill_w = 104.0;
            let pill_h = 18.0;
            let pill_pos = if place_bottom_right {
                Point::new(
                    plot.x + plot.width - pill_w - 6.0,
                    plot.y + plot.height - pill_h - 6.0,
                )
            } else {
                Point::new(plot.x + 6.0, plot.y + 6.0)
            };
            let pill = Path::rounded_rectangle(pill_pos, Size::new(pill_w, pill_h), 3.0.into());
            frame.fill(&pill, t.bg_elevated.scale_alpha(0.88));
            frame.stroke(
                &pill,
                Stroke::default().with_width(1.0).with_color(t.border),
            );
            frame.fill_text(Text {
                content: label,
                position: Point::new(pill_pos.x + 6.0, pill_pos.y + 3.0),
                color: t.text_primary,
                size: 10.5.into(),
                ..Text::default()
            });
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging_idx.is_some() {
            mouse::Interaction::Grabbing
        } else if state.hovered_idx.is_some() {
            mouse::Interaction::Pointer
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spline_identity_and_endpoints_match_expected() {
        let linear = vec![[0.0, 0.0], [1.0, 1.0]];
        let lut = sample_curve_lut(&linear);
        for (i, &val) in lut.iter().enumerate() {
            assert_eq!(val as usize, i, "identity LUT mismatch at {i}");
        }

        let inverted = vec![[0.0, 1.0], [1.0, 0.0]];
        let inv_lut = sample_curve_lut(&inverted);
        assert_eq!(inv_lut[0], 255);
        assert_eq!(inv_lut[255], 0);
    }

    #[test]
    fn spline_remains_monotone_and_bounded_for_s_curve() {
        let s_curve = vec![[0.0, 0.0], [0.25, 0.12], [0.75, 0.88], [1.0, 1.0]];
        let lut = sample_curve_lut(&s_curve);
        for pair in lut.windows(2) {
            assert!(
                pair[0] <= pair[1],
                "monotone curve inverted: {} -> {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn insert_move_and_remove_enforce_separation_and_min_endpoints() {
        let pts = vec![[0.0, 0.0], [1.0, 1.0]];
        let (pts, idx) = insert_control_point(&pts, [0.5, 0.65]).expect("insert center point");
        assert_eq!(idx, 1);
        assert_eq!(pts.len(), 3);

        // Attempt to drag interior point past left/right neighbors — must clamp with >= 0.004 gap.
        let moved_left = move_control_point(&pts, 1, [0.0, 0.3]).expect("move left");
        assert!(moved_left[1][0] >= moved_left[0][0] + MIN_X_SEPARATION - 1e-6);

        let moved_right = move_control_point(&pts, 1, [1.0, 0.8]).expect("move right");
        assert!(moved_right[1][0] <= moved_right[2][0] - MIN_X_SEPARATION + 1e-6);

        // Removing endpoints is forbidden.
        assert!(remove_interior_point(&pts, 0).is_none());
        assert!(remove_interior_point(&pts, 2).is_none());

        // Removing interior point succeeds, leaving 2 endpoints; further removal forbidden.
        let removed = remove_interior_point(&pts, 1).expect("remove interior point");
        assert_eq!(removed.len(), 2);
        assert!(remove_interior_point(&removed, 0).is_none());
        assert!(remove_interior_point(&removed, 1).is_none());
    }

    #[test]
    fn insert_rejects_overcrowded_interval() {
        let pts = vec![[0.0, 0.0], [0.006, 0.1], [1.0, 1.0]];
        // Gap [0.0, 0.006] is < 2 * 0.004 = 0.008, so inserting at 0.003 has no room.
        assert!(insert_control_point(&pts, [0.003, 0.05]).is_none());
    }
}
