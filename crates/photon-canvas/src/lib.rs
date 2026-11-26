//! GPU-accelerated WGSL image viewport (`iced::widget::shader`) powered by `wgpu 27`.
//!
//! Supports zoom-at-cursor, pan, checkerboard transparency backdrop, canvas background,
//! real-time 60fps Color & Light adjustments (exposure, highlights, shadows, brightness,
//! contrast, saturation), live arbitrary-angle rotation preview, Before/After Original
//! split/hold comparison, and interactive Paint/Eraser/Eyedropper/Crop/Zoom tools.

mod gpu;
mod transform;
pub mod vector_overlay;

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::keyboard::Modifiers;
use iced::mouse;
use iced::widget::shader::{self, Action};
use iced::{Event, Rectangle};
use photon_protocol::{
    BlendMode, CanvasAntiAliasingMode, ColorAdjustments, DrawShapeKind, GradientKind,
    GradientRepeat, GradientStop, LayerTransformSpec, VectorHandleKind, VectorLayerData,
    VectorNodeMode, VectorPathNode, ViewportDenoiseMode,
};

pub use gpu::{ImageQuadPrimitive, ImageShaderPipeline, MAX_GPU_TEXTURE_DIM};
pub use transform::{
    TransformDragMode, TransformHandle, TransformPreview, apply_transform_drag,
    hit_test_transform_handle, transform_doc_to_local, transform_handle_positions,
    transform_local_to_doc, transform_obb_corners,
};
pub use vector_overlay::{
    VectorDragKind, VectorDragState, VectorHit, apply_live_vector_drag, geometry_to_bezier_nodes,
    hit_test_vector_layer,
};

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
    HealingBrush,
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
    PathSelect,
    Text,
    PaintBucket,
    Gradient,
    Transform,
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

    pub fn is_pan_on_left_drag(self, crop_active: bool) -> bool {
        match self {
            Self::Hand => true,
            Self::Pointer => !crop_active,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientDragMode {
    New,
    MoveStart,
    MoveEnd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GradientPreview {
    pub start: Option<[f32; 2]>,
    pub end: Option<[f32; 2]>,
    pub kind: GradientKind,
    pub repeat: GradientRepeat,
    pub stops: Vec<GradientStop>,
    pub opacity: f32,
    pub reverse: bool,
    pub blend_mode: BlendMode,
    pub interactive: bool,
}

impl Default for GradientPreview {
    fn default() -> Self {
        Self {
            start: None,
            end: None,
            kind: GradientKind::Linear,
            repeat: GradientRepeat::None,
            stops: Vec::new(),
            opacity: 1.0,
            reverse: false,
            blend_mode: BlendMode::Normal,
            interactive: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CanvasInteractionState {
    pub dragging: bool,
    pub moving_layer: bool,
    pub move_start_px: Option<[f32; 2]>,
    pub move_current_px: Option<[f32; 2]>,
    pub space_pan_active: bool,
    pub cropping: bool,
    pub painting: bool,
    pub resizing_brush: bool,
    pub brush_resize_start_x: f32,
    pub brush_resize_start_radius: f32,
    pub brush_resize_anchor_doc_pos: Option<[f32; 2]>,
    pub sampling: bool,
    pub dragging_split: bool,
    pub selecting: bool,
    pub transform_dragging: bool,
    pub transform_drag_mode: Option<TransformDragMode>,
    pub transform_anchor_px: Option<[f32; 2]>,
    pub transform_start_spec: Option<LayerTransformSpec>,
    pub transform_live_spec: Option<LayerTransformSpec>,
    pub gradient_drag_mode: Option<GradientDragMode>,
    pub healing_source_px: Option<[f32; 2]>,
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
    pub vector_drag: Option<VectorDragState>,
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
    HealingSourcePicked([f64; 2]),
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
    GradientDraftUpdated {
        start: [f32; 2],
        end: [f32; 2],
    },
    GradientDraftApply,
    GradientDraftCancel,
    TransformUpdated(LayerTransformSpec),
    TransformCommit,
    TransformCancel,
    MoveLayerCommit {
        dx: f32,
        dy: f32,
    },
    BrushRadiusChanged(f32),
    VectorSelectShape {
        shape_id: u64,
        node_index: Option<usize>,
    },
    VectorMoveNode {
        shape_id: u64,
        node_index: usize,
        anchor: [f32; 2],
    },
    VectorMoveHandle {
        shape_id: u64,
        node_index: usize,
        handle: VectorHandleKind,
        position: [f32; 2],
        mode: VectorNodeMode,
    },
    VectorAddNodeAtPoint {
        shape_id: u64,
        point: [f32; 2],
        max_distance: f32,
    },
    VectorDeleteNode {
        shape_id: u64,
        node_index: usize,
    },
    VectorTranslateShape {
        shape_id: u64,
        dx: f32,
        dy: f32,
    },
    TextCanvasClick {
        x: f32,
        y: f32,
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
    /// Active brush/eraser/liquify/healing radius in document pixels (`0.0` disables cursor ring).
    pub brush_radius_px: f32,
    /// When true (Editor), mouse-active tools reserve Left-drag exclusively; Pointer (no crop), Hand,
    /// Middle-mouse drag, and Space+Left-drag pan freely.
    pub restrict_pan_to_hand: bool,
    /// Stabilizer leash radius in document pixels (`0.0` disables stabilizer overlay).
    pub stabilizer_radius: f32,
    /// Optional 256×1 RGBA8 1D tonal curve LUT (`[R, G, B, 255]` per entry `0..=255`).
    pub curve_lut: Option<[u8; 1024]>,
    pub aa_mode: CanvasAntiAliasingMode,
    pub denoise_mode: ViewportDenoiseMode,
    pub healing_source_px: Option<[f32; 2]>,
    pub transform_preview: Option<TransformPreview>,
    pub gradient_preview: Option<GradientPreview>,
    pub active_vector_layer: Option<VectorLayerData>,
    pub selected_vector_node: Option<usize>,
}

impl Default for ImageCanvasProgram {
    fn default() -> Self {
        Self {
            frame: None,
            pair_frame: None,
            dual_gap_px: 0.0,
            compare_frame: None,
            selection_mask: None,
            has_selection: false,
            quick_mask_active: false,
            marching_ants_phase: 0.0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            adjustments: ColorAdjustments::default(),
            dark_theme: true,
            crop_rect: None,
            crop_aspect: None,
            quarter_turns: 0,
            tool: CanvasTool::Pointer,
            preview_angle_rad: 0.0,
            preview_fill: [0.0; 4],
            canvas_bg: None,
            viewport_bg: None,
            compare_mode: 0,
            compare_split_x: 0.5,
            brush_radius_px: 0.0,
            restrict_pan_to_hand: false,
            stabilizer_radius: 0.0,
            curve_lut: None,
            aa_mode: CanvasAntiAliasingMode::default(),
            denoise_mode: ViewportDenoiseMode::default(),
            healing_source_px: None,
            transform_preview: None,
            gradient_preview: None,
            active_vector_layer: None,
            selected_vector_node: None,
        }
    }
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
        if let Some(frame) = self.frame.as_ref() {
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
        } else if let Some(tp) = &self.transform_preview {
            let f = tp.base_frame.as_ref().unwrap_or(&tp.layer_frame);
            Some((f.width.max(1) as f32, f.height.max(1) as f32))
        } else {
            None
        }
    }

    fn is_quick_color_pick_active(&self, state: &CanvasInteractionState) -> bool {
        (state.modifiers.alt() || state.modifiers.control())
            && !state.resizing_brush
            && !state.painting
            && !state.moving_layer
            && !state.dragging
            && !self.tool.is_selection()
            && !matches!(
                self.tool,
                CanvasTool::Hand
                    | CanvasTool::Zoom
                    | CanvasTool::HealingBrush
                    | CanvasTool::Transform
                    | CanvasTool::PathSelect
            )
    }

    fn effective_transform_preview(
        &self,
        state: &CanvasInteractionState,
    ) -> Option<TransformPreview> {
        let mut tp = self.transform_preview.clone()?;
        if state.transform_dragging
            && let Some(live) = state.transform_live_spec
        {
            tp.spec = live;
        } else if state.moving_layer
            && self.tool == CanvasTool::Pointer
            && let (Some(a), Some(b)) = (state.move_start_px, state.move_current_px)
        {
            tp.spec.translate_x += b[0] - a[0];
            tp.spec.translate_y += b[1] - a[1];
        }
        Some(tp)
    }

    fn effective_gradient_preview(
        &self,
        state: &CanvasInteractionState,
    ) -> Option<GradientPreview> {
        if self.tool != CanvasTool::Gradient {
            return None;
        }
        let mut gp = self.gradient_preview.clone()?;
        if state.drawing_shape
            && let (Some(a), Some(b)) = (state.draw_anchor_px, state.draw_current_px)
        {
            gp.start = Some(a);
            gp.end = Some(b);
        }
        Some(gp)
    }

    fn doc_px_to_screen(&self, doc_px: [f32; 2], bounds: Rectangle) -> Option<[f32; 2]> {
        let (left, top, drawn_w, drawn_h, fw, fh) = self.fit_geometry(bounds)?;
        Some([
            bounds.x + left + (doc_px[0] / fw.max(1.0)) * drawn_w,
            bounds.y + top + (doc_px[1] / fh.max(1.0)) * drawn_h,
        ])
    }

    fn hit_test_gradient_handle(
        &self,
        pos: iced::Point,
        bounds: Rectangle,
    ) -> Option<GradientDragMode> {
        let gp = self.gradient_preview.as_ref().filter(|g| g.interactive)?;
        let (sa, sb) = (gp.start?, gp.end?);
        let threshold_sq = 12.0 * 12.0;
        if let Some([sx, sy]) = self.doc_px_to_screen(sa, bounds) {
            let dx = pos.x - sx;
            let dy = pos.y - sy;
            if dx * dx + dy * dy <= threshold_sq {
                return Some(GradientDragMode::MoveStart);
            }
        }
        if let Some([ex, ey]) = self.doc_px_to_screen(sb, bounds) {
            let dx = pos.x - ex;
            let dy = pos.y - ey;
            if dx * dx + dy * dy <= threshold_sq {
                return Some(GradientDragMode::MoveEnd);
            }
        }
        None
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
        let drawn_w = fit_w * self.zoom.max(0.02);
        let drawn_h = fit_h * self.zoom.max(0.02);
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

    /// Convert viewport screen coordinates into document pixel coordinates without clamping
    /// to `[0..fw, 0..fh]`, so Transform handles/rotations outside canvas bounds work accurately.
    pub fn screen_to_image_px_unclamped(
        &self,
        pos: iced::Point,
        bounds: Rectangle,
    ) -> Option<[f64; 2]> {
        let (left, top, drawn_w, drawn_h, fw, fh) = self.fit_geometry(bounds)?;
        let rel_x = pos.x - bounds.x;
        let rel_y = pos.y - bounds.y;
        let u = (rel_x - left) / drawn_w;
        let v = (rel_y - top) / drawn_h;
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

fn snap_angle_15(anchor: [f32; 2], pt: [f32; 2]) -> [f32; 2] {
    let dx = pt[0] - anchor[0];
    let dy = pt[1] - anchor[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-4 {
        return pt;
    }
    let step = std::f32::consts::FRAC_PI_6 * 0.5; // 15 degrees
    let snapped = (dy.atan2(dx) / step).round() * step;
    [
        anchor[0] + len * snapped.cos(),
        anchor[1] + len * snapped.sin(),
    ]
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
            Event::Window(iced::window::Event::Unfocused) => {
                let had_transient = state.space_pan_active
                    || state.dragging
                    || state.painting
                    || state.resizing_brush
                    || state.selecting
                    || state.cropping
                    || state.transform_dragging
                    || state.sampling
                    || state.dragging_split
                    || state.drawing_shape
                    || state.bezier_dragging_handle;
                state.space_pan_active = false;
                state.dragging = false;
                state.painting = false;
                state.resizing_brush = false;
                state.brush_resize_anchor_doc_pos = None;
                state.selecting = false;
                state.cropping = false;
                state.transform_dragging = false;
                state.transform_drag_mode = None;
                state.transform_anchor_px = None;
                state.transform_start_spec = None;
                state.transform_live_spec = None;
                state.sampling = false;
                state.dragging_split = false;
                state.drawing_shape = false;
                state.gradient_drag_mode = None;
                state.bezier_dragging_handle = false;
                state.last_cursor = None;
                state.stabilizer_pos_px = None;
                if had_transient {
                    return Some(Action::request_redraw());
                }
            }
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(mods)) => {
                if state.modifiers != *mods {
                    state.modifiers = *mods;
                    return Some(Action::request_redraw());
                }
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
                        if self.tool == CanvasTool::Transform
                            && self.transform_preview.is_some() =>
                    {
                        state.transform_dragging = false;
                        state.transform_drag_mode = None;
                        state.transform_anchor_px = None;
                        state.transform_start_spec = None;
                        state.transform_live_spec = None;
                        return Some(Action::publish(CanvasEvent::TransformCommit).and_capture());
                    }
                    Key::Named(Named::Escape)
                        if self.tool == CanvasTool::Transform
                            && self.transform_preview.is_some() =>
                    {
                        state.transform_dragging = false;
                        state.transform_drag_mode = None;
                        state.transform_anchor_px = None;
                        state.transform_start_spec = None;
                        state.transform_live_spec = None;
                        return Some(Action::publish(CanvasEvent::TransformCancel).and_capture());
                    }
                    Key::Named(Named::Enter)
                        if self.tool == CanvasTool::Gradient
                            && self
                                .gradient_preview
                                .as_ref()
                                .is_some_and(|g| g.start.is_some() && g.end.is_some()) =>
                    {
                        state.drawing_shape = false;
                        state.gradient_drag_mode = None;
                        return Some(
                            Action::publish(CanvasEvent::GradientDraftApply).and_capture(),
                        );
                    }
                    Key::Named(Named::Escape)
                        if self.tool == CanvasTool::Gradient
                            && self
                                .gradient_preview
                                .as_ref()
                                .is_some_and(|g| g.start.is_some() && g.end.is_some()) =>
                    {
                        state.drawing_shape = false;
                        state.gradient_drag_mode = None;
                        return Some(
                            Action::publish(CanvasEvent::GradientDraftCancel).and_capture(),
                        );
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
                    Key::Named(Named::Backspace | Named::Delete)
                        if self.tool == CanvasTool::PathSelect =>
                    {
                        if let (Some(vdata), Some(node_idx)) =
                            (&self.active_vector_layer, self.selected_vector_node)
                            && let Some(sid) = vdata
                                .selected_shape_ids
                                .last()
                                .copied()
                                .or_else(|| vdata.shapes.last().map(|s| s.id))
                        {
                            return Some(
                                Action::publish(CanvasEvent::VectorDeleteNode {
                                    shape_id: sid,
                                    node_index: node_idx,
                                })
                                .and_capture(),
                            );
                        }
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
                if state.modifiers.shift()
                    && matches!(self.tool, CanvasTool::Brush | CanvasTool::Eraser)
                {
                    let step = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => *y * 2.0,
                        mouse::ScrollDelta::Pixels { y, .. } => (*y / 12.0).clamp(-4.0, 4.0),
                    };
                    if step.abs() > 0.01 {
                        let next_r = (self.brush_radius_px + step).round().clamp(1.0, 200.0);
                        return Some(
                            Action::publish(CanvasEvent::BrushRadiusChanged(next_r)).and_capture(),
                        );
                    }
                    return None;
                }
                if state.modifiers.shift() {
                    let dx = match delta {
                        mouse::ScrollDelta::Lines { x, y } => {
                            let v = if x.abs() > 0.001 { *x } else { *y };
                            v * 48.0
                        }
                        mouse::ScrollDelta::Pixels { x, y } => {
                            if x.abs() > 0.5 {
                                *x
                            } else {
                                *y
                            }
                        }
                    };
                    if dx.abs() > 0.01 {
                        return Some(
                            Action::publish(CanvasEvent::PanBy { dx, dy: 0.0 }).and_capture(),
                        );
                    }
                    return None;
                }
                match delta {
                    mouse::ScrollDelta::Pixels { x, y } if x.abs() > 0.5 && x.abs() > y.abs() => {
                        return Some(
                            Action::publish(CanvasEvent::PanBy { dx: *x, dy: 0.0 }).and_capture(),
                        );
                    }
                    mouse::ScrollDelta::Lines { x, y } if x.abs() > 0.001 && x.abs() > y.abs() => {
                        return Some(
                            Action::publish(CanvasEvent::PanBy {
                                dx: *x * 48.0,
                                dy: 0.0,
                            })
                            .and_capture(),
                        );
                    }
                    _ => {}
                }
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
                // Middle-mouse drag ALWAYS pans freely regardless of `restrict_pan_to_hand` or active tool.
                state.dragging = true;
                state.last_cursor = cursor.position();
                return Some(Action::capture());
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
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
                // (1) Global Space+Left-drag or Hand tool -> ALWAYS pan freely
                if state.space_pan_active || self.tool == CanvasTool::Hand {
                    state.dragging = true;
                    state.last_cursor = cursor.position();
                    return Some(Action::capture());
                }

                if let Some(pos) = cursor.position()
                    && self.is_over_split_handle(pos, bounds)
                {
                    state.dragging_split = true;
                    return Some(Action::capture());
                }

                // (2) HealingBrush Alt+Click -> pick healing source instead of global SampleColor
                if self.tool == CanvasTool::HealingBrush && state.modifiers.alt() {
                    if let Some(pos) = cursor.position()
                        && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                    {
                        state.healing_source_px = Some([px as f32, py as f32]);
                        return Some(
                            Action::publish(CanvasEvent::HealingSourcePicked([px, py]))
                                .and_capture(),
                        );
                    }
                    return None;
                }

                // (3) Global Alt+Click / Ctrl+Click on non-selection/non-transform/non-healing tools -> SampleColor
                if (state.modifiers.alt() || state.modifiers.control())
                    && !self.tool.is_selection()
                    && !matches!(
                        self.tool,
                        CanvasTool::Hand
                            | CanvasTool::Zoom
                            | CanvasTool::HealingBrush
                            | CanvasTool::Transform
                            | CanvasTool::PathSelect
                    )
                    && let Some(pos) = cursor.position()
                    && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                    && let Some(frame) = &self.frame
                {
                    state.sampling = true;
                    let ix = (px.floor() as u32).min(frame.width.saturating_sub(1));
                    let iy = (py.floor() as u32).min(frame.height.saturating_sub(1));
                    return Some(Action::publish(CanvasEvent::SampleColor([ix, iy])).and_capture());
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
                    CanvasTool::Brush
                    | CanvasTool::Eraser
                    | CanvasTool::Liquify
                    | CanvasTool::HealingBrush => {
                        if state.modifiers.shift()
                            && matches!(self.tool, CanvasTool::Brush | CanvasTool::Eraser)
                            && let Some(pos) = cursor.position()
                        {
                            state.resizing_brush = true;
                            state.brush_resize_start_x = pos.x;
                            state.brush_resize_start_radius = self.brush_radius_px.max(1.0);
                            state.brush_resize_anchor_doc_pos = self
                                .screen_to_image_px(pos, bounds)
                                .map(|[dx, dy]| [dx as f32, dy as f32]);
                            return Some(Action::capture());
                        }
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
                    CanvasTool::Transform => {
                        if let Some(eff_tp) = self.effective_transform_preview(state)
                            && let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px_unclamped(pos, bounds)
                            && let Some((_, _, drawn_w, _, fw, _)) = self.fit_geometry(bounds)
                        {
                            let doc_per_screen = (fw / drawn_w.max(1.0)).max(1e-4);
                            let hit_radius_doc = 11.0 * doc_per_screen;
                            let rot_stem_doc = 24.0 * doc_per_screen;
                            let pt = [px as f32, py as f32];
                            if let Some(mode) =
                                hit_test_transform_handle(&eff_tp, pt, hit_radius_doc, rot_stem_doc)
                            {
                                state.transform_dragging = true;
                                state.transform_drag_mode = Some(mode);
                                state.transform_anchor_px = Some(pt);
                                state.transform_start_spec = Some(eff_tp.spec);
                                state.transform_live_spec = Some(eff_tp.spec);
                                return Some(Action::capture());
                            }
                        }
                        return None;
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
                    CanvasTool::DrawLine | CanvasTool::DrawRect | CanvasTool::DrawEllipse => {
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
                    CanvasTool::Gradient => {
                        if let Some(pos) = cursor.position() {
                            if let Some(mode) = self.hit_test_gradient_handle(pos, bounds)
                                && let Some(gp) = &self.gradient_preview
                                && let (Some(sa), Some(sb)) = (gp.start, gp.end)
                            {
                                state.drawing_shape = true;
                                state.gradient_drag_mode = Some(mode);
                                state.draw_anchor_px = Some(sa);
                                state.draw_current_px = Some(sb);
                                return Some(Action::capture());
                            }
                            if let Some([px, py]) = self.screen_to_image_px_unclamped(pos, bounds) {
                                let pt = [px as f32, py as f32];
                                state.drawing_shape = true;
                                state.gradient_drag_mode = Some(GradientDragMode::New);
                                state.draw_anchor_px = Some(pt);
                                state.draw_current_px = Some(pt);
                                return Some(Action::capture());
                            }
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
                    CanvasTool::PathSelect => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px_unclamped(pos, bounds)
                            && let Some(ref vdata) = self.active_vector_layer
                        {
                            let is_double = check_double_click(state, pos);
                            state.last_click_time = Some(Instant::now());
                            state.last_click_pos = Some(pos);
                            let pt = [px as f32, py as f32];
                            let doc_per_screen = self
                                .fit_geometry(bounds)
                                .map(|(_, _, drawn_w, _, fw, _)| (fw / drawn_w.max(1.0)).max(1e-4))
                                .unwrap_or(1.0);
                            let hit_radius = 9.0 * doc_per_screen;
                            if let Some(hit) = hit_test_vector_layer(vdata, pt, hit_radius) {
                                match hit {
                                    VectorHit::Handle {
                                        shape_id,
                                        node_index,
                                        handle,
                                        mode,
                                    } => {
                                        let eff_mode = if state.modifiers.alt() {
                                            VectorNodeMode::Corner
                                        } else {
                                            mode
                                        };
                                        state.vector_drag = Some(VectorDragState {
                                            kind: VectorDragKind::Handle {
                                                shape_id,
                                                node_index,
                                                handle,
                                                mode: eff_mode,
                                            },
                                            start_px: pt,
                                            current_px: pt,
                                        });
                                        return Some(
                                            Action::publish(CanvasEvent::VectorSelectShape {
                                                shape_id,
                                                node_index: Some(node_index),
                                            })
                                            .and_capture(),
                                        );
                                    }
                                    VectorHit::Anchor {
                                        shape_id,
                                        node_index,
                                    } => {
                                        let kind = if state.modifiers.alt() {
                                            VectorDragKind::Handle {
                                                shape_id,
                                                node_index,
                                                handle: VectorHandleKind::HandleOut,
                                                mode: VectorNodeMode::Symmetric,
                                            }
                                        } else {
                                            VectorDragKind::Anchor {
                                                shape_id,
                                                node_index,
                                            }
                                        };
                                        state.vector_drag = Some(VectorDragState {
                                            kind,
                                            start_px: pt,
                                            current_px: pt,
                                        });
                                        return Some(
                                            Action::publish(CanvasEvent::VectorSelectShape {
                                                shape_id,
                                                node_index: Some(node_index),
                                            })
                                            .and_capture(),
                                        );
                                    }
                                    VectorHit::Segment { shape_id } => {
                                        if is_double
                                            || state.modifiers.control()
                                            || state.modifiers.logo()
                                        {
                                            return Some(
                                                Action::publish(
                                                    CanvasEvent::VectorAddNodeAtPoint {
                                                        shape_id,
                                                        point: pt,
                                                        max_distance: hit_radius * 2.0,
                                                    },
                                                )
                                                .and_capture(),
                                            );
                                        }
                                        state.vector_drag = Some(VectorDragState {
                                            kind: VectorDragKind::TranslateShape { shape_id },
                                            start_px: pt,
                                            current_px: pt,
                                        });
                                        return Some(
                                            Action::publish(CanvasEvent::VectorSelectShape {
                                                shape_id,
                                                node_index: None,
                                            })
                                            .and_capture(),
                                        );
                                    }
                                    VectorHit::ShapeBody { shape_id } => {
                                        state.vector_drag = Some(VectorDragState {
                                            kind: VectorDragKind::TranslateShape { shape_id },
                                            start_px: pt,
                                            current_px: pt,
                                        });
                                        return Some(
                                            Action::publish(CanvasEvent::VectorSelectShape {
                                                shape_id,
                                                node_index: None,
                                            })
                                            .and_capture(),
                                        );
                                    }
                                }
                            }
                        }
                        return None;
                    }
                    CanvasTool::Text => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                        {
                            return Some(
                                Action::publish(CanvasEvent::TextCanvasClick {
                                    x: px as f32,
                                    y: py as f32,
                                })
                                .and_capture(),
                            );
                        }
                        return None;
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
                    CanvasTool::Pointer => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px_unclamped(pos, bounds)
                        {
                            let pt = [px as f32, py as f32];
                            state.moving_layer = true;
                            state.move_start_px = Some(pt);
                            state.move_current_px = Some(pt);
                            return Some(Action::capture());
                        }
                        return None;
                    }
                    CanvasTool::Hand => {
                        state.dragging = true;
                        state.last_cursor = cursor.position();
                        return Some(Action::capture());
                    }
                    CanvasTool::Crop | CanvasTool::RotateAngle => {
                        // Reserve Left-click exclusively for the active tool; never accidentally pan.
                        return None;
                    }
                }
                return None;
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.moving_layer {
                    state.moving_layer = false;
                    let start = state.move_start_px.take();
                    let cur = state.move_current_px.take();
                    if let (Some(a), Some(b)) = (start, cur) {
                        let dx = (b[0] - a[0]).round();
                        let dy = (b[1] - a[1]).round();
                        if dx.abs() >= 1.0 || dy.abs() >= 1.0 {
                            return Some(
                                Action::publish(CanvasEvent::MoveLayerCommit { dx, dy })
                                    .and_capture(),
                            );
                        }
                    }
                    return Some(Action::capture());
                }
                if state.dragging_split {
                    state.dragging_split = false;
                    return Some(Action::capture());
                }
                if state.transform_dragging {
                    state.transform_dragging = false;
                    state.transform_drag_mode = None;
                    state.transform_anchor_px = None;
                    state.transform_start_spec = None;
                    state.transform_live_spec = None;
                    return Some(Action::capture());
                }
                if let Some(drag) = state.vector_drag.take() {
                    let dx = drag.current_px[0] - drag.start_px[0];
                    let dy = drag.current_px[1] - drag.start_px[1];
                    if dx.hypot(dy) > 0.25 {
                        let ev = match drag.kind {
                            VectorDragKind::Anchor {
                                shape_id,
                                node_index,
                            } => CanvasEvent::VectorMoveNode {
                                shape_id,
                                node_index,
                                anchor: drag.current_px,
                            },
                            VectorDragKind::Handle {
                                shape_id,
                                node_index,
                                handle,
                                mode,
                            } => CanvasEvent::VectorMoveHandle {
                                shape_id,
                                node_index,
                                handle,
                                position: drag.current_px,
                                mode,
                            },
                            VectorDragKind::TranslateShape { shape_id } => {
                                CanvasEvent::VectorTranslateShape { shape_id, dx, dy }
                            }
                        };
                        return Some(Action::publish(ev).and_capture());
                    }
                    return Some(Action::request_redraw());
                }
                if state.resizing_brush {
                    state.resizing_brush = false;
                    state.brush_resize_anchor_doc_pos = None;
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
                    let grad_mode = state.gradient_drag_mode.take();
                    let anchor = state.draw_anchor_px.take();
                    let cur = state.draw_current_px.take();
                    if let (Some(a), Some(b)) = (anchor, cur) {
                        let ev = match self.tool {
                            CanvasTool::Gradient => {
                                let interactive = self
                                    .gradient_preview
                                    .as_ref()
                                    .is_some_and(|g| g.interactive);
                                if interactive {
                                    let len_sq = (b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2);
                                    if len_sq < 1.0 {
                                        if grad_mode == Some(GradientDragMode::New) {
                                            CanvasEvent::GradientDraftCancel
                                        } else {
                                            return Some(Action::request_redraw());
                                        }
                                    } else {
                                        CanvasEvent::GradientDraftUpdated { start: a, end: b }
                                    }
                                } else {
                                    CanvasEvent::DrawGradientCommit { start: a, end: b }
                                }
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
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.vector_drag.is_some() => {
                if let Some([px, py]) = self.screen_to_image_px_unclamped(*position, bounds)
                    && let Some(ref mut drag) = state.vector_drag
                {
                    drag.current_px = [px as f32, py as f32];
                    return Some(Action::request_redraw());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.transform_dragging => {
                if let (Some(tp), Some(mode), Some(anchor), Some(start_spec), Some([px, py])) = (
                    &self.transform_preview,
                    state.transform_drag_mode,
                    state.transform_anchor_px,
                    state.transform_start_spec,
                    self.screen_to_image_px_unclamped(*position, bounds),
                ) {
                    let spec = apply_transform_drag(
                        tp,
                        start_spec,
                        mode,
                        anchor,
                        [px as f32, py as f32],
                        state.modifiers.shift(),
                        state.modifiers.alt(),
                    );
                    state.transform_live_spec = Some(spec);
                    return Some(
                        Action::publish(CanvasEvent::TransformUpdated(spec)).and_capture(),
                    );
                }
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
                if self.tool == CanvasTool::Gradient {
                    if let Some([px, py]) = self.screen_to_image_px_unclamped(*position, bounds) {
                        let raw_pt = [px as f32, py as f32];
                        let snap = state.modifiers.shift();
                        match state.gradient_drag_mode {
                            Some(GradientDragMode::MoveStart) => {
                                let fixed = state.draw_current_px.unwrap_or(raw_pt);
                                let next = if snap {
                                    snap_angle_15(fixed, raw_pt)
                                } else {
                                    raw_pt
                                };
                                state.draw_anchor_px = Some(next);
                            }
                            _ => {
                                let fixed = state.draw_anchor_px.unwrap_or(raw_pt);
                                let next = if snap {
                                    snap_angle_15(fixed, raw_pt)
                                } else {
                                    raw_pt
                                };
                                state.draw_current_px = Some(next);
                            }
                        }
                        return Some(Action::request_redraw());
                    }
                } else if let Some([px, py]) = self.screen_to_image_px(*position, bounds) {
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
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.moving_layer => {
                if let Some([px, py]) = self.screen_to_image_px_unclamped(*position, bounds) {
                    let mut pt = [px as f32, py as f32];
                    if state.modifiers.shift()
                        && let Some(start) = state.move_start_px
                    {
                        let dx = (pt[0] - start[0]).abs();
                        let dy = (pt[1] - start[1]).abs();
                        if dx >= dy {
                            pt[1] = start[1];
                        } else {
                            pt[0] = start[0];
                        }
                    }
                    state.move_current_px = Some(pt);
                    return Some(Action::request_redraw());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.resizing_brush => {
                let dx = position.x - state.brush_resize_start_x;
                let next_r = (state.brush_resize_start_radius + dx * 0.25)
                    .round()
                    .clamp(1.0, 200.0);
                return Some(
                    Action::publish(CanvasEvent::BrushRadiusChanged(next_r)).and_capture(),
                );
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
                            | CanvasTool::HealingBrush
                            | CanvasTool::QuickSelect
                            | CanvasTool::Transform
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
        let quick_pick = self.is_quick_color_pick_active(state) || state.sampling;
        let show_brush_ring = self.brush_radius_px > 0.0
            && !quick_pick
            && matches!(
                self.tool,
                CanvasTool::Brush
                    | CanvasTool::Eraser
                    | CanvasTool::Liquify
                    | CanvasTool::HealingBrush
                    | CanvasTool::QuickSelect
            )
            && (state.resizing_brush || cursor.is_over(bounds));
        let cursor_doc_pos = if state.resizing_brush
            && let Some(anchor) = state.brush_resize_anchor_doc_pos
        {
            anchor
        } else if show_brush_ring
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
            && let (Some(a), Some(b), Some((doc_w, doc_h))) = (
                state.select_anchor_px,
                state.select_current_px,
                self.effective_doc_size(),
            ) {
            let fw = doc_w.max(1.0);
            let fh = doc_h.max(1.0);
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
        let transform_preview = if self.tool == CanvasTool::Transform
            || (self.tool == CanvasTool::Pointer && state.moving_layer)
        {
            self.effective_transform_preview(state)
        } else {
            None
        };
        let gradient_preview = self.effective_gradient_preview(state);

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
            curve_lut: self.curve_lut,
            aa_mode: self.aa_mode,
            denoise_mode: self.denoise_mode,
            transform_preview,
            gradient_preview,
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
        } else if state.dragging || state.moving_layer {
            mouse::Interaction::Grabbing
        } else if !cursor.is_over(bounds) {
            mouse::Interaction::default()
        } else if state.resizing_brush {
            mouse::Interaction::ResizingHorizontally
        } else if state.sampling || self.is_quick_color_pick_active(state) {
            mouse::Interaction::Crosshair
        } else if state.space_pan_active || self.tool == CanvasTool::Pointer {
            mouse::Interaction::Grab
        } else if self.tool == CanvasTool::Transform {
            if state.transform_dragging {
                return match state.transform_drag_mode {
                    Some(TransformDragMode::Scale(h)) => match h {
                        TransformHandle::TopLeft | TransformHandle::BottomRight => {
                            mouse::Interaction::ResizingDiagonallyDown
                        }
                        TransformHandle::TopRight | TransformHandle::BottomLeft => {
                            mouse::Interaction::ResizingDiagonallyUp
                        }
                        TransformHandle::Top | TransformHandle::Bottom => {
                            mouse::Interaction::ResizingVertically
                        }
                        TransformHandle::Left | TransformHandle::Right => {
                            mouse::Interaction::ResizingHorizontally
                        }
                        _ => mouse::Interaction::Grabbing,
                    },
                    _ => mouse::Interaction::Grabbing,
                };
            }
            if let Some(eff_tp) = self.effective_transform_preview(state)
                && let Some(pos) = cursor.position()
                && let Some([px, py]) = self.screen_to_image_px_unclamped(pos, bounds)
                && let Some((_, _, drawn_w, _, fw, _)) = self.fit_geometry(bounds)
            {
                let doc_per_screen = (fw / drawn_w.max(1.0)).max(1e-4);
                let hit_radius_doc = 11.0 * doc_per_screen;
                let rot_stem_doc = 24.0 * doc_per_screen;
                if let Some(mode) = hit_test_transform_handle(
                    &eff_tp,
                    [px as f32, py as f32],
                    hit_radius_doc,
                    rot_stem_doc,
                ) {
                    return match mode {
                        TransformDragMode::Rotate => mouse::Interaction::Grab,
                        TransformDragMode::Move => mouse::Interaction::Grab,
                        TransformDragMode::Scale(h) => match h {
                            TransformHandle::TopLeft | TransformHandle::BottomRight => {
                                mouse::Interaction::ResizingDiagonallyDown
                            }
                            TransformHandle::TopRight | TransformHandle::BottomLeft => {
                                mouse::Interaction::ResizingDiagonallyUp
                            }
                            TransformHandle::Top | TransformHandle::Bottom => {
                                mouse::Interaction::ResizingVertically
                            }
                            TransformHandle::Left | TransformHandle::Right => {
                                mouse::Interaction::ResizingHorizontally
                            }
                            _ => mouse::Interaction::Grab,
                        },
                    };
                }
            }
            mouse::Interaction::default()
        } else if self.crop_rect.is_some()
            || self.tool.is_selection()
            || matches!(
                self.tool,
                CanvasTool::Brush
                    | CanvasTool::Eraser
                    | CanvasTool::Liquify
                    | CanvasTool::HealingBrush
                    | CanvasTool::Eyedropper
                    | CanvasTool::Crop
                    | CanvasTool::DrawLine
                    | CanvasTool::DrawRect
                    | CanvasTool::DrawEllipse
                    | CanvasTool::DrawPolygon
                    | CanvasTool::DrawPolyline
                    | CanvasTool::BezierPen
                    | CanvasTool::PathSelect
                    | CanvasTool::Text
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
    pub aa_uv: [f32; 2],
    pub aa_kind: f32,
    pub _pad: f32,
}

impl OverlayVertex {
    const fn flat(pos: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            pos,
            color,
            aa_uv: [0.0, 0.0],
            aa_kind: 0.0,
            _pad: 0.0,
        }
    }

    const fn aa(pos: [f32; 2], color: [f32; 4], aa_uv: [f32; 2], aa_kind: f32) -> Self {
        Self {
            pos,
            color,
            aa_uv,
            aa_kind,
            _pad: 0.0,
        }
    }
}

impl ImageCanvasProgram {
    #[allow(clippy::too_many_arguments)]
    fn push_overlay_line_ndc(
        &self,
        lines: &mut Vec<OverlayVertex>,
        tris: &mut Vec<OverlayVertex>,
        a: [f32; 2],
        b: [f32; 2],
        color: [f32; 4],
        half_width_px: f32,
        bounds: Rectangle,
    ) {
        if self.aa_mode == CanvasAntiAliasingMode::HighQuality {
            let vp_w = bounds.width.max(1.0);
            let vp_h = bounds.height.max(1.0);
            let ax = (a[0] + 1.0) * 0.5 * vp_w;
            let ay = (1.0 - a[1]) * 0.5 * vp_h;
            let bx = (b[0] + 1.0) * 0.5 * vp_w;
            let by = (1.0 - b[1]) * 0.5 * vp_h;
            let dx = bx - ax;
            let dy = by - ay;
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.25 {
                let nx = (-dy / len) * half_width_px;
                let ny = (dx / len) * half_width_px;
                let to_ndc = |x: f32, y: f32| [(x / vp_w) * 2.0 - 1.0, 1.0 - (y / vp_h) * 2.0];
                let p0 = to_ndc(ax + nx, ay + ny);
                let p1 = to_ndc(ax - nx, ay - ny);
                let p2 = to_ndc(bx - nx, by - ny);
                let p3 = to_ndc(bx + nx, by + ny);
                tris.extend_from_slice(&[
                    OverlayVertex::aa(p0, color, [0.0, 1.0], 1.0),
                    OverlayVertex::aa(p1, color, [0.0, -1.0], 1.0),
                    OverlayVertex::aa(p2, color, [0.0, -1.0], 1.0),
                    OverlayVertex::aa(p0, color, [0.0, 1.0], 1.0),
                    OverlayVertex::aa(p2, color, [0.0, -1.0], 1.0),
                    OverlayVertex::aa(p3, color, [0.0, 1.0], 1.0),
                ]);
                return;
            }
        }
        lines.push(OverlayVertex::flat(a, color));
        lines.push(OverlayVertex::flat(b, color));
    }

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
            let aa_kind = if self.aa_mode == CanvasAntiAliasingMode::HighQuality {
                2.0
            } else {
                0.0
            };
            tris.extend_from_slice(&[
                OverlayVertex::aa(tl, color, [-1.0, 1.0], aa_kind),
                OverlayVertex::aa(bl, color, [-1.0, -1.0], aa_kind),
                OverlayVertex::aa(br, color, [1.0, -1.0], aa_kind),
                OverlayVertex::aa(tl, color, [-1.0, 1.0], aa_kind),
                OverlayVertex::aa(br, color, [1.0, -1.0], aa_kind),
                OverlayVertex::aa(tr, color, [1.0, 1.0], aa_kind),
            ]);
        }
    }

    fn push_overlay_square(
        &self,
        tris: &mut Vec<OverlayVertex>,
        doc_px: [f32; 2],
        half_px: f32,
        color: [f32; 4],
        bounds: Rectangle,
    ) {
        if let Some([cx, cy]) = self.doc_px_to_ndc(doc_px, bounds) {
            let [half_w, half_h_neg] = self.screen_delta_to_ndc(half_px, half_px, bounds);
            let half_h = half_h_neg.abs();
            let tl = [cx - half_w, cy + half_h];
            let tr = [cx + half_w, cy + half_h];
            let bl = [cx - half_w, cy - half_h];
            let br = [cx + half_w, cy - half_h];
            tris.extend_from_slice(&[
                OverlayVertex::flat(tl, color),
                OverlayVertex::flat(bl, color),
                OverlayVertex::flat(br, color),
                OverlayVertex::flat(tl, color),
                OverlayVertex::flat(br, color),
                OverlayVertex::flat(tr, color),
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
            lines.push(OverlayVertex::flat(
                [
                    center_ndc[0] + rx_ndc * a0.cos(),
                    center_ndc[1] + ry_ndc * a0.sin(),
                ],
                color,
            ));
            lines.push(OverlayVertex::flat(
                [
                    center_ndc[0] + rx_ndc * a1.cos(),
                    center_ndc[1] + ry_ndc * a1.sin(),
                ],
                color,
            ));
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

        // Quick color-pick / Eyedropper pipette reticle anchored at the cursor hotspot
        if (self.is_quick_color_pick_active(state) || state.sampling)
            && cursor.is_over(bounds)
            && let Some(pos) = cursor.position()
            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
            && let Some(tip) = self.doc_px_to_ndc([px as f32, py as f32], bounds)
        {
            let d = |sx: f32, sy: f32| -> [f32; 2] {
                let [dx, dy_neg] = self.screen_delta_to_ndc(sx, sy, bounds);
                [tip[0] + dx, tip[1] - dy_neg]
            };
            let dark = [0.05, 0.06, 0.09, 0.94];
            let bright = [1.0, 0.96, 0.45, 0.98];
            let segs = [
                (d(2.0, -2.0), d(11.0, -11.0)),
                (d(11.0, -11.0), d(15.0, -15.0)),
                (d(7.5, -12.5), d(12.5, -7.5)),
                (d(13.0, -17.0), d(17.0, -13.0)),
            ];
            for (a, b) in segs {
                self.push_overlay_line_ndc(&mut lines, &mut tris, a, b, dark, 2.8, bounds);
            }
            for (a, b) in segs {
                self.push_overlay_line_ndc(&mut lines, &mut tris, a, b, bright, 1.4, bounds);
            }
        }

        // Healing Brush source `+` crosshair
        if self.tool == CanvasTool::HealingBrush
            && let Some(src) = self.healing_source_px.or(state.healing_source_px)
            && let Some(c_ndc) = self.doc_px_to_ndc(src, bounds)
        {
            let [dx, dy_neg] = self.screen_delta_to_ndc(9.0, 9.0, bounds);
            let dy = dy_neg.abs();
            let [rx, ry_neg] = self.screen_delta_to_ndc(4.5, 4.5, bounds);
            let dark = [0.05, 0.06, 0.08, 0.92];
            let bright = [0.35, 0.95, 1.0, 0.98];
            self.push_overlay_line_ndc(
                &mut lines,
                &mut tris,
                [c_ndc[0] - dx, c_ndc[1]],
                [c_ndc[0] + dx, c_ndc[1]],
                dark,
                2.0,
                bounds,
            );
            self.push_overlay_line_ndc(
                &mut lines,
                &mut tris,
                [c_ndc[0], c_ndc[1] - dy],
                [c_ndc[0], c_ndc[1] + dy],
                dark,
                2.0,
                bounds,
            );
            self.push_overlay_line_ndc(
                &mut lines,
                &mut tris,
                [c_ndc[0] - dx, c_ndc[1]],
                [c_ndc[0] + dx, c_ndc[1]],
                bright,
                1.1,
                bounds,
            );
            self.push_overlay_line_ndc(
                &mut lines,
                &mut tris,
                [c_ndc[0], c_ndc[1] - dy],
                [c_ndc[0], c_ndc[1] + dy],
                bright,
                1.1,
                bounds,
            );
            self.push_overlay_circle(&mut lines, c_ndc, rx, ry_neg.abs(), bright, 16);
        }

        if self.tool == CanvasTool::Pointer
            && state.moving_layer
            && let Some(eff_tp) = self.effective_transform_preview(state)
        {
            let corners = transform_obb_corners(&eff_tp);
            let dark = [0.05, 0.06, 0.09, 0.85];
            let frame_col = [0.35, 0.88, 1.0, 0.9];
            for i in 0..4 {
                if let (Some(na), Some(nb)) = (
                    self.doc_px_to_ndc(corners[i], bounds),
                    self.doc_px_to_ndc(corners[(i + 1) % 4], bounds),
                ) {
                    self.push_overlay_line_ndc(&mut lines, &mut tris, na, nb, dark, 2.0, bounds);
                    self.push_overlay_line_ndc(
                        &mut lines, &mut tris, na, nb, frame_col, 1.0, bounds,
                    );
                }
            }
        }

        // Interactive Free Transform OBB, 8 resize handles, external rotation handle, and pivot crosshair
        if self.tool == CanvasTool::Transform
            && let Some(eff_tp) = self.effective_transform_preview(state)
            && let Some((_, _, drawn_w, _, fw, _)) = self.fit_geometry(bounds)
        {
            let doc_per_screen = (fw / drawn_w.max(1.0)).max(1e-4);
            let rot_stem_doc = 24.0 * doc_per_screen;
            let corners = transform_obb_corners(&eff_tp);
            let handles = transform_handle_positions(&eff_tp, rot_stem_doc);
            let dark = [0.05, 0.06, 0.09, 0.92];
            let frame_col = [0.35, 0.88, 1.0, 0.98];

            for i in 0..4 {
                if let (Some(na), Some(nb)) = (
                    self.doc_px_to_ndc(corners[i], bounds),
                    self.doc_px_to_ndc(corners[(i + 1) % 4], bounds),
                ) {
                    self.push_overlay_line_ndc(&mut lines, &mut tris, na, nb, dark, 2.0, bounds);
                    self.push_overlay_line_ndc(
                        &mut lines, &mut tris, na, nb, frame_col, 1.15, bounds,
                    );
                }
            }

            // Stem from Top midpoint to external Rotate handle
            if let (Some(n_top), Some(n_rot)) = (
                self.doc_px_to_ndc(handles[1].1, bounds),
                self.doc_px_to_ndc(handles[8].1, bounds),
            ) {
                self.push_overlay_line_ndc(&mut lines, &mut tris, n_top, n_rot, dark, 2.0, bounds);
                self.push_overlay_line_ndc(
                    &mut lines, &mut tris, n_top, n_rot, frame_col, 1.15, bounds,
                );
            }

            // 8 resize handles (squares with dark border + white fill)
            for (_, pos) in &handles[..8] {
                self.push_overlay_square(&mut tris, *pos, 4.5, dark, bounds);
                self.push_overlay_square(&mut tris, *pos, 3.2, [1.0, 1.0, 1.0, 0.98], bounds);
            }

            // External rotation handle
            self.push_overlay_dot(&mut tris, handles[8].1, 5.2, dark, bounds);
            self.push_overlay_dot(&mut tris, handles[8].1, 3.8, frame_col, bounds);

            // Pivot crosshair
            if let Some(p_ndc) = self.doc_px_to_ndc(handles[9].1, bounds) {
                let [dx, dy_neg] = self.screen_delta_to_ndc(7.0, 7.0, bounds);
                let dy = dy_neg.abs();
                let [rx, ry_neg] = self.screen_delta_to_ndc(4.0, 4.0, bounds);
                let piv_col = [1.0, 0.82, 0.25, 0.95];
                self.push_overlay_line_ndc(
                    &mut lines,
                    &mut tris,
                    [p_ndc[0] - dx, p_ndc[1]],
                    [p_ndc[0] + dx, p_ndc[1]],
                    piv_col,
                    1.2,
                    bounds,
                );
                self.push_overlay_line_ndc(
                    &mut lines,
                    &mut tris,
                    [p_ndc[0], p_ndc[1] - dy],
                    [p_ndc[0], p_ndc[1] + dy],
                    piv_col,
                    1.2,
                    bounds,
                );
                self.push_overlay_circle(&mut lines, p_ndc, rx, ry_neg.abs(), piv_col, 16);
            }
        }

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
                self.push_overlay_line_ndc(
                    &mut lines,
                    &mut tris,
                    a,
                    b,
                    [1.0, 0.8, 0.25, 0.9],
                    1.2,
                    bounds,
                );
                self.push_overlay_dot(&mut tris, stab, 3.0, [1.0, 0.8, 0.25, 0.95], bounds);
            }
        }

        // Shape & Gradient & BezierPen overlays
        let line_color = [0.95, 0.98, 1.0, 0.92];
        let rubber_color = [0.35, 0.90, 1.0, 0.95];

        if self.tool == CanvasTool::Gradient
            && let Some(gp) = self.effective_gradient_preview(state)
            && let (Some(a), Some(b)) = (gp.start, gp.end)
            && let (Some(na), Some(nb)) =
                (self.doc_px_to_ndc(a, bounds), self.doc_px_to_ndc(b, bounds))
        {
            let dark = [0.05, 0.06, 0.08, 0.9];
            self.push_overlay_line_ndc(&mut lines, &mut tris, na, nb, dark, 2.0, bounds);
            self.push_overlay_line_ndc(&mut lines, &mut tris, na, nb, rubber_color, 1.2, bounds);
            self.push_overlay_dot(&mut tris, a, 5.5, dark, bounds);
            self.push_overlay_dot(&mut tris, a, 4.0, [0.2, 0.85, 1.0, 1.0], bounds);
            self.push_overlay_dot(&mut tris, b, 5.5, dark, bounds);
            self.push_overlay_dot(&mut tris, b, 4.0, [1.0, 1.0, 1.0, 0.98], bounds);
        }

        if state.drawing_shape {
            match self.tool {
                CanvasTool::Gradient => {}
                CanvasTool::DrawLine => {
                    if let (Some(a), Some(b)) = (state.draw_anchor_px, state.draw_current_px)
                        && let (Some(na), Some(nb)) =
                            (self.doc_px_to_ndc(a, bounds), self.doc_px_to_ndc(b, bounds))
                    {
                        self.push_overlay_line_ndc(
                            &mut lines,
                            &mut tris,
                            na,
                            nb,
                            rubber_color,
                            1.2,
                            bounds,
                        );
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
                                self.push_overlay_line_ndc(
                                    &mut lines,
                                    &mut tris,
                                    na,
                                    nb,
                                    rubber_color,
                                    1.2,
                                    bounds,
                                );
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
                                self.push_overlay_line_ndc(
                                    &mut lines,
                                    &mut tris,
                                    na,
                                    nb,
                                    rubber_color,
                                    1.2,
                                    bounds,
                                );
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
                            self.push_overlay_line_ndc(
                                &mut lines, &mut tris, na, nb, line_color, 1.2, bounds,
                            );
                        }
                    }
                    if let (Some(&last), Some(cur)) =
                        (state.draw_points.last(), state.draw_current_px)
                        && let (Some(na), Some(nb)) = (
                            self.doc_px_to_ndc(last, bounds),
                            self.doc_px_to_ndc(cur, bounds),
                        )
                    {
                        self.push_overlay_line_ndc(
                            &mut lines,
                            &mut tris,
                            na,
                            nb,
                            rubber_color,
                            1.2,
                            bounds,
                        );
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
                                self.push_overlay_line_ndc(
                                    &mut lines, &mut tris, na, nb, line_color, 1.2, bounds,
                                );
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
                                self.push_overlay_line_ndc(
                                    &mut lines,
                                    &mut tris,
                                    na,
                                    nb,
                                    rubber_color,
                                    1.2,
                                    bounds,
                                );
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
                                self.push_overlay_line_ndc(
                                    &mut lines,
                                    &mut tris,
                                    na,
                                    nb,
                                    handle_color,
                                    1.0,
                                    bounds,
                                );
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

        // Active Vector Layer 1px path outlines, anchor squares, and Bezier handle tangents
        self.append_active_vector_layer_overlay(state, &mut lines, &mut tris, bounds);

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
                self.push_overlay_line_ndc(&mut lines, &mut tris, a, b, line_color, 1.2, bounds);
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
                self.push_overlay_line_ndc(&mut lines, &mut tris, a, b, rubber_color, 1.2, bounds);
            }
            for pair in state.magnetic_preview_px.windows(2) {
                if let (Some(a), Some(b)) = (
                    self.doc_px_to_ndc(pair[0], bounds),
                    self.doc_px_to_ndc(pair[1], bounds),
                ) {
                    self.push_overlay_line_ndc(
                        &mut lines,
                        &mut tris,
                        a,
                        b,
                        rubber_color,
                        1.2,
                        bounds,
                    );
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
            self.push_overlay_line_ndc(&mut lines, &mut tris, a, b, rubber_color, 1.2, bounds);
        }

        // Vertex anchor dots and close-snap ring for polygon-style tools
        if matches!(
            self.tool,
            CanvasTool::PolygonLasso | CanvasTool::MagneticLasso | CanvasTool::BezierSelect
        ) {
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
                let dot_color = if i == 0 {
                    [0.20, 0.80, 1.0, 1.0]
                } else {
                    [1.0, 1.0, 1.0, 0.95]
                };
                self.push_overlay_square(&mut tris, pt, 3.5, dot_color, bounds);
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
                if let Some(c_ndc) = self.doc_px_to_ndc(first, bounds) {
                    let [rx, ry_neg] = self.screen_delta_to_ndc(9.0, 9.0, bounds);
                    let ring_color = if is_snapped {
                        [0.20, 1.0, 0.55, 1.0]
                    } else {
                        [1.0, 1.0, 1.0, 0.65]
                    };
                    self.push_overlay_circle(&mut lines, c_ndc, rx, ry_neg.abs(), ring_color, 20);
                }
            }
        }

        (lines, tris)
    }
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

    fn dummy_frame(w: u32, h: u32) -> CpuImageFrame {
        CpuImageFrame {
            key: format!("dummy_{w}x{h}"),
            revision: 1,
            width: w,
            height: h,
            rgba: Arc::new(Vec::new()),
        }
    }

    #[test]
    fn uniforms_16_byte_alignment_and_size() {
        use std::mem::{offset_of, size_of};
        assert_eq!(size_of::<gpu::Uniforms>(), 592);
        assert_eq!(size_of::<gpu::Uniforms>() % 16, 0);
        // vec4<f32> fields must be 16-byte aligned
        assert_eq!(offset_of!(gpu::Uniforms, crop_rect) % 16, 0);
        assert_eq!(offset_of!(gpu::Uniforms, canvas_bg) % 16, 0);
        assert_eq!(offset_of!(gpu::Uniforms, preview_fill) % 16, 0);
        assert_eq!(offset_of!(gpu::Uniforms, viewport_bg) % 16, 0);
        assert_eq!(offset_of!(gpu::Uniforms, sel_preview_rect) % 16, 0);
        assert_eq!(offset_of!(gpu::Uniforms, gradient_line) % 16, 0);
        assert_eq!(offset_of!(gpu::Uniforms, gradient_params) % 16, 0);
        assert_eq!(offset_of!(gpu::Uniforms, gradient_meta) % 16, 0);
        assert_eq!(offset_of!(gpu::Uniforms, gradient_stop_colors) % 16, 0);
        assert_eq!(offset_of!(gpu::Uniforms, gradient_stop_pos) % 16, 0);
        // vec2<f32> fields must be 8-byte aligned
        assert_eq!(offset_of!(gpu::Uniforms, viewport_size) % 8, 0);
        assert_eq!(offset_of!(gpu::Uniforms, image_size) % 8, 0);
        assert_eq!(offset_of!(gpu::Uniforms, pan) % 8, 0);
        assert_eq!(offset_of!(gpu::Uniforms, cursor_doc_pos) % 8, 0);
        assert_eq!(offset_of!(gpu::Uniforms, pair_size) % 8, 0);
        assert_eq!(offset_of!(gpu::Uniforms, transform_layer_offset) % 8, 0);
        assert_eq!(offset_of!(gpu::Uniforms, transform_layer_size) % 8, 0);
        assert_eq!(offset_of!(gpu::Uniforms, transform_translate) % 8, 0);
        assert_eq!(offset_of!(gpu::Uniforms, transform_scale) % 8, 0);
        assert_eq!(offset_of!(gpu::Uniforms, transform_pivot) % 8, 0);
        assert_eq!(offset_of!(gpu::Uniforms, transform_cos_sin) % 8, 0);
    }

    #[test]
    fn doc_px_to_ndc_maps_oversized_8000x6000_image_accurately() {
        let prog = ImageCanvasProgram {
            frame: Some(dummy_frame(8000, 6000)),
            zoom: 2.0,
            pan_x: 40.0,
            pan_y: -30.0,
            ..Default::default()
        };
        let bounds = Rectangle::new(iced::Point::new(0.0, 0.0), iced::Size::new(800.0, 600.0));
        // Center of 8000x6000 is (4000, 3000) -> screen (400+40, 300-30) = (440, 270)
        let center_ndc = prog.doc_px_to_ndc([4000.0, 3000.0], bounds).unwrap();
        assert!((center_ndc[0] - (440.0 / 800.0 * 2.0 - 1.0)).abs() < 1e-4);
        assert!((center_ndc[1] - (1.0 - 270.0 / 600.0 * 2.0)).abs() < 1e-4);

        // Round-trip screen -> doc_px -> ndc at an off-center coordinate on 8000x6000
        let screen_pt = iced::Point::new(520.0, 190.0);
        let doc_px = prog.screen_to_image_px(screen_pt, bounds).unwrap();
        let ndc = prog
            .doc_px_to_ndc([doc_px[0] as f32, doc_px[1] as f32], bounds)
            .unwrap();
        assert!((ndc[0] - (520.0 / 800.0 * 2.0 - 1.0)).abs() < 1e-3);
        assert!((ndc[1] - (1.0 - 190.0 / 600.0 * 2.0)).abs() < 1e-3);
    }

    #[test]
    fn middle_mouse_and_space_drag_pan_with_all_tools() {
        let bounds = Rectangle::new(iced::Point::new(0.0, 0.0), iced::Size::new(800.0, 600.0));
        let cursor_a = mouse::Cursor::Available(iced::Point::new(200.0, 200.0));
        let cursor_b = mouse::Cursor::Available(iced::Point::new(235.0, 220.0));

        for tool in [
            CanvasTool::Pointer,
            CanvasTool::Hand,
            CanvasTool::Brush,
            CanvasTool::HealingBrush,
            CanvasTool::Crop,
            CanvasTool::RotateAngle,
            CanvasTool::Gradient,
            CanvasTool::Transform,
        ] {
            let prog = ImageCanvasProgram {
                frame: Some(dummy_frame(1000, 800)),
                tool,
                restrict_pan_to_hand: true,
                ..Default::default()
            };
            let mut state = CanvasInteractionState::default();

            // Middle-mouse drag ALWAYS pans
            let act_press = shader::Program::update(
                &prog,
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)),
                bounds,
                cursor_a,
            );
            assert!(act_press.is_some());
            assert!(state.dragging, "middle click should drag for {tool:?}");

            let act_move = shader::Program::update(
                &prog,
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved {
                    position: iced::Point::new(235.0, 220.0),
                }),
                bounds,
                cursor_b,
            );
            assert!(matches!(
                act_move.and_then(|a| a.into_inner().0),
                Some(CanvasEvent::PanBy { dx, dy }) if (dx - 35.0).abs() < 1e-4 && (dy - 20.0).abs() < 1e-4
            ));

            shader::Program::update(
                &prog,
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)),
                bounds,
                cursor_b,
            );
            assert!(!state.dragging);

            // Space + Left-drag ALWAYS pans
            shader::Program::update(
                &prog,
                &mut state,
                &Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Space),
                    modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Space),
                    physical_key: iced::keyboard::key::Physical::Code(
                        iced::keyboard::key::Code::Space,
                    ),
                    location: iced::keyboard::Location::Standard,
                    modifiers: Modifiers::empty(),
                    text: None,
                    repeat: false,
                }),
                bounds,
                cursor_a,
            );
            assert!(state.space_pan_active);

            shader::Program::update(
                &prog,
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor_a,
            );
            assert!(state.dragging, "space+left should drag for {tool:?}");

            let act_space_move = shader::Program::update(
                &prog,
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved {
                    position: iced::Point::new(210.0, 190.0),
                }),
                bounds,
                cursor_b,
            );
            assert!(matches!(
                act_space_move.and_then(|a| a.into_inner().0),
                Some(CanvasEvent::PanBy { dx, dy }) if (dx - 10.0).abs() < 1e-4 && (dy + 10.0).abs() < 1e-4
            ));
        }

        // Move tool (CanvasTool::Pointer) moves the active layer on Left-drag rather than panning
        // the viewport; Brush/Transform do NOT pan on Left-drag.
        let pointer_prog = ImageCanvasProgram {
            frame: Some(dummy_frame(1000, 800)),
            tool: CanvasTool::Pointer,
            restrict_pan_to_hand: true,
            ..Default::default()
        };
        let mut ptr_state = CanvasInteractionState::default();
        shader::Program::update(
            &pointer_prog,
            &mut ptr_state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor_a,
        );
        assert!(ptr_state.moving_layer);
        assert!(!ptr_state.dragging);

        for tool in [
            CanvasTool::Brush,
            CanvasTool::HealingBrush,
            CanvasTool::Crop,
            CanvasTool::RotateAngle,
            CanvasTool::Transform,
        ] {
            let tool_prog = ImageCanvasProgram {
                frame: Some(dummy_frame(1000, 800)),
                tool,
                restrict_pan_to_hand: true,
                ..Default::default()
            };
            let mut st = CanvasInteractionState::default();
            shader::Program::update(
                &tool_prog,
                &mut st,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor_a,
            );
            assert!(!st.dragging, "left click on {tool:?} must never start pan");
        }
    }

    #[test]
    fn window_unfocused_clears_transient_interaction_flags() {
        let prog = ImageCanvasProgram::default();
        let mut state = CanvasInteractionState {
            space_pan_active: true,
            dragging: true,
            painting: true,
            selecting: true,
            cropping: true,
            transform_dragging: true,
            ..Default::default()
        };
        let bounds = Rectangle::new(iced::Point::ORIGIN, iced::Size::new(800.0, 600.0));
        let act = shader::Program::update(
            &prog,
            &mut state,
            &Event::Window(iced::window::Event::Unfocused),
            bounds,
            mouse::Cursor::Unavailable,
        );
        assert!(act.is_some());
        assert!(!state.space_pan_active);
        assert!(!state.dragging);
        assert!(!state.painting);
        assert!(!state.selecting);
        assert!(!state.cropping);
        assert!(!state.transform_dragging);
    }

    #[test]
    fn shift_wheel_and_horizontal_pixels_emit_pan_by() {
        let prog = ImageCanvasProgram {
            frame: Some(dummy_frame(1000, 800)),
            ..Default::default()
        };
        let bounds = Rectangle::new(iced::Point::ORIGIN, iced::Size::new(800.0, 600.0));
        let cursor = mouse::Cursor::Available(iced::Point::new(100.0, 100.0));
        let mut state = CanvasInteractionState {
            modifiers: Modifiers::SHIFT,
            ..Default::default()
        };

        let shift_wheel = shader::Program::update(
            &prog,
            &mut state,
            &Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
            }),
            bounds,
            cursor,
        )
        .and_then(|a| a.into_inner().0);
        assert!(matches!(
            shift_wheel,
            Some(CanvasEvent::PanBy { dx, dy }) if dx > 10.0 && dy.abs() < 1e-5
        ));

        state.modifiers = Modifiers::empty();
        let horiz_pixels = shader::Program::update(
            &prog,
            &mut state,
            &Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: -32.0, y: 2.0 },
            }),
            bounds,
            cursor,
        )
        .and_then(|a| a.into_inner().0);
        assert!(matches!(
            horiz_pixels,
            Some(CanvasEvent::PanBy { dx, dy }) if (dx + 32.0).abs() < 1e-4 && dy.abs() < 1e-5
        ));
    }

    #[test]
    fn healing_brush_alt_click_picks_source_and_left_drag_strokes() {
        let prog = ImageCanvasProgram {
            frame: Some(dummy_frame(400, 300)),
            tool: CanvasTool::HealingBrush,
            brush_radius_px: 12.0,
            ..Default::default()
        };
        let bounds = Rectangle::new(iced::Point::ORIGIN, iced::Size::new(400.0, 300.0));
        let cursor = mouse::Cursor::Available(iced::Point::new(120.0, 90.0));
        let mut state = CanvasInteractionState {
            modifiers: Modifiers::ALT,
            ..Default::default()
        };

        let picked = shader::Program::update(
            &prog,
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .and_then(|a| a.into_inner().0);
        assert!(matches!(
            picked,
            Some(CanvasEvent::HealingSourcePicked([x, y])) if (x - 120.0).abs() < 1e-3 && (y - 90.0).abs() < 1e-3
        ));
        assert!(state.healing_source_px.is_some());

        let (lines, _) = prog.build_vector_overlay(&state, cursor, bounds);
        assert!(!lines.is_empty(), "expected healing source '+' crosshair");

        state.modifiers = Modifiers::empty();
        let begin = shader::Program::update(
            &prog,
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .and_then(|a| a.into_inner().0);
        assert!(matches!(begin, Some(CanvasEvent::StrokeBegin(_))));
        assert!(state.painting);
    }

    #[test]
    fn transform_handle_hit_test_and_drag_math() {
        let tp = TransformPreview {
            base_frame: Some(dummy_frame(800, 600)),
            layer_frame: dummy_frame(200, 100),
            layer_offset: [100, 100],
            layer_size: [200, 100],
            spec: LayerTransformSpec::default(),
        };

        let handles = transform_handle_positions(&tp, 24.0);
        // External rotate handle is above Top midpoint (200, 100 - 24)
        assert_eq!(
            hit_test_transform_handle(&tp, handles[8].1, 8.0, 24.0),
            Some(TransformDragMode::Rotate)
        );
        // All 8 resize handles hit cleanly
        for (h, pos) in &handles[..8] {
            assert_eq!(
                hit_test_transform_handle(&tp, *pos, 8.0, 24.0),
                Some(TransformDragMode::Scale(*h))
            );
        }
        // Inside OBB hits Move
        assert_eq!(
            hit_test_transform_handle(&tp, [200.0, 150.0], 8.0, 24.0),
            Some(TransformDragMode::Move)
        );
        // Outside OBB & handles hits None
        assert_eq!(
            hit_test_transform_handle(&tp, [50.0, 50.0], 8.0, 24.0),
            None
        );

        // Scale BottomRight with Shift aspect-lock & non-Alt (TopLeft stays pinned at [100, 100])
        let scaled = apply_transform_drag(
            &tp,
            tp.spec,
            TransformDragMode::Scale(TransformHandle::BottomRight),
            [300.0, 200.0],
            [400.0, 220.0],
            true,
            false,
        );
        assert!((scaled.scale_x - 1.5).abs() < 1e-4);
        assert!((scaled.scale_y - 1.5).abs() < 1e-4);
        let tl_after = transform_local_to_doc([0.0, 0.0], tp.layer_offset, tp.layer_size, scaled);
        assert!((tl_after[0] - 100.0).abs() < 1e-3);
        assert!((tl_after[1] - 100.0).abs() < 1e-3);

        // Scale BottomRight with Alt (center-scale, translate stays 0)
        let center_scaled = apply_transform_drag(
            &tp,
            tp.spec,
            TransformDragMode::Scale(TransformHandle::BottomRight),
            [300.0, 200.0],
            [350.0, 225.0],
            false,
            true,
        );
        assert!((center_scaled.translate_x).abs() < 1e-5);
        assert!((center_scaled.translate_y).abs() < 1e-5);
        assert!((center_scaled.scale_x - 1.5).abs() < 1e-4);
        assert!((center_scaled.scale_y - 1.5).abs() < 1e-4);

        // Enter emits TransformCommit, Esc emits TransformCancel
        let prog = ImageCanvasProgram {
            frame: Some(dummy_frame(800, 600)),
            tool: CanvasTool::Transform,
            transform_preview: Some(tp),
            ..Default::default()
        };
        let bounds = Rectangle::new(iced::Point::ORIGIN, iced::Size::new(800.0, 600.0));
        let mut st = CanvasInteractionState::default();
        let enter_ev = shader::Program::update(
            &prog,
            &mut st,
            &Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
                modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
                physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
                location: iced::keyboard::Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            bounds,
            mouse::Cursor::Unavailable,
        )
        .and_then(|a| a.into_inner().0);
        assert!(matches!(enter_ev, Some(CanvasEvent::TransformCommit)));

        let esc_ev = shader::Program::update(
            &prog,
            &mut st,
            &Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                physical_key: iced::keyboard::key::Physical::Code(
                    iced::keyboard::key::Code::Escape,
                ),
                location: iced::keyboard::Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            bounds,
            mouse::Cursor::Unavailable,
        )
        .and_then(|a| a.into_inner().0);
        assert!(matches!(esc_ev, Some(CanvasEvent::TransformCancel)));

        // Unclamped Transform pointer handling: selection near top edge (offset [0, 0]) puts
        // Rotate handle at y = -24 (outside [0..800, 0..600]); clicking & dragging must still hit Rotate.
        let edge_tp = TransformPreview {
            base_frame: Some(dummy_frame(800, 600)),
            layer_frame: dummy_frame(100, 100),
            layer_offset: [0, 0],
            layer_size: [100, 100],
            spec: LayerTransformSpec::default(),
        };
        let edge_prog = ImageCanvasProgram {
            frame: Some(dummy_frame(800, 600)),
            tool: CanvasTool::Transform,
            transform_preview: Some(edge_tp.clone()),
            zoom: 0.5,
            ..Default::default()
        };
        let edge_bounds = Rectangle::new(iced::Point::ORIGIN, iced::Size::new(800.0, 600.0));
        let (left, top, drawn_w, drawn_h, fw, fh) = edge_prog.fit_geometry(edge_bounds).unwrap();
        let doc_per_screen = (fw / drawn_w.max(1.0)).max(1e-4);
        let edge_handles = transform_handle_positions(&edge_tp, 24.0 * doc_per_screen);
        let rot_doc = edge_handles[8].1;
        assert!(rot_doc[1] < 0.0, "rotate handle should sit above y=0");
        let rot_screen = iced::Point::new(
            left + (rot_doc[0] / fw) * drawn_w,
            top + (rot_doc[1] / fh) * drawn_h,
        );
        assert!(edge_bounds.contains(rot_screen));
        let mut edge_st = CanvasInteractionState::default();
        let _ = shader::Program::update(
            &edge_prog,
            &mut edge_st,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            edge_bounds,
            mouse::Cursor::Available(rot_screen),
        );
        assert_eq!(edge_st.transform_drag_mode, Some(TransformDragMode::Rotate));
        let drag_ev = shader::Program::update(
            &edge_prog,
            &mut edge_st,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: iced::Point::new(rot_screen.x + 50.0, rot_screen.y),
            }),
            edge_bounds,
            mouse::Cursor::Available(iced::Point::new(rot_screen.x + 50.0, rot_screen.y)),
        )
        .and_then(|a| a.into_inner().0);
        assert!(matches!(
            drag_ev,
            Some(CanvasEvent::TransformUpdated(s)) if s.rotation_deg > 1.0
        ));
    }

    #[test]
    fn gradient_snap_angle_15_snaps_45_and_90_degrees() {
        let s45 = snap_angle_15([100.0, 100.0], [200.0, 195.0]);
        assert!((s45[0] - s45[1]).abs() < 1e-2, "expected 45 deg: {s45:?}");
        let s90 = snap_angle_15([100.0, 100.0], [104.0, 200.0]);
        assert!((s90[0] - 100.0).abs() < 1e-2, "expected 90 deg: {s90:?}");
    }

    #[test]
    fn interactive_gradient_drag_handles_and_keys() {
        let bounds = Rectangle::new(iced::Point::ORIGIN, iced::Size::new(800.0, 600.0));
        let prog = ImageCanvasProgram {
            frame: Some(dummy_frame(800, 600)),
            tool: CanvasTool::Gradient,
            gradient_preview: Some(GradientPreview {
                start: Some([100.0, 100.0]),
                end: Some([300.0, 100.0]),
                interactive: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let (left, top, drawn_w, drawn_h, fw, fh) = prog.fit_geometry(bounds).unwrap();
        let to_screen = |doc: [f32; 2]| {
            iced::Point::new(
                left + (doc[0] / fw) * drawn_w,
                top + (doc[1] / fh) * drawn_h,
            )
        };
        let mut st = CanvasInteractionState::default();
        let end_screen = to_screen([300.0, 100.0]);
        let _ = shader::Program::update(
            &prog,
            &mut st,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(end_screen),
        );
        assert_eq!(st.gradient_drag_mode, Some(GradientDragMode::MoveEnd));
        let moved_screen = to_screen([400.0, 200.0]);
        let _ = shader::Program::update(
            &prog,
            &mut st,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: moved_screen,
            }),
            bounds,
            mouse::Cursor::Available(moved_screen),
        );
        let rel = shader::Program::update(
            &prog,
            &mut st,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(moved_screen),
        )
        .and_then(|a| a.into_inner().0);
        assert!(matches!(
            rel,
            Some(CanvasEvent::GradientDraftUpdated { .. })
        ));

        let enter_ev = shader::Program::update(
            &prog,
            &mut st,
            &Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
                modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
                physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
                location: iced::keyboard::Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            bounds,
            mouse::Cursor::Available(moved_screen),
        )
        .and_then(|a| a.into_inner().0);
        assert!(matches!(enter_ev, Some(CanvasEvent::GradientDraftApply)));

        let esc_ev = shader::Program::update(
            &prog,
            &mut st,
            &Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                physical_key: iced::keyboard::key::Physical::Code(
                    iced::keyboard::key::Code::Escape,
                ),
                location: iced::keyboard::Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            bounds,
            mouse::Cursor::Available(moved_screen),
        )
        .and_then(|a| a.into_inner().0);
        assert!(matches!(esc_ev, Some(CanvasEvent::GradientDraftCancel)));
    }
}
