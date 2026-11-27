//! Dedicated lightweight GPU canvas for Photon Viewer (`Single` & `Double` spread modes).
//!
//! Contains zero Editor state (no layers, brushes, vector overlays, crop, or selection masks),
//! compiling and warming up only a single minimal textured-quad WGSL shader.

mod gpu;

use std::sync::Arc;

use iced::mouse::{self, Cursor};
use iced::widget::shader::{self, Action};
use iced::{Event, Point, Rectangle};
use photon_protocol::{CanvasAntiAliasingMode, ColorAdjustments, ViewportDenoiseMode};

use gpu::ViewerQuadPrimitive;

#[derive(Debug, Clone)]
pub struct ViewerImageFrame {
    pub key: String,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ViewerCanvasTool {
    #[default]
    Pointer,
    Hand,
    MarqueeRect,
}

#[derive(Debug, Clone)]
pub enum ViewerCanvasEvent {
    ZoomBy {
        factor: f32,
        cursor: Option<[f32; 2]>,
        bounds: [f32; 2],
    },
    PanBy {
        dx: f32,
        dy: f32,
    },
    ResetFit,
    ContextMenuRequested {
        pos: [f32; 2],
        is_right_pair: bool,
    },
    SelectMarqueeCommit {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}

/// Compute unified spread dimensions `(spread_w, spread_h, left_disp_w, gap_img)`.
pub fn dual_spread_geometry(
    left_size: (u32, u32),
    right_size: (u32, u32),
    gap_px: f32,
) -> (f32, f32, f32, f32) {
    let lw = left_size.0.max(1) as f32;
    let lh = left_size.1.max(1) as f32;
    let rw = right_size.0.max(1) as f32;
    let rh = right_size.1.max(1) as f32;
    let target_h = lh.max(rh);
    let left_w = (lw * (target_h / lh)).round();
    let right_w = (rw * (target_h / rh)).round();
    let gap = if gap_px > 0.0 {
        gap_px * (target_h / 900.0).max(1.0)
    } else {
        0.0
    };
    (left_w + gap + right_w, target_h, left_w, gap)
}

#[derive(Debug, Clone)]
pub struct ViewerCanvasProgram {
    pub frame: Option<ViewerImageFrame>,
    pub pair_frame: Option<ViewerImageFrame>,
    pub dual_gap_px: f32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub adjustments: ColorAdjustments,
    pub dark_theme: bool,
    pub quarter_turns: u8,
    pub tool: ViewerCanvasTool,
    pub viewport_bg: Option<[u8; 3]>,
    pub aa_mode: CanvasAntiAliasingMode,
    pub denoise_mode: ViewportDenoiseMode,
}

impl Default for ViewerCanvasProgram {
    fn default() -> Self {
        Self {
            frame: None,
            pair_frame: None,
            dual_gap_px: 0.0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            adjustments: ColorAdjustments::default(),
            dark_theme: true,
            quarter_turns: 0,
            tool: ViewerCanvasTool::Pointer,
            viewport_bg: None,
            aa_mode: CanvasAntiAliasingMode::default(),
            denoise_mode: ViewportDenoiseMode::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ViewerCanvasState {
    dragging: bool,
    last_cursor: Option<Point>,
    marquee_start: Option<Point>,
    marquee_current: Option<Point>,
    last_click_time: Option<std::time::Instant>,
}

impl ViewerCanvasProgram {
    fn effective_doc_size(&self) -> Option<(f32, f32)> {
        let frame = self.frame.as_ref()?;
        if let Some(pair) = &self.pair_frame {
            let (sw, sh, _, _) = dual_spread_geometry(
                (frame.width, frame.height),
                (pair.width, pair.height),
                self.dual_gap_px,
            );
            Some((sw, sh))
        } else {
            Some((frame.width.max(1) as f32, frame.height.max(1) as f32))
        }
    }

    fn screen_to_image_uv(&self, pos: Point, bounds: Rectangle) -> Option<[f32; 2]> {
        let (doc_w, doc_h) = self.effective_doc_size()?;
        let turns = self.quarter_turns % 4;
        let (rot_w, rot_h) = if turns % 2 == 1 {
            (doc_h, doc_w)
        } else {
            (doc_w, doc_h)
        };
        let fit = (bounds.width / rot_w.max(1.0)).min(bounds.height / rot_h.max(1.0));
        let scale = (fit * self.zoom.max(0.02)).max(0.0001);
        let disp_w = rot_w * scale;
        let disp_h = rot_h * scale;
        let origin_x = bounds.x + (bounds.width - disp_w) * 0.5 + self.pan_x;
        let origin_y = bounds.y + (bounds.height - disp_h) * 0.5 + self.pan_y;
        let u = (pos.x - origin_x) / disp_w;
        let v = (pos.y - origin_y) / disp_h;
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        let uv = match turns {
            1 => [v, 1.0 - u],
            2 => [1.0 - u, 1.0 - v],
            3 => [1.0 - v, u],
            _ => [u, v],
        };
        Some(uv)
    }

    fn screen_to_doc_px(&self, pos: Point, bounds: Rectangle) -> Option<[f32; 2]> {
        let (doc_w, doc_h) = self.effective_doc_size()?;
        let uv = self.screen_to_image_uv(pos, bounds)?;
        Some([uv[0] * doc_w, uv[1] * doc_h])
    }
}

impl shader::Program<ViewerCanvasEvent> for ViewerCanvasProgram {
    type State = ViewerCanvasState;
    type Primitive = ViewerQuadPrimitive;

    fn draw(&self, state: &Self::State, _cursor: Cursor, bounds: Rectangle) -> Self::Primitive {
        let marquee_rect = match (state.marquee_start, state.marquee_current) {
            (Some(a), Some(b)) => {
                let x = a.x.min(b.x) - bounds.x;
                let y = a.y.min(b.y) - bounds.y;
                let w = (b.x - a.x).abs();
                let h = (b.y - a.y).abs();
                if w >= 2.0 && h >= 2.0 {
                    Some([x, y, w, h])
                } else {
                    None
                }
            }
            _ => None,
        };

        ViewerQuadPrimitive {
            frame: self.frame.clone(),
            pair_frame: self.pair_frame.clone(),
            dual_gap_px: self.dual_gap_px,
            zoom: self.zoom,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            adjustments: self.adjustments,
            dark_theme: self.dark_theme,
            quarter_turns: self.quarter_turns,
            viewport_bg: self.viewport_bg,
            aa_mode: self.aa_mode,
            denoise_mode: self.denoise_mode,
            marquee_rect,
            bounds_size: [bounds.width.max(1.0), bounds.height.max(1.0)],
        }
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<ViewerCanvasEvent>> {
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let dy = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y * 40.0,
                    mouse::ScrollDelta::Pixels { y, .. } => *y,
                };
                if dy.abs() > 0.01 {
                    let factor = if dy > 0.0 { 1.12 } else { 1.0 / 1.12 };
                    let rel_cursor = cursor.position_in(bounds).map(|p| [p.x, p.y]);
                    return Some(
                        Action::publish(ViewerCanvasEvent::ZoomBy {
                            factor,
                            cursor: rel_cursor,
                            bounds: [bounds.width.max(1.0), bounds.height.max(1.0)],
                        })
                        .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if cursor.is_over(bounds) =>
            {
                if let Some(pos) = cursor.position() {
                    let mut is_right_pair = false;
                    if let (Some(frame), Some(pair)) = (&self.frame, &self.pair_frame)
                        && let Some(uv) = self.screen_to_image_uv(pos, bounds)
                    {
                        let (sw, _, left_w, gap) = dual_spread_geometry(
                            (frame.width, frame.height),
                            (pair.width, pair.height),
                            self.dual_gap_px,
                        );
                        let split_uv = (left_w + gap * 0.5) / sw.max(1.0);
                        is_right_pair = uv[0] > split_uv;
                    }
                    return Some(
                        Action::publish(ViewerCanvasEvent::ContextMenuRequested {
                            pos: [pos.x, pos.y],
                            is_right_pair,
                        })
                        .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle))
                if cursor.is_over(bounds) =>
            {
                state.dragging = true;
                state.last_cursor = cursor.position();
                return Some(Action::capture());
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                if self.tool == ViewerCanvasTool::MarqueeRect {
                    let pos = cursor.position()?;
                    state.marquee_start = Some(pos);
                    state.marquee_current = Some(pos);
                    return Some(Action::capture());
                }
                let now = std::time::Instant::now();
                if let Some(prev) = state.last_click_time
                    && now.duration_since(prev).as_millis() < 320
                {
                    state.last_click_time = None;
                    state.dragging = false;
                    return Some(Action::publish(ViewerCanvasEvent::ResetFit).and_capture());
                }
                state.last_click_time = Some(now);
                state.dragging = true;
                state.last_cursor = cursor.position();
                return Some(Action::capture());
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.marquee_start.is_some() {
                    if let Some(pos) = cursor.position() {
                        state.marquee_current = Some(pos);
                        return Some(Action::request_redraw());
                    }
                } else if state.dragging
                    && let Some(pos) = cursor.position()
                {
                    if let Some(prev) = state.last_cursor {
                        let dx = pos.x - prev.x;
                        let dy = pos.y - prev.y;
                        state.last_cursor = Some(pos);
                        if dx.abs() > 0.0 || dy.abs() > 0.0 {
                            return Some(
                                Action::publish(ViewerCanvasEvent::PanBy { dx, dy }).and_capture(),
                            );
                        }
                    } else {
                        state.last_cursor = Some(pos);
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Left | mouse::Button::Middle,
            )) => {
                if let (Some(a), Some(b)) =
                    (state.marquee_start.take(), state.marquee_current.take())
                    && let (Some(da), Some(db)) = (
                        self.screen_to_doc_px(a, bounds),
                        self.screen_to_doc_px(b, bounds),
                    )
                {
                    let x = da[0].min(db[0]);
                    let y = da[1].min(db[1]);
                    let w = (db[0] - da[0]).abs();
                    let h = (db[1] - da[1]).abs();
                    if w >= 4.0 && h >= 4.0 {
                        return Some(
                            Action::publish(ViewerCanvasEvent::SelectMarqueeCommit {
                                x,
                                y,
                                width: w,
                                height: h,
                            })
                            .and_capture(),
                        );
                    }
                    return Some(Action::request_redraw());
                }
                state.dragging = false;
                state.last_cursor = None;
            }
            _ => {}
        }
        None
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if !cursor.is_over(bounds) {
            return mouse::Interaction::Idle;
        }
        match self.tool {
            ViewerCanvasTool::MarqueeRect => mouse::Interaction::Crosshair,
            ViewerCanvasTool::Hand => {
                if state.dragging {
                    mouse::Interaction::Grabbing
                } else {
                    mouse::Interaction::Grab
                }
            }
            ViewerCanvasTool::Pointer => {
                if state.dragging {
                    mouse::Interaction::Grabbing
                } else {
                    mouse::Interaction::Idle
                }
            }
        }
    }
}
