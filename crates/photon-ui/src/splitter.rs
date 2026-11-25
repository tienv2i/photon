//! Interactive horizontal and vertical panel splitter handles for resizing docked panels.

use std::time::{Duration, Instant};

use iced::mouse;
use iced::widget::canvas::{self, Action, Frame, Geometry, Path, Stroke};
use iced::{Element, Length, Point, Rectangle, Renderer, Theme};

use crate::theme::PaletteTokens;

/// Side of the viewport where a docked panel sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSplitterEdge {
    Left,
    Right,
}

impl PanelSplitterEdge {
    pub const fn direction(self) -> f32 {
        match self {
            Self::Left => 1.0,
            Self::Right => -1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SplitterAxis {
    HorizontalPx { direction: f32 },
    VerticalPct { container_height_hint: i32 },
}

/// Interactive 6px horizontal resize handle for docked sidebars (`direction`: `+1.0` left, `-1.0` right).
///
/// Double-clicking resets the panel width to `default_w`.
pub fn panel_splitter<'a, M: Clone + 'a>(
    width_px: f32,
    direction: f32,
    min_w: f32,
    max_w: f32,
    default_w: f32,
    on_drag: impl Fn(f32) -> M + 'a,
    on_release: impl Fn(f32) -> M + 'a,
) -> Element<'a, M> {
    let program = SplitterProgram {
        axis: SplitterAxis::HorizontalPx { direction },
        value: width_px,
        min_val: min_w,
        max_val: max_w,
        default_val: default_w,
        on_drag: Box::new(on_drag),
        on_release: Box::new(on_release),
    };

    iced::widget::canvas(program)
        .width(Length::Fixed(6.0))
        .height(Length::Fill)
        .into()
}

/// Interactive 6px full-width vertical resize handle for stacked panels (`ratio_pct` in `min_pct..=max_pct`).
///
/// Double-clicking resets the split ratio to `default_pct`.
pub fn vertical_panel_splitter<'a, M: Clone + 'a>(
    ratio_pct: f32,
    container_height_hint: f32,
    min_pct: f32,
    max_pct: f32,
    default_pct: f32,
    on_drag: impl Fn(f32) -> M + 'a,
    on_release: impl Fn(f32) -> M + 'a,
) -> Element<'a, M> {
    let program = SplitterProgram {
        axis: SplitterAxis::VerticalPct {
            container_height_hint: container_height_hint.max(100.0).round() as i32,
        },
        value: ratio_pct,
        min_val: min_pct,
        max_val: max_pct,
        default_val: default_pct,
        on_drag: Box::new(on_drag),
        on_release: Box::new(on_release),
    };

    iced::widget::canvas(program)
        .width(Length::Fill)
        .height(Length::Fixed(6.0))
        .into()
}

struct SplitterProgram<'a, M> {
    axis: SplitterAxis,
    value: f32,
    min_val: f32,
    max_val: f32,
    default_val: f32,
    on_drag: Box<dyn Fn(f32) -> M + 'a>,
    on_release: Box<dyn Fn(f32) -> M + 'a>,
}

#[derive(Debug, Default)]
struct SplitterState {
    dragging: bool,
    hovered: bool,
    drag_start_coord: f32,
    start_value: f32,
    current_value: f32,
    last_click: Option<Instant>,
}

fn clamp_rounded(raw: f32, min_val: f32, max_val: f32) -> f32 {
    let hi = max_val.max(min_val);
    raw.clamp(min_val, hi).round()
}

impl<M> SplitterProgram<'_, M> {
    fn coord(&self, pos: Point) -> f32 {
        match self.axis {
            SplitterAxis::HorizontalPx { .. } => pos.x,
            SplitterAxis::VerticalPct { .. } => pos.y,
        }
    }

    fn delta_value(&self, current_coord: f32, start_coord: f32) -> f32 {
        let px_delta = current_coord - start_coord;
        match self.axis {
            SplitterAxis::HorizontalPx { direction } => px_delta * direction,
            SplitterAxis::VerticalPct {
                container_height_hint,
            } => (px_delta / (container_height_hint as f32).max(100.0)) * 100.0,
        }
    }
}

impl<'a, M: Clone + 'a> canvas::Program<M> for SplitterProgram<'a, M> {
    type State = SplitterState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if !cursor.is_over(bounds) {
                    return None;
                }
                let pos = cursor.position()?;
                let now = Instant::now();
                if let Some(prev) = state.last_click
                    && now.duration_since(prev) <= Duration::from_millis(350)
                {
                    state.last_click = None;
                    state.dragging = false;
                    let reset_val = clamp_rounded(self.default_val, self.min_val, self.max_val);
                    state.current_value = reset_val;
                    return Some(Action::publish((self.on_release)(reset_val)).and_capture());
                }

                state.last_click = Some(now);
                state.dragging = true;
                state.drag_start_coord = self.coord(pos);
                state.start_value = self.value;
                state.current_value = self.value;
                Some(Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.dragging {
                    let delta = self.delta_value(self.coord(*position), state.drag_start_coord);
                    let next_val =
                        clamp_rounded(state.start_value + delta, self.min_val, self.max_val);
                    state.current_value = next_val;
                    return Some(Action::publish((self.on_drag)(next_val)).and_capture());
                }
                let over = cursor.is_over(bounds);
                if over != state.hovered {
                    state.hovered = over;
                    return Some(Action::request_redraw());
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging {
                    state.dragging = false;
                    let final_val = clamp_rounded(state.current_value, self.min_val, self.max_val);
                    return Some(Action::publish((self.on_release)(final_val)).and_capture());
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
        let line = match self.axis {
            SplitterAxis::HorizontalPx { .. } => {
                let cx = bounds.width * 0.5;
                Path::line(Point::new(cx, 0.0), Point::new(cx, bounds.height))
            }
            SplitterAxis::VerticalPct { .. } => {
                let cy = bounds.height * 0.5;
                Path::line(Point::new(0.0, cy), Point::new(bounds.width, cy))
            }
        };

        let (width, color) = if state.dragging || state.hovered {
            (2.0, t.accent)
        } else {
            (1.0, t.border)
        };
        frame.stroke(&line, Stroke::default().with_width(width).with_color(color));
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging || cursor.is_over(bounds) {
            match self.axis {
                SplitterAxis::HorizontalPx { .. } => mouse::Interaction::ResizingHorizontally,
                SplitterAxis::VerticalPct { .. } => mouse::Interaction::ResizingVertically,
            }
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitter_clamps_and_computes_horizontal_and_vertical_deltas() {
        assert_eq!(PanelSplitterEdge::Left.direction(), 1.0);
        assert_eq!(PanelSplitterEdge::Right.direction(), -1.0);
        assert_eq!(clamp_rounded(120.4, 180.0, 420.0), 180.0);
        assert_eq!(clamp_rounded(500.2, 180.0, 420.0), 420.0);
        assert_eq!(clamp_rounded(256.6, 180.0, 420.0), 257.0);

        let v_prog = SplitterProgram {
            axis: SplitterAxis::VerticalPct {
                container_height_hint: 500,
            },
            value: 58.0,
            min_val: 20.0,
            max_val: 80.0,
            default_val: 58.0,
            on_drag: Box::new(|v| v),
            on_release: Box::new(|v| v),
        };
        assert!((v_prog.delta_value(150.0, 100.0) - 10.0).abs() < 1e-4);
        assert_eq!(
            clamp_rounded(58.0 + v_prog.delta_value(300.0, 100.0), 20.0, 80.0),
            80.0
        );
    }
}
