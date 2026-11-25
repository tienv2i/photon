//! GPU-accelerated WGSL image viewport (`iced::widget::shader`) powered by `wgpu 27`.
//!
//! Supports zoom-at-cursor, pan, checkerboard transparency backdrop, canvas background,
//! real-time 60fps Color & Light adjustments (exposure, highlights, shadows, brightness,
//! contrast, saturation), live arbitrary-angle rotation preview, Before/After Original
//! split/hold comparison, and interactive Paint/Eraser/Eyedropper/Crop/Zoom tools.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::keyboard::Modifiers;
use iced::mouse;
use iced::wgpu;
use iced::widget::shader::{self, Action, Pipeline, Primitive, Viewport};
use iced::{Event, Rectangle};
use image::{DynamicImage, RgbaImage};
use photon_protocol::{ColorAdjustments, DrawShapeKind, VectorPathNode};

#[derive(Debug, Clone)]
pub struct CpuImageFrame {
    pub key: String,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct CpuMaskFrame {
    pub key: String,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub alpha: Arc<Vec<u8>>,
}

/// Interactive tool active on `ImageCanvasProgram`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CanvasTool {
    #[default]
    Pointer,
    Hand,
    Zoom,
    Brush,
    Eraser,
    Liquify,
    Eyedropper,
    Crop,
    RotateAngle,
    MarqueeRect,
    MarqueeEllipse,
    SingleRow,
    SingleCol,
    Lasso,
    PolygonLasso,
    MagneticLasso,
    BezierSelect,
    QuickSelect,
    MagicWand,
    DrawLine,
    DrawRect,
    DrawEllipse,
    DrawPolygon,
    DrawPolyline,
    BezierPen,
    PaintBucket,
    Gradient,
}

impl CanvasTool {
    pub fn is_selection(self) -> bool {
        matches!(
            self,
            Self::MarqueeRect
                | Self::MarqueeEllipse
                | Self::SingleRow
                | Self::SingleCol
                | Self::Lasso
                | Self::PolygonLasso
                | Self::MagneticLasso
                | Self::BezierSelect
                | Self::QuickSelect
                | Self::MagicWand
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct CanvasInteractionState {
    pub dragging: bool,
    pub space_pan_active: bool,
    pub cropping: bool,
    pub painting: bool,
    pub sampling: bool,
    pub dragging_split: bool,
    pub selecting: bool,
    pub modifiers: Modifiers,
    pub last_cursor: Option<iced::Point>,
    pub last_click_time: Option<Instant>,
    pub last_click_pos: Option<iced::Point>,
    pub crop_anchor_uv: Option<[f32; 2]>,
    pub select_anchor_px: Option<[f32; 2]>,
    pub select_current_px: Option<[f32; 2]>,
    pub select_points_px: Vec<[f32; 2]>,
    pub magnetic_preview_px: Vec<[f32; 2]>,
    pub select_segment_lens: Vec<usize>,
    pub drawing_shape: bool,
    pub draw_anchor_px: Option<[f32; 2]>,
    pub draw_current_px: Option<[f32; 2]>,
    pub draw_points: Vec<[f32; 2]>,
    pub bezier_nodes: Vec<VectorPathNode>,
    pub bezier_dragging_handle: bool,
    pub stabilizer_pos_px: Option<[f32; 2]>,
}

/// Default studio canvas/viewport backdrop RGB matching `PaletteTokens::bg_canvas`.
pub const fn default_viewport_bg_rgb(dark_theme: bool) -> [u8; 3] {
    if dark_theme {
        [16, 18, 20]
    } else {
        [233, 236, 240]
    }
}

#[derive(Debug, Clone)]
pub enum CanvasEvent {
    ZoomBy {
        factor: f32,
        cursor: Option<[f32; 2]>,
        bounds: [f32; 2],
    },
    PanBy {
        dx: f32,
        dy: f32,
    },
    CropUpdated([f32; 4]),
    StrokeBegin([f64; 2]),
    StrokeStep(Vec<[f64; 2]>),
    StrokeEnd(bool),
    SampleColor([u32; 2]),
    CompareSplitChanged(f32),
    ContextMenuRequested {
        pos: [f32; 2],
        is_right_pair: bool,
    },
    SelectMarqueeCommit {
        is_ellipse: bool,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        shift: bool,
        alt: bool,
    },
    SelectLineCommit {
        is_row: bool,
        coord: u32,
        shift: bool,
        alt: bool,
    },
    SelectPolygonCommit {
        points: Vec<[f32; 2]>,
        shift: bool,
        alt: bool,
    },
    SelectMagicWand {
        x: u32,
        y: u32,
        shift: bool,
        alt: bool,
    },
    SelectQuickStroke {
        points: Vec<[f64; 2]>,
        shift: bool,
        alt: bool,
    },
    DrawShapeCommit {
        kind: DrawShapeKind,
        points: Vec<[f32; 2]>,
        nodes: Vec<VectorPathNode>,
        closed: bool,
    },
    BezierToSelectionCommit {
        nodes: Vec<VectorPathNode>,
    },
    FloodFillClick {
        x: u32,
        y: u32,
    },
    DrawGradientCommit {
        start: [f32; 2],
        end: [f32; 2],
    },
    ResetFit,
}

pub struct ImageCanvasProgram {
    pub frame: Option<CpuImageFrame>,
    pub pair_frame: Option<CpuImageFrame>,
    pub dual_gap_px: f32,
    pub compare_frame: Option<CpuImageFrame>,
    pub selection_mask: Option<CpuMaskFrame>,
    pub has_selection: bool,
    pub quick_mask_active: bool,
    pub marching_ants_phase: f32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub adjustments: ColorAdjustments,
    pub dark_theme: bool,
    pub crop_rect: Option<[f32; 4]>,
    pub crop_aspect: Option<f32>,
    pub quarter_turns: u8,
    pub tool: CanvasTool,
    pub preview_angle_rad: f32,
    pub preview_fill: [f32; 4],
    pub canvas_bg: Option<[u8; 3]>,
    pub viewport_bg: Option<[u8; 3]>,
    /// 0 = Off, 1 = Split (draggable divider), 2 = Side-by-Side (Original left | Edited right), 3 = HoldOriginal
    pub compare_mode: u8,
    pub compare_split_x: f32,
    /// Active brush/eraser/liquify radius in document pixels (`0.0` disables cursor ring).
    pub brush_radius_px: f32,
    /// When true (e.g. Editor), Pointer & Middle-click do not pan; only Hand tool or Space+Drag pans.
    pub restrict_pan_to_hand: bool,
    /// Stabilizer leash radius in document pixels (`0.0` disables stabilizer overlay).
    pub stabilizer_radius: f32,
}

/// Compute unified spread dimensions `(spread_w, spread_h, left_disp_w, gap_img)`
/// matching KogaView's `ViewerDualSpread` height-normalization.
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

impl ImageCanvasProgram {
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

    fn fit_geometry(&self, bounds: Rectangle) -> Option<(f32, f32, f32, f32, f32, f32)> {
        let (doc_w, doc_h) = self.effective_doc_size()?;
        let vp_w = bounds.width.max(1.0);
        let vp_h = bounds.height.max(1.0);
        let (fw, fh) = if self.quarter_turns % 2 == 1 {
            (doc_h, doc_w)
        } else {
            (doc_w, doc_h)
        };
        let ca = self.preview_angle_rad.cos().abs();
        let sa = self.preview_angle_rad.sin().abs();
        let eff_w = (fw * ca + fh * sa).max(1.0);
        let eff_h = (fw * sa + fh * ca).max(1.0);
        let vp_aspect = vp_w / vp_h;
        let img_aspect = eff_w / eff_h;
        let (fit_w, fit_h) = if img_aspect > vp_aspect {
            (vp_w, vp_w / img_aspect)
        } else {
            (vp_h * img_aspect, vp_h)
        };
        let drawn_w = fit_w * self.zoom.max(0.05);
        let drawn_h = fit_h * self.zoom.max(0.05);
        let cx = vp_w * 0.5 + self.pan_x;
        let cy = vp_h * 0.5 + self.pan_y;
        let left = cx - drawn_w * 0.5;
        let top = cy - drawn_h * 0.5;
        Some((left, top, drawn_w, drawn_h, fw, fh))
    }

    pub fn screen_to_image_uv(&self, pos: iced::Point, bounds: Rectangle) -> Option<[f32; 2]> {
        let (left, top, drawn_w, drawn_h, _, _) = self.fit_geometry(bounds)?;
        let rel_x = pos.x - bounds.x;
        let rel_y = pos.y - bounds.y;
        Some([
            ((rel_x - left) / drawn_w).clamp(0.0, 1.0),
            ((rel_y - top) / drawn_h).clamp(0.0, 1.0),
        ])
    }

    pub fn screen_to_image_px(&self, pos: iced::Point, bounds: Rectangle) -> Option<[f64; 2]> {
        let (left, top, drawn_w, drawn_h, fw, fh) = self.fit_geometry(bounds)?;
        let rel_x = pos.x - bounds.x;
        let rel_y = pos.y - bounds.y;
        let u = ((rel_x - left) / drawn_w).clamp(0.0, 1.0);
        let v = ((rel_y - top) / drawn_h).clamp(0.0, 1.0);
        Some([f64::from(u * fw), f64::from(v * fh)])
    }

    fn constrained_crop(&self, anchor: [f32; 2], cur: [f32; 2]) -> Option<[f32; 4]> {
        let frame = self.frame.as_ref()?;
        let Some(target_ratio) = self.crop_aspect else {
            let x0 = anchor[0].min(cur[0]);
            let y0 = anchor[1].min(cur[1]);
            let x1 = anchor[0].max(cur[0]);
            let y1 = anchor[1].max(cur[1]);
            if (x1 - x0) > 0.01 && (y1 - y0) > 0.01 {
                return Some([x0, y0, x1, y1]);
            }
            return None;
        };

        let img_aspect = (frame.width.max(1) as f32) / (frame.height.max(1) as f32);
        let uv_ratio = (target_ratio / img_aspect).max(0.01);
        let dx = cur[0] - anchor[0];
        let dy = cur[1] - anchor[1];
        let sx = if dx >= 0.0 { 1.0_f32 } else { -1.0_f32 };
        let sy = if dy >= 0.0 { 1.0_f32 } else { -1.0_f32 };

        let mut w = dx.abs();
        let mut h = w / uv_ratio;
        if h > dy.abs() {
            h = dy.abs();
            w = h * uv_ratio;
        }
        let max_w = if sx > 0.0 { 1.0 - anchor[0] } else { anchor[0] };
        let max_h = if sy > 0.0 { 1.0 - anchor[1] } else { anchor[1] };
        if w > max_w {
            w = max_w;
            h = w / uv_ratio;
        }
        if h > max_h {
            h = max_h;
            w = h * uv_ratio;
        }
        if w <= 0.01 || h <= 0.01 {
            return None;
        }
        let end_x = (anchor[0] + sx * w).clamp(0.0, 1.0);
        let end_y = (anchor[1] + sy * h).clamp(0.0, 1.0);
        Some([
            anchor[0].min(end_x),
            anchor[1].min(end_y),
            anchor[0].max(end_x),
            anchor[1].max(end_y),
        ])
    }

    fn is_over_split_handle(&self, pos: iced::Point, bounds: Rectangle) -> bool {
        if self.compare_mode != 1 {
            return false;
        }
        let split_x = bounds.x + bounds.width * self.compare_split_x.clamp(0.0, 1.0);
        (pos.x - split_x).abs() <= 10.0 && pos.y >= bounds.y && pos.y <= bounds.y + bounds.height
    }

    fn doc_px_to_ndc(&self, doc_px: [f32; 2], bounds: Rectangle) -> Option<[f32; 2]> {
        let (left, top, drawn_w, drawn_h, fw, fh) = self.fit_geometry(bounds)?;
        let vp_w = bounds.width.max(1.0);
        let vp_h = bounds.height.max(1.0);
        let sx = left + (doc_px[0] / fw.max(1.0)) * drawn_w;
        let sy = top + (doc_px[1] / fh.max(1.0)) * drawn_h;
        Some([(sx / vp_w) * 2.0 - 1.0, 1.0 - (sy / vp_h) * 2.0])
    }

    fn screen_delta_to_ndc(&self, dx: f32, dy: f32, bounds: Rectangle) -> [f32; 2] {
        let vp_w = bounds.width.max(1.0);
        let vp_h = bounds.height.max(1.0);
        [dx * 2.0 / vp_w, -dy * 2.0 / vp_h]
    }
}

fn commit_polygon_selection(state: &mut CanvasInteractionState) -> Option<Action<CanvasEvent>> {
    if state.select_points_px.len() < 3 {
        return None;
    }
    let pts = std::mem::take(&mut state.select_points_px);
    state.select_segment_lens.clear();
    state.magnetic_preview_px.clear();
    state.select_current_px = None;
    state.selecting = false;
    Some(
        Action::publish(CanvasEvent::SelectPolygonCommit {
            points: pts,
            shift: state.modifiers.shift(),
            alt: state.modifiers.alt(),
        })
        .and_capture(),
    )
}

#[inline]
fn rgba_luma(rgba: &[u8], w: u32, h: u32, x: i32, y: i32) -> f32 {
    let cx = x.clamp(0, w as i32 - 1) as usize;
    let cy = y.clamp(0, h as i32 - 1) as usize;
    let idx = (cy * w as usize + cx) * 4;
    0.2126 * f32::from(rgba[idx])
        + 0.7152 * f32::from(rgba[idx + 1])
        + 0.0722 * f32::from(rgba[idx + 2])
}

#[inline]
fn sobel_gradient_at(rgba: &[u8], w: u32, h: u32, x: i32, y: i32) -> f32 {
    let tl = rgba_luma(rgba, w, h, x - 1, y - 1);
    let tc = rgba_luma(rgba, w, h, x, y - 1);
    let tr = rgba_luma(rgba, w, h, x + 1, y - 1);
    let ml = rgba_luma(rgba, w, h, x - 1, y);
    let mr = rgba_luma(rgba, w, h, x + 1, y);
    let bl = rgba_luma(rgba, w, h, x - 1, y + 1);
    let bc = rgba_luma(rgba, w, h, x, y + 1);
    let br = rgba_luma(rgba, w, h, x + 1, y + 1);
    let gx = -tl + tr - 2.0 * ml + 2.0 * mr - bl + br;
    let gy = -tl - 2.0 * tc - tr + bl + 2.0 * bc + br;
    (gx * gx + gy * gy).sqrt()
}

/// Snap a cursor coordinate to the strongest Sobel edge pixel within `radius` pixels.
pub fn snap_cursor_to_edge(frame: &CpuImageFrame, cursor: [f32; 2], radius: i32) -> [f32; 2] {
    let w = frame.width;
    let h = frame.height;
    if w < 3 || h < 3 || frame.rgba.len() < (w as usize * h as usize * 4) {
        return cursor;
    }
    let cx = (cursor[0].round() as i32).clamp(0, w as i32 - 1);
    let cy = (cursor[1].round() as i32).clamp(0, h as i32 - 1);
    let r2 = radius * radius;
    let mut best_g = 0.0_f32;
    let mut best = [cx as f32, cy as f32];
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > r2 {
                continue;
            }
            let x = (cx + dx).clamp(0, w as i32 - 1);
            let y = (cy + dy).clamp(0, h as i32 - 1);
            let g = sobel_gradient_at(&frame.rgba, w, h, x, y);
            if g > best_g {
                best_g = g;
                best = [x as f32, y as f32];
            }
        }
    }
    if best_g > 1e-3 { best } else { cursor }
}

/// Solve an 8-connected Dijkstra shortest path along Sobel edges from `anchor` to
/// the strongest edge pixel within `r = 6px` of `cursor`, over a local window with 32px margin.
pub fn magnetic_snap(frame: &CpuImageFrame, anchor: [f32; 2], cursor: [f32; 2]) -> Vec<[f32; 2]> {
    let w = frame.width;
    let h = frame.height;
    if w < 3 || h < 3 || frame.rgba.len() < (w as usize * h as usize * 4) {
        return vec![cursor];
    }

    let min_x = (anchor[0].min(cursor[0]).floor() as i32 - 32).max(0) as u32;
    let max_x = (anchor[0].max(cursor[0]).ceil() as i32 + 32).min(w as i32 - 1) as u32;
    let min_y = (anchor[1].min(cursor[1]).floor() as i32 - 32).max(0) as u32;
    let max_y = (anchor[1].max(cursor[1]).ceil() as i32 + 32).min(h as i32 - 1) as u32;

    let win_w = (max_x - min_x + 1) as usize;
    let win_h = (max_y - min_y + 1) as usize;
    let total = win_w * win_h;
    if total == 0 || total > 1_000_000 {
        return vec![cursor];
    }

    let mut grad = vec![0.0_f32; total];
    let mut g_max = 0.0_f32;
    for ly in 0..win_h {
        let gy = (min_y as usize + ly) as i32;
        for lx in 0..win_w {
            let gx = (min_x as usize + lx) as i32;
            let g = sobel_gradient_at(&frame.rgba, w, h, gx, gy);
            grad[ly * win_w + lx] = g;
            if g > g_max {
                g_max = g;
            }
        }
    }

    let ax = (anchor[0].round() as i32).clamp(min_x as i32, max_x as i32) as u32;
    let ay = (anchor[1].round() as i32).clamp(min_y as i32, max_y as i32) as u32;
    let cx = (cursor[0].round() as i32).clamp(min_x as i32, max_x as i32);
    let cy = (cursor[1].round() as i32).clamp(min_y as i32, max_y as i32);

    // Snap cursor to strongest edge pixel within r = 6px
    let mut tx = cx as u32;
    let mut ty = cy as u32;
    let mut best_edge = 0.0_f32;
    for dy in -6..=6 {
        for dx in -6..=6 {
            if dx * dx + dy * dy > 36 {
                continue;
            }
            let x = (cx + dx).clamp(min_x as i32, max_x as i32) as u32;
            let y = (cy + dy).clamp(min_y as i32, max_y as i32) as u32;
            let g = grad[(y - min_y) as usize * win_w + (x - min_x) as usize];
            if g > best_edge {
                best_edge = g;
                tx = x;
                ty = y;
            }
        }
    }

    let start_idx = (ay - min_y) as usize * win_w + (ax - min_x) as usize;
    let goal_idx = (ty - min_y) as usize * win_w + (tx - min_x) as usize;
    if start_idx == goal_idx {
        return vec![[tx as f32, ty as f32]];
    }

    let mut dist = vec![u32::MAX; total];
    let mut prev = vec![u32::MAX; total];
    let mut heap = BinaryHeap::new();
    dist[start_idx] = 0;
    heap.push((Reverse(0_u32), start_idx));

    const DIRS: [(i32, i32, f32); 8] = [
        (-1, 0, 1.0),
        (1, 0, 1.0),
        (0, -1, 1.0),
        (0, 1, 1.0),
        (-1, -1, std::f32::consts::SQRT_2),
        (1, -1, std::f32::consts::SQRT_2),
        (-1, 1, std::f32::consts::SQRT_2),
        (1, 1, std::f32::consts::SQRT_2),
    ];

    let inv_g_max = if g_max > 1e-4 { 1.0 / g_max } else { 0.0 };

    while let Some((Reverse(d), u)) = heap.pop() {
        if u == goal_idx {
            break;
        }
        if d > dist[u] {
            continue;
        }
        let ux = (u % win_w) as i32;
        let uy = (u / win_w) as i32;
        for &(dx, dy, step_len) in &DIRS {
            let nx = ux + dx;
            let ny = uy + dy;
            if nx < 0 || ny < 0 || nx >= win_w as i32 || ny >= win_h as i32 {
                continue;
            }
            let v = ny as usize * win_w + nx as usize;
            let norm_g = (grad[v] * inv_g_max).clamp(0.0, 1.0);
            let edge_cost = (step_len * (1.0 - 0.92 * norm_g) * 1024.0).max(1.0) as u32;
            let nd = d.saturating_add(edge_cost);
            if nd < dist[v] {
                dist[v] = nd;
                prev[v] = u as u32;
                heap.push((Reverse(nd), v));
            }
        }
    }

    if prev[goal_idx] == u32::MAX {
        return vec![[tx as f32, ty as f32]];
    }

    let mut path = Vec::new();
    let mut cur = goal_idx;
    while cur != start_idx && cur != u32::MAX as usize {
        let lx = (cur % win_w) as u32 + min_x;
        let ly = (cur / win_w) as u32 + min_y;
        path.push([lx as f32, ly as f32]);
        cur = prev[cur] as usize;
    }
    path.reverse();
    path
}

fn commit_draw_polyline(
    state: &mut CanvasInteractionState,
    tool: CanvasTool,
) -> Option<Action<CanvasEvent>> {
    let closed = tool == CanvasTool::DrawPolygon;
    let min_pts = if closed { 3 } else { 2 };
    if state.draw_points.len() < min_pts {
        return None;
    }
    let points = std::mem::take(&mut state.draw_points);
    state.draw_current_px = None;
    state.drawing_shape = false;
    let kind = if closed {
        DrawShapeKind::Polygon
    } else {
        DrawShapeKind::Polyline
    };
    Some(
        Action::publish(CanvasEvent::DrawShapeCommit {
            kind,
            points,
            nodes: Vec::new(),
            closed,
        })
        .and_capture(),
    )
}

fn commit_bezier_path(
    state: &mut CanvasInteractionState,
    closed: bool,
    to_selection: bool,
) -> Option<Action<CanvasEvent>> {
    if state.bezier_nodes.len() < 2 {
        return None;
    }
    let nodes = std::mem::take(&mut state.bezier_nodes);
    state.draw_current_px = None;
    state.drawing_shape = false;
    state.bezier_dragging_handle = false;
    let event = if to_selection {
        CanvasEvent::BezierToSelectionCommit { nodes }
    } else {
        CanvasEvent::DrawShapeCommit {
            kind: DrawShapeKind::BezierPath,
            points: Vec::new(),
            nodes,
            closed,
        }
    };
    Some(Action::publish(event).and_capture())
}

fn check_double_click(state: &CanvasInteractionState, pos: iced::Point) -> bool {
    let now = Instant::now();
    state
        .last_click_time
        .is_some_and(|t| now.duration_since(t) < Duration::from_millis(350))
        && state.last_click_pos.is_some_and(|p| {
            let dx = pos.x - p.x;
            let dy = pos.y - p.y;
            (dx * dx + dy * dy).sqrt() <= 6.0
        })
}

impl shader::Program<CanvasEvent> for ImageCanvasProgram {
    type State = CanvasInteractionState;
    type Primitive = ImageQuadPrimitive;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<CanvasEvent>> {
        match event {
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(mods)) => {
                state.modifiers = *mods;
            }
            Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) => {
                use iced::keyboard::Key;
                use iced::keyboard::key::Named;
                match key {
                    Key::Named(Named::Space) => {
                        if !state.space_pan_active {
                            state.space_pan_active = true;
                            return Some(Action::request_redraw());
                        }
                    }
                    Key::Named(Named::Enter)
                        if state.selecting
                            && matches!(
                                self.tool,
                                CanvasTool::PolygonLasso
                                    | CanvasTool::MagneticLasso
                                    | CanvasTool::BezierSelect
                            ) =>
                    {
                        if let Some(action) = commit_polygon_selection(state) {
                            return Some(action);
                        }
                    }
                    Key::Named(Named::Backspace)
                        if state.selecting
                            && matches!(
                                self.tool,
                                CanvasTool::PolygonLasso
                                    | CanvasTool::MagneticLasso
                                    | CanvasTool::BezierSelect
                            ) =>
                    {
                        if self.tool == CanvasTool::MagneticLasso {
                            let n = state.select_segment_lens.pop().unwrap_or(1);
                            for _ in 0..n {
                                state.select_points_px.pop();
                            }
                        } else {
                            state.select_points_px.pop();
                        }
                        state.magnetic_preview_px.clear();
                        if state.select_points_px.is_empty() {
                            state.selecting = false;
                            state.select_current_px = None;
                        }
                        return Some(Action::request_redraw());
                    }
                    Key::Named(Named::Escape)
                        if state.selecting
                            && matches!(
                                self.tool,
                                CanvasTool::PolygonLasso
                                    | CanvasTool::MagneticLasso
                                    | CanvasTool::BezierSelect
                            ) =>
                    {
                        state.select_points_px.clear();
                        state.select_segment_lens.clear();
                        state.magnetic_preview_px.clear();
                        state.select_current_px = None;
                        state.selecting = false;
                        return Some(Action::request_redraw());
                    }
                    Key::Named(Named::Enter)
                        if state.drawing_shape
                            && matches!(
                                self.tool,
                                CanvasTool::DrawPolygon | CanvasTool::DrawPolyline
                            ) =>
                    {
                        if let Some(action) = commit_draw_polyline(state, self.tool) {
                            return Some(action);
                        }
                    }
                    Key::Named(Named::Backspace)
                        if state.drawing_shape
                            && matches!(
                                self.tool,
                                CanvasTool::DrawPolygon | CanvasTool::DrawPolyline
                            ) =>
                    {
                        state.draw_points.pop();
                        if state.draw_points.is_empty() {
                            state.drawing_shape = false;
                            state.draw_current_px = None;
                        }
                        return Some(Action::request_redraw());
                    }
                    Key::Named(Named::Escape)
                        if state.drawing_shape
                            && matches!(
                                self.tool,
                                CanvasTool::DrawPolygon | CanvasTool::DrawPolyline
                            ) =>
                    {
                        state.draw_points.clear();
                        state.draw_current_px = None;
                        state.drawing_shape = false;
                        return Some(Action::request_redraw());
                    }
                    Key::Named(Named::Enter)
                        if !state.bezier_nodes.is_empty() && self.tool == CanvasTool::BezierPen =>
                    {
                        let to_sel = state.modifiers.control() || state.modifiers.logo();
                        if let Some(action) = commit_bezier_path(state, false, to_sel) {
                            return Some(action);
                        }
                    }
                    Key::Named(Named::Backspace)
                        if !state.bezier_nodes.is_empty() && self.tool == CanvasTool::BezierPen =>
                    {
                        state.bezier_nodes.pop();
                        if state.bezier_nodes.is_empty() {
                            state.drawing_shape = false;
                            state.bezier_dragging_handle = false;
                            state.draw_current_px = None;
                        }
                        return Some(Action::request_redraw());
                    }
                    Key::Named(Named::Escape)
                        if !state.bezier_nodes.is_empty() && self.tool == CanvasTool::BezierPen =>
                    {
                        state.bezier_nodes.clear();
                        state.drawing_shape = false;
                        state.bezier_dragging_handle = false;
                        state.draw_current_px = None;
                        return Some(Action::request_redraw());
                    }
                    _ => {}
                }
            }
            Event::Keyboard(iced::keyboard::Event::KeyReleased { key, .. }) => {
                if matches!(
                    key,
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Space)
                ) && state.space_pan_active
                {
                    state.space_pan_active = false;
                    if state.dragging {
                        state.dragging = false;
                        state.last_cursor = None;
                    }
                    return Some(Action::request_redraw());
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let y = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / 60.0,
                };
                if y.abs() > 0.001 {
                    let factor = if y > 0.0 { 1.12 } else { 1.0 / 1.12 };
                    let rel_cursor = cursor.position_in(bounds).map(|p| [p.x, p.y]);
                    return Some(
                        Action::publish(CanvasEvent::ZoomBy {
                            factor,
                            cursor: rel_cursor,
                            bounds: [bounds.width.max(1.0), bounds.height.max(1.0)],
                        })
                        .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle))
                if cursor.is_over(bounds) =>
            {
                if self.restrict_pan_to_hand && self.tool != CanvasTool::Hand {
                    return None;
                }
                state.dragging = true;
                state.last_cursor = cursor.position();
                return Some(Action::capture());
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                if state.selecting
                    && matches!(
                        self.tool,
                        CanvasTool::MarqueeRect
                            | CanvasTool::MarqueeEllipse
                            | CanvasTool::Lasso
                            | CanvasTool::QuickSelect
                    )
                {
                    state.selecting = false;
                    let shift = state.modifiers.shift();
                    let alt = state.modifiers.alt();
                    match self.tool {
                        CanvasTool::MarqueeRect | CanvasTool::MarqueeEllipse => {
                            let anchor = state.select_anchor_px.take();
                            let cur = state.select_current_px.take();
                            if let (Some(a), Some(b)) = (anchor, cur) {
                                let x = a[0].min(b[0]);
                                let y = a[1].min(b[1]);
                                let w = (a[0] - b[0]).abs();
                                let h = (a[1] - b[1]).abs();
                                if w >= 1.0 && h >= 1.0 {
                                    return Some(
                                        Action::publish(CanvasEvent::SelectMarqueeCommit {
                                            is_ellipse: self.tool == CanvasTool::MarqueeEllipse,
                                            x,
                                            y,
                                            width: w,
                                            height: h,
                                            shift,
                                            alt,
                                        })
                                        .and_capture(),
                                    );
                                }
                            }
                        }
                        CanvasTool::Lasso => {
                            let pts = std::mem::take(&mut state.select_points_px);
                            if pts.len() >= 3 {
                                return Some(
                                    Action::publish(CanvasEvent::SelectPolygonCommit {
                                        points: pts,
                                        shift,
                                        alt,
                                    })
                                    .and_capture(),
                                );
                            }
                        }
                        CanvasTool::QuickSelect => {
                            let pts: Vec<[f64; 2]> = std::mem::take(&mut state.select_points_px)
                                .into_iter()
                                .map(|[x, y]| [f64::from(x), f64::from(y)])
                                .collect();
                            if !pts.is_empty() {
                                return Some(
                                    Action::publish(CanvasEvent::SelectQuickStroke {
                                        points: pts,
                                        shift,
                                        alt,
                                    })
                                    .and_capture(),
                                );
                            }
                        }
                        _ => {}
                    }
                    return Some(Action::capture());
                }
                if state.dragging {
                    state.dragging = false;
                    state.last_cursor = None;
                    return Some(Action::capture());
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if cursor.is_over(bounds) =>
            {
                if self.tool == CanvasTool::Zoom {
                    let rel_cursor = cursor.position_in(bounds).map(|p| [p.x, p.y]);
                    return Some(
                        Action::publish(CanvasEvent::ZoomBy {
                            factor: 0.8,
                            cursor: rel_cursor,
                            bounds: [bounds.width.max(1.0), bounds.height.max(1.0)],
                        })
                        .and_capture(),
                    );
                }
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
                        Action::publish(CanvasEvent::ContextMenuRequested {
                            pos: [pos.x, pos.y],
                            is_right_pair,
                        })
                        .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                if let Some(pos) = cursor.position()
                    && self.is_over_split_handle(pos, bounds)
                {
                    state.dragging_split = true;
                    return Some(Action::capture());
                }

                // (1) Global Space+Drag or Hand tool -> pan
                if state.space_pan_active || self.tool == CanvasTool::Hand {
                    state.dragging = true;
                    state.last_cursor = cursor.position();
                    return Some(Action::capture());
                }

                // (2) Global Alt+Click on non-selection tools -> SampleColor
                if state.modifiers.alt()
                    && !self.tool.is_selection()
                    && !matches!(self.tool, CanvasTool::Hand | CanvasTool::Zoom)
                {
                    if let Some(pos) = cursor.position()
                        && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                        && let Some(frame) = &self.frame
                    {
                        state.sampling = true;
                        let ix = (px.floor() as u32).min(frame.width.saturating_sub(1));
                        let iy = (py.floor() as u32).min(frame.height.saturating_sub(1));
                        return Some(
                            Action::publish(CanvasEvent::SampleColor([ix, iy])).and_capture(),
                        );
                    }
                }

                match self.tool {
                    CanvasTool::Zoom => {
                        let factor = if state.modifiers.shift() { 0.8 } else { 1.25 };
                        let rel_cursor = cursor.position_in(bounds).map(|p| [p.x, p.y]);
                        return Some(
                            Action::publish(CanvasEvent::ZoomBy {
                                factor,
                                cursor: rel_cursor,
                                bounds: [bounds.width.max(1.0), bounds.height.max(1.0)],
                            })
                            .and_capture(),
                        );
                    }
                    CanvasTool::Brush | CanvasTool::Eraser | CanvasTool::Liquify => {
                        if let Some(pos) = cursor.position()
                            && let Some(pt) = self.screen_to_image_px(pos, bounds)
                        {
                            state.painting = true;
                            if self.stabilizer_radius > 0.0
                                && matches!(self.tool, CanvasTool::Brush | CanvasTool::Eraser)
                            {
                                state.stabilizer_pos_px = Some([pt[0] as f32, pt[1] as f32]);
                            }
                            return Some(
                                Action::publish(CanvasEvent::StrokeBegin(pt)).and_capture(),
                            );
                        }
                    }
                    CanvasTool::Eyedropper => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                            && let Some(frame) = &self.frame
                        {
                            state.sampling = true;
                            let ix = (px.floor() as u32).min(frame.width.saturating_sub(1));
                            let iy = (py.floor() as u32).min(frame.height.saturating_sub(1));
                            return Some(
                                Action::publish(CanvasEvent::SampleColor([ix, iy])).and_capture(),
                            );
                        }
                    }
                    CanvasTool::MarqueeRect | CanvasTool::MarqueeEllipse => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                        {
                            state.selecting = true;
                            state.select_anchor_px = Some([px as f32, py as f32]);
                            state.select_current_px = Some([px as f32, py as f32]);
                            return Some(Action::capture());
                        }
                    }
                    CanvasTool::SingleRow | CanvasTool::SingleCol => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                            && let Some(frame) = &self.frame
                        {
                            let is_row = self.tool == CanvasTool::SingleRow;
                            let coord = if is_row {
                                (py.floor() as u32).min(frame.height.saturating_sub(1))
                            } else {
                                (px.floor() as u32).min(frame.width.saturating_sub(1))
                            };
                            return Some(
                                Action::publish(CanvasEvent::SelectLineCommit {
                                    is_row,
                                    coord,
                                    shift: state.modifiers.shift(),
                                    alt: state.modifiers.alt(),
                                })
                                .and_capture(),
                            );
                        }
                    }
                    CanvasTool::Lasso | CanvasTool::QuickSelect => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                        {
                            state.selecting = true;
                            state.select_points_px.clear();
                            state.select_points_px.push([px as f32, py as f32]);
                            return Some(Action::capture());
                        }
                    }
                    CanvasTool::PolygonLasso
                    | CanvasTool::MagneticLasso
                    | CanvasTool::BezierSelect => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                        {
                            let now = Instant::now();
                            let is_double_click = state.last_click_time.is_some_and(|t| {
                                now.duration_since(t) < Duration::from_millis(350)
                            }) && state.last_click_pos.is_some_and(|p| {
                                let dx = pos.x - p.x;
                                let dy = pos.y - p.y;
                                (dx * dx + dy * dy).sqrt() <= 6.0
                            });
                            if is_double_click && state.select_points_px.len() >= 3 {
                                state.last_click_time = None;
                                state.last_click_pos = None;
                                if let Some(action) = commit_polygon_selection(state) {
                                    return Some(action);
                                }
                            }
                            state.last_click_time = Some(now);
                            state.last_click_pos = Some(pos);

                            let pt = [px as f32, py as f32];
                            if state.select_points_px.len() >= 3 {
                                let first = state.select_points_px[0];
                                let dx = pt[0] - first[0];
                                let dy = pt[1] - first[1];
                                if (dx * dx + dy * dy).sqrt() <= 10.0 / self.zoom.max(0.2) {
                                    state.last_click_time = None;
                                    state.last_click_pos = None;
                                    if let Some(action) = commit_polygon_selection(state) {
                                        return Some(action);
                                    }
                                }
                            }
                            state.selecting = true;
                            if self.tool == CanvasTool::MagneticLasso {
                                if state.select_points_px.is_empty() {
                                    let snapped = self
                                        .frame
                                        .as_ref()
                                        .map(|f| snap_cursor_to_edge(f, pt, 6))
                                        .unwrap_or(pt);
                                    state.select_points_px.push(snapped);
                                    state.select_segment_lens.push(1);
                                } else if let Some(&last_anchor) = state.select_points_px.last() {
                                    let path = self
                                        .frame
                                        .as_ref()
                                        .map(|f| magnetic_snap(f, last_anchor, pt))
                                        .unwrap_or_else(|| vec![pt]);
                                    let seg_len = path.len();
                                    state.select_points_px.extend(path);
                                    state.select_segment_lens.push(seg_len);
                                    state.magnetic_preview_px.clear();
                                }
                            } else {
                                state.select_points_px.push(pt);
                            }
                            state.select_current_px = Some(pt);
                            return Some(Action::request_redraw());
                        }
                    }
                    CanvasTool::MagicWand => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                            && let Some(frame) = &self.frame
                        {
                            let ix = (px.floor() as u32).min(frame.width.saturating_sub(1));
                            let iy = (py.floor() as u32).min(frame.height.saturating_sub(1));
                            return Some(
                                Action::publish(CanvasEvent::SelectMagicWand {
                                    x: ix,
                                    y: iy,
                                    shift: state.modifiers.shift(),
                                    alt: state.modifiers.alt(),
                                })
                                .and_capture(),
                            );
                        }
                    }
                    CanvasTool::DrawLine
                    | CanvasTool::DrawRect
                    | CanvasTool::DrawEllipse
                    | CanvasTool::Gradient => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                        {
                            let pt = [px as f32, py as f32];
                            state.drawing_shape = true;
                            state.draw_anchor_px = Some(pt);
                            state.draw_current_px = Some(pt);
                            return Some(Action::capture());
                        }
                    }
                    CanvasTool::DrawPolygon | CanvasTool::DrawPolyline => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                        {
                            if check_double_click(state, pos) {
                                state.last_click_time = None;
                                state.last_click_pos = None;
                                if let Some(action) = commit_draw_polyline(state, self.tool) {
                                    return Some(action);
                                }
                            }
                            state.last_click_time = Some(Instant::now());
                            state.last_click_pos = Some(pos);

                            let pt = [px as f32, py as f32];
                            if self.tool == CanvasTool::DrawPolygon && state.draw_points.len() >= 3
                            {
                                let first = state.draw_points[0];
                                let dx = pt[0] - first[0];
                                let dy = pt[1] - first[1];
                                if (dx * dx + dy * dy).sqrt() <= 10.0 / self.zoom.max(0.2) {
                                    state.last_click_time = None;
                                    state.last_click_pos = None;
                                    if let Some(action) = commit_draw_polyline(state, self.tool) {
                                        return Some(action);
                                    }
                                }
                            }
                            state.drawing_shape = true;
                            state.draw_points.push(pt);
                            state.draw_current_px = Some(pt);
                            return Some(Action::request_redraw());
                        }
                    }
                    CanvasTool::BezierPen => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                        {
                            let pt = [px as f32, py as f32];
                            if state.bezier_nodes.len() >= 3 {
                                let first = state.bezier_nodes[0].anchor;
                                let dx = pt[0] - first[0];
                                let dy = pt[1] - first[1];
                                if (dx * dx + dy * dy).sqrt() <= 10.0 / self.zoom.max(0.2)
                                    && let Some(action) = commit_bezier_path(state, true, false)
                                {
                                    return Some(action);
                                }
                            }
                            state.drawing_shape = true;
                            state.bezier_dragging_handle = true;
                            state.bezier_nodes.push(VectorPathNode {
                                anchor: pt,
                                handle_in: None,
                                handle_out: None,
                            });
                            state.draw_current_px = Some(pt);
                            return Some(Action::capture());
                        }
                    }
                    CanvasTool::PaintBucket => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                            && let Some(frame) = &self.frame
                        {
                            let ix = (px.floor() as u32).min(frame.width.saturating_sub(1));
                            let iy = (py.floor() as u32).min(frame.height.saturating_sub(1));
                            return Some(
                                Action::publish(CanvasEvent::FloodFillClick { x: ix, y: iy })
                                    .and_capture(),
                            );
                        }
                    }
                    CanvasTool::Crop | CanvasTool::Pointer if self.crop_rect.is_some() => {
                        if let Some(pos) = cursor.position()
                            && let Some(uv) = self.screen_to_image_uv(pos, bounds)
                        {
                            state.cropping = true;
                            state.crop_anchor_uv = Some(uv);
                            return Some(Action::capture());
                        }
                    }
                    _ => {}
                }

                if self.restrict_pan_to_hand && self.tool == CanvasTool::Pointer {
                    return None;
                }
                state.dragging = true;
                state.last_cursor = cursor.position();
                return Some(Action::capture());
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging_split {
                    state.dragging_split = false;
                    return Some(Action::capture());
                }
                if state.painting {
                    state.painting = false;
                    state.stabilizer_pos_px = None;
                    return Some(Action::publish(CanvasEvent::StrokeEnd(true)).and_capture());
                }
                if state.bezier_dragging_handle {
                    state.bezier_dragging_handle = false;
                    return Some(Action::request_redraw());
                }
                if state.drawing_shape
                    && matches!(
                        self.tool,
                        CanvasTool::DrawLine
                            | CanvasTool::DrawRect
                            | CanvasTool::DrawEllipse
                            | CanvasTool::Gradient
                    )
                {
                    state.drawing_shape = false;
                    let anchor = state.draw_anchor_px.take();
                    let cur = state.draw_current_px.take();
                    if let (Some(a), Some(b)) = (anchor, cur) {
                        let ev = match self.tool {
                            CanvasTool::Gradient => {
                                CanvasEvent::DrawGradientCommit { start: a, end: b }
                            }
                            CanvasTool::DrawLine => CanvasEvent::DrawShapeCommit {
                                kind: DrawShapeKind::Line,
                                points: vec![a, b],
                                nodes: Vec::new(),
                                closed: false,
                            },
                            CanvasTool::DrawRect => CanvasEvent::DrawShapeCommit {
                                kind: DrawShapeKind::Rect,
                                points: vec![a, b],
                                nodes: Vec::new(),
                                closed: true,
                            },
                            CanvasTool::DrawEllipse => CanvasEvent::DrawShapeCommit {
                                kind: DrawShapeKind::Ellipse,
                                points: vec![a, b],
                                nodes: Vec::new(),
                                closed: true,
                            },
                            _ => unreachable!(),
                        };
                        return Some(Action::publish(ev).and_capture());
                    }
                    return Some(Action::capture());
                }
                if state.sampling {
                    state.sampling = false;
                    return Some(Action::capture());
                }
                if state.cropping {
                    state.cropping = false;
                    state.crop_anchor_uv = None;
                    return Some(Action::capture());
                }
                if state.selecting
                    && matches!(
                        self.tool,
                        CanvasTool::MarqueeRect
                            | CanvasTool::MarqueeEllipse
                            | CanvasTool::Lasso
                            | CanvasTool::QuickSelect
                    )
                {
                    state.selecting = false;
                    let shift = state.modifiers.shift();
                    let alt = state.modifiers.alt();
                    match self.tool {
                        CanvasTool::MarqueeRect | CanvasTool::MarqueeEllipse => {
                            let anchor = state.select_anchor_px.take();
                            let cur = state.select_current_px.take();
                            if let (Some(a), Some(b)) = (anchor, cur) {
                                let x = a[0].min(b[0]);
                                let y = a[1].min(b[1]);
                                let w = (a[0] - b[0]).abs();
                                let h = (a[1] - b[1]).abs();
                                if w >= 1.0 && h >= 1.0 {
                                    return Some(
                                        Action::publish(CanvasEvent::SelectMarqueeCommit {
                                            is_ellipse: self.tool == CanvasTool::MarqueeEllipse,
                                            x,
                                            y,
                                            width: w,
                                            height: h,
                                            shift,
                                            alt,
                                        })
                                        .and_capture(),
                                    );
                                }
                            }
                        }
                        CanvasTool::Lasso => {
                            let pts = std::mem::take(&mut state.select_points_px);
                            if pts.len() >= 3 {
                                return Some(
                                    Action::publish(CanvasEvent::SelectPolygonCommit {
                                        points: pts,
                                        shift,
                                        alt,
                                    })
                                    .and_capture(),
                                );
                            }
                        }
                        CanvasTool::QuickSelect => {
                            let pts: Vec<[f64; 2]> = std::mem::take(&mut state.select_points_px)
                                .into_iter()
                                .map(|[x, y]| [f64::from(x), f64::from(y)])
                                .collect();
                            if !pts.is_empty() {
                                return Some(
                                    Action::publish(CanvasEvent::SelectQuickStroke {
                                        points: pts,
                                        shift,
                                        alt,
                                    })
                                    .and_capture(),
                                );
                            }
                        }
                        _ => {}
                    }
                    return Some(Action::capture());
                }
                if state.dragging {
                    state.dragging = false;
                    state.last_cursor = None;
                    return Some(Action::capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging_split => {
                let frac = ((position.x - bounds.x) / bounds.width.max(1.0)).clamp(0.02, 0.98);
                return Some(Action::publish(CanvasEvent::CompareSplitChanged(frac)).and_capture());
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.painting => {
                if let Some(pt) = self.screen_to_image_px(*position, bounds) {
                    if self.stabilizer_radius > 0.0
                        && matches!(self.tool, CanvasTool::Brush | CanvasTool::Eraser)
                    {
                        let cur = [pt[0] as f32, pt[1] as f32];
                        let prev = state.stabilizer_pos_px.unwrap_or(cur);
                        let dx = cur[0] - prev[0];
                        let dy = cur[1] - prev[1];
                        let dist = (dx * dx + dy * dy).sqrt();
                        if dist > self.stabilizer_radius {
                            let scale = (dist - self.stabilizer_radius) / dist;
                            state.stabilizer_pos_px =
                                Some([prev[0] + dx * scale, prev[1] + dy * scale]);
                        }
                    }
                    return Some(Action::publish(CanvasEvent::StrokeStep(vec![pt])).and_capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.sampling => {
                if let Some([px, py]) = self.screen_to_image_px(*position, bounds)
                    && let Some(frame) = &self.frame
                {
                    let ix = (px.floor() as u32).min(frame.width.saturating_sub(1));
                    let iy = (py.floor() as u32).min(frame.height.saturating_sub(1));
                    return Some(Action::publish(CanvasEvent::SampleColor([ix, iy])).and_capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.cropping => {
                if let (Some(anchor), Some(cur)) = (
                    state.crop_anchor_uv,
                    self.screen_to_image_uv(*position, bounds),
                ) && let Some(rect) = self.constrained_crop(anchor, cur)
                {
                    return Some(Action::publish(CanvasEvent::CropUpdated(rect)).and_capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.selecting => {
                if let Some([px, py]) = self.screen_to_image_px(*position, bounds) {
                    let pt = [px as f32, py as f32];
                    state.select_current_px = Some(pt);
                    if matches!(self.tool, CanvasTool::Lasso | CanvasTool::QuickSelect) {
                        state.select_points_px.push(pt);
                    } else if self.tool == CanvasTool::MagneticLasso
                        && let (Some(&last_anchor), Some(frame)) =
                            (state.select_points_px.last(), self.frame.as_ref())
                    {
                        state.magnetic_preview_px = magnetic_snap(frame, last_anchor, pt);
                    }
                    return Some(Action::request_redraw());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.drawing_shape => {
                if let Some([px, py]) = self.screen_to_image_px(*position, bounds) {
                    let pt = [px as f32, py as f32];
                    if state.bezier_dragging_handle
                        && self.tool == CanvasTool::BezierPen
                        && let Some(last) = state.bezier_nodes.last_mut()
                    {
                        let dx = pt[0] - last.anchor[0];
                        let dy = pt[1] - last.anchor[1];
                        if dx * dx + dy * dy > 4.0 {
                            last.handle_out = Some(pt);
                            last.handle_in = Some([last.anchor[0] - dx, last.anchor[1] - dy]);
                        }
                    }
                    state.draw_current_px = Some(pt);
                    return Some(Action::request_redraw());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => {
                if let Some(prev) = state.last_cursor {
                    let dx = position.x - prev.x;
                    let dy = position.y - prev.y;
                    state.last_cursor = Some(*position);
                    return Some(Action::publish(CanvasEvent::PanBy { dx, dy }).and_capture());
                }
                state.last_cursor = Some(*position);
            }
            Event::Mouse(mouse::Event::CursorMoved { .. })
                if cursor.is_over(bounds)
                    && matches!(
                        self.tool,
                        CanvasTool::Brush
                            | CanvasTool::Eraser
                            | CanvasTool::Liquify
                            | CanvasTool::QuickSelect
                    ) =>
            {
                return Some(Action::request_redraw());
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        let show_brush_ring = self.brush_radius_px > 0.0
            && matches!(
                self.tool,
                CanvasTool::Brush
                    | CanvasTool::Eraser
                    | CanvasTool::Liquify
                    | CanvasTool::QuickSelect
            )
            && cursor.is_over(bounds);
        let cursor_doc_pos = if show_brush_ring
            && let Some(pos) = cursor.position()
            && let Some([dx, dy]) = self.screen_to_image_px(pos, bounds)
        {
            [dx as f32, dy as f32]
        } else {
            [-10000.0, -10000.0]
        };
        let cursor_radius_px = if show_brush_ring {
            self.brush_radius_px
        } else {
            0.0
        };
        let (sel_preview_kind, sel_preview_rect) = if state.selecting
            && matches!(
                self.tool,
                CanvasTool::MarqueeRect | CanvasTool::MarqueeEllipse
            )
            && let (Some(a), Some(b), Some(frame)) = (
                state.select_anchor_px,
                state.select_current_px,
                self.frame.as_ref(),
            ) {
            let fw = frame.width.max(1) as f32;
            let fh = frame.height.max(1) as f32;
            let u0 = (a[0].min(b[0]) / fw).clamp(0.0, 1.0);
            let v0 = (a[1].min(b[1]) / fh).clamp(0.0, 1.0);
            let u1 = (a[0].max(b[0]) / fw).clamp(0.0, 1.0);
            let v1 = (a[1].max(b[1]) / fh).clamp(0.0, 1.0);
            let kind = if self.tool == CanvasTool::MarqueeEllipse {
                2.0
            } else {
                1.0
            };
            (kind, [u0, v0, u1, v1])
        } else {
            (0.0, [0.0; 4])
        };
        let (overlay_lines, overlay_tris) = self.build_vector_overlay(state, cursor, bounds);

        ImageQuadPrimitive {
            frame: self.frame.clone(),
            pair_frame: self.pair_frame.clone(),
            dual_gap_px: self.dual_gap_px,
            compare_frame: self.compare_frame.clone(),
            selection_mask: self.selection_mask.clone(),
            has_selection: self.has_selection,
            quick_mask_active: self.quick_mask_active,
            marching_ants_phase: self.marching_ants_phase,
            sel_preview_kind,
            sel_preview_rect,
            zoom: self.zoom,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            adjustments: self.adjustments,
            dark_theme: self.dark_theme,
            crop_rect: self.crop_rect,
            quarter_turns: self.quarter_turns % 4,
            preview_angle_rad: self.preview_angle_rad,
            preview_fill: self.preview_fill,
            canvas_bg: self.canvas_bg,
            viewport_bg: self.viewport_bg,
            compare_mode: self.compare_mode,
            compare_split_x: self.compare_split_x,
            cursor_doc_pos,
            cursor_radius_px,
            bounds_size: [bounds.width.max(1.0), bounds.height.max(1.0)],
            overlay_lines,
            overlay_tris,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging_split
            || (cursor
                .position()
                .is_some_and(|p| self.is_over_split_handle(p, bounds)))
        {
            mouse::Interaction::ResizingHorizontally
        } else if state.dragging {
            mouse::Interaction::Grabbing
        } else if state.space_pan_active && cursor.is_over(bounds) {
            mouse::Interaction::Grab
        } else if !cursor.is_over(bounds) {
            mouse::Interaction::default()
        } else if self.crop_rect.is_some()
            || self.tool.is_selection()
            || matches!(
                self.tool,
                CanvasTool::Brush
                    | CanvasTool::Eraser
                    | CanvasTool::Liquify
                    | CanvasTool::Eyedropper
                    | CanvasTool::Crop
                    | CanvasTool::DrawLine
                    | CanvasTool::DrawRect
                    | CanvasTool::DrawEllipse
                    | CanvasTool::DrawPolygon
                    | CanvasTool::DrawPolyline
                    | CanvasTool::BezierPen
                    | CanvasTool::PaintBucket
                    | CanvasTool::Gradient
            )
        {
            mouse::Interaction::Crosshair
        } else if self.tool == CanvasTool::Zoom {
            mouse::Interaction::ZoomIn
        } else {
            mouse::Interaction::Grab
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct OverlayVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

impl ImageCanvasProgram {
    fn push_overlay_dot(
        &self,
        tris: &mut Vec<OverlayVertex>,
        doc_px: [f32; 2],
        radius_px: f32,
        color: [f32; 4],
        bounds: Rectangle,
    ) {
        if let Some([cx, cy]) = self.doc_px_to_ndc(doc_px, bounds) {
            let [half_w, half_h_neg] = self.screen_delta_to_ndc(radius_px, radius_px, bounds);
            let half_h = half_h_neg.abs();
            let tl = [cx - half_w, cy + half_h];
            let tr = [cx + half_w, cy + half_h];
            let bl = [cx - half_w, cy - half_h];
            let br = [cx + half_w, cy - half_h];
            tris.extend_from_slice(&[
                OverlayVertex { pos: tl, color },
                OverlayVertex { pos: bl, color },
                OverlayVertex { pos: br, color },
                OverlayVertex { pos: tl, color },
                OverlayVertex { pos: br, color },
                OverlayVertex { pos: tr, color },
            ]);
        }
    }

    fn push_overlay_circle(
        &self,
        lines: &mut Vec<OverlayVertex>,
        center_ndc: [f32; 2],
        rx_ndc: f32,
        ry_ndc: f32,
        color: [f32; 4],
        segments: usize,
    ) {
        for s in 0..segments {
            let a0 = (s as f32 / segments as f32) * std::f32::consts::TAU;
            let a1 = ((s + 1) as f32 / segments as f32) * std::f32::consts::TAU;
            lines.push(OverlayVertex {
                pos: [
                    center_ndc[0] + rx_ndc * a0.cos(),
                    center_ndc[1] + ry_ndc * a0.sin(),
                ],
                color,
            });
            lines.push(OverlayVertex {
                pos: [
                    center_ndc[0] + rx_ndc * a1.cos(),
                    center_ndc[1] + ry_ndc * a1.sin(),
                ],
                color,
            });
        }
    }

    fn build_vector_overlay(
        &self,
        state: &CanvasInteractionState,
        cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> (Vec<OverlayVertex>, Vec<OverlayVertex>) {
        let mut lines = Vec::new();
        let mut tris = Vec::new();

        // Stabilizer leash/circle overlay for Brush / Eraser
        if self.stabilizer_radius > 0.0
            && matches!(self.tool, CanvasTool::Brush | CanvasTool::Eraser)
            && cursor.is_over(bounds)
            && let Some(pos) = cursor.position()
            && let Some([cx, cy]) = self.screen_to_image_px(pos, bounds)
        {
            let cur_doc = [cx as f32, cy as f32];
            let center_doc = state.stabilizer_pos_px.unwrap_or(cur_doc);
            if let (Some(c_ndc), Some(edge_ndc)) = (
                self.doc_px_to_ndc(center_doc, bounds),
                self.doc_px_to_ndc(
                    [center_doc[0] + self.stabilizer_radius, center_doc[1]],
                    bounds,
                ),
            ) {
                let rx = (edge_ndc[0] - c_ndc[0]).abs();
                let aspect = bounds.width.max(1.0) / bounds.height.max(1.0);
                let ry = rx * aspect;
                self.push_overlay_circle(&mut lines, c_ndc, rx, ry, [0.35, 0.85, 1.0, 0.75], 24);
            }
            if state.painting
                && let Some(stab) = state.stabilizer_pos_px
                && let (Some(a), Some(b)) = (
                    self.doc_px_to_ndc(stab, bounds),
                    self.doc_px_to_ndc(cur_doc, bounds),
                )
            {
                lines.push(OverlayVertex {
                    pos: a,
                    color: [1.0, 0.8, 0.25, 0.9],
                });
                lines.push(OverlayVertex {
                    pos: b,
                    color: [1.0, 0.8, 0.25, 0.9],
                });
                self.push_overlay_dot(&mut tris, stab, 3.0, [1.0, 0.8, 0.25, 0.95], bounds);
            }
        }

        // Shape & Gradient & BezierPen overlays
        let line_color = [0.95, 0.98, 1.0, 0.92];
        let rubber_color = [0.35, 0.90, 1.0, 0.95];

        if state.drawing_shape {
            match self.tool {
                CanvasTool::DrawLine | CanvasTool::Gradient => {
                    if let (Some(a), Some(b)) = (state.draw_anchor_px, state.draw_current_px)
                        && let (Some(na), Some(nb)) =
                            (self.doc_px_to_ndc(a, bounds), self.doc_px_to_ndc(b, bounds))
                    {
                        lines.push(OverlayVertex {
                            pos: na,
                            color: rubber_color,
                        });
                        lines.push(OverlayVertex {
                            pos: nb,
                            color: rubber_color,
                        });
                        self.push_overlay_dot(&mut tris, a, 3.5, [0.2, 0.8, 1.0, 1.0], bounds);
                        self.push_overlay_dot(&mut tris, b, 3.5, [1.0, 1.0, 1.0, 0.95], bounds);
                    }
                }
                CanvasTool::DrawRect => {
                    if let (Some(a), Some(b)) = (state.draw_anchor_px, state.draw_current_px) {
                        let x0 = a[0].min(b[0]);
                        let y0 = a[1].min(b[1]);
                        let x1 = a[0].max(b[0]);
                        let y1 = a[1].max(b[1]);
                        let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
                        for i in 0..4 {
                            if let (Some(na), Some(nb)) = (
                                self.doc_px_to_ndc(corners[i], bounds),
                                self.doc_px_to_ndc(corners[(i + 1) % 4], bounds),
                            ) {
                                lines.push(OverlayVertex {
                                    pos: na,
                                    color: rubber_color,
                                });
                                lines.push(OverlayVertex {
                                    pos: nb,
                                    color: rubber_color,
                                });
                            }
                        }
                    }
                }
                CanvasTool::DrawEllipse => {
                    if let (Some(a), Some(b)) = (state.draw_anchor_px, state.draw_current_px) {
                        let cx = (a[0] + b[0]) * 0.5;
                        let cy = (a[1] + b[1]) * 0.5;
                        let rx = (a[0] - b[0]).abs() * 0.5;
                        let ry = (a[1] - b[1]).abs() * 0.5;
                        const SEGS: usize = 32;
                        for s in 0..SEGS {
                            let t0 = (s as f32 / SEGS as f32) * std::f32::consts::TAU;
                            let t1 = ((s + 1) as f32 / SEGS as f32) * std::f32::consts::TAU;
                            let p0 = [cx + rx * t0.cos(), cy + ry * t0.sin()];
                            let p1 = [cx + rx * t1.cos(), cy + ry * t1.sin()];
                            if let (Some(na), Some(nb)) = (
                                self.doc_px_to_ndc(p0, bounds),
                                self.doc_px_to_ndc(p1, bounds),
                            ) {
                                lines.push(OverlayVertex {
                                    pos: na,
                                    color: rubber_color,
                                });
                                lines.push(OverlayVertex {
                                    pos: nb,
                                    color: rubber_color,
                                });
                            }
                        }
                    }
                }
                CanvasTool::DrawPolygon | CanvasTool::DrawPolyline
                    if !state.draw_points.is_empty() =>
                {
                    for pair in state.draw_points.windows(2) {
                        if let (Some(na), Some(nb)) = (
                            self.doc_px_to_ndc(pair[0], bounds),
                            self.doc_px_to_ndc(pair[1], bounds),
                        ) {
                            lines.push(OverlayVertex {
                                pos: na,
                                color: line_color,
                            });
                            lines.push(OverlayVertex {
                                pos: nb,
                                color: line_color,
                            });
                        }
                    }
                    if let (Some(&last), Some(cur)) =
                        (state.draw_points.last(), state.draw_current_px)
                        && let (Some(na), Some(nb)) = (
                            self.doc_px_to_ndc(last, bounds),
                            self.doc_px_to_ndc(cur, bounds),
                        )
                    {
                        lines.push(OverlayVertex {
                            pos: na,
                            color: rubber_color,
                        });
                        lines.push(OverlayVertex {
                            pos: nb,
                            color: rubber_color,
                        });
                    }
                    for (i, &pt) in state.draw_points.iter().enumerate() {
                        let c = if i == 0 {
                            [0.20, 0.80, 1.0, 1.0]
                        } else {
                            [1.0, 1.0, 1.0, 0.95]
                        };
                        self.push_overlay_dot(&mut tris, pt, 3.5, c, bounds);
                    }
                    if self.tool == CanvasTool::DrawPolygon && state.draw_points.len() >= 3 {
                        let first = state.draw_points[0];
                        let snap_thresh = 10.0 / self.zoom.max(0.2);
                        let is_snapped = state.draw_current_px.is_some_and(|cur| {
                            let dx = cur[0] - first[0];
                            let dy = cur[1] - first[1];
                            (dx * dx + dy * dy).sqrt() <= snap_thresh
                        });
                        if let Some(c_ndc) = self.doc_px_to_ndc(first, bounds) {
                            let [rx, ry_neg] = self.screen_delta_to_ndc(9.0, 9.0, bounds);
                            let ring_color = if is_snapped {
                                [0.20, 1.0, 0.55, 1.0]
                            } else {
                                [1.0, 1.0, 1.0, 0.65]
                            };
                            self.push_overlay_circle(
                                &mut lines,
                                c_ndc,
                                rx,
                                ry_neg.abs(),
                                ring_color,
                                20,
                            );
                        }
                    }
                }
                CanvasTool::BezierPen if !state.bezier_nodes.is_empty() => {
                    let handle_color = [1.0, 0.78, 0.25, 0.90];
                    // Tessellate placed Bezier segments
                    for pair in state.bezier_nodes.windows(2) {
                        let p0 = pair[0].anchor;
                        let p1 = pair[0].handle_out.unwrap_or(p0);
                        let p3 = pair[1].anchor;
                        let p2 = pair[1].handle_in.unwrap_or(p3);
                        const STEPS: usize = 16;
                        for s in 0..STEPS {
                            let t0 = s as f32 / STEPS as f32;
                            let t1 = (s + 1) as f32 / STEPS as f32;
                            let eval = |t: f32| -> [f32; 2] {
                                let u = 1.0 - t;
                                let b0 = u * u * u;
                                let b1 = 3.0 * u * u * t;
                                let b2 = 3.0 * u * t * t;
                                let b3 = t * t * t;
                                [
                                    b0 * p0[0] + b1 * p1[0] + b2 * p2[0] + b3 * p3[0],
                                    b0 * p0[1] + b1 * p1[1] + b2 * p2[1] + b3 * p3[1],
                                ]
                            };
                            if let (Some(na), Some(nb)) = (
                                self.doc_px_to_ndc(eval(t0), bounds),
                                self.doc_px_to_ndc(eval(t1), bounds),
                            ) {
                                lines.push(OverlayVertex {
                                    pos: na,
                                    color: line_color,
                                });
                                lines.push(OverlayVertex {
                                    pos: nb,
                                    color: line_color,
                                });
                            }
                        }
                    }
                    // Rubber-band curve to cursor when not pulling handle
                    if !state.bezier_dragging_handle
                        && let (Some(last), Some(cur)) =
                            (state.bezier_nodes.last(), state.draw_current_px)
                    {
                        let p0 = last.anchor;
                        let p1 = last.handle_out.unwrap_or(p0);
                        let p2 = cur;
                        let p3 = cur;
                        const STEPS: usize = 12;
                        for s in 0..STEPS {
                            let t0 = s as f32 / STEPS as f32;
                            let t1 = (s + 1) as f32 / STEPS as f32;
                            let eval = |t: f32| -> [f32; 2] {
                                let u = 1.0 - t;
                                let b0 = u * u * u;
                                let b1 = 3.0 * u * u * t;
                                let b2 = 3.0 * u * t * t;
                                let b3 = t * t * t;
                                [
                                    b0 * p0[0] + b1 * p1[0] + b2 * p2[0] + b3 * p3[0],
                                    b0 * p0[1] + b1 * p1[1] + b2 * p2[1] + b3 * p3[1],
                                ]
                            };
                            if let (Some(na), Some(nb)) = (
                                self.doc_px_to_ndc(eval(t0), bounds),
                                self.doc_px_to_ndc(eval(t1), bounds),
                            ) {
                                lines.push(OverlayVertex {
                                    pos: na,
                                    color: rubber_color,
                                });
                                lines.push(OverlayVertex {
                                    pos: nb,
                                    color: rubber_color,
                                });
                            }
                        }
                    }
                    // Tangent handles and anchor dots
                    for (i, node) in state.bezier_nodes.iter().enumerate() {
                        for h in [node.handle_in, node.handle_out].into_iter().flatten() {
                            if let (Some(na), Some(nb)) = (
                                self.doc_px_to_ndc(node.anchor, bounds),
                                self.doc_px_to_ndc(h, bounds),
                            ) {
                                lines.push(OverlayVertex {
                                    pos: na,
                                    color: handle_color,
                                });
                                lines.push(OverlayVertex {
                                    pos: nb,
                                    color: handle_color,
                                });
                            }
                            self.push_overlay_dot(&mut tris, h, 2.5, handle_color, bounds);
                        }
                        let c = if i == 0 {
                            [0.20, 0.80, 1.0, 1.0]
                        } else {
                            [1.0, 1.0, 1.0, 0.95]
                        };
                        self.push_overlay_dot(&mut tris, node.anchor, 3.5, c, bounds);
                    }
                    if state.bezier_nodes.len() >= 3 {
                        let first = state.bezier_nodes[0].anchor;
                        let snap_thresh = 10.0 / self.zoom.max(0.2);
                        let is_snapped = state.draw_current_px.is_some_and(|cur| {
                            let dx = cur[0] - first[0];
                            let dy = cur[1] - first[1];
                            (dx * dx + dy * dy).sqrt() <= snap_thresh
                        });
                        if let Some(c_ndc) = self.doc_px_to_ndc(first, bounds) {
                            let [rx, ry_neg] = self.screen_delta_to_ndc(9.0, 9.0, bounds);
                            let ring_color = if is_snapped {
                                [0.20, 1.0, 0.55, 1.0]
                            } else {
                                [1.0, 1.0, 1.0, 0.65]
                            };
                            self.push_overlay_circle(
                                &mut lines,
                                c_ndc,
                                rx,
                                ry_neg.abs(),
                                ring_color,
                                20,
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        if !state.selecting
            || state.select_points_px.is_empty()
            || !matches!(
                self.tool,
                CanvasTool::PolygonLasso
                    | CanvasTool::MagneticLasso
                    | CanvasTool::BezierSelect
                    | CanvasTool::Lasso
            )
        {
            return (lines, tris);
        }

        let line_color = [0.95, 0.98, 1.0, 0.92];
        let rubber_color = [0.35, 0.90, 1.0, 0.95];

        // Placed polylines
        for pair in state.select_points_px.windows(2) {
            if let (Some(a), Some(b)) = (
                self.doc_px_to_ndc(pair[0], bounds),
                self.doc_px_to_ndc(pair[1], bounds),
            ) {
                lines.push(OverlayVertex {
                    pos: a,
                    color: line_color,
                });
                lines.push(OverlayVertex {
                    pos: b,
                    color: line_color,
                });
            }
        }

        // Rubber-band line or live magnetic preview
        if self.tool == CanvasTool::MagneticLasso && !state.magnetic_preview_px.is_empty() {
            if let (Some(&last), Some(&first_prev)) = (
                state.select_points_px.last(),
                state.magnetic_preview_px.first(),
            ) && let (Some(a), Some(b)) = (
                self.doc_px_to_ndc(last, bounds),
                self.doc_px_to_ndc(first_prev, bounds),
            ) {
                lines.push(OverlayVertex {
                    pos: a,
                    color: rubber_color,
                });
                lines.push(OverlayVertex {
                    pos: b,
                    color: rubber_color,
                });
            }
            for pair in state.magnetic_preview_px.windows(2) {
                if let (Some(a), Some(b)) = (
                    self.doc_px_to_ndc(pair[0], bounds),
                    self.doc_px_to_ndc(pair[1], bounds),
                ) {
                    lines.push(OverlayVertex {
                        pos: a,
                        color: rubber_color,
                    });
                    lines.push(OverlayVertex {
                        pos: b,
                        color: rubber_color,
                    });
                }
            }
        } else if matches!(
            self.tool,
            CanvasTool::PolygonLasso | CanvasTool::MagneticLasso | CanvasTool::BezierSelect
        ) && let (Some(&last), Some(cur)) =
            (state.select_points_px.last(), state.select_current_px)
            && let (Some(a), Some(b)) = (
                self.doc_px_to_ndc(last, bounds),
                self.doc_px_to_ndc(cur, bounds),
            )
        {
            lines.push(OverlayVertex {
                pos: a,
                color: rubber_color,
            });
            lines.push(OverlayVertex {
                pos: b,
                color: rubber_color,
            });
        }

        // Vertex anchor dots and close-snap ring for polygon-style tools
        if matches!(
            self.tool,
            CanvasTool::PolygonLasso | CanvasTool::MagneticLasso | CanvasTool::BezierSelect
        ) {
            let [half_w, half_h_neg] = self.screen_delta_to_ndc(3.5, 3.5, bounds);
            let half_h = half_h_neg.abs();
            let last_idx = state.select_points_px.len() - 1;
            let stride = if self.tool == CanvasTool::MagneticLasso {
                12
            } else {
                1
            };
            for (i, &pt) in state.select_points_px.iter().enumerate() {
                if i != 0 && i != last_idx && i % stride != 0 {
                    continue;
                }
                if let Some([cx, cy]) = self.doc_px_to_ndc(pt, bounds) {
                    let dot_color = if i == 0 {
                        [0.20, 0.80, 1.0, 1.0]
                    } else {
                        [1.0, 1.0, 1.0, 0.95]
                    };
                    let tl = [cx - half_w, cy + half_h];
                    let tr = [cx + half_w, cy + half_h];
                    let bl = [cx - half_w, cy - half_h];
                    let br = [cx + half_w, cy - half_h];
                    tris.extend_from_slice(&[
                        OverlayVertex {
                            pos: tl,
                            color: dot_color,
                        },
                        OverlayVertex {
                            pos: bl,
                            color: dot_color,
                        },
                        OverlayVertex {
                            pos: br,
                            color: dot_color,
                        },
                        OverlayVertex {
                            pos: tl,
                            color: dot_color,
                        },
                        OverlayVertex {
                            pos: br,
                            color: dot_color,
                        },
                        OverlayVertex {
                            pos: tr,
                            color: dot_color,
                        },
                    ]);
                }
            }

            // Close-snap ring at first vertex when >= 3 vertices
            if state.select_points_px.len() >= 3 {
                let first = state.select_points_px[0];
                let snap_thresh = 10.0 / self.zoom.max(0.2);
                let is_snapped = state.select_current_px.is_some_and(|cur| {
                    let dx = cur[0] - first[0];
                    let dy = cur[1] - first[1];
                    (dx * dx + dy * dy).sqrt() <= snap_thresh
                });
                if let Some([cx, cy]) = self.doc_px_to_ndc(first, bounds) {
                    let [rx, ry_neg] = self.screen_delta_to_ndc(9.0, 9.0, bounds);
                    let ry = ry_neg.abs();
                    let ring_color = if is_snapped {
                        [0.20, 1.0, 0.55, 1.0]
                    } else {
                        [1.0, 1.0, 1.0, 0.65]
                    };
                    const SEGMENTS: usize = 20;
                    for s in 0..SEGMENTS {
                        let a0 = (s as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
                        let a1 = ((s + 1) as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
                        lines.push(OverlayVertex {
                            pos: [cx + rx * a0.cos(), cy + ry * a0.sin()],
                            color: ring_color,
                        });
                        lines.push(OverlayVertex {
                            pos: [cx + rx * a1.cos(), cy + ry * a1.sin()],
                            color: ring_color,
                        });
                    }
                }
            }
        }

        (lines, tris)
    }
}

#[derive(Debug, Clone)]
pub struct ImageQuadPrimitive {
    pub frame: Option<CpuImageFrame>,
    pub pair_frame: Option<CpuImageFrame>,
    pub dual_gap_px: f32,
    pub compare_frame: Option<CpuImageFrame>,
    pub selection_mask: Option<CpuMaskFrame>,
    pub has_selection: bool,
    pub quick_mask_active: bool,
    pub marching_ants_phase: f32,
    pub sel_preview_kind: f32,
    pub sel_preview_rect: [f32; 4],
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub adjustments: ColorAdjustments,
    pub dark_theme: bool,
    pub crop_rect: Option<[f32; 4]>,
    pub quarter_turns: u8,
    pub preview_angle_rad: f32,
    pub preview_fill: [f32; 4],
    pub canvas_bg: Option<[u8; 3]>,
    pub viewport_bg: Option<[u8; 3]>,
    pub compare_mode: u8,
    pub compare_split_x: f32,
    pub cursor_doc_pos: [f32; 2],
    pub cursor_radius_px: f32,
    pub bounds_size: [f32; 2],
    pub overlay_lines: Vec<OverlayVertex>,
    pub overlay_tris: Vec<OverlayVertex>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    viewport_size: [f32; 2],
    image_size: [f32; 2],
    pan: [f32; 2],
    zoom: f32,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    exposure: f32,
    highlights: f32,
    shadows: f32,
    has_image: f32,
    dark_theme: f32,
    crop_active: f32,
    crop_rect: [f32; 4],
    quarter_turns: f32,
    preview_angle_rad: f32,
    has_canvas_bg: f32,
    has_compare: f32,
    canvas_bg: [f32; 4],
    preview_fill: [f32; 4],
    compare_mode: f32,
    compare_split_x: f32,
    gamma: f32,
    cursor_radius_px: f32,
    cursor_doc_pos: [f32; 2],
    has_viewport_bg: f32,
    _pad: f32,
    viewport_bg: [f32; 4],
    pair_size: [f32; 2],
    has_pair: f32,
    dual_gap_px: f32,
    has_selection: f32,
    quick_mask_active: f32,
    marching_ants_phase: f32,
    sel_preview_kind: f32,
    sel_preview_rect: [f32; 4],
}

impl Uniforms {
    fn as_bytes(&self) -> &[u8] {
        // Safety: Uniforms is #[repr(C)] containing only plain f32 values (192 bytes).
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self).cast::<u8>(),
                std::mem::size_of::<Self>(),
            )
        }
    }
}

struct CachedSimpleTexture {
    revision: u64,
    width: u32,
    height: u32,
    view: wgpu::TextureView,
}

struct CachedGpuTexture {
    revision: u64,
    width: u32,
    height: u32,
    image_view: wgpu::TextureView,
    bound_compare: Option<(String, u64)>,
    bound_sel: Option<(String, u64)>,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct ImageShaderPipeline {
    pipeline: wgpu::RenderPipeline,
    overlay_line_pipeline: wgpu::RenderPipeline,
    overlay_tri_pipeline: wgpu::RenderPipeline,
    overlay_line_buffer: wgpu::Buffer,
    overlay_line_capacity: usize,
    overlay_line_count: u32,
    overlay_tri_buffer: wgpu::Buffer,
    overlay_tri_capacity: usize,
    overlay_tri_count: u32,
    placeholder_uniform_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    placeholder_view: wgpu::TextureView,
    placeholder_bind_group: wgpu::BindGroup,
    textures: HashMap<String, CachedGpuTexture>,
    compare_textures: HashMap<String, CachedSimpleTexture>,
    sel_mask_textures: HashMap<String, CachedSimpleTexture>,
    recent_keys: Vec<String>,
}

const WGSL_SHADER: &str = r#"
struct Uniforms {
    viewport_size: vec2<f32>,
    image_size: vec2<f32>,
    pan: vec2<f32>,
    zoom: f32,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    exposure: f32,
    highlights: f32,
    shadows: f32,
    has_image: f32,
    dark_theme: f32,
    crop_active: f32,
    crop_rect: vec4<f32>,
    quarter_turns: f32,
    preview_angle_rad: f32,
    has_canvas_bg: f32,
    has_compare: f32,
    canvas_bg: vec4<f32>,
    preview_fill: vec4<f32>,
    compare_mode: f32,
    compare_split_x: f32,
    gamma: f32,
    cursor_radius_px: f32,
    cursor_doc_pos: vec2<f32>,
    has_viewport_bg: f32,
    _pad: f32,
    viewport_bg: vec4<f32>,
    pair_size: vec2<f32>,
    has_pair: f32,
    dual_gap_px: f32,
    has_selection: f32,
    quick_mask_active: f32,
    marching_ants_phase: f32,
    sel_preview_kind: f32,
    sel_preview_rect: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var img_tex: texture_2d<f32>;
@group(0) @binding(2) var img_sampler: sampler;
@group(0) @binding(3) var compare_tex: texture_2d<f32>;
@group(0) @binding(4) var sel_mask_tex: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    var out: VsOut;
    out.pos = vec4<f32>(positions[idx], 0.0, 1.0);
    out.uv = uvs[idx];
    return out;
}

fn apply_color_and_light(in_rgb: vec3<f32>) -> vec3<f32> {
    var rgb = in_rgb * exp2(clamp(u.exposure, -2.0, 2.0));
    let gamma = clamp(u.gamma, 0.2, 3.0);
    if (abs(gamma - 1.0) > 0.0001) {
        rgb = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / gamma));
    }
    let luma_pre = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let shadows_w = 1.0 - smoothstep(0.0, 0.55, luma_pre);
    let highlights_w = smoothstep(0.45, 1.0, luma_pre);
    let tone_delta = u.shadows * 0.5 * shadows_w + u.highlights * 0.5 * highlights_w;
    rgb = rgb + vec3<f32>(tone_delta);

    let c = clamp(u.contrast, -1.0, 1.0);
    let contrast = select(1.0 + c, 1.0 / (1.0 - c * 0.999), c >= 0.0);
    rgb = (rgb - vec3<f32>(0.5)) * contrast + vec3<f32>(0.5) + vec3<f32>(u.brightness);

    let luma = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let sat = max(1.0 + u.saturation, 0.0);
    return clamp(mix(vec3<f32>(luma), rgb, sat), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Compute the UV of `pixel` within a fitted image, given a sub-viewport
// defined by its left edge `vp_x0` and width `vp_w`, full height `vp_h`.
// Uses the same zoom/pan uniforms for synchronized navigation.
// Returns a vec3: (uv.x, uv.y, in_bounds) where in_bounds is 1.0 or 0.0.
fn half_vp_uv(pixel: vec2<f32>, img_sz: vec2<f32>, vp_x0: f32, vp_w: f32, vp_h: f32) -> vec3<f32> {
    let half_vp = vec2<f32>(vp_w, vp_h);
    let img_aspect = img_sz.x / max(img_sz.y, 1.0);
    let hv_aspect = vp_w / max(vp_h, 1.0);
    var fit: vec2<f32>;
    if (img_aspect > hv_aspect) {
        fit = vec2<f32>(vp_w, vp_w / img_aspect);
    } else {
        fit = vec2<f32>(vp_h * img_aspect, vp_h);
    }
    let drawn = fit * u.zoom;
    // Pan is relative to the half-viewport centre
    let half_center = vec2<f32>(vp_x0 + vp_w * 0.5, vp_h * 0.5) + u.pan;
    let tl = half_center - drawn * 0.5;
    let uv = (pixel - tl) / drawn;
    let in_bounds = select(0.0, 1.0,
        uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0);
    return vec3<f32>(clamp(uv.x, 0.0, 1.0), clamp(uv.y, 0.0, 1.0), in_bounds);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let pixel = in.uv * u.viewport_size;
    let default_bg = select(
        vec3<f32>(0.9137255, 0.9254902, 0.9411765),
        vec3<f32>(0.0627451, 0.07058824, 0.078431375),
        u.dark_theme > 0.5
    );
    let bg = select(default_bg, u.viewport_bg.rgb, u.has_viewport_bg > 0.5);

    if (u.has_image < 0.5) {
        return vec4<f32>(bg, 1.0);
    }

    let turns = i32(round(u.quarter_turns)) & 3;
    let odd_rot = (turns & 1) == 1;

    // ── Mode 2: Side-by-Side ──────────────────────────────────────────────────
    // Left half  = compare_frame (Original), fitted to left half-viewport.
    // Right half = primary edited frame,     fitted to right half-viewport.
    // Both halves share the same zoom & pan → synchronized navigation.
    if (u.compare_mode > 1.5 && u.compare_mode < 2.5 && u.has_compare > 0.5) {
        let vp_w = u.viewport_size.x;
        let vp_h = u.viewport_size.y;
        let half_w = vp_w * 0.5;
        let center_px = half_w;
        let sbs_size = select(u.image_size, vec2<f32>(u.image_size.y, u.image_size.x), odd_rot);

        // Checkerboard for this pixel
        let tile2 = vec2<i32>(floor(pixel / 10.0));
        let odd2 = ((tile2.x + tile2.y) & 1) == 1;
        let ca2 = select(vec3<f32>(0.85, 0.85, 0.85), vec3<f32>(0.18, 0.19, 0.21), u.dark_theme > 0.5);
        let cb2 = select(vec3<f32>(0.75, 0.75, 0.75), vec3<f32>(0.24, 0.25, 0.28), u.dark_theme > 0.5);
        var bd2 = select(ca2, cb2, odd2);
        if (u.has_canvas_bg > 0.5) {
            bd2 = mix(bd2, u.canvas_bg.rgb, u.canvas_bg.a);
        }

        // Crisp 1-px white center divider
        if (abs(pixel.x - center_px) < 1.0) {
            return vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }

        var out2: vec3<f32> = bg;
        let is_left = pixel.x < center_px;
        let r = half_vp_uv(pixel, sbs_size, select(half_w, 0.0, is_left), half_w, vp_h);
        if (r.z > 0.5) {
            var sbs_uv = r.xy;
            if (turns == 1) {
                sbs_uv = vec2<f32>(sbs_uv.y, 1.0 - sbs_uv.x);
            } else if (turns == 2) {
                sbs_uv = vec2<f32>(1.0 - sbs_uv.x, 1.0 - sbs_uv.y);
            } else if (turns == 3) {
                sbs_uv = vec2<f32>(1.0 - sbs_uv.y, sbs_uv.x);
            }
            if (is_left) {
                // Left half: Original (compare_tex)
                let s = textureSample(compare_tex, img_sampler, sbs_uv);
                out2 = mix(bd2, s.rgb, s.a);
            } else {
                // Right half: Edited primary (img_tex)
                let s = textureSample(img_tex, img_sampler, sbs_uv);
                let adj = apply_color_and_light(s.rgb);
                out2 = mix(bd2, adj, s.a);
            }
        }
        return vec4<f32>(out2, 1.0);
    }
    // ─────────────────────────────────────────────────────────────────────────
    var doc_size = u.image_size;
    var dual_left_w = u.image_size.x;
    var dual_right_w = 0.0;
    var dual_gap = 0.0;
    if (u.has_pair > 0.5) {
        let target_h = max(u.image_size.y, u.pair_size.y);
        dual_left_w = round(u.image_size.x * (target_h / max(u.image_size.y, 1.0)));
        dual_right_w = round(u.pair_size.x * (target_h / max(u.pair_size.y, 1.0)));
        if (u.dual_gap_px > 0.0) {
            dual_gap = u.dual_gap_px * max(target_h / 900.0, 1.0);
        }
        doc_size = vec2<f32>(dual_left_w + dual_gap + dual_right_w, target_h);
    }
    let base_size = select(doc_size, vec2<f32>(doc_size.y, doc_size.x), odd_rot);

    let ca = cos(u.preview_angle_rad);
    let sa = sin(u.preview_angle_rad);
    let abs_ca = abs(ca);
    let abs_sa = abs(sa);
    let eff_size = vec2<f32>(
        base_size.x * abs_ca + base_size.y * abs_sa,
        base_size.x * abs_sa + base_size.y * abs_ca
    );

    let vp_aspect = u.viewport_size.x / max(u.viewport_size.y, 1.0);
    let img_aspect = eff_size.x / max(eff_size.y, 1.0);
    var fit_size = u.viewport_size;
    if (img_aspect > vp_aspect) {
        fit_size.y = u.viewport_size.x / img_aspect;
    } else {
        fit_size.x = u.viewport_size.y * img_aspect;
    }
    let drawn_size = fit_size * u.zoom;
    let center = u.viewport_size * 0.5 + u.pan;
    let top_left = center - drawn_size * 0.5;
    let img_uv = (pixel - top_left) / drawn_size;

    if (img_uv.x < 0.0 || img_uv.x > 1.0 || img_uv.y < 0.0 || img_uv.y > 1.0) {
        return vec4<f32>(bg, 1.0);
    }

    // Checkerboard behind alpha regions
    let tile = vec2<i32>(floor(pixel / 10.0));
    let odd = ((tile.x + tile.y) & 1) == 1;
    let check_a = select(vec3<f32>(0.85, 0.85, 0.85), vec3<f32>(0.18, 0.19, 0.21), u.dark_theme > 0.5);
    let check_b = select(vec3<f32>(0.75, 0.75, 0.75), vec3<f32>(0.24, 0.25, 0.28), u.dark_theme > 0.5);
    var backdrop = select(check_a, check_b, odd);
    if (u.has_canvas_bg > 0.5) {
        backdrop = mix(backdrop, u.canvas_bg.rgb, u.canvas_bg.a);
    }

    var rot_uv = img_uv;
    var in_rot_bounds = true;
    if (abs(u.preview_angle_rad) > 0.0001) {
        let p_rot = (img_uv - vec2<f32>(0.5)) * eff_size;
        let p_src = vec2<f32>(
            p_rot.x * ca + p_rot.y * sa,
            -p_rot.x * sa + p_rot.y * ca
        );
        rot_uv = p_src / base_size + vec2<f32>(0.5);
        in_rot_bounds = rot_uv.x >= 0.0 && rot_uv.x <= 1.0 && rot_uv.y >= 0.0 && rot_uv.y <= 1.0;
    }

    var tex_uv = clamp(rot_uv, vec2<f32>(0.0), vec2<f32>(1.0));
    if (turns == 1) {
        tex_uv = vec2<f32>(tex_uv.y, 1.0 - tex_uv.x);
    } else if (turns == 2) {
        tex_uv = vec2<f32>(1.0 - tex_uv.x, 1.0 - tex_uv.y);
    } else if (turns == 3) {
        tex_uv = vec2<f32>(1.0 - tex_uv.y, tex_uv.x);
    }

    let spread_x = tex_uv.x * doc_size.x;
    let is_dual = u.has_pair > 0.5;
    let in_dual_gap = is_dual && spread_x >= dual_left_w && spread_x < (dual_left_w + dual_gap);
    let on_right_page = is_dual && spread_x >= (dual_left_w + dual_gap);

    let primary_uv = select(
        tex_uv,
        vec2<f32>(clamp(spread_x / max(dual_left_w, 1.0), 0.0, 1.0), tex_uv.y),
        is_dual
    );
    let secondary_uv = select(
        tex_uv,
        vec2<f32>(clamp((spread_x - dual_left_w - dual_gap) / max(dual_right_w, 1.0), 0.0, 1.0), tex_uv.y),
        is_dual
    );

    let prim_texel = textureSample(img_tex, img_sampler, primary_uv);
    let sec_texel = textureSample(compare_tex, img_sampler, secondary_uv);
    let texel = select(prim_texel, sec_texel, on_right_page);
    let comp_texel = sec_texel;

    if (in_rot_bounds && in_dual_gap) {
        return vec4<f32>(bg, 1.0);
    }

    var out_rgb = backdrop;
    if (!in_rot_bounds) {
        out_rgb = mix(backdrop, u.preview_fill.rgb, u.preview_fill.a);
    } else {
        // Mode 1 (Split): left of split_x shows compare; Mode 3 (HoldOriginal): whole image shows compare
        let show_compare = u.has_compare > 0.5 && (
            u.compare_mode > 2.5 ||
            (u.compare_mode > 0.5 && u.compare_mode < 1.5 && in.uv.x < u.compare_split_x)
        );
        if (show_compare) {
            out_rgb = mix(backdrop, comp_texel.rgb, comp_texel.a);
        } else {
            let adj_rgb = apply_color_and_light(texel.rgb);
            out_rgb = mix(backdrop, adj_rgb, texel.a);
        }
    }

    // Mode 1 (Split): draggable vertical divider line
    if (u.compare_mode > 0.5 && u.compare_mode < 1.5) {
        let split_px = u.compare_split_x * u.viewport_size.x;
        if (abs(pixel.x - split_px) < 1.25) {
            out_rgb = vec3<f32>(1.0, 1.0, 1.0);
        }
    }

    if (u.crop_active > 0.5) {
        let c0 = u.crop_rect.xy;
        let c1 = u.crop_rect.zw;
        let inside = img_uv.x >= c0.x && img_uv.x <= c1.x && img_uv.y >= c0.y && img_uv.y <= c1.y;
        if (!inside) {
            out_rgb = out_rgb * 0.35;
        } else {
            let px_uv = 1.5 / max(drawn_size, vec2<f32>(1.0));
            let on_edge = (img_uv.x - c0.x) < px_uv.x || (c1.x - img_uv.x) < px_uv.x || (img_uv.y - c0.y) < px_uv.y || (c1.y - img_uv.y) < px_uv.y;
            if (on_edge) {
                out_rgb = vec3<f32>(1.0, 1.0, 1.0);
            } else {
                let c_size = max(c1 - c0, vec2<f32>(0.001));
                let rel = (img_uv - c0) / c_size;
                let g_uv = 1.0 / max(drawn_size * c_size, vec2<f32>(1.0));
                let on_third = abs(rel.x - 0.33333) < g_uv.x || abs(rel.x - 0.66667) < g_uv.x || abs(rel.y - 0.33333) < g_uv.y || abs(rel.y - 0.66667) < g_uv.y;
                if (on_third) {
                    out_rgb = mix(out_rgb, vec3<f32>(1.0), 0.28);
                }
            }
        }
    }

    if (u.has_selection > 0.5 || u.quick_mask_active > 0.5) {
        let sel_m = textureSampleLevel(sel_mask_tex, img_sampler, img_uv, 0.0).r;
        if (u.quick_mask_active > 0.5) {
            let unmasked = 1.0 - sel_m;
            out_rgb = mix(out_rgb, vec3<f32>(0.92, 0.14, 0.16), unmasked * 0.45);
        }
        if (u.has_selection > 0.5) {
            let step_uv = 1.0 / max(drawn_size, vec2<f32>(1.0));
            let ml = textureSampleLevel(sel_mask_tex, img_sampler, img_uv - vec2<f32>(step_uv.x, 0.0), 0.0).r;
            let mr = textureSampleLevel(sel_mask_tex, img_sampler, img_uv + vec2<f32>(step_uv.x, 0.0), 0.0).r;
            let mu = textureSampleLevel(sel_mask_tex, img_sampler, img_uv - vec2<f32>(0.0, step_uv.y), 0.0).r;
            let md = textureSampleLevel(sel_mask_tex, img_sampler, img_uv + vec2<f32>(0.0, step_uv.y), 0.0).r;
            let min_n = min(min(ml, mr), min(mu, md));
            let max_n = max(max(ml, mr), max(mu, md));
            if (sel_m >= 0.5 && min_n < 0.5 || sel_m < 0.5 && max_n >= 0.5) {
                let dash = fract((pixel.x + pixel.y) * 0.125 - u.marching_ants_phase);
                out_rgb = select(vec3<f32>(0.08, 0.08, 0.10), vec3<f32>(1.0, 1.0, 1.0), dash < 0.5);
            }
        }
    }

    if (u.sel_preview_kind > 0.5) {
        let s0 = u.sel_preview_rect.xy;
        let s1 = u.sel_preview_rect.zw;
        let px_uv = 1.35 / max(drawn_size, vec2<f32>(1.0));
        var on_sel_edge = false;
        if (u.sel_preview_kind < 1.5) {
            let in_rect = img_uv.x >= s0.x && img_uv.x <= s1.x && img_uv.y >= s0.y && img_uv.y <= s1.y;
            let near_x = abs(img_uv.x - s0.x) < px_uv.x || abs(img_uv.x - s1.x) < px_uv.x;
            let near_y = abs(img_uv.y - s0.y) < px_uv.y || abs(img_uv.y - s1.y) < px_uv.y;
            on_sel_edge = in_rect && (near_x || near_y);
        } else {
            let c = (s0 + s1) * 0.5;
            let r = max((s1 - s0) * 0.5, vec2<f32>(0.0005));
            let d = length((img_uv - c) / r);
            let step_d = max(px_uv.x / r.x, px_uv.y / r.y);
            on_sel_edge = abs(d - 1.0) <= step_d;
        }
        if (on_sel_edge) {
            let dash = fract((pixel.x + pixel.y) * 0.125 - u.marching_ants_phase);
            out_rgb = select(vec3<f32>(0.08, 0.08, 0.10), vec3<f32>(1.0, 1.0, 1.0), dash < 0.5);
        }
    }

    if (u.cursor_radius_px > 0.5 && u.cursor_doc_pos.x > -9000.0) {
        let doc_px = rot_uv * base_size;
        let dist = length(doc_px - u.cursor_doc_pos);
        let px_per_screen = max(base_size.x / max(drawn_size.x, 1.0), 0.25);
        let diff = abs(dist - u.cursor_radius_px);
        if (diff <= px_per_screen * 0.9) {
            out_rgb = vec3<f32>(1.0, 1.0, 1.0);
        } else if (diff <= px_per_screen * 1.75) {
            out_rgb = vec3<f32>(0.05, 0.05, 0.08);
        }
    }
    return vec4<f32>(out_rgb, 1.0);
}
"#;

const OVERLAY_WGSL_SHADER: &str = r#"
struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_overlay(in: VsIn) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(in.pos, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_overlay(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

impl Pipeline for ImageShaderPipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("photon_image_shader"),
            source: wgpu::ShaderSource::Wgsl(WGSL_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("photon_image_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("photon_image_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("photon_image_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });

        let placeholder_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("photon_image_placeholder_uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("photon_image_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let placeholder_view = upload_rgba_texture(device, queue, 1, 1, &[0, 0, 0, 0]);
        let placeholder_bind_group = build_bind_group(
            device,
            &bind_group_layout,
            &placeholder_uniform_buffer,
            &sampler,
            &placeholder_view,
            &placeholder_view,
            &placeholder_view,
        );

        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("photon_overlay_shader"),
            source: wgpu::ShaderSource::Wgsl(OVERLAY_WGSL_SHADER.into()),
        });
        let overlay_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("photon_overlay_pl"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let overlay_vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<OverlayVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };
        let make_overlay_pipeline = |label: &str, topology: wgpu::PrimitiveTopology| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&overlay_pl),
                vertex: wgpu::VertexState {
                    module: &overlay_shader,
                    entry_point: Some("vs_overlay"),
                    buffers: std::slice::from_ref(&overlay_vbl),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &overlay_shader,
                    entry_point: Some("fs_overlay"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                multiview: None,
                cache: None,
            })
        };
        let overlay_line_pipeline = make_overlay_pipeline(
            "photon_overlay_line_pipeline",
            wgpu::PrimitiveTopology::LineList,
        );
        let overlay_tri_pipeline = make_overlay_pipeline(
            "photon_overlay_tri_pipeline",
            wgpu::PrimitiveTopology::TriangleList,
        );
        let initial_cap = 256usize;
        let overlay_line_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("photon_overlay_line_vb"),
            size: (initial_cap * std::mem::size_of::<OverlayVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let overlay_tri_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("photon_overlay_tri_vb"),
            size: (initial_cap * std::mem::size_of::<OverlayVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            overlay_line_pipeline,
            overlay_tri_pipeline,
            overlay_line_buffer,
            overlay_line_capacity: initial_cap,
            overlay_line_count: 0,
            overlay_tri_buffer,
            overlay_tri_capacity: initial_cap,
            overlay_tri_count: 0,
            placeholder_uniform_buffer,
            bind_group_layout,
            sampler,
            placeholder_view,
            placeholder_bind_group,
            textures: HashMap::new(),
            compare_textures: HashMap::new(),
            sel_mask_textures: HashMap::new(),
            recent_keys: Vec::new(),
        }
    }

    fn trim(&mut self) {
        if self.textures.len() > 16 {
            let keep: Vec<String> = self.recent_keys.iter().rev().take(4).cloned().collect();
            self.textures.retain(|k, _| keep.contains(k));
        }
        if self.compare_textures.len() > 8 {
            self.compare_textures.clear();
        }
    }
}

impl Primitive for ImageQuadPrimitive {
    type Pipeline = ImageShaderPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        let mut has_image = 0.0f32;
        let mut img_size = [1.0f32, 1.0f32];
        let mut has_compare = 0.0f32;
        let mut has_pair = 0.0f32;
        let mut pair_size = [1.0f32, 1.0f32];

        let secondary_ref = if self.compare_mode > 0 {
            self.compare_frame.as_ref()
        } else {
            self.pair_frame.as_ref()
        };

        let secondary_key_rev = secondary_ref.map(|c| (c.key.clone(), c.revision));
        if let Some(sec) = secondary_ref {
            let needs_sec = pipeline
                .compare_textures
                .get(&sec.key)
                .is_none_or(|t| t.revision != sec.revision);
            if needs_sec {
                let max_dim = device.limits().max_texture_dimension_2d.min(4096);
                let (w, h, bytes) = fit_within_gpu_limit(sec.width, sec.height, &sec.rgba, max_dim);
                let view = upload_rgba_texture(device, queue, w, h, &bytes);
                pipeline.compare_textures.insert(
                    sec.key.clone(),
                    CachedSimpleTexture {
                        revision: sec.revision,
                        width: w,
                        height: h,
                        view,
                    },
                );
            }
            if self.compare_mode > 0 {
                has_compare = 1.0;
            } else if let Some(cached) = pipeline.compare_textures.get(&sec.key) {
                has_pair = 1.0;
                pair_size = [cached.width as f32, cached.height as f32];
            }
        }

        let secondary_view = secondary_ref
            .and_then(|c| pipeline.compare_textures.get(&c.key))
            .map(|t| &t.view)
            .unwrap_or(&pipeline.placeholder_view);

        let sel_key_rev = self
            .selection_mask
            .as_ref()
            .map(|m| (m.key.clone(), m.revision));
        if let Some(sel) = &self.selection_mask {
            let needs_sel = pipeline
                .sel_mask_textures
                .get(&sel.key)
                .is_none_or(|t| t.revision != sel.revision);
            if needs_sel {
                let mut rgba = Vec::with_capacity(sel.alpha.len() * 4);
                for &a in sel.alpha.iter() {
                    rgba.extend_from_slice(&[a, a, a, 255]);
                }
                let max_dim = device.limits().max_texture_dimension_2d.min(4096);
                let (w, h, bytes) = fit_within_gpu_limit(sel.width, sel.height, &rgba, max_dim);
                let view = upload_rgba_texture(device, queue, w, h, &bytes);
                pipeline.sel_mask_textures.insert(
                    sel.key.clone(),
                    CachedSimpleTexture {
                        revision: sel.revision,
                        width: w,
                        height: h,
                        view,
                    },
                );
            }
        }
        let sel_view = self
            .selection_mask
            .as_ref()
            .and_then(|m| pipeline.sel_mask_textures.get(&m.key))
            .map(|t| &t.view)
            .unwrap_or(&pipeline.placeholder_view);

        if let Some(frame) = &self.frame {
            let needs_upload = pipeline
                .textures
                .get(&frame.key)
                .is_none_or(|tex| tex.revision != frame.revision);

            if needs_upload {
                let max_dim = device.limits().max_texture_dimension_2d.min(4096);
                let (w, h, bytes) =
                    fit_within_gpu_limit(frame.width, frame.height, &frame.rgba, max_dim);
                let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("photon_image_uniforms"),
                    size: std::mem::size_of::<Uniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let image_view = upload_rgba_texture(device, queue, w, h, &bytes);
                let bind_group = build_bind_group(
                    device,
                    &pipeline.bind_group_layout,
                    &uniform_buffer,
                    &pipeline.sampler,
                    &image_view,
                    secondary_view,
                    sel_view,
                );
                pipeline.textures.insert(
                    frame.key.clone(),
                    CachedGpuTexture {
                        revision: frame.revision,
                        width: w,
                        height: h,
                        image_view,
                        bound_compare: secondary_key_rev.clone(),
                        bound_sel: sel_key_rev.clone(),
                        uniform_buffer,
                        bind_group,
                    },
                );
            } else if let Some(tex) = pipeline.textures.get_mut(&frame.key)
                && (tex.bound_compare != secondary_key_rev || tex.bound_sel != sel_key_rev)
            {
                tex.bind_group = build_bind_group(
                    device,
                    &pipeline.bind_group_layout,
                    &tex.uniform_buffer,
                    &pipeline.sampler,
                    &tex.image_view,
                    secondary_view,
                    sel_view,
                );
                tex.bound_compare = secondary_key_rev;
                tex.bound_sel = sel_key_rev;
            }

            if let Some(tex) = pipeline.textures.get(&frame.key) {
                has_image = 1.0;
                img_size = [tex.width as f32, tex.height as f32];
                pipeline.recent_keys.retain(|k| k != &frame.key);
                pipeline.recent_keys.push(frame.key.clone());
                if pipeline.recent_keys.len() > 32 {
                    pipeline.recent_keys.remove(0);
                }
            }
        }

        let (has_canvas_bg, canvas_bg) = match self.canvas_bg {
            Some([r, g, b]) => (
                1.0,
                [
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    1.0,
                ],
            ),
            None => (0.0, [0.0; 4]),
        };

        let (has_viewport_bg, viewport_bg) = match self.viewport_bg {
            Some([r, g, b]) => (
                1.0,
                [
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    1.0,
                ],
            ),
            None => (0.0, [0.0; 4]),
        };

        let uniforms = Uniforms {
            viewport_size: self.bounds_size,
            image_size: img_size,
            pan: [self.pan_x, self.pan_y],
            zoom: self.zoom.max(0.05),
            brightness: self.adjustments.brightness,
            contrast: self.adjustments.contrast,
            saturation: self.adjustments.saturation,
            exposure: self.adjustments.exposure,
            highlights: self.adjustments.highlights,
            shadows: self.adjustments.shadows,
            has_image,
            dark_theme: if self.dark_theme { 1.0 } else { 0.0 },
            crop_active: if self.crop_rect.is_some() { 1.0 } else { 0.0 },
            crop_rect: self.crop_rect.unwrap_or([0.0, 0.0, 1.0, 1.0]),
            quarter_turns: f32::from(self.quarter_turns % 4),
            preview_angle_rad: self.preview_angle_rad,
            has_canvas_bg,
            has_compare,
            canvas_bg,
            preview_fill: self.preview_fill,
            compare_mode: f32::from(self.compare_mode),
            compare_split_x: self.compare_split_x.clamp(0.0, 1.0),
            gamma: self.adjustments.gamma.clamp(0.2, 3.0),
            cursor_radius_px: self.cursor_radius_px.max(0.0),
            cursor_doc_pos: self.cursor_doc_pos,
            has_viewport_bg,
            _pad: 0.0,
            viewport_bg,
            pair_size,
            has_pair,
            dual_gap_px: self.dual_gap_px.max(0.0),
            has_selection: if self.has_selection { 1.0 } else { 0.0 },
            quick_mask_active: if self.quick_mask_active { 1.0 } else { 0.0 },
            marching_ants_phase: self.marching_ants_phase,
            sel_preview_kind: self.sel_preview_kind,
            sel_preview_rect: self.sel_preview_rect,
        };

        let target_buf = self
            .frame
            .as_ref()
            .and_then(|f| pipeline.textures.get(&f.key))
            .map(|t| &t.uniform_buffer)
            .unwrap_or(&pipeline.placeholder_uniform_buffer);
        queue.write_buffer(target_buf, 0, uniforms.as_bytes());

        pipeline.overlay_line_count = upload_overlay_vertices(
            device,
            queue,
            &mut pipeline.overlay_line_buffer,
            &mut pipeline.overlay_line_capacity,
            &self.overlay_lines,
            "photon_overlay_line_vb",
        );
        pipeline.overlay_tri_count = upload_overlay_vertices(
            device,
            queue,
            &mut pipeline.overlay_tri_buffer,
            &mut pipeline.overlay_tri_capacity,
            &self.overlay_tris,
            "photon_overlay_tri_vb",
        );
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        let bind_group = self
            .frame
            .as_ref()
            .and_then(|f| pipeline.textures.get(&f.key))
            .map(|t| &t.bind_group)
            .unwrap_or(&pipeline.placeholder_bind_group);

        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.draw(0..6, 0..1);

        if pipeline.overlay_line_count > 0 {
            render_pass.set_pipeline(&pipeline.overlay_line_pipeline);
            render_pass.set_vertex_buffer(0, pipeline.overlay_line_buffer.slice(..));
            render_pass.draw(0..pipeline.overlay_line_count, 0..1);
        }
        if pipeline.overlay_tri_count > 0 {
            render_pass.set_pipeline(&pipeline.overlay_tri_pipeline);
            render_pass.set_vertex_buffer(0, pipeline.overlay_tri_buffer.slice(..));
            render_pass.draw(0..pipeline.overlay_tri_count, 0..1);
        }
        true
    }
}

fn upload_overlay_vertices(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &mut wgpu::Buffer,
    capacity: &mut usize,
    vertices: &[OverlayVertex],
    label: &str,
) -> u32 {
    if vertices.is_empty() {
        return 0;
    }
    if vertices.len() > *capacity {
        let new_cap = vertices.len().next_power_of_two().max(64);
        *buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (new_cap * std::mem::size_of::<OverlayVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        *capacity = new_cap;
    }
    // Safety: OverlayVertex is #[repr(C)] containing only plain f32 values.
    let bytes = unsafe {
        std::slice::from_raw_parts(
            vertices.as_ptr().cast::<u8>(),
            std::mem::size_of_val(vertices),
        )
    };
    queue.write_buffer(buffer, 0, bytes);
    vertices.len() as u32
}

fn fit_within_gpu_limit(w: u32, h: u32, rgba: &[u8], max_dim: u32) -> (u32, u32, Vec<u8>) {
    if w <= max_dim && h <= max_dim {
        return (w, h, rgba.to_vec());
    }
    if let Some(img) = RgbaImage::from_raw(w, h, rgba.to_vec()) {
        let scaled = DynamicImage::ImageRgba8(img)
            .thumbnail(max_dim, max_dim)
            .to_rgba8();
        let (nw, nh) = scaled.dimensions();
        return (nw, nh, scaled.into_raw());
    }
    (w, h, rgba.to_vec())
}

fn upload_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width: width.max(1),
        height: height.max(1),
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("photon_image_texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    if rgba.len() == (size.width as usize) * (size.height as usize) * 4 {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size.width),
                rows_per_image: Some(size.height),
            },
            size,
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn build_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    image_view: &wgpu::TextureView,
    compare_view: &wgpu::TextureView,
    sel_mask_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("photon_image_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(image_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(compare_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(sel_mask_view),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dual_spread_geometry_normalizes_height_and_scales_gap() {
        let (sw, sh, left_w, gap) = dual_spread_geometry((800, 1200), (1200, 1800), 8.0);
        assert_eq!(sh, 1800.0);
        assert_eq!(left_w, 1200.0);
        assert_eq!(gap, 16.0);
        assert_eq!(sw, 1200.0 + 16.0 + 1200.0);

        let (sw0, sh0, lw0, g0) = dual_spread_geometry((800, 900), (800, 900), 0.0);
        assert_eq!((sw0, sh0, lw0, g0), (1600.0, 900.0, 800.0, 0.0));
    }

    #[test]
    fn default_viewport_bg_rgb_matches_palette_canvas() {
        assert_eq!(default_viewport_bg_rgb(true), [16, 18, 20]);
        assert_eq!(default_viewport_bg_rgb(false), [233, 236, 240]);
    }

    #[test]
    fn magnetic_snap_follows_high_contrast_edge_and_snaps_cursor() {
        // 48x48 image with a vertical high-contrast edge at x = 20
        let w = 48u32;
        let h = 48u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let val = if x >= 20 { 255u8 } else { 0u8 };
                let idx = ((y * w + x) * 4) as usize;
                rgba[idx] = val;
                rgba[idx + 1] = val;
                rgba[idx + 2] = val;
                rgba[idx + 3] = 255;
            }
        }
        let frame = CpuImageFrame {
            key: "test_edge".to_string(),
            revision: 1,
            width: w,
            height: h,
            rgba: Arc::new(rgba),
        };
        // Cursor is 3px off the edge at x=23, y=35; anchor is on the edge at (20, 10)
        let path = magnetic_snap(&frame, [20.0, 10.0], [23.0, 35.0]);
        assert!(!path.is_empty());
        let last = *path.last().unwrap();
        // Cursor should snap to the edge (x = 19 or 20)
        assert!(
            (last[0] - 19.5).abs() <= 1.0,
            "expected snapped x near edge 19..=20, got {}",
            last[0]
        );
        // Intermediate points should stay close to the vertical edge
        for pt in &path {
            assert!(
                (pt[0] - 19.5).abs() <= 1.5,
                "expected path point near edge 19..=20, got {:?}",
                pt
            );
        }
    }

    #[test]
    fn magnetic_snap_handles_flat_image_without_panicking() {
        let frame = CpuImageFrame {
            key: "flat".to_string(),
            revision: 1,
            width: 32,
            height: 32,
            rgba: Arc::new(vec![128u8; 32 * 32 * 4]),
        };
        let path = magnetic_snap(&frame, [4.0, 4.0], [20.0, 20.0]);
        assert!(!path.is_empty());
        assert_eq!(*path.last().unwrap(), [20.0, 20.0]);
    }
}
