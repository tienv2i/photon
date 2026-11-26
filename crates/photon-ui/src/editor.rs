//! Reusable pure-`iced` Editor controls: tool rail buttons, FG/BG color pair,
//! 3x3 canvas anchor selector, inspector panel tabs, checker swatches, and
//! interactive tonal curves spline canvas.

use crate::typography::text;
use iced::mouse;
use iced::widget::canvas::{self, Action, Frame, Geometry, Path, Stroke};
use iced::widget::{
    Button, Space, button, column, container, image as iced_image, mouse_area, row, stack,
    text_input,
};
use iced::{
    Alignment, Border, Color, ContentFit, Element, Length, Point, Rectangle, Renderer, Size, Theme,
};

use crate::button::{self as ui_button, ButtonVariant};
use crate::icons::{self, Icon};
pub use crate::panel::{
    SliderStepperSpec, int_slider_stepper_row, numeric_stepper, slider_control_row, slider_stepper,
    slider_stepper_row,
};
use crate::theme::{
    PaletteTokens, RADIUS_MD, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, input_style,
};
use crate::tooltip::{TooltipPosition, with_optional_tooltip, with_tooltip_pos};

/// Compact square tool button for the Editor's left tool rail.
pub fn tool_rail_button<'a, M: Clone + 'a>(
    icon: Icon,
    active: bool,
    enabled: bool,
    on_press: Option<M>,
) -> Button<'a, M> {
    let icon_color = if active { Some(Color::WHITE) } else { None };

    let mut b = button(
        container(icons::render(icon, 16.0, icon_color))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(Length::Fixed(34.0))
    .height(Length::Fixed(34.0))
    .padding(0)
    .style(move |theme, status| {
        let t = PaletteTokens::for_theme(theme);
        let base = if active {
            button::Style {
                background: Some(t.accent.into()),
                text_color: t.accent_text,
                border: Border {
                    color: t.accent,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            }
        } else {
            button::Style {
                background: Some(t.bg_elevated.into()),
                text_color: t.text_secondary,
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            }
        };

        match status {
            button::Status::Active => base,
            button::Status::Hovered => {
                if active {
                    button::Style {
                        background: Some(t.accent_hover.into()),
                        ..base
                    }
                } else {
                    button::Style {
                        background: Some(t.bg_hover.into()),
                        text_color: t.text_primary,
                        border: Border {
                            color: t.border_strong,
                            ..base.border
                        },
                        ..base
                    }
                }
            }
            button::Status::Pressed => button::Style {
                background: Some(t.bg_pressed.into()),
                ..base
            },
            button::Status::Disabled => button::Style {
                background: base.background.map(|bg| bg.scale_alpha(0.45)),
                text_color: base.text_color.scale_alpha(0.45),
                border: Border {
                    color: base.border.color.scale_alpha(0.45),
                    ..base.border
                },
                ..base
            },
        }
    });

    if enabled && let Some(msg) = on_press {
        b = b.on_press(msg);
    }
    b
}

/// Left tool-rail button wrapped with a right-aligned tooltip (`TooltipPosition::Right`).
pub fn tool_rail_button_tipped<'a, M: Clone + 'a>(
    icon: Icon,
    active: bool,
    enabled: bool,
    on_press: Option<M>,
    tip: impl Into<String>,
) -> Element<'a, M> {
    with_tooltip_pos(
        tool_rail_button(icon, active, enabled, on_press),
        tip,
        TooltipPosition::Right,
    )
}

/// Overlapping Foreground / Background color swatches with Swap and Reset-to-B/W controls.
pub fn color_pair_widget<'a, M: Clone + 'a>(
    fg: Color,
    bg: Option<Color>,
    on_fg: M,
    on_bg: M,
    on_swap: M,
    on_reset: M,
) -> Element<'a, M> {
    let bg_swatch = container(checker_swatch(bg, 22.0, 22.0, Some(on_bg))).padding(iced::Padding {
        top: 14.0,
        left: 14.0,
        right: 0.0,
        bottom: 0.0,
    });

    let fg_swatch = checker_swatch(Some(fg), 22.0, 22.0, Some(on_fg));

    let overlap = stack![bg_swatch, fg_swatch]
        .width(Length::Fixed(36.0))
        .height(Length::Fixed(36.0));

    let swap_btn = ui_button::compact_icon(Icon::Swap, ButtonVariant::Ghost, Some(on_swap));
    let reset_btn = ui_button::compact_icon(Icon::Reset, ButtonVariant::Ghost, Some(on_reset));

    row![
        overlap,
        column![swap_btn, reset_btn]
            .spacing(2)
            .align_x(Alignment::Center),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// 3x3 canvas anchor selector (`0=TopLeft .. 4=Center .. 8=BottomRight`).
pub fn anchor_grid_3x3<'a, M: Clone + 'a>(
    selected: usize,
    on_select: impl Fn(usize) -> M,
) -> Element<'a, M> {
    const GLYPHS: [&str; 9] = ["↖", "↑", "↗", "←", "•", "→", "↙", "↓", "↘"];

    let mut grid_col = column![].spacing(4);
    for row_idx in 0..3 {
        let mut r = row![].spacing(4).align_y(Alignment::Center);
        for col_idx in 0..3 {
            let idx = row_idx * 3 + col_idx;
            let active = idx == selected;
            let glyph = GLYPHS[idx];
            r = r.push(
                button(
                    container(text(glyph).size(13))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center),
                )
                .width(Length::Fixed(32.0))
                .height(Length::Fixed(28.0))
                .padding(0)
                .style(ui_button::style_for(ButtonVariant::Tab { active }))
                .on_press(on_select(idx)),
            );
        }
        grid_col = grid_col.push(r);
    }
    grid_col.into()
}

/// Segmented inspector tab strip (`(label, optional_icon, active, on_select)`).
pub fn panel_tabs<'a, M: Clone + 'a>(tabs: Vec<(String, Option<Icon>, bool, M)>) -> Element<'a, M> {
    let mut r = row![].spacing(4).align_y(Alignment::Center);
    for (label, icon, active, on_select) in tabs {
        let icon_color = if active { Some(Color::WHITE) } else { None };
        let content: Element<'a, M> = if let Some(ic) = icon {
            row![
                icons::render(ic, 13.0, icon_color),
                text(label).size(TEXT_SIZE_CONTROL)
            ]
            .spacing(5)
            .align_y(Alignment::Center)
            .into()
        } else {
            text(label).size(TEXT_SIZE_CONTROL).into()
        };

        let mut btn = button(
            container(content)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([6, 8])
        .style(ui_button::style_for(ButtonVariant::Tab { active }));

        if !active {
            btn = btn.on_press(on_select);
        }
        r = r.push(btn);
    }

    container(r)
        .padding(3)
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

/// Color swatch displaying a crisp checkerboard when `color` is `None` (transparent).
pub fn checker_swatch<'a, M: Clone + 'a>(
    color: Option<Color>,
    width: f32,
    height: f32,
    on_press: Option<M>,
) -> Element<'a, M> {
    let graphic = iced::widget::canvas(CheckerSwatchProgram { color })
        .width(Length::Fixed(width))
        .height(Length::Fixed(height));

    let mut btn = button(graphic)
        .padding(1)
        .width(Length::Fixed(width + 2.0))
        .height(Length::Fixed(height + 2.0))
        .style(|theme, status| {
            let t = PaletteTokens::for_theme(theme);
            let border_color = match status {
                button::Status::Hovered | button::Status::Pressed => t.accent,
                _ => t.border_strong,
            };
            button::Style {
                background: Some(t.bg_surface.into()),
                text_color: t.text_primary,
                border: Border {
                    color: border_color,
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..Default::default()
            }
        });

    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

#[derive(Debug, Clone, Copy)]
struct CheckerSwatchProgram {
    color: Option<Color>,
}

impl<M> canvas::Program<M> for CheckerSwatchProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let size = bounds.size();

        match self.color {
            Some(c) if c.a >= 0.999 => {
                frame.fill_rectangle(Point::ORIGIN, size, c);
            }
            other => {
                draw_checkerboard(&mut frame, size, 5.0);
                if let Some(c) = other {
                    frame.fill_rectangle(Point::ORIGIN, size, c);
                } else {
                    let t = PaletteTokens::for_theme(theme);
                    let slash = Path::line(
                        Point::new(1.5, size.height - 1.5),
                        Point::new(size.width - 1.5, 1.5),
                    );
                    frame.stroke(
                        &slash,
                        Stroke::default().with_width(1.6).with_color(t.danger),
                    );
                }
            }
        }

        vec![frame.into_geometry()]
    }
}

fn draw_checkerboard(frame: &mut Frame, size: Size, cell: f32) {
    let light = Color::from_rgb8(0xd4, 0xd4, 0xd8);
    let dark = Color::from_rgb8(0x9a, 0x9a, 0xa2);
    let cols = (size.width / cell).ceil() as usize;
    let rows = (size.height / cell).ceil() as usize;
    for r in 0..rows {
        for c in 0..cols {
            let color = if (r + c) % 2 == 0 { light } else { dark };
            let x = c as f32 * cell;
            let y = r as f32 * cell;
            let w = (size.width - x).min(cell);
            let h = (size.height - y).min(cell);
            if w > 0.0 && h > 0.0 {
                frame.fill_rectangle(Point::new(x, y), Size::new(w, h), color);
            }
        }
    }
}

/// Interactive tonal curves spline editor (`[x, y]` points in `0.0..=1.0`).
///
/// Left-click adds or drags control points; right-click removes an interior control point.
pub fn curves_canvas<'a, M: Clone + 'a>(
    points: &'a [[f32; 2]],
    channel_color: Color,
    on_change: impl Fn(Vec<[f32; 2]>) -> M + 'a,
) -> Element<'a, M> {
    let program = CurvesCanvasProgram {
        points,
        channel_color,
        on_change: Box::new(on_change),
    };

    container(
        iced::widget::canvas(program)
            .width(Length::Fill)
            .height(Length::Fixed(180.0)),
    )
    .width(Length::Fill)
    .padding(1)
    .style(|theme| {
        let t = PaletteTokens::for_theme(theme);
        container::Style {
            background: Some(t.bg_canvas.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: RADIUS_SM.into(),
            },
            ..Default::default()
        }
    })
    .into()
}

struct CurvesCanvasProgram<'a, M> {
    points: &'a [[f32; 2]],
    channel_color: Color,
    on_change: Box<dyn Fn(Vec<[f32; 2]>) -> M + 'a>,
}

#[derive(Debug, Default)]
struct CurvesCanvasState {
    dragging_idx: Option<usize>,
    hovered_idx: Option<usize>,
}

const CURVE_PAD: f32 = 8.0;
const POINT_HIT_RADIUS: f32 = 10.0;

fn normalized_points(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut pts: Vec<[f32; 2]> = if points.is_empty() {
        vec![[0.0, 0.0], [1.0, 1.0]]
    } else {
        points
            .iter()
            .map(|[x, y]| [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)])
            .collect()
    };
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

fn plot_rect(size: Size) -> Rectangle {
    let w = (size.width - CURVE_PAD * 2.0).max(16.0);
    let h = (size.height - CURVE_PAD * 2.0).max(16.0);
    Rectangle::new(Point::new(CURVE_PAD, CURVE_PAD), Size::new(w, h))
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

/// Evaluate monotone cubic Hermite spline (Fritsch-Carlson) for sorted control points.
fn eval_monotone_spline(pts: &[[f32; 2]], x: f32) -> f32 {
    let n = pts.len();
    if n == 0 {
        return x.clamp(0.0, 1.0);
    }
    if x <= pts[0][0] {
        return pts[0][1];
    }
    if x >= pts[n - 1][0] {
        return pts[n - 1][1];
    }

    let mut d = vec![0.0_f32; n.saturating_sub(1)];
    let mut m = vec![0.0_f32; n];

    for i in 0..(n - 1) {
        let dx = (pts[i + 1][0] - pts[i][0]).max(1e-5);
        d[i] = (pts[i + 1][1] - pts[i][1]) / dx;
    }

    m[0] = d[0];
    for i in 1..(n - 1) {
        if d[i - 1] * d[i] <= 0.0 {
            m[i] = 0.0;
        } else {
            m[i] = 0.5 * (d[i - 1] + d[i]);
        }
    }
    m[n - 1] = d[n - 2];

    for i in 0..(n - 1) {
        if d[i].abs() < 1e-6 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
        } else {
            let a = m[i] / d[i];
            let b = m[i + 1] / d[i];
            let s = a * a + b * b;
            if s > 9.0 {
                let tau = 3.0 / s.sqrt();
                m[i] = tau * a * d[i];
                m[i + 1] = tau * b * d[i];
            }
        }
    }

    let seg = (0..(n - 1)).rfind(|&i| x >= pts[i][0]).unwrap_or(0);
    let h = (pts[seg + 1][0] - pts[seg][0]).max(1e-5);
    let t = ((x - pts[seg][0]) / h).clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;

    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;

    (h00 * pts[seg][1] + h10 * h * m[seg] + h01 * pts[seg + 1][1] + h11 * h * m[seg + 1])
        .clamp(0.0, 1.0)
}

impl<'a, M: Clone + 'a> canvas::Program<M> for CurvesCanvasProgram<'a, M> {
    type State = CurvesCanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        let local_bounds = Rectangle::new(Point::ORIGIN, bounds.size());
        let plot = plot_rect(bounds.size());
        let pts = normalized_points(self.points);

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let pos = cursor.position_in(bounds)?;
                if let Some(idx) = nearest_point(plot, &pts, pos) {
                    state.dragging_idx = Some(idx);
                    state.hovered_idx = Some(idx);
                    return Some(Action::capture());
                }

                let new_pt = from_screen(plot, pos);
                let mut updated = pts;
                let insert_idx = updated
                    .binary_search_by(|p| p[0].total_cmp(&new_pt[0]))
                    .unwrap_or_else(|i| i);
                updated.insert(insert_idx, new_pt);
                state.dragging_idx = Some(insert_idx);
                state.hovered_idx = Some(insert_idx);
                Some(Action::publish((self.on_change)(updated)).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let pos = cursor.position_in(bounds)?;
                if let Some(idx) = nearest_point(plot, &pts, pos)
                    && pts.len() > 2
                    && idx > 0
                    && idx + 1 < pts.len()
                {
                    let mut updated = pts;
                    updated.remove(idx);
                    state.dragging_idx = None;
                    state.hovered_idx = None;
                    return Some(Action::publish((self.on_change)(updated)).and_capture());
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(idx) = state.dragging_idx
                    && let Some(pos) = cursor.position_from(bounds.position())
                    && idx < pts.len()
                {
                    let mut updated = pts;
                    let [mut nx, ny] = from_screen(plot, pos);
                    let min_x = if idx == 0 {
                        0.0
                    } else {
                        updated[idx - 1][0] + 0.01
                    };
                    let max_x = if idx + 1 >= updated.len() {
                        1.0
                    } else {
                        updated[idx + 1][0] - 0.01
                    };
                    nx = nx.clamp(min_x, max_x.max(min_x));
                    updated[idx] = [nx, ny];
                    return Some(Action::publish((self.on_change)(updated)).and_capture());
                }

                let next_hover = cursor
                    .position_in(bounds)
                    .filter(|p| local_bounds.contains(*p))
                    .and_then(|pos| nearest_point(plot, &pts, pos));
                if next_hover != state.hovered_idx {
                    state.hovered_idx = next_hover;
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
        let plot = plot_rect(bounds.size());

        // 4x4 grid lines
        let grid_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(t.border.scale_alpha(0.65));
        for i in 0..=4 {
            let frac = i as f32 / 4.0;
            let x = plot.x + frac * plot.width;
            let y = plot.y + frac * plot.height;
            frame.stroke(
                &Path::line(Point::new(x, plot.y), Point::new(x, plot.y + plot.height)),
                grid_stroke,
            );
            frame.stroke(
                &Path::line(Point::new(plot.x, y), Point::new(plot.x + plot.width, y)),
                grid_stroke,
            );
        }

        // Identity diagonal reference
        let diag = Path::line(to_screen(plot, [0.0, 0.0]), to_screen(plot, [1.0, 1.0]));
        frame.stroke(
            &diag,
            Stroke::default()
                .with_width(1.0)
                .with_color(t.text_muted.scale_alpha(0.45)),
        );

        let pts = normalized_points(self.points);

        // Monotone spline curve
        let curve = Path::new(|builder| {
            let steps = 64usize;
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
        for (idx, &pt) in pts.iter().enumerate() {
            let center = to_screen(plot, pt);
            let active = state.dragging_idx == Some(idx) || state.hovered_idx == Some(idx);
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
                Stroke::default().with_width(1.6).with_color(if active {
                    t.text_primary
                } else {
                    self.channel_color
                }),
            );
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging_idx.is_some() || state.hovered_idx.is_some() {
            mouse::Interaction::Pointer
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

/// Small spacer helper for compact toolbars in Editor panels.
pub fn spacer_h<'a, M: 'a>(width: f32) -> Element<'a, M> {
    Space::new().width(Length::Fixed(width)).into()
}

/// Display / input unit for image and canvas dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LengthUnit {
    #[default]
    Px,
    Cm,
    Inch,
    Percent,
}

impl LengthUnit {
    pub const ALL: [Self; 4] = [Self::Px, Self::Cm, Self::Inch, Self::Percent];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Px => "px",
            Self::Cm => "cm",
            Self::Inch => "in",
            Self::Percent => "%",
        }
    }
}

/// Convert a pixel count to the chosen display unit at `dpi` (and `ref_px` for `%`).
pub fn px_to_unit(px: u32, unit: LengthUnit, dpi: f32, ref_px: u32) -> f32 {
    let safe_dpi = dpi.max(1.0);
    match unit {
        LengthUnit::Px => px as f32,
        LengthUnit::Inch => px as f32 / safe_dpi,
        LengthUnit::Cm => (px as f32 / safe_dpi) * 2.54,
        LengthUnit::Percent => {
            if ref_px == 0 {
                100.0
            } else {
                (px as f32 / ref_px as f32) * 100.0
            }
        }
    }
}

/// Convert a value in `unit` back to clamped pixel count (`>= 1`).
pub fn unit_to_px(val: f32, unit: LengthUnit, dpi: f32, ref_px: u32) -> u32 {
    let safe_dpi = dpi.max(1.0);
    let px = match unit {
        LengthUnit::Px => val,
        LengthUnit::Inch => val * safe_dpi,
        LengthUnit::Cm => (val / 2.54) * safe_dpi,
        LengthUnit::Percent => (val / 100.0) * ref_px.max(1) as f32,
    };
    if px.is_finite() {
        px.round().max(1.0) as u32
    } else {
        1
    }
}

/// Specification for `dimension_ratio_grid`.
pub struct DimensionRatioSpec<'a, M> {
    pub width_label: String,
    pub height_label: String,
    pub ratio_label: String,
    pub unit_label: String,
    pub dpi_label: String,
    pub width_str: &'a str,
    pub height_str: &'a str,
    pub ratio_str: &'a str,
    pub unit: LengthUnit,
    pub dpi: f32,
    pub lock_width: bool,
    pub lock_height: bool,
    pub lock_ratio: bool,
    pub on_width_change: Box<dyn Fn(String) -> M + 'a>,
    pub on_height_change: Box<dyn Fn(String) -> M + 'a>,
    pub on_ratio_change: Box<dyn Fn(String) -> M + 'a>,
    pub on_unit_change: Box<dyn Fn(LengthUnit) -> M + 'a>,
    pub on_dpi_change: Box<dyn Fn(f32) -> M + 'a>,
    pub on_toggle_lock_width: M,
    pub on_toggle_lock_height: M,
    pub on_toggle_lock_ratio: M,
    pub ratio_presets: Vec<(String, M)>,
}

/// Shared Width / Height / Ratio / Unit / DPI grid with lock toggles and preset chips.
pub fn dimension_ratio_grid<'a, M: Clone + 'a>(spec: DimensionRatioSpec<'a, M>) -> Element<'a, M> {
    let lock_btn = |locked: bool, msg: M| {
        ui_button::compact_icon(
            if locked { Icon::Lock } else { Icon::Unlock },
            if locked {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            Some(msg),
        )
    };

    let width_col = column![
        text(spec.width_label)
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
        row![
            text_input("", spec.width_str)
                .on_input(spec.on_width_change)
                .padding([6, 8])
                .size(TEXT_SIZE_CONTROL)
                .style(input_style)
                .width(Length::Fill),
            lock_btn(spec.lock_width, spec.on_toggle_lock_width),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    ]
    .spacing(3)
    .width(Length::Fill);

    let height_col = column![
        text(spec.height_label)
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
        row![
            text_input("", spec.height_str)
                .on_input(spec.on_height_change)
                .padding([6, 8])
                .size(TEXT_SIZE_CONTROL)
                .style(input_style)
                .width(Length::Fill),
            lock_btn(spec.lock_height, spec.on_toggle_lock_height),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    ]
    .spacing(3)
    .width(Length::Fill);

    let ratio_col = column![
        text(spec.ratio_label)
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
        row![
            text_input("4:3", spec.ratio_str)
                .on_input(spec.on_ratio_change)
                .padding([6, 8])
                .size(TEXT_SIZE_CONTROL)
                .style(input_style)
                .width(Length::Fill),
            lock_btn(spec.lock_ratio, spec.on_toggle_lock_ratio),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    ]
    .spacing(3)
    .width(Length::Fill);

    let mut unit_row = row![].spacing(3).align_y(Alignment::Center);
    for u in LengthUnit::ALL {
        let active = spec.unit == u;
        unit_row = unit_row.push(
            button(text(u.label()).size(TEXT_SIZE_CAPTION))
                .padding([4, 7])
                .style(ui_button::style_for(ButtonVariant::Tab { active }))
                .on_press((spec.on_unit_change)(u)),
        );
    }

    let unit_col = column![
        text(spec.unit_label)
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
        unit_row,
    ]
    .spacing(3);

    let mut root = column![
        row![width_col, height_col].spacing(8),
        row![ratio_col, unit_col].spacing(8).align_y(Alignment::End),
    ]
    .spacing(8)
    .width(Length::Fill);

    if matches!(spec.unit, LengthUnit::Cm | LengthUnit::Inch) {
        let mut dpi_row = row![
            text(spec.dpi_label).size(TEXT_SIZE_CAPTION).style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
            iced::widget::space::horizontal(),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        for preset_dpi in [72.0_f32, 96.0, 150.0, 300.0, 600.0] {
            let active = (spec.dpi - preset_dpi).abs() < 0.5;
            dpi_row = dpi_row.push(
                button(text(format!("{}", preset_dpi as u32)).size(TEXT_SIZE_CAPTION))
                    .padding([3, 6])
                    .style(ui_button::style_for(ButtonVariant::Tab { active }))
                    .on_press((spec.on_dpi_change)(preset_dpi)),
            );
        }
        root = root.push(dpi_row);
    }

    if !spec.ratio_presets.is_empty() {
        let mut presets_row = row![].spacing(4).align_y(Alignment::Center);
        for (label, msg) in spec.ratio_presets {
            presets_row = presets_row.push(
                button(text(label).size(TEXT_SIZE_CAPTION))
                    .padding([3, 7])
                    .style(ui_button::style_for(ButtonVariant::Secondary))
                    .on_press(msg),
            );
        }
        root = root.push(presets_row);
    }

    root.into()
}

/// Descriptor for a compact KogaView-style layer stack row built via [`layer_row`].
pub struct LayerRowSpec<'a, M> {
    pub name: String,
    pub subtitle: Option<String>,
    pub active: bool,
    pub visible: bool,
    pub locked: bool,
    pub has_mask: bool,
    pub mask_enabled: bool,
    pub mask_linked: bool,
    pub editing_mask: bool,
    pub thumbnail: Option<iced_image::Handle>,
    pub mask_thumbnail: Option<iced_image::Handle>,
    pub renaming: Option<&'a str>,
    pub on_select: M,
    pub on_toggle_visible: M,
    pub on_toggle_lock: M,
    pub on_toggle_mask_edit: Option<M>,
    pub on_toggle_mask_link: Option<M>,
    pub on_toggle_mask: Option<M>,
    pub on_start_rename: Option<M>,
    pub on_rename_input: Option<Box<dyn Fn(String) -> M + 'a>>,
    pub on_rename_submit: Option<M>,
    pub on_right_press: Option<M>,
    pub visibility_tooltip: Option<String>,
    pub lock_tooltip: Option<String>,
    pub mask_tooltip: Option<String>,
    pub mask_link_tooltip: Option<String>,
    pub mask_toggle_tooltip: Option<String>,
    pub rename_tooltip: Option<String>,
}

/// KogaView-style layer row with visibility eye, live layer thumbnail, optional mask link +
/// grayscale mask thumbnail (with disabled badge), inline rename field, quick add/remove mask,
/// lock toggle, and right-click context menu.
pub fn layer_row<'a, M: Clone + 'a>(spec: LayerRowSpec<'a, M>) -> Element<'a, M> {
    let active = spec.active;
    let editing_mask = spec.editing_mask;
    let visible = spec.visible;
    let mask_enabled = spec.mask_enabled;

    let vis_btn = with_optional_tooltip(
        ui_button::compact_icon(
            if visible { Icon::Eye } else { Icon::EyeOff },
            ButtonVariant::Ghost,
            Some(spec.on_toggle_visible),
        ),
        spec.visibility_tooltip,
        TooltipPosition::Top,
    );

    let thumb_inner: Element<'a, M> = if let Some(handle) = spec.thumbnail {
        iced_image(handle)
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(28.0))
            .content_fit(ContentFit::Contain)
            .into()
    } else {
        icons::render_muted(Icon::Layers, 15.0).into()
    };

    let thumb_box = button(
        container(thumb_inner)
            .width(Length::Fixed(30.0))
            .height(Length::Fixed(30.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |theme| {
                let t = PaletteTokens::for_theme(theme);
                container::Style {
                    background: Some(t.bg_app.into()),
                    border: Border {
                        color: if active && !editing_mask {
                            t.accent
                        } else {
                            t.border
                        },
                        width: if active && !editing_mask { 1.5 } else { 1.0 },
                        radius: 3.0.into(),
                    },
                    ..Default::default()
                }
            }),
    )
    .padding(0)
    .style(|_theme, _status| button::Style {
        background: None,
        ..Default::default()
    })
    .on_press(spec.on_select.clone());

    let center_el: Element<'a, M> = if let Some(rename_val) = spec.renaming {
        let mut ti = text_input("", rename_val)
            .padding([4, 6])
            .size(TEXT_SIZE_CONTROL)
            .style(input_style)
            .width(Length::Fill);
        if let Some(on_input) = spec.on_rename_input {
            ti = ti.on_input(on_input);
        }
        if let Some(on_submit) = spec.on_rename_submit {
            ti = ti.on_submit(on_submit);
        }
        ti.into()
    } else {
        let mut label_col = column![
            text(spec.name)
                .size(TEXT_SIZE_CONTROL)
                .wrapping(text::Wrapping::None)
                .style(move |theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(if visible {
                            t.text_primary
                        } else {
                            t.text_muted
                        }),
                    }
                }),
        ]
        .spacing(1)
        .width(Length::Fill);
        if let Some(sub) = spec.subtitle {
            label_col = label_col.push(text(sub).size(10).wrapping(text::Wrapping::None).style(
                |theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                },
            ));
        }
        button(label_col)
            .padding([3, 4])
            .width(Length::Fill)
            .style(|_theme, _status| button::Style {
                background: None,
                text_color: Color::WHITE,
                ..Default::default()
            })
            .on_press(spec.on_select)
            .into()
    };

    let mut r = row![vis_btn].spacing(4).align_y(Alignment::Center);

    if spec.has_mask {
        let mask_inner: Element<'a, M> = if let Some(handle) = spec.mask_thumbnail {
            let img_el: Element<'a, M> = iced_image(handle)
                .width(Length::Fixed(28.0))
                .height(Length::Fixed(28.0))
                .content_fit(ContentFit::Contain)
                .into();
            if mask_enabled {
                img_el
            } else {
                iced::widget::stack![
                    img_el,
                    container(text("✕").size(14).style(|_| text::Style {
                        color: Some(Color::from_rgb(0.95, 0.30, 0.30)),
                    }))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                ]
                .into()
            }
        } else {
            icons::render_muted(Icon::Mask, 14.0).into()
        };

        let mask_box_container = container(mask_inner)
            .width(Length::Fixed(30.0))
            .height(Length::Fixed(30.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |theme| {
                let t = PaletteTokens::for_theme(theme);
                container::Style {
                    background: Some(Color::from_rgb(0.08, 0.08, 0.09).into()),
                    border: Border {
                        color: if active && editing_mask {
                            t.accent
                        } else if mask_enabled {
                            t.border
                        } else {
                            Color::from_rgb(0.85, 0.28, 0.28)
                        },
                        width: if active && editing_mask { 2.0 } else { 1.0 },
                        radius: 3.0.into(),
                    },
                    ..Default::default()
                }
            });
        let mut mask_btn = button(mask_box_container)
            .padding(0)
            .style(|_theme, _status| button::Style {
                background: None,
                ..Default::default()
            });
        if let Some(msg) = spec.on_toggle_mask_edit {
            mask_btn = mask_btn.on_press(msg);
        }

        // Link button sits BETWEEN the layer thumbnail and the mask thumbnail:
        // [Eye] [Layer Thumb] [Link] [Mask Thumb] ...
        let link_btn = with_optional_tooltip(
            ui_button::compact_icon(
                if spec.mask_linked {
                    Icon::Lock
                } else {
                    Icon::Unlock
                },
                ButtonVariant::Tab {
                    active: spec.mask_linked,
                },
                spec.on_toggle_mask_link,
            ),
            spec.mask_link_tooltip,
            TooltipPosition::Top,
        );
        let thumbs_pair = row![
            thumb_box,
            link_btn,
            with_optional_tooltip(mask_btn, spec.mask_tooltip, TooltipPosition::Top),
        ]
        .spacing(3)
        .align_y(Alignment::Center);
        r = r.push(thumbs_pair);
    } else {
        r = r.push(thumb_box);
    }

    r = r.push(center_el);

    if let Some(on_rename) = spec.on_start_rename {
        r = r.push(with_optional_tooltip(
            ui_button::compact_icon(Icon::Rename, ButtonVariant::Ghost, Some(on_rename)),
            spec.rename_tooltip,
            TooltipPosition::Top,
        ));
    }

    if spec.on_toggle_mask.is_some() || spec.has_mask {
        r = r.push(with_optional_tooltip(
            ui_button::compact_icon(
                if spec.has_mask {
                    Icon::Trash
                } else {
                    Icon::Mask
                },
                ButtonVariant::Ghost,
                spec.on_toggle_mask,
            ),
            spec.mask_toggle_tooltip,
            TooltipPosition::Top,
        ));
    }

    let lock_btn = with_optional_tooltip(
        ui_button::compact_icon(
            if spec.locked {
                Icon::Lock
            } else {
                Icon::Unlock
            },
            ButtonVariant::Ghost,
            Some(spec.on_toggle_lock),
        ),
        spec.lock_tooltip,
        TooltipPosition::Top,
    );
    r = r.push(lock_btn);

    let card = container(r)
        .padding([4, 6])
        .width(Length::Fill)
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

    if let Some(right_msg) = spec.on_right_press {
        mouse_area(card).on_right_press(right_msg).into()
    } else {
        card.into()
    }
}

/// Descriptor for a single action button in [`layer_action_bar`].
pub struct LayerActionButton<M> {
    pub icon: Icon,
    pub active: bool,
    pub tooltip: Option<String>,
    pub on_press: Option<M>,
}

impl<M> LayerActionButton<M> {
    pub fn new(icon: Icon, on_press: Option<M>) -> Self {
        Self {
            icon,
            active: false,
            tooltip: None,
            on_press,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn tooltip(mut self, tip: impl Into<String>) -> Self {
        self.tooltip = Some(tip.into());
        self
    }
}

/// Compact layer stack bottom action bar (Add / Duplicate / Move Up / Down / Merge / Flatten / Delete).
pub fn layer_action_bar<'a, M: Clone + 'a>(actions: Vec<LayerActionButton<M>>) -> Element<'a, M> {
    let mut r = row![].spacing(4).align_y(Alignment::Center);
    for act in actions {
        let variant = if act.active {
            ButtonVariant::Tab { active: true }
        } else {
            ButtonVariant::Secondary
        };
        let btn = ui_button::IconButton::new(act.icon)
            .size(14.0)
            .padding([5, 6])
            .variant(variant)
            .maybe_on_press(act.on_press)
            .maybe_tooltip(act.tooltip)
            .tooltip_position(TooltipPosition::Top)
            .into_element();
        r = r.push(
            container(btn)
                .width(Length::Fill)
                .align_x(Alignment::Center),
        );
    }
    r.width(Length::Fill).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_points_handles_empty_single_and_unsorted() {
        assert_eq!(normalized_points(&[]), vec![[0.0, 0.0], [1.0, 1.0]]);
        assert_eq!(
            normalized_points(&[[0.2, 0.4]]),
            vec![[0.2, 0.4], [1.0, 1.0]]
        );
        assert_eq!(
            normalized_points(&[[0.8, 0.9], [0.1, 0.2]]),
            vec![[0.1, 0.2], [0.8, 0.9]]
        );
    }

    #[test]
    fn monotone_spline_is_identity_and_clamped() {
        let linear = vec![[0.0, 0.0], [1.0, 1.0]];
        assert!((eval_monotone_spline(&linear, 0.5) - 0.5).abs() < 1e-4);

        let s_curve = vec![[0.0, 0.0], [0.25, 0.15], [0.75, 0.85], [1.0, 1.0]];
        for step in 0..=20 {
            let x = step as f32 / 20.0;
            let y = eval_monotone_spline(&s_curve, x);
            assert!((0.0..=1.0).contains(&y), "y out of bounds at x={x}: {y}");
        }
    }

    #[test]
    fn length_unit_conversions_roundtrip_accurately() {
        assert_eq!(px_to_unit(600, LengthUnit::Inch, 300.0, 1200), 2.0);
        assert_eq!(unit_to_px(2.0, LengthUnit::Inch, 300.0, 1200), 600);
        let cm = px_to_unit(300, LengthUnit::Cm, 300.0, 1200);
        assert!((cm - 2.54).abs() < 1e-3);
        assert_eq!(unit_to_px(cm, LengthUnit::Cm, 300.0, 1200), 300);
        assert_eq!(px_to_unit(600, LengthUnit::Percent, 96.0, 1200), 50.0);
        assert_eq!(unit_to_px(50.0, LengthUnit::Percent, 96.0, 1200), 600);
        assert_eq!(unit_to_px(0.0, LengthUnit::Px, 96.0, 100), 1);
    }
}
