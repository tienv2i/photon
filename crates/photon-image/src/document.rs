//! Non-destructive editor document, edit operations, bounded CoW history,
//! live brush/eraser/liquify stroke sessions, and mask-aware painting.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{Rgba, RgbaImage};
use photon_protocol::{
    ApplyTarget, BrushSettings, CanvasAnchor, ColorAdjustments, ColorBalanceAdjustment,
    CurvesAdjustment, DrawShapeKind, FlipAxis, GradientColorMode, GradientKind, HsvAdjustment,
    LiquifyMode, LiveStrokeKind, ResizeFilter, ResizeMode, SelectionBooleanOp, SelectionModifyOp,
    SelectionShape, ShapeFillMode, UiText, VectorPathNode,
};

use crate::{
    ImageError, Layer, LayerOperation, LayerStack, MAX_DOCUMENT_PIXELS, MAX_HISTORY_STEPS,
    MAX_IMAGE_DIMENSION, MAX_SNAPSHOT_BYTES, SelectionMask, blend_layer_onto, codec,
    composite_stack, decode_rgba, drawing, liquify, pixels, rasterize_shape,
};

#[derive(Debug, Clone)]
pub enum EditOp {
    Resize {
        width: u32,
        height: u32,
        filter: ResizeFilter,
        mode: ResizeMode,
        fill: Option<[u8; 3]>,
        fill_alpha: u8,
    },
    Crop {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        fill: Option<[u8; 3]>,
        fill_alpha: u8,
    },
    Rotate {
        degrees: i32,
    },
    Flip {
        axis: FlipAxis,
    },
    AdjustColor(ColorAdjustments),
    CanvasSize {
        width: u32,
        height: u32,
        anchor: CanvasAnchor,
        fill: Option<[u8; 3]>,
        fill_alpha: u8,
    },
    RotateAngle {
        degrees: f64,
        fill: Option<[u8; 3]>,
        fill_alpha: u8,
    },
    AdjustHsv(HsvAdjustment),
    ColorBalance(ColorBalanceAdjustment),
    AdjustCurves(CurvesAdjustment),
    Paint {
        points: Vec<[f64; 2]>,
        color: [u8; 3],
        radius: f32,
        opacity: f32,
        erase: bool,
        brush: BrushSettings,
    },
    Liquify {
        points: Vec<[f64; 2]>,
        mode: LiquifyMode,
        radius: f32,
        strength: f32,
    },
    LiveStrokeCommit {
        label: UiText,
        pixels: Arc<RgbaImage>,
        mask: Option<Arc<Vec<u8>>>,
    },
    DrawShape {
        kind: DrawShapeKind,
        points: Vec<[f32; 2]>,
        nodes: Vec<VectorPathNode>,
        fill_mode: ShapeFillMode,
        closed: bool,
        color: [u8; 3],
        radius: f32,
        opacity: f32,
        brush: BrushSettings,
    },
    FloodFill {
        x: u32,
        y: u32,
        tolerance: u8,
        contiguous: bool,
        color: [u8; 3],
        opacity: f32,
    },
    DrawGradient {
        start: [f32; 2],
        end: [f32; 2],
        kind: GradientKind,
        color_mode: GradientColorMode,
        fg_color: [u8; 3],
        bg_color: [u8; 3],
        opacity: f32,
    },
    Layer(LayerOperation),
}

impl EditOp {
    pub fn ui_label(&self) -> UiText {
        match self {
            Self::Resize { width, height, .. } => UiText::with(
                "op_resize",
                [("width", width.to_string()), ("height", height.to_string())],
            ),
            Self::Crop { width, height, .. } => UiText::with(
                "op_crop",
                [("width", width.to_string()), ("height", height.to_string())],
            ),
            Self::Rotate { degrees } => {
                UiText::with("op_rotate", [("degrees", degrees.to_string())])
            }
            Self::Flip { axis } => {
                let axis_str = match axis {
                    FlipAxis::Horizontal => "horizontal",
                    FlipAxis::Vertical => "vertical",
                };
                UiText::with("op_flip", [("axis", axis_str)])
            }
            Self::AdjustColor(_) => UiText::new("op_adjust_color"),
            Self::CanvasSize { width, height, .. } => UiText::with(
                "op_canvas_size",
                [("width", width.to_string()), ("height", height.to_string())],
            ),
            Self::RotateAngle { degrees, .. } => {
                UiText::with("op_rotate_angle", [("degrees", format!("{degrees:.1}"))])
            }
            Self::AdjustHsv(_) => UiText::new("op_adjust_hsv"),
            Self::ColorBalance(_) => UiText::new("op_color_balance"),
            Self::AdjustCurves(_) => UiText::new("op_adjust_curves"),
            Self::Paint { erase, .. } => {
                if *erase {
                    UiText::new("op_eraser")
                } else {
                    UiText::new("op_brush")
                }
            }
            Self::Liquify { .. } => UiText::new("op_liquify"),
            Self::LiveStrokeCommit { label, .. } => label.clone(),
            Self::DrawShape { .. } => UiText::new("op_draw_shape"),
            Self::FloodFill { .. } => UiText::new("op_flood_fill"),
            Self::DrawGradient { .. } => UiText::new("op_gradient"),
            Self::Layer(op) => op.ui_label(),
        }
    }
}

#[derive(Debug, Clone)]
struct Step {
    operation: EditOp,
    target: ApplyTarget,
    before: LayerStack,
}

#[derive(Debug, Clone)]
struct LiveStrokeSession {
    before_stack: LayerStack,
    stroke: LiveStrokeKind,
    last_point: [f64; 2],
    editing_mask: bool,
}

/// Non-destructive working document with bounded CoW layer-stack history.
#[derive(Debug, Clone)]
pub struct EditorDocument {
    pub source_path: PathBuf,
    pub save_path: PathBuf,
    pub original: Arc<RgbaImage>,
    pub original_stack: LayerStack,
    pub stack: LayerStack,
    pub current: Arc<RgbaImage>,
    undo_stack: Vec<Step>,
    redo_stack: Vec<(EditOp, ApplyTarget)>,
    saved_at_depth: Option<usize>,
    pub pixel_revision: u64,
    pub original_revision: u64,
    pub selection_revision: u64,
    pub history_trimmed: bool,
    live_stroke: Option<LiveStrokeSession>,
}

impl EditorDocument {
    pub fn open(path: PathBuf) -> Result<Self, ImageError> {
        let rgba = decode_rgba(&path)?;
        Ok(Self::from_rgba(path, rgba))
    }

    pub fn from_rgba(path: PathBuf, rgba: RgbaImage) -> Self {
        let original = Arc::new(rgba);
        let stack = LayerStack::from_single(Arc::clone(&original));
        Self {
            save_path: path.clone(),
            source_path: path,
            original_stack: stack.clone(),
            current: Arc::clone(&original),
            original,
            stack,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            saved_at_depth: Some(0),
            pixel_revision: 1,
            original_revision: 1,
            selection_revision: 1,
            history_trimmed: false,
            live_stroke: None,
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.stack.canvas_width, self.stack.canvas_height)
    }

    pub fn original_pixels(&self) -> Arc<RgbaImage> {
        Arc::clone(&self.original)
    }

    pub fn working_pixels(&self) -> Arc<RgbaImage> {
        Arc::clone(&self.current)
    }

    pub fn stack(&self) -> &LayerStack {
        &self.stack
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn history_labels(&self) -> Vec<UiText> {
        self.undo_stack
            .iter()
            .map(|s| s.operation.ui_label())
            .collect()
    }

    pub fn redo_labels(&self) -> Vec<UiText> {
        self.redo_stack
            .iter()
            .rev()
            .map(|(op, _)| op.ui_label())
            .collect()
    }

    pub fn modified(&self) -> bool {
        self.saved_at_depth != Some(self.undo_stack.len()) || self.live_stroke.is_some()
    }

    pub fn apply(&mut self, op: EditOp) -> Result<bool, ImageError> {
        self.apply_with_target(op, ApplyTarget::WholeImage)
    }

    pub fn apply_layer_op(&mut self, op: LayerOperation) -> Result<bool, ImageError> {
        self.apply_with_target(EditOp::Layer(op), ApplyTarget::WholeImage)
    }

    pub fn select_layer(&mut self, id: u64) -> Result<bool, ImageError> {
        self.stack.select(id)
    }

    pub fn apply_with_target(
        &mut self,
        op: EditOp,
        target: ApplyTarget,
    ) -> Result<bool, ImageError> {
        if self.live_stroke.is_some() {
            let _ = self.live_stroke_end(true);
        }

        if let EditOp::Layer(ref layer_op) = op {
            let resolved_op = match layer_op {
                LayerOperation::AddEmpty { name } if name.trim().is_empty() => {
                    LayerOperation::AddEmpty {
                        name: format!("Layer {}", self.stack.next_id),
                    }
                }
                other => other.clone(),
            };
            // Switching between editing layer pixels and editing mask only changes
            // the active target (like `select_layer`), without clearing redo history.
            if matches!(resolved_op, LayerOperation::SetEditingMask { .. }) {
                let changed = self.stack.apply_op(&resolved_op)?;
                if changed {
                    self.pixel_revision += 1;
                }
                return Ok(changed);
            }
            let before = self.stack.clone();
            let changed = self.stack.apply_op(&resolved_op)?;
            if !changed {
                return Ok(false);
            }
            self.push_step(EditOp::Layer(resolved_op), target, before);
            self.current = composite_stack(&self.stack);
            self.pixel_revision += 1;
            return Ok(true);
        }

        if self.is_noop(self.dimensions(), &op)? {
            return Ok(false);
        }

        let before = self.stack.clone();
        self.execute_op(&op, target)?;
        self.push_step(op, target, before);
        self.current = composite_stack(&self.stack);
        self.pixel_revision += 1;
        Ok(true)
    }

    pub fn live_stroke_begin(
        &mut self,
        stroke: LiveStrokeKind,
        x: f64,
        y: f64,
    ) -> Result<bool, ImageError> {
        if self.live_stroke.is_some() {
            let _ = self.live_stroke_end(true);
        }
        if !x.is_finite() || !y.is_finite() {
            return Err(ImageError::InvalidEdit(
                "stroke coordinates must be finite".into(),
            ));
        }
        match stroke {
            LiveStrokeKind::Paint {
                radius, opacity, ..
            } => {
                validate_paint(&[[x, y]], radius, opacity)?;
            }
            LiveStrokeKind::Liquify {
                radius, strength, ..
            } => {
                validate_liquify(&[[x, y]], radius, strength)?;
            }
        }
        let active = self.stack.active_layer();
        if active.locked {
            return Err(ImageError::coded("layer_locked"));
        }
        if !active.visible {
            return Err(ImageError::coded("layer_hidden"));
        }
        let editing_mask = matches!(stroke, LiveStrokeKind::Paint { .. })
            && active.editing_mask
            && active.mask.is_some();
        let before_stack = self.stack.clone();
        self.live_stroke = Some(LiveStrokeSession {
            before_stack,
            stroke,
            last_point: [x, y],
            editing_mask,
        });
        self.apply_live_segment(&[[x, y]]);
        self.current = composite_stack(&self.stack);
        self.pixel_revision += 1;
        Ok(true)
    }

    pub fn live_stroke_step(&mut self, points: &[[f64; 2]]) -> Result<bool, ImageError> {
        if points.is_empty() {
            return Ok(false);
        }
        if points.iter().any(|[x, y]| !x.is_finite() || !y.is_finite()) {
            return Err(ImageError::InvalidEdit(
                "stroke coordinates must be finite".into(),
            ));
        }
        let Some(last) = self.live_stroke.as_ref().map(|s| s.last_point) else {
            return Ok(false);
        };
        let mut seg = Vec::with_capacity(points.len() + 1);
        seg.push(last);
        seg.extend_from_slice(points);
        if let Some(session) = self.live_stroke.as_mut()
            && let Some(&tail) = points.last()
        {
            session.last_point = tail;
        }
        self.apply_live_segment(&seg);
        self.current = composite_stack(&self.stack);
        self.pixel_revision += 1;
        Ok(true)
    }

    pub fn live_stroke_end(&mut self, commit: bool) -> Result<bool, ImageError> {
        let Some(session) = self.live_stroke.take() else {
            return Ok(false);
        };
        if !commit {
            self.stack = session.before_stack;
            self.current = composite_stack(&self.stack);
            self.pixel_revision += 1;
            return Ok(true);
        }
        let label = match session.stroke {
            LiveStrokeKind::Paint { erase, .. } => {
                if erase {
                    UiText::new("op_eraser")
                } else {
                    UiText::new("op_brush")
                }
            }
            LiveStrokeKind::Liquify { .. } => UiText::new("op_liquify"),
        };
        let active = self.stack.active_layer();
        let op = EditOp::LiveStrokeCommit {
            label,
            pixels: Arc::clone(&active.pixels),
            mask: active.mask.as_ref().map(Arc::clone),
        };
        self.push_step(op, ApplyTarget::ActiveLayer, session.before_stack);
        self.current = composite_stack(&self.stack);
        self.pixel_revision += 1;
        Ok(true)
    }

    fn apply_live_segment(&mut self, points: &[[f64; 2]]) {
        let Some(session) = self.live_stroke.as_ref() else {
            return;
        };
        let stroke = session.stroke;
        let editing_mask = session.editing_mask;
        let active = self.stack.active_layer_mut();
        let ox = f64::from(active.offset_x);
        let oy = f64::from(active.offset_y);
        let mx_off = f64::from(active.offset_x + active.mask_offset_x);
        let my_off = f64::from(active.offset_y + active.mask_offset_y);
        match stroke {
            LiveStrokeKind::Paint {
                color,
                radius,
                opacity,
                erase,
                brush,
            } => {
                if editing_mask && let Some(mask_arc) = active.mask.as_ref() {
                    let mask_pts: Vec<[f64; 2]> = points
                        .iter()
                        .map(|[x, y]| [x - mx_off, y - my_off])
                        .collect();
                    let (lw, lh) = active.pixels.dimensions();
                    let next_mask = paint_mask_stroke(
                        mask_arc, lw, lh, &mask_pts, color, radius, opacity, erase, brush,
                    );
                    active.mask = Some(Arc::new(next_mask));
                    return;
                }
                let local_pts: Vec<[f64; 2]> =
                    points.iter().map(|[x, y]| [x - ox, y - oy]).collect();
                let painted = pixels::paint_stroke_with_brush(
                    &active.pixels,
                    &local_pts,
                    color,
                    radius,
                    opacity,
                    erase,
                    brush,
                );
                active.pixels = Arc::new(painted);
            }
            LiveStrokeKind::Liquify {
                mode,
                radius,
                strength,
            } => {
                let local_pts: Vec<[f64; 2]> =
                    points.iter().map(|[x, y]| [x - ox, y - oy]).collect();
                let warped = liquify::liquify_stroke(
                    &active.pixels,
                    active.original.as_deref(),
                    &local_pts,
                    mode,
                    radius,
                    strength,
                );
                active.pixels = Arc::new(warped);
            }
        }
    }

    pub fn undo(&mut self) -> Result<bool, ImageError> {
        let Some(step) = self.undo_stack.pop() else {
            return Ok(false);
        };
        self.stack = step.before;
        self.current = composite_stack(&self.stack);
        self.redo_stack.push((step.operation, step.target));
        self.pixel_revision += 1;
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, ImageError> {
        let Some((op, target)) = self.redo_stack.pop() else {
            return Ok(false);
        };
        let before = self.stack.clone();
        let res = match &op {
            EditOp::Layer(layer_op) => self.stack.apply_op(layer_op).map(|_| ()),
            EditOp::LiveStrokeCommit { pixels, mask, .. } => {
                let active = self.stack.active_layer_mut();
                active.pixels = Arc::clone(pixels);
                if let Some(m) = mask {
                    active.mask = Some(Arc::clone(m));
                }
                Ok(())
            }
            _ => self.execute_op(&op, target),
        };
        if let Err(err) = res {
            self.redo_stack.push((op, target));
            return Err(err);
        }
        self.push_preserving_redo(op, target, before);
        self.current = composite_stack(&self.stack);
        self.pixel_revision += 1;
        Ok(true)
    }

    /// Jump directly to `target_len` applied steps (`0` = initial state, `undo_len()` = current).
    pub fn jump_history(&mut self, target_len: usize) -> Result<bool, ImageError> {
        let mut changed = false;
        while self.undo_stack.len() > target_len {
            if !self.undo()? {
                break;
            }
            changed = true;
        }
        while self.undo_stack.len() < target_len && !self.redo_stack.is_empty() {
            if !self.redo()? {
                break;
            }
            changed = true;
        }
        Ok(changed)
    }

    pub fn reset(&mut self) -> bool {
        if self.undo_stack.is_empty()
            && self.redo_stack.is_empty()
            && self.live_stroke.is_none()
            && !self.history_trimmed
            && Arc::ptr_eq(&self.current, &self.original)
        {
            return false;
        }
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.stack = self.original_stack.clone();
        self.current = Arc::clone(&self.original);
        self.history_trimmed = false;
        self.live_stroke = None;
        if self.saved_at_depth != Some(0) {
            self.saved_at_depth = None;
        }
        self.pixel_revision += 1;
        true
    }

    pub fn latest_color_adjustments(&self) -> ColorAdjustments {
        for step in self.undo_stack.iter().rev() {
            if let EditOp::AdjustColor(adj) = &step.operation {
                return *adj;
            }
        }
        ColorAdjustments::default()
    }

    /// Save non-destructively: creates `<path>.bak` when overwriting an existing file,
    /// writes to a staging `.photon-tmp` file, and atomically renames into place.
    pub fn save_to(&mut self, target: Option<&Path>) -> Result<PathBuf, ImageError> {
        let dest = target
            .map(PathBuf::from)
            .unwrap_or_else(|| self.save_path.clone());
        let saved = codec::save_to(&self.current, &dest)?;
        self.save_path = saved.clone();
        self.saved_at_depth = Some(self.undo_stack.len());
        Ok(saved)
    }

    /// Export a delivery copy without modifying document state or source file.
    pub fn export_to(
        &self,
        dest: &Path,
        quality: Option<u8>,
        metadata: bool,
        max_edge: Option<u32>,
    ) -> Result<PathBuf, ImageError> {
        codec::export_to(
            &self.source_path,
            &self.current,
            dest,
            quality,
            metadata,
            max_edge,
        )
    }

    fn push_step(&mut self, operation: EditOp, target: ApplyTarget, before: LayerStack) {
        self.redo_stack.clear();
        if let Some(saved_depth) = self.saved_at_depth
            && saved_depth > self.undo_stack.len()
        {
            self.saved_at_depth = None;
        }
        self.push_preserving_redo(operation, target, before);
    }

    fn push_preserving_redo(&mut self, operation: EditOp, target: ApplyTarget, before: LayerStack) {
        self.undo_stack.push(Step {
            operation,
            target,
            before,
        });
        self.trim_history();
    }

    fn trim_history(&mut self) {
        while self.undo_stack.len() > MAX_HISTORY_STEPS {
            self.undo_stack.remove(0);
            self.history_trimmed = true;
            self.saved_at_depth = self.saved_at_depth.and_then(|d| d.checked_sub(1));
        }
        while self.compute_snapshot_bytes() > MAX_SNAPSHOT_BYTES && self.undo_stack.len() > 1 {
            self.undo_stack.remove(0);
            self.history_trimmed = true;
            self.saved_at_depth = self.saved_at_depth.and_then(|d| d.checked_sub(1));
        }
    }

    fn compute_snapshot_bytes(&self) -> u64 {
        let mut seen: HashSet<*const RgbaImage> = HashSet::new();
        let mut total = 0u64;
        for step in &self.undo_stack {
            for layer in &step.before.layers {
                let ptr = Arc::as_ptr(&layer.pixels);
                if seen.insert(ptr) {
                    total += layer.pixels.as_raw().len() as u64;
                }
            }
        }
        total
    }

    fn is_simple_single_layer(&self) -> bool {
        self.stack.layers.len() == 1
            && self.stack.background.is_none()
            && self.stack.layers[0].offset_x == 0
            && self.stack.layers[0].offset_y == 0
            && self.stack.layers[0].dimensions()
                == (self.stack.canvas_width, self.stack.canvas_height)
    }

    fn is_noop(&self, (w, h): (u32, u32), op: &EditOp) -> Result<bool, ImageError> {
        match op {
            EditOp::Resize {
                width,
                height,
                mode,
                fill,
                ..
            } => {
                validate_dimensions(*width, *height)?;
                if *mode == ResizeMode::Pad && fill.is_none() {
                    return Err(ImageError::InvalidEdit(
                        "resize pad mode requires a fill color".into(),
                    ));
                }
                Ok(*width == w && *height == h)
            }
            EditOp::Crop {
                x,
                y,
                width,
                height,
                fill,
                ..
            } => {
                validate_dimensions(*width, *height)?;
                if fill.is_none() {
                    if *x < 0
                        || *y < 0
                        || i64::from(*x) + i64::from(*width) > i64::from(w)
                        || i64::from(*y) + i64::from(*height) > i64::from(h)
                    {
                        return Err(ImageError::InvalidEdit(format!(
                            "crop {width}x{height} at ({x},{y}) exceeds image {w}x{h}"
                        )));
                    }
                } else {
                    let right = i64::from(*x) + i64::from(*width);
                    let bottom = i64::from(*y) + i64::from(*height);
                    if right <= 0
                        || bottom <= 0
                        || i64::from(*x) >= i64::from(w)
                        || i64::from(*y) >= i64::from(h)
                    {
                        return Err(ImageError::InvalidEdit(format!(
                            "grow crop {width}x{height} at ({x},{y}) does not overlap image {w}x{h}"
                        )));
                    }
                }
                Ok(*x == 0 && *y == 0 && *width == w && *height == h)
            }
            EditOp::Rotate { degrees } => {
                let norm = degrees.rem_euclid(360);
                if !matches!(norm, 0 | 90 | 180 | 270) {
                    return Err(ImageError::InvalidEdit(format!(
                        "rotation must be a multiple of 90 degrees, got {degrees}"
                    )));
                }
                Ok(norm == 0)
            }
            EditOp::Flip { .. } => Ok(false),
            EditOp::CanvasSize { width, height, .. } => {
                validate_dimensions(*width, *height)?;
                Ok(*width == w && *height == h)
            }
            EditOp::RotateAngle { degrees, .. } => {
                if !degrees.is_finite() {
                    return Err(ImageError::InvalidEdit(format!(
                        "rotation angle must be finite, got {degrees}"
                    )));
                }
                let norm = degrees.rem_euclid(360.0);
                Ok(norm < 1e-4 || (360.0 - norm) < 1e-4)
            }
            EditOp::AdjustColor(adj) => {
                let vals = [
                    adj.brightness,
                    adj.contrast,
                    adj.saturation,
                    adj.exposure,
                    adj.highlights,
                    adj.shadows,
                    adj.gamma,
                ];
                if vals.iter().any(|v| !v.is_finite()) {
                    return Err(ImageError::InvalidEdit(
                        "color adjustments must be finite".into(),
                    ));
                }
                Ok(adj.brightness.abs() < 1e-4
                    && adj.contrast.abs() < 1e-4
                    && adj.saturation.abs() < 1e-4
                    && adj.exposure.abs() < 1e-4
                    && adj.highlights.abs() < 1e-4
                    && adj.shadows.abs() < 1e-4
                    && (adj.gamma - 1.0).abs() < 1e-4)
            }
            EditOp::AdjustHsv(adj) => {
                if !adj.hue.is_finite() || !adj.saturation.is_finite() || !adj.value.is_finite() {
                    return Err(ImageError::InvalidEdit(
                        "HSV adjustments must be finite".into(),
                    ));
                }
                Ok(adj.hue.abs() < 1e-4 && adj.saturation.abs() < 1e-4 && adj.value.abs() < 1e-4)
            }
            EditOp::ColorBalance(adj) => {
                let all = adj
                    .shadows
                    .iter()
                    .chain(adj.midtones.iter())
                    .chain(adj.highlights.iter());
                if all.clone().any(|v| !v.is_finite()) {
                    return Err(ImageError::InvalidEdit(
                        "color balance values must be finite".into(),
                    ));
                }
                Ok(all.copied().all(|v| v.abs() < 1e-4))
            }
            EditOp::AdjustCurves(c) => {
                let id = pixels::is_identity_lut(&pixels::build_curve_lut(&c.rgb))
                    && pixels::is_identity_lut(&pixels::build_curve_lut(&c.red))
                    && pixels::is_identity_lut(&pixels::build_curve_lut(&c.green))
                    && pixels::is_identity_lut(&pixels::build_curve_lut(&c.blue));
                Ok(id)
            }
            EditOp::Paint {
                points,
                radius,
                opacity,
                ..
            } => {
                validate_paint(points, *radius, *opacity)?;
                Ok(false)
            }
            EditOp::Liquify {
                points,
                radius,
                strength,
                ..
            } => {
                validate_liquify(points, *radius, *strength)?;
                Ok(false)
            }
            EditOp::LiveStrokeCommit { .. } | EditOp::Layer(_) => Ok(false),
            EditOp::DrawShape { points, nodes, .. } => {
                if points
                    .iter()
                    .any(|p| !p[0].is_finite() || !p[1].is_finite())
                    || nodes.iter().any(|n| {
                        !n.anchor[0].is_finite()
                            || !n.anchor[1].is_finite()
                            || n.handle_in
                                .is_some_and(|h| !h[0].is_finite() || !h[1].is_finite())
                            || n.handle_out
                                .is_some_and(|h| !h[0].is_finite() || !h[1].is_finite())
                    })
                {
                    return Err(ImageError::InvalidEdit(
                        "shape coordinates must be finite".into(),
                    ));
                }
                Ok(points.is_empty() && nodes.is_empty())
            }
            EditOp::FloodFill { x, y, .. } => Ok(*x >= w || *y >= h),
            EditOp::DrawGradient { start, end, .. } => {
                if !start[0].is_finite()
                    || !start[1].is_finite()
                    || !end[0].is_finite()
                    || !end[1].is_finite()
                {
                    return Err(ImageError::InvalidEdit(
                        "gradient coordinates must be finite".into(),
                    ));
                }
                let dx = end[0] - start[0];
                let dy = end[1] - start[1];
                Ok(dx * dx + dy * dy < 1e-6)
            }
        }
    }

    fn execute_op(&mut self, op: &EditOp, target: ApplyTarget) -> Result<(), ImageError> {
        if target == ApplyTarget::NewLayer {
            let next = render_single_op(&self.current, None, op)?;
            let name = match op {
                EditOp::AdjustColor(_) => "Color & Light",
                EditOp::AdjustHsv(_) => "HSV Adjustment",
                EditOp::ColorBalance(_) => "Color Balance",
                EditOp::AdjustCurves(_) => "Curves",
                _ => "Layer",
            }
            .to_owned();
            self.stack.apply_op(&LayerOperation::AddImage {
                name,
                offset_x: 0,
                offset_y: 0,
                pixels: Arc::new(next),
            })?;
            return Ok(());
        }

        // Canvas-wide geometry operations always apply to the whole stack.
        match op {
            EditOp::Crop {
                x,
                y,
                width,
                height,
                fill,
                fill_alpha,
            } => {
                if self.is_simple_single_layer() {
                    let layer = &mut self.stack.layers[0];
                    let next = render_single_op(&layer.pixels, layer.original.as_deref(), op)?;
                    transform_layer_geometry(layer, op)?;
                    self.stack.canvas_width = next.width();
                    self.stack.canvas_height = next.height();
                    layer.pixels = Arc::new(next);
                } else {
                    let old_w = self.stack.canvas_width;
                    let old_h = self.stack.canvas_height;
                    self.stack.canvas_width = *width;
                    self.stack.canvas_height = *height;
                    for layer in &mut self.stack.layers {
                        layer.offset_x -= *x;
                        layer.offset_y -= *y;
                    }
                    if *x < 0
                        || *y < 0
                        || i64::from(*x) + i64::from(*width) > i64::from(old_w)
                        || i64::from(*y) + i64::from(*height) > i64::from(old_h)
                    {
                        fill_expanded_bottom_layer(&mut self.stack, *fill, *fill_alpha);
                    }
                }
                return Ok(());
            }
            EditOp::Resize {
                width,
                height,
                filter,
                mode,
                fill,
                fill_alpha,
            } => {
                if self.is_simple_single_layer() {
                    let layer = &mut self.stack.layers[0];
                    let next = render_single_op(&layer.pixels, layer.original.as_deref(), op)?;
                    transform_layer_geometry(layer, op)?;
                    self.stack.canvas_width = next.width();
                    self.stack.canvas_height = next.height();
                    layer.pixels = Arc::new(next);
                } else if *mode != ResizeMode::Stretch {
                    let next = pixels::resize_with_mode(
                        &self.current,
                        *width,
                        *height,
                        *filter,
                        *mode,
                        fill.unwrap_or([0, 0, 0]),
                        *fill_alpha,
                    );
                    let id = self.stack.next_id;
                    self.stack.next_id += 1;
                    self.stack.canvas_width = *width;
                    self.stack.canvas_height = *height;
                    self.stack.layers = vec![Layer::new(id, "Background", Arc::new(next))];
                    self.stack.active_layer_id = id;
                } else {
                    let old_w = f64::from(self.stack.canvas_width.max(1));
                    let old_h = f64::from(self.stack.canvas_height.max(1));
                    let sx = f64::from(*width) / old_w;
                    let sy = f64::from(*height) / old_h;
                    for layer in &mut self.stack.layers {
                        let (lw, lh) = layer.dimensions();
                        let new_lw = ((f64::from(lw) * sx).round() as u32).max(1);
                        let new_lh = ((f64::from(lh) * sy).round() as u32).max(1);
                        let sub_op = EditOp::Resize {
                            width: new_lw,
                            height: new_lh,
                            filter: *filter,
                            mode: ResizeMode::Stretch,
                            fill: None,
                            fill_alpha: 255,
                        };
                        let scaled = render_single_op(&layer.pixels, None, &sub_op)?;
                        transform_layer_geometry(layer, &sub_op)?;
                        layer.pixels = Arc::new(scaled);
                        layer.offset_x = (f64::from(layer.offset_x) * sx).round() as i32;
                        layer.offset_y = (f64::from(layer.offset_y) * sy).round() as i32;
                    }
                    self.stack.canvas_width = *width;
                    self.stack.canvas_height = *height;
                }
                return Ok(());
            }
            EditOp::CanvasSize {
                width,
                height,
                anchor,
                fill,
                fill_alpha,
            } => {
                if self.is_simple_single_layer() {
                    let layer = &mut self.stack.layers[0];
                    let next = render_single_op(&layer.pixels, layer.original.as_deref(), op)?;
                    transform_layer_geometry(layer, op)?;
                    self.stack.canvas_width = next.width();
                    self.stack.canvas_height = next.height();
                    layer.pixels = Arc::new(next);
                } else {
                    let old_w = self.stack.canvas_width;
                    let old_h = self.stack.canvas_height;
                    let (dx, dy) = pixels::anchor_offset(old_w, old_h, *width, *height, *anchor);
                    for layer in &mut self.stack.layers {
                        layer.offset_x += dx;
                        layer.offset_y += dy;
                    }
                    self.stack.canvas_width = *width;
                    self.stack.canvas_height = *height;
                    if *width > old_w || *height > old_h {
                        fill_expanded_bottom_layer(&mut self.stack, *fill, *fill_alpha);
                    }
                }
                return Ok(());
            }
            _ => {}
        }

        // Paint always targets the active layer (or its mask if `editing_mask`).
        if let EditOp::Paint {
            points,
            color,
            radius,
            opacity,
            erase,
            brush,
        } = op
        {
            let active = self.stack.active_layer();
            if active.locked {
                return Err(ImageError::coded("layer_locked"));
            }
            if !active.visible {
                return Err(ImageError::coded("layer_hidden"));
            }
            let ox = f64::from(active.offset_x);
            let oy = f64::from(active.offset_y);
            let local_pts: Vec<[f64; 2]> = points.iter().map(|[x, y]| [x - ox, y - oy]).collect();

            if active.editing_mask
                && let Some(mask_arc) = active.mask.as_ref()
            {
                let mx_off = f64::from(active.offset_x + active.mask_offset_x);
                let my_off = f64::from(active.offset_y + active.mask_offset_y);
                let mask_pts: Vec<[f64; 2]> = points
                    .iter()
                    .map(|[x, y]| [x - mx_off, y - my_off])
                    .collect();
                let (lw, lh) = active.dimensions();
                let next_mask = paint_mask_stroke(
                    mask_arc, lw, lh, &mask_pts, *color, *radius, *opacity, *erase, *brush,
                );
                self.stack.active_layer_mut().mask = Some(Arc::new(next_mask));
                return Ok(());
            }

            let next = pixels::paint_stroke_with_brush(
                &active.pixels,
                &local_pts,
                *color,
                *radius,
                *opacity,
                *erase,
                *brush,
            );
            self.stack.active_layer_mut().pixels = Arc::new(next);
            return Ok(());
        }

        // Liquify always targets the active layer.
        if let EditOp::Liquify {
            points,
            mode,
            radius,
            strength,
        } = op
        {
            let active = self.stack.active_layer();
            if active.locked {
                return Err(ImageError::coded("layer_locked"));
            }
            if !active.visible {
                return Err(ImageError::coded("layer_hidden"));
            }
            let ox = f64::from(active.offset_x);
            let oy = f64::from(active.offset_y);
            let local_pts: Vec<[f64; 2]> = points.iter().map(|[x, y]| [x - ox, y - oy]).collect();
            let next = liquify::liquify_stroke(
                &active.pixels,
                active.original.as_deref(),
                &local_pts,
                *mode,
                *radius,
                *strength,
            );
            self.stack.active_layer_mut().pixels = Arc::new(next);
            return Ok(());
        }

        if matches!(
            op,
            EditOp::DrawShape { .. } | EditOp::FloodFill { .. } | EditOp::DrawGradient { .. }
        ) {
            if self.stack.selection.quick_mask_active {
                let (cw, ch) = (self.stack.canvas_width, self.stack.canvas_height);
                let mask_img = drawing::mask_to_rgba(&self.stack.selection.alpha, cw, ch);
                let next_img = render_drawing_op(&mask_img, op, 0, 0, None, true);
                self.stack.selection.alpha = Arc::new(drawing::rgba_to_mask(&next_img));
                self.stack.selection.recompute_bounds();
                self.selection_revision += 1;
                return Ok(());
            }

            let active = self.stack.active_layer();
            if active.locked {
                return Err(ImageError::coded("layer_locked"));
            }
            if !active.visible {
                return Err(ImageError::coded("layer_hidden"));
            }
            let sel_snapshot = self
                .stack
                .selection
                .has_selection()
                .then(|| self.stack.selection.clone());

            if active.editing_mask
                && let Some(mask_arc) = active.mask.as_ref()
            {
                let mx_i = active.offset_x + active.mask_offset_x;
                let my_i = active.offset_y + active.mask_offset_y;
                let (lw, lh) = active.dimensions();
                let mask_img = drawing::mask_to_rgba(mask_arc, lw, lh);
                let next_img =
                    render_drawing_op(&mask_img, op, mx_i, my_i, sel_snapshot.as_ref(), true);
                let mut next_mask = drawing::rgba_to_mask(&next_img);
                if let Some(ref sel) = sel_snapshot {
                    clip_mask_by_selection(mask_arc, &mut next_mask, lw, lh, mx_i, my_i, sel);
                }
                self.stack.active_layer_mut().mask = Some(Arc::new(next_mask));
                return Ok(());
            }

            let ox_i = active.offset_x;
            let oy_i = active.offset_y;
            let mut next =
                render_drawing_op(&active.pixels, op, ox_i, oy_i, sel_snapshot.as_ref(), false);
            if let Some(ref sel) = sel_snapshot {
                clip_rgba_by_selection(&active.pixels, &mut next, ox_i, oy_i, sel);
            }
            self.stack.active_layer_mut().pixels = Arc::new(next);
            return Ok(());
        }

        if target == ApplyTarget::ActiveLayer {
            let active = self.stack.active_layer();
            if active.locked {
                return Err(ImageError::coded("layer_locked"));
            }
            if !active.visible {
                return Err(ImageError::coded("layer_hidden"));
            }
            let (old_w, old_h) = active.dimensions();
            let next = render_single_op(&active.pixels, active.original.as_deref(), op)?;
            let (new_w, new_h) = next.dimensions();
            let active_mut = self.stack.active_layer_mut();
            transform_layer_geometry(active_mut, op)?;
            if new_w != old_w || new_h != old_h {
                active_mut.offset_x += (old_w as i32 - new_w as i32) / 2;
                active_mut.offset_y += (old_h as i32 - new_h as i32) / 2;
            }
            active_mut.pixels = Arc::new(next);
            return Ok(());
        }

        if self.is_simple_single_layer() {
            let layer = &mut self.stack.layers[0];
            let next = render_single_op(&layer.pixels, layer.original.as_deref(), op)?;
            transform_layer_geometry(layer, op)?;
            self.stack.canvas_width = next.width();
            self.stack.canvas_height = next.height();
            layer.pixels = Arc::new(next);
            return Ok(());
        }

        match op {
            EditOp::Rotate { degrees } => {
                let cw = self.stack.canvas_width;
                let ch = self.stack.canvas_height;
                let norm = degrees.rem_euclid(360);
                for layer in &mut self.stack.layers {
                    let (lw, lh) = layer.dimensions();
                    let next = render_single_op(&layer.pixels, None, op)?;
                    transform_layer_geometry(layer, op)?;
                    let (ox, oy) = (layer.offset_x, layer.offset_y);
                    let (new_ox, new_oy) = match norm {
                        90 => (ch as i32 - oy - lh as i32, ox),
                        180 => (cw as i32 - ox - lw as i32, ch as i32 - oy - lh as i32),
                        270 => (oy, cw as i32 - ox - lw as i32),
                        _ => (ox, oy),
                    };
                    layer.pixels = Arc::new(next);
                    layer.offset_x = new_ox;
                    layer.offset_y = new_oy;
                }
                if norm == 90 || norm == 270 {
                    self.stack.canvas_width = ch;
                    self.stack.canvas_height = cw;
                }
            }
            EditOp::Flip { axis } => {
                let cw = self.stack.canvas_width;
                let ch = self.stack.canvas_height;
                for layer in &mut self.stack.layers {
                    let (lw, lh) = layer.dimensions();
                    let next = render_single_op(&layer.pixels, None, op)?;
                    transform_layer_geometry(layer, op)?;
                    match axis {
                        FlipAxis::Horizontal => {
                            layer.offset_x = cw as i32 - layer.offset_x - lw as i32;
                        }
                        FlipAxis::Vertical => {
                            layer.offset_y = ch as i32 - layer.offset_y - lh as i32;
                        }
                    }
                    layer.pixels = Arc::new(next);
                }
            }
            EditOp::AdjustColor(_)
            | EditOp::AdjustHsv(_)
            | EditOp::ColorBalance(_)
            | EditOp::AdjustCurves(_) => {
                for layer in &mut self.stack.layers {
                    let next = render_single_op(&layer.pixels, None, op)?;
                    layer.pixels = Arc::new(next);
                }
            }
            _ => {
                let next = render_single_op(&self.current, None, op)?;
                let (nw, nh) = next.dimensions();
                let id = self.stack.next_id;
                self.stack.next_id += 1;
                self.stack.canvas_width = nw;
                self.stack.canvas_height = nh;
                self.stack.layers = vec![Layer::new(id, "Background", Arc::new(next))];
                self.stack.active_layer_id = id;
            }
        }
        Ok(())
    }

    pub fn selection(&self) -> &SelectionMask {
        &self.stack.selection
    }

    pub fn select_all(&mut self) {
        self.stack.selection.select_all();
        self.selection_revision += 1;
    }

    pub fn deselect(&mut self) {
        self.stack.selection.deselect();
        self.selection_revision += 1;
    }

    pub fn reselect(&mut self) -> bool {
        let ok = self.stack.selection.reselect();
        if ok {
            self.selection_revision += 1;
        }
        ok
    }

    pub fn invert_selection(&mut self) {
        self.stack.selection.invert();
        self.selection_revision += 1;
    }

    pub fn select_shape(&mut self, shape: &SelectionShape, op: SelectionBooleanOp) {
        let (w, h) = (self.stack.canvas_width, self.stack.canvas_height);
        let active = self.stack.active_layer();
        let incoming = rasterize_shape(
            w,
            h,
            shape,
            &self.current,
            Some((&active.pixels, active.offset_x, active.offset_y)),
        );
        self.stack.selection.combine(&incoming, op);
        self.selection_revision += 1;
    }

    pub fn modify_selection(&mut self, op: SelectionModifyOp) {
        self.stack.selection.modify(op, &self.current);
        self.selection_revision += 1;
    }

    pub fn toggle_quick_mask(&mut self) -> bool {
        self.stack.selection.quick_mask_active = !self.stack.selection.quick_mask_active;
        self.selection_revision += 1;
        self.stack.selection.quick_mask_active
    }

    pub fn select_from_layer_alpha(&mut self, op: SelectionBooleanOp) {
        let (cw, ch) = (self.stack.canvas_width, self.stack.canvas_height);
        let active = self.stack.active_layer();
        let (lw, lh) = active.dimensions();
        let mut incoming = vec![0u8; (cw as usize) * (ch as usize)];
        for cy in 0..ch {
            let ly = cy as i32 - active.offset_y;
            if ly < 0 || ly >= lh as i32 {
                continue;
            }
            for cx in 0..cw {
                let lx = cx as i32 - active.offset_x;
                if lx < 0 || lx >= lw as i32 {
                    continue;
                }
                incoming[(cy * cw + cx) as usize] =
                    active.pixels.get_pixel(lx as u32, ly as u32).0[3];
            }
        }
        self.stack.selection.combine(&incoming, op);
        self.selection_revision += 1;
    }

    pub fn selection_to_mask(&mut self) -> Result<(), ImageError> {
        let active = self.stack.active_layer();
        if active.locked {
            return Err(ImageError::coded("layer_locked"));
        }
        let (lw, lh) = active.dimensions();
        let ox = active.offset_x;
        let oy = active.offset_y;
        let mut mask = vec![0u8; (lw as usize) * (lh as usize)];
        for y in 0..lh {
            for x in 0..lw {
                mask[(y * lw + x) as usize] =
                    self.stack.selection.alpha_at(ox + x as i32, oy + y as i32);
            }
        }
        let before = self.stack.clone();
        let id = active.id;
        let active_mut = self.stack.active_layer_mut();
        active_mut.mask = Some(Arc::new(mask));
        active_mut.mask_enabled = true;
        self.push_step(
            EditOp::Layer(LayerOperation::AddMask {
                id,
                reveal_all: true,
            }),
            ApplyTarget::ActiveLayer,
            before,
        );
        self.current = composite_stack(&self.stack);
        self.pixel_revision += 1;
        Ok(())
    }

    pub fn clear_selected_pixels(&mut self) -> Result<(), ImageError> {
        if !self.stack.selection.has_selection() {
            return Ok(());
        }
        let active = self.stack.active_layer();
        if active.locked {
            return Err(ImageError::coded("layer_locked"));
        }
        let before = self.stack.clone();
        let (lw, lh) = active.dimensions();
        let ox = active.offset_x;
        let oy = active.offset_y;
        let mut next = (*active.pixels).clone();
        for y in 0..lh {
            for x in 0..lw {
                let s = self.stack.selection.alpha_at(ox + x as i32, oy + y as i32);
                if s > 0 {
                    let p = next.get_pixel_mut(x, y);
                    let keep = 255 - u16::from(s);
                    p.0[3] = ((u16::from(p.0[3]) * keep + 127) / 255) as u8;
                    if p.0[3] == 0 {
                        p.0 = [0, 0, 0, 0];
                    }
                }
            }
        }
        let label = UiText::new("op_selection_clear");
        let pixels_arc = Arc::new(next);
        self.stack.active_layer_mut().pixels = Arc::clone(&pixels_arc);
        self.push_step(
            EditOp::LiveStrokeCommit {
                label,
                pixels: pixels_arc,
                mask: None,
            },
            ApplyTarget::ActiveLayer,
            before,
        );
        self.current = composite_stack(&self.stack);
        self.pixel_revision += 1;
        Ok(())
    }

    pub fn duplicate_selection_to_layer(&mut self) -> Result<(), ImageError> {
        if !self.stack.selection.has_selection() {
            return Ok(());
        }
        let active = self.stack.active_layer();
        let (lw, lh) = active.dimensions();
        let ox = active.offset_x;
        let oy = active.offset_y;
        let mut next = (*active.pixels).clone();
        for y in 0..lh {
            for x in 0..lw {
                let s = self.stack.selection.alpha_at(ox + x as i32, oy + y as i32);
                let p = next.get_pixel_mut(x, y);
                p.0[3] = ((u16::from(p.0[3]) * u16::from(s) + 127) / 255) as u8;
                if p.0[3] == 0 {
                    p.0 = [0, 0, 0, 0];
                }
            }
        }
        let op = LayerOperation::AddImage {
            name: "Selection".to_owned(),
            offset_x: ox,
            offset_y: oy,
            pixels: Arc::new(next),
        };
        self.apply_with_target(EditOp::Layer(op), ApplyTarget::ActiveLayer)?;
        Ok(())
    }

    pub fn crop_to_selection(&mut self) -> Result<bool, ImageError> {
        let Some([bx, by, bw, bh]) = self.stack.selection.bounds else {
            return Ok(false);
        };
        self.apply_with_target(
            EditOp::Crop {
                x: bx as i32,
                y: by as i32,
                width: bw,
                height: bh,
                fill: None,
                fill_alpha: 255,
            },
            ApplyTarget::WholeImage,
        )
    }
}

fn render_drawing_op(
    src: &RgbaImage,
    op: &EditOp,
    offset_x: i32,
    offset_y: i32,
    selection: Option<&SelectionMask>,
    is_mask: bool,
) -> RgbaImage {
    let to_gray = |rgb: [u8; 3]| -> [u8; 3] {
        let g = drawing::rgb_luminance(rgb);
        [g, g, g]
    };
    let ox = offset_x as f32;
    let oy = offset_y as f32;
    match op {
        EditOp::DrawShape {
            kind,
            points,
            nodes,
            fill_mode,
            closed,
            color,
            radius,
            opacity,
            brush,
        } => {
            let local_pts: Vec<[f32; 2]> = points.iter().map(|p| [p[0] - ox, p[1] - oy]).collect();
            let local_nodes: Vec<VectorPathNode> = nodes
                .iter()
                .map(|n| VectorPathNode {
                    anchor: [n.anchor[0] - ox, n.anchor[1] - oy],
                    handle_in: n.handle_in.map(|h| [h[0] - ox, h[1] - oy]),
                    handle_out: n.handle_out.map(|h| [h[0] - ox, h[1] - oy]),
                })
                .collect();
            let c = if is_mask { to_gray(*color) } else { *color };
            drawing::draw_shape(
                src,
                *kind,
                &local_pts,
                &local_nodes,
                *fill_mode,
                *closed,
                c,
                *radius,
                *opacity,
                *brush,
            )
        }
        EditOp::FloodFill {
            x,
            y,
            tolerance,
            contiguous,
            color,
            opacity,
        } => {
            let lx = (*x as i64) - i64::from(offset_x);
            let ly = (*y as i64) - i64::from(offset_y);
            let (w, h) = src.dimensions();
            if lx < 0 || ly < 0 || lx >= i64::from(w) || ly >= i64::from(h) {
                return src.clone();
            }
            let c = if is_mask { to_gray(*color) } else { *color };
            drawing::flood_fill(
                src,
                lx as u32,
                ly as u32,
                c,
                *opacity,
                *tolerance,
                *contiguous,
                selection.map(|s| (s, offset_x, offset_y)),
            )
        }
        EditOp::DrawGradient {
            start,
            end,
            kind,
            color_mode,
            fg_color,
            bg_color,
            opacity,
        } => {
            let fg = if is_mask {
                to_gray(*fg_color)
            } else {
                *fg_color
            };
            let bg = if is_mask {
                to_gray(*bg_color)
            } else {
                *bg_color
            };
            drawing::draw_gradient(
                src,
                [start[0] - ox, start[1] - oy],
                [end[0] - ox, end[1] - oy],
                *kind,
                *color_mode,
                fg,
                bg,
                *opacity,
            )
        }
        _ => src.clone(),
    }
}

fn clip_rgba_by_selection(
    before: &RgbaImage,
    after: &mut RgbaImage,
    offset_x: i32,
    offset_y: i32,
    sel: &SelectionMask,
) {
    let (w, h) = before.dimensions();
    if after.dimensions() != (w, h) {
        return;
    }
    for y in 0..h {
        let cy = offset_y + y as i32;
        for x in 0..w {
            let s = sel.alpha_at(offset_x + x as i32, cy);
            if s == 255 {
                continue;
            }
            let b = before.get_pixel(x, y).0;
            if s == 0 {
                after.put_pixel(x, y, Rgba(b));
            } else {
                let a = after.get_pixel(x, y).0;
                let t = s as u16;
                let inv = 255 - t;
                let blended = [
                    ((b[0] as u16 * inv + a[0] as u16 * t + 127) / 255) as u8,
                    ((b[1] as u16 * inv + a[1] as u16 * t + 127) / 255) as u8,
                    ((b[2] as u16 * inv + a[2] as u16 * t + 127) / 255) as u8,
                    ((b[3] as u16 * inv + a[3] as u16 * t + 127) / 255) as u8,
                ];
                after.put_pixel(x, y, Rgba(blended));
            }
        }
    }
}

fn clip_mask_by_selection(
    before: &[u8],
    after: &mut [u8],
    lw: u32,
    lh: u32,
    offset_x: i32,
    offset_y: i32,
    sel: &SelectionMask,
) {
    let len = (lw as usize).saturating_mul(lh as usize);
    if before.len() < len || after.len() < len {
        return;
    }
    for y in 0..lh {
        let cy = offset_y + y as i32;
        for x in 0..lw {
            let s = sel.alpha_at(offset_x + x as i32, cy);
            if s == 255 {
                continue;
            }
            let idx = (y * lw + x) as usize;
            if s == 0 {
                after[idx] = before[idx];
            } else {
                let t = s as u16;
                let inv = 255 - t;
                after[idx] = ((before[idx] as u16 * inv + after[idx] as u16 * t + 127) / 255) as u8;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_mask_stroke(
    existing: &[u8],
    lw: u32,
    lh: u32,
    local_pts: &[[f64; 2]],
    color: [u8; 3],
    radius: f32,
    opacity: f32,
    erase: bool,
    brush: BrushSettings,
) -> Vec<u8> {
    let mut mask_img = RgbaImage::new(lw, lh);
    for y in 0..lh {
        let row = (y as usize) * (lw as usize);
        for x in 0..lw {
            let v = existing.get(row + x as usize).copied().unwrap_or(255);
            mask_img.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    let gray = if erase {
        0u8
    } else {
        (0.299 * f64::from(color[0]) + 0.587 * f64::from(color[1]) + 0.114 * f64::from(color[2]))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    let painted = pixels::paint_stroke_with_brush(
        &mask_img,
        local_pts,
        [gray, gray, gray],
        radius,
        opacity,
        false,
        brush,
    );
    let mut out = vec![0u8; (lw as usize) * (lh as usize)];
    for y in 0..lh {
        let row = (y as usize) * (lw as usize);
        for x in 0..lw {
            out[row + x as usize] = painted.get_pixel(x, y).0[0];
        }
    }
    out
}

fn transform_layer_geometry(layer: &mut Layer, op: &EditOp) -> Result<(), ImageError> {
    let is_geom = matches!(
        op,
        EditOp::Resize { .. }
            | EditOp::Crop { .. }
            | EditOp::Rotate { .. }
            | EditOp::Flip { .. }
            | EditOp::CanvasSize { .. }
            | EditOp::RotateAngle { .. }
    );
    if !is_geom {
        return Ok(());
    }
    let (lw, lh) = layer.dimensions();
    if let Some(orig) = &layer.original {
        let next_orig = render_single_op(orig, None, op)?;
        layer.original = Some(Arc::new(next_orig));
    }
    if let Some(mask) = &layer.mask
        && mask.len() == (lw as usize) * (lh as usize)
    {
        let mut mask_img = RgbaImage::new(lw, lh);
        for y in 0..lh {
            let row = (y as usize) * (lw as usize);
            for x in 0..lw {
                let v = mask[row + x as usize];
                mask_img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }
        // Keep newly revealed canvas areas unmasked (white = 255) on grow/pad/rotate_angle.
        let mask_op = match op {
            EditOp::Resize {
                width,
                height,
                filter,
                mode,
                fill,
                ..
            } => EditOp::Resize {
                width: *width,
                height: *height,
                filter: *filter,
                mode: *mode,
                fill: fill.map(|_| [255, 255, 255]),
                fill_alpha: 255,
            },
            EditOp::Crop {
                x,
                y,
                width,
                height,
                fill,
                ..
            } => EditOp::Crop {
                x: *x,
                y: *y,
                width: *width,
                height: *height,
                fill: fill.map(|_| [255, 255, 255]),
                fill_alpha: 255,
            },
            EditOp::CanvasSize {
                width,
                height,
                anchor,
                ..
            } => EditOp::CanvasSize {
                width: *width,
                height: *height,
                anchor: *anchor,
                fill: Some([255, 255, 255]),
                fill_alpha: 255,
            },
            EditOp::RotateAngle { degrees, .. } => EditOp::RotateAngle {
                degrees: *degrees,
                fill: Some([255, 255, 255]),
                fill_alpha: 255,
            },
            other => other.clone(),
        };
        let next_img = render_single_op(&mask_img, None, &mask_op)?;
        let (nw, nh) = next_img.dimensions();
        let mut out_mask = vec![255u8; (nw as usize) * (nh as usize)];
        for y in 0..nh {
            let row = (y as usize) * (nw as usize);
            for x in 0..nw {
                out_mask[row + x as usize] = next_img.get_pixel(x, y).0[0];
            }
        }
        layer.mask = Some(Arc::new(out_mask));
        if layer.mask_offset_x != 0 || layer.mask_offset_y != 0 {
            let (mox, moy) = (layer.mask_offset_x, layer.mask_offset_y);
            match op {
                EditOp::Flip {
                    axis: FlipAxis::Horizontal,
                } => layer.mask_offset_x = -mox,
                EditOp::Flip {
                    axis: FlipAxis::Vertical,
                } => layer.mask_offset_y = -moy,
                EditOp::Rotate { degrees } => {
                    let (nx, ny) = match degrees.rem_euclid(360) {
                        90 => (-moy, mox),
                        180 => (-mox, -moy),
                        270 => (moy, -mox),
                        _ => (mox, moy),
                    };
                    layer.mask_offset_x = nx;
                    layer.mask_offset_y = ny;
                }
                EditOp::Resize {
                    width,
                    height,
                    mode: ResizeMode::Stretch,
                    ..
                } => {
                    let sx = f64::from(*width) / f64::from(lw.max(1));
                    let sy = f64::from(*height) / f64::from(lh.max(1));
                    layer.mask_offset_x = (f64::from(mox) * sx).round() as i32;
                    layer.mask_offset_y = (f64::from(moy) * sy).round() as i32;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub fn validate_dimensions(width: u32, height: u32) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidEdit(
            "dimensions must be greater than 0".into(),
        ));
    }
    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(ImageError::InvalidEdit(format!(
            "{width}x{height} exceeds max dimension {MAX_IMAGE_DIMENSION}"
        )));
    }
    if u64::from(width) * u64::from(height) > MAX_DOCUMENT_PIXELS {
        return Err(ImageError::InvalidEdit(format!(
            "{width}x{height} exceeds max pixel count {MAX_DOCUMENT_PIXELS}"
        )));
    }
    Ok(())
}

fn validate_paint(points: &[[f64; 2]], radius: f32, opacity: f32) -> Result<(), ImageError> {
    if points.is_empty() {
        return Err(ImageError::InvalidEdit("paint stroke is empty".into()));
    }
    if points.iter().any(|[x, y]| !x.is_finite() || !y.is_finite()) {
        return Err(ImageError::InvalidEdit(
            "paint stroke contains non-finite coordinates".into(),
        ));
    }
    if !radius.is_finite() || !(0.5..=4096.0).contains(&radius) {
        return Err(ImageError::InvalidEdit(format!(
            "brush radius must be in 0.5..=4096, got {radius}"
        )));
    }
    if !opacity.is_finite() || opacity <= 0.0 || opacity > 1.0 {
        return Err(ImageError::InvalidEdit(format!(
            "brush opacity must be in (0, 1], got {opacity}"
        )));
    }
    Ok(())
}

fn validate_liquify(points: &[[f64; 2]], radius: f32, strength: f32) -> Result<(), ImageError> {
    if points.is_empty() {
        return Err(ImageError::InvalidEdit("liquify stroke is empty".into()));
    }
    if points.iter().any(|[x, y]| !x.is_finite() || !y.is_finite()) {
        return Err(ImageError::InvalidEdit(
            "liquify stroke contains non-finite coordinates".into(),
        ));
    }
    if !radius.is_finite() || !(4.0..=400.0).contains(&radius) {
        return Err(ImageError::InvalidEdit(format!(
            "liquify radius must be in 4..=400, got {radius}"
        )));
    }
    if !strength.is_finite() || !(0.05..=1.0).contains(&strength) {
        return Err(ImageError::InvalidEdit(format!(
            "liquify strength must be in 0.05..=1.0, got {strength}"
        )));
    }
    Ok(())
}

pub fn render_single_op(
    src: &RgbaImage,
    baseline: Option<&RgbaImage>,
    op: &EditOp,
) -> Result<RgbaImage, ImageError> {
    use image::imageops;
    let out = match op {
        EditOp::Resize {
            width,
            height,
            filter,
            mode,
            fill,
            fill_alpha,
        } => pixels::resize_with_mode(
            src,
            *width,
            *height,
            *filter,
            *mode,
            fill.unwrap_or([0, 0, 0]),
            *fill_alpha,
        ),
        EditOp::Crop {
            x,
            y,
            width,
            height,
            fill,
            fill_alpha,
        } => match fill {
            Some(rgb) => pixels::grow_crop(src, *x, *y, *width, *height, *rgb, *fill_alpha),
            None => imageops::crop_imm(src, *x as u32, *y as u32, *width, *height).to_image(),
        },
        EditOp::Rotate { degrees } => match degrees.rem_euclid(360) {
            90 => imageops::rotate90(src),
            180 => imageops::rotate180(src),
            270 => imageops::rotate270(src),
            _ => src.clone(),
        },
        EditOp::Flip { axis } => match axis {
            FlipAxis::Horizontal => imageops::flip_horizontal(src),
            FlipAxis::Vertical => imageops::flip_vertical(src),
        },
        EditOp::AdjustColor(adj) => pixels::adjust_color_and_light(src, adj),
        EditOp::CanvasSize {
            width,
            height,
            anchor,
            fill,
            fill_alpha,
        } => pixels::canvas_size(
            src,
            *width,
            *height,
            *anchor,
            fill.unwrap_or([0, 0, 0]),
            *fill_alpha,
        ),
        EditOp::RotateAngle {
            degrees,
            fill,
            fill_alpha,
        } => pixels::rotate_angle(src, *degrees, fill.unwrap_or([0, 0, 0]), *fill_alpha),
        EditOp::AdjustHsv(adj) => pixels::adjust_hsv(src, adj),
        EditOp::ColorBalance(adj) => pixels::color_balance(src, adj),
        EditOp::AdjustCurves(curves) => pixels::adjust_curves(src, curves),
        EditOp::Paint {
            points,
            color,
            radius,
            opacity,
            erase,
            brush,
        } => {
            pixels::paint_stroke_with_brush(src, points, *color, *radius, *opacity, *erase, *brush)
        }
        EditOp::Liquify {
            points,
            mode,
            radius,
            strength,
        } => liquify::liquify_stroke(src, baseline, points, *mode, *radius, *strength),
        EditOp::LiveStrokeCommit { pixels, .. } => (**pixels).clone(),
        EditOp::DrawShape { .. } | EditOp::FloodFill { .. } | EditOp::DrawGradient { .. } => {
            render_drawing_op(src, op, 0, 0, None, false)
        }
        EditOp::Layer(_) => src.clone(),
    };
    Ok(out)
}

fn fill_expanded_bottom_layer(stack: &mut LayerStack, fill: Option<[u8; 3]>, fill_alpha: u8) {
    if fill_alpha == 0 {
        return;
    }
    let Some(rgb) = fill else {
        return;
    };
    if stack.layers.is_empty() {
        return;
    }
    let mut bg = RgbaImage::from_pixel(
        stack.canvas_width.max(1),
        stack.canvas_height.max(1),
        Rgba([rgb[0], rgb[1], rgb[2], fill_alpha]),
    );
    blend_layer_onto(&mut bg, &stack.layers[0]);
    stack.layers[0].pixels = Arc::new(bg);
    stack.layers[0].offset_x = 0;
    stack.layers[0].offset_y = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use photon_protocol::BlendMode;

    #[test]
    fn non_destructive_undo_redo_reset() {
        let img = RgbaImage::from_pixel(100, 50, Rgba([120, 80, 40, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("/tmp/fake.png"), img);
        assert!(!doc.modified());

        doc.apply(EditOp::Rotate { degrees: 90 }).unwrap();
        assert_eq!(doc.dimensions(), (50, 100));
        assert!(doc.modified());

        doc.apply(EditOp::Crop {
            x: 0,
            y: 0,
            width: 20,
            height: 30,
            fill: None,
            fill_alpha: 255,
        })
        .unwrap();
        assert_eq!(doc.dimensions(), (20, 30));

        assert!(doc.undo().unwrap());
        assert_eq!(doc.dimensions(), (50, 100));
        assert!(doc.redo().unwrap());
        assert_eq!(doc.dimensions(), (20, 30));
        assert!(doc.reset());
        assert_eq!(doc.dimensions(), (100, 50));
        assert!(!doc.modified());
    }

    #[test]
    fn layer_stack_blend_reorder_merge_flatten_and_background() {
        let img = RgbaImage::from_pixel(10, 10, Rgba([100, 100, 100, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("/tmp/layers.png"), img);
        assert_eq!(doc.stack().layers.len(), 1);

        assert!(
            doc.apply_layer_op(LayerOperation::AddEmpty {
                name: "Overlay".into()
            })
            .unwrap()
        );
        assert_eq!(doc.stack().layers.len(), 2);
        let top_id = doc.stack().active_layer_id;

        doc.apply(EditOp::Paint {
            points: vec![[5.0, 5.0]],
            color: [200, 50, 50],
            radius: 3.0,
            opacity: 1.0,
            erase: false,
            brush: BrushSettings::default(),
        })
        .unwrap();

        doc.apply_layer_op(LayerOperation::SetProps {
            id: top_id,
            name: None,
            visible: None,
            locked: None,
            opacity: Some(0.5),
            blend_mode: Some(BlendMode::Multiply),
            offset_x: None,
            offset_y: None,
        })
        .unwrap();
        assert_ne!(doc.working_pixels().get_pixel(5, 5).0, [100, 100, 100, 255]);

        assert!(
            doc.apply_layer_op(LayerOperation::MergeDown { id: top_id })
                .unwrap()
        );
        assert_eq!(doc.stack().layers.len(), 1);

        assert!(
            doc.apply_layer_op(LayerOperation::SetCanvasBackground {
                color: Some([10, 20, 30])
            })
            .unwrap()
        );
        assert_eq!(doc.stack().background, Some([10, 20, 30]));
        assert!(doc.apply_layer_op(LayerOperation::Flatten).unwrap());
        assert_eq!(doc.stack().background, None);
    }

    #[test]
    fn layer_mask_paint_invert_disable_apply_and_delete() {
        let img = RgbaImage::from_pixel(16, 16, Rgba([200, 100, 50, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("/tmp/mask.png"), img);
        let id = doc.stack().active_layer_id;

        assert!(
            doc.apply_layer_op(LayerOperation::AddMask {
                id,
                reveal_all: true
            })
            .unwrap()
        );
        assert!(doc.stack().active_layer().mask.is_some());
        assert!(doc.stack().active_layer().editing_mask);

        // Erase on mask hides center pixel
        doc.apply(EditOp::Paint {
            points: vec![[8.0, 8.0]],
            color: [255, 255, 255],
            radius: 2.0,
            opacity: 1.0,
            erase: true,
            brush: BrushSettings::default(),
        })
        .unwrap();
        assert_eq!(doc.working_pixels().get_pixel(8, 8).0[3], 0);

        // Disable mask restores visibility temporarily
        assert!(
            doc.apply_layer_op(LayerOperation::SetMaskEnabled { id, enabled: false })
                .unwrap()
        );
        assert_eq!(doc.working_pixels().get_pixel(8, 8).0[3], 255);

        // Re-enable, invert, and apply mask permanently
        assert!(
            doc.apply_layer_op(LayerOperation::SetMaskEnabled { id, enabled: true })
                .unwrap()
        );
        assert!(
            doc.apply_layer_op(LayerOperation::InvertMask { id })
                .unwrap()
        );
        assert_eq!(doc.working_pixels().get_pixel(8, 8).0[3], 255);
        assert_eq!(doc.working_pixels().get_pixel(0, 0).0[3], 0);

        assert!(
            doc.apply_layer_op(LayerOperation::ApplyMask { id })
                .unwrap()
        );
        assert!(doc.stack().active_layer().mask.is_none());
        assert_eq!(doc.working_pixels().get_pixel(0, 0).0[3], 0);
    }

    #[test]
    fn unlinked_mask_offset_paint_flip_and_editing_toggle_preserves_redo() {
        let img = RgbaImage::from_pixel(16, 16, Rgba([180, 90, 45, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("/tmp/mask_unlinked.png"), img);
        let id = doc.stack().active_layer_id;

        doc.apply_layer_op(LayerOperation::AddMask {
            id,
            reveal_all: true,
        })
        .unwrap();
        doc.apply_layer_op(LayerOperation::SetMaskLinked { id, linked: false })
            .unwrap();
        // Shift unlinked mask right by 4 px
        doc.apply_layer_op(LayerOperation::SetProps {
            id,
            name: None,
            visible: None,
            locked: None,
            opacity: None,
            blend_mode: None,
            offset_x: Some(4),
            offset_y: None,
        })
        .unwrap();
        assert_eq!(doc.stack().active_layer().mask_offset_x, 4);

        // Live stroke at canvas (10, 8) -> mask local (6, 8)
        doc.live_stroke_begin(
            LiveStrokeKind::Paint {
                color: [255, 255, 255],
                radius: 2.0,
                opacity: 1.0,
                erase: true,
                brush: BrushSettings::default(),
            },
            10.0,
            8.0,
        )
        .unwrap();
        doc.live_stroke_end(true).unwrap();
        let mask_val_at_6_8 = doc.stack().active_layer().mask.as_ref().unwrap()[8 * 16 + 6];
        assert_eq!(mask_val_at_6_8, 0);
        assert_eq!(doc.working_pixels().get_pixel(10, 8).0[3], 0);

        // Undo stroke, toggle SetEditingMask off & on -> redo_stack MUST still work!
        assert!(doc.undo().unwrap());
        assert!(doc.can_redo());
        assert!(
            doc.apply_layer_op(LayerOperation::SetEditingMask { id, editing: false })
                .unwrap()
        );
        assert!(doc.can_redo());
        assert!(
            doc.apply_layer_op(LayerOperation::SetEditingMask { id, editing: true })
                .unwrap()
        );
        assert!(doc.redo().unwrap());
        assert_eq!(doc.working_pixels().get_pixel(10, 8).0[3], 0);

        // Flip horizontal preserves alignment of shifted mask offset (-4)
        doc.apply(EditOp::Flip {
            axis: FlipAxis::Horizontal,
        })
        .unwrap();
        assert_eq!(doc.stack().active_layer().mask_offset_x, -4);
        assert_eq!(doc.working_pixels().get_pixel(5, 8).0[3], 0);
    }

    #[test]
    fn filter_and_adjustment_layers_composite_with_mask_non_destructively() {
        let img = RgbaImage::from_pixel(8, 8, Rgba([100, 120, 140, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("/tmp/effect_layer.png"), img);

        doc.apply_layer_op(LayerOperation::AddEffect {
            name: String::new(),
            effect: photon_protocol::LayerEffect::Invert,
        })
        .unwrap();
        assert_eq!(doc.stack().layers.len(), 2);
        assert_eq!(doc.working_pixels().get_pixel(0, 0).0, [155, 135, 115, 255]);

        // Add mask (reveal all), erase half via mask -> masked region reverts to original pixels!
        let eid = doc.stack().active_layer_id;
        doc.apply_layer_op(LayerOperation::AddMask {
            id: eid,
            reveal_all: true,
        })
        .unwrap();
        doc.apply(EditOp::Paint {
            points: vec![[0.0, 0.0]],
            color: [0, 0, 0],
            radius: 1.5,
            opacity: 1.0,
            erase: true,
            brush: BrushSettings::default(),
        })
        .unwrap();
        assert_eq!(doc.working_pixels().get_pixel(0, 0).0, [100, 120, 140, 255]);
        assert_eq!(doc.working_pixels().get_pixel(7, 7).0, [155, 135, 115, 255]);
    }

    #[test]
    fn liquify_push_and_reconstruct_and_gamma() {
        let mut img = RgbaImage::from_pixel(24, 24, Rgba([100, 100, 100, 255]));
        for y in 8..16 {
            for x in 8..16 {
                img.put_pixel(x, y, Rgba([240, 40, 40, 255]));
            }
        }
        let mut doc = EditorDocument::from_rgba(PathBuf::from("/tmp/liq.png"), img);

        doc.apply(EditOp::Liquify {
            points: vec![[12.0, 12.0], [18.0, 12.0]],
            mode: LiquifyMode::Push,
            radius: 6.0,
            strength: 0.9,
        })
        .unwrap();
        let pushed = doc.working_pixels().get_pixel(16, 12).0;
        assert!(pushed[0] > 120);

        doc.apply(EditOp::Liquify {
            points: vec![[16.0, 12.0]],
            mode: LiquifyMode::Reconstruct,
            radius: 8.0,
            strength: 1.0,
        })
        .unwrap();
        assert_eq!(
            doc.working_pixels().get_pixel(16, 12).0,
            [100, 100, 100, 255]
        );

        // Gamma adjustment changes midtone luminance
        assert!(
            doc.apply(EditOp::AdjustColor(ColorAdjustments {
                gamma: 1.8,
                ..ColorAdjustments::default()
            }))
            .unwrap()
        );
        assert!(doc.working_pixels().get_pixel(0, 0).0[0] > 100);
    }

    #[test]
    fn canvas_size_rotate_angle_grow_crop_and_resize_modes() {
        let img = RgbaImage::from_pixel(20, 10, Rgba([80, 120, 160, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("/tmp/geom.png"), img);

        doc.apply(EditOp::CanvasSize {
            width: 40,
            height: 20,
            anchor: CanvasAnchor::Center,
            fill: Some([255, 0, 0]),
            fill_alpha: 255,
        })
        .unwrap();
        assert_eq!(doc.dimensions(), (40, 20));
        assert_eq!(doc.working_pixels().get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(
            doc.working_pixels().get_pixel(20, 10).0,
            [80, 120, 160, 255]
        );

        doc.apply(EditOp::RotateAngle {
            degrees: 45.0,
            fill: None,
            fill_alpha: 0,
        })
        .unwrap();
        let (rw, rh) = doc.dimensions();
        assert!(rw > 40 && rh > 20);

        doc.apply(EditOp::Resize {
            width: 30,
            height: 30,
            filter: ResizeFilter::Triangle,
            mode: ResizeMode::Pad,
            fill: Some([0, 255, 0]),
            fill_alpha: 255,
        })
        .unwrap();
        assert_eq!(doc.dimensions(), (30, 30));

        assert!(
            doc.apply(EditOp::Crop {
                x: -5,
                y: -5,
                width: 20,
                height: 20,
                fill: Some([0, 0, 255]),
                fill_alpha: 255,
            })
            .is_ok()
        );
        assert!(
            doc.apply(EditOp::Crop {
                x: 100,
                y: 100,
                width: 10,
                height: 10,
                fill: Some([0, 0, 255]),
                fill_alpha: 255,
            })
            .is_err()
        );
    }

    #[test]
    fn color_light_curves_hsv_balance_and_live_stroke() {
        let img = RgbaImage::from_pixel(16, 16, Rgba([100, 140, 180, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("/tmp/color.png"), img);

        assert!(
            doc.apply(EditOp::AdjustColor(ColorAdjustments {
                exposure: 0.5,
                highlights: -0.2,
                shadows: 0.3,
                brightness: 0.05,
                contrast: 0.1,
                saturation: 0.2,
                gamma: 1.0,
            }))
            .unwrap()
        );

        assert!(
            doc.apply(EditOp::AdjustHsv(HsvAdjustment {
                hue: 30.0,
                saturation: 0.1,
                value: 0.05,
            }))
            .unwrap()
        );

        assert!(
            doc.apply(EditOp::ColorBalance(ColorBalanceAdjustment {
                shadows: [0.1, 0.0, -0.1],
                midtones: [0.0, 0.1, 0.0],
                highlights: [-0.1, 0.0, 0.1],
                preserve_luminosity: true,
            }))
            .unwrap()
        );

        assert!(
            doc.apply(EditOp::AdjustCurves(CurvesAdjustment {
                rgb: vec![[0.0, 0.0], [0.5, 0.65], [1.0, 1.0]],
                red: vec![],
                green: vec![],
                blue: vec![],
            }))
            .unwrap()
        );

        assert!(
            doc.live_stroke_begin(
                LiveStrokeKind::Paint {
                    color: [255, 0, 0],
                    radius: 2.0,
                    opacity: 1.0,
                    erase: false,
                    brush: BrushSettings {
                        tip: photon_protocol::BrushTip::Chalk,
                        ..BrushSettings::default()
                    },
                },
                4.0,
                4.0,
            )
            .unwrap()
        );
        assert!(doc.live_stroke_step(&[[8.0, 8.0]]).unwrap());
        assert!(doc.live_stroke_end(true).unwrap());

        assert_eq!(doc.undo_len(), 5);
        assert!(doc.jump_history(2).unwrap());
        assert_eq!(doc.undo_len(), 2);
        assert_eq!(doc.redo_labels().len(), 3);
    }

    #[test]
    fn history_bounded_and_save_bak_and_export() {
        let dir = std::env::temp_dir().join(format!("photon-img-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("pic.jpg");
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(40, 20, Rgba([120, 60, 30, 255])))
            .to_rgb8()
            .save(&src_path)
            .unwrap();

        let mut doc = EditorDocument::open(src_path.clone()).unwrap();
        for _ in 0..35 {
            doc.apply(EditOp::Flip {
                axis: FlipAxis::Horizontal,
            })
            .unwrap();
        }
        assert_eq!(doc.undo_len(), MAX_HISTORY_STEPS);
        assert!(doc.history_trimmed);

        let saved = doc.save_to(None).unwrap();
        assert_eq!(saved, src_path);
        assert!(dir.join("pic.jpg.bak").exists());
        assert!(!doc.modified());

        assert!(doc.export_to(&src_path, Some(85), true, Some(20)).is_err());

        let export_path = dir.join("pic_export.jpg");
        doc.export_to(&export_path, Some(85), true, Some(20))
            .unwrap();
        assert_eq!(crate::probe_dimensions(&export_path), Some((20, 10)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drawing_ops_clip_to_selection_support_masks_and_undo_redo() {
        let base = RgbaImage::from_pixel(24, 24, Rgba([10, 10, 10, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("test.png"), base);

        doc.select_shape(
            &SelectionShape::Rect {
                x: 8.0,
                y: 8.0,
                width: 8.0,
                height: 8.0,
                feather: 0.0,
                anti_alias: false,
            },
            SelectionBooleanOp::Replace,
        );
        let changed = doc
            .apply_with_target(
                EditOp::DrawShape {
                    kind: DrawShapeKind::Rect,
                    points: vec![[0.0, 0.0], [24.0, 24.0]],
                    nodes: Vec::new(),
                    fill_mode: ShapeFillMode::Fill,
                    closed: true,
                    color: [220, 40, 40],
                    radius: 2.0,
                    opacity: 1.0,
                    brush: BrushSettings::default(),
                },
                ApplyTarget::ActiveLayer,
            )
            .unwrap();
        assert!(changed);
        assert_eq!(doc.working_pixels().get_pixel(12, 12).0, [220, 40, 40, 255]);
        assert_eq!(doc.working_pixels().get_pixel(2, 2).0, [10, 10, 10, 255]);

        assert!(doc.undo().unwrap());
        assert_eq!(doc.working_pixels().get_pixel(12, 12).0, [10, 10, 10, 255]);
        assert!(doc.redo().unwrap());
        assert_eq!(doc.working_pixels().get_pixel(12, 12).0, [220, 40, 40, 255]);

        doc.deselect();
        assert!(doc.toggle_quick_mask());
        doc.apply_with_target(
            EditOp::FloodFill {
                x: 0,
                y: 0,
                tolerance: 0,
                contiguous: true,
                color: [255, 255, 255],
                opacity: 1.0,
            },
            ApplyTarget::ActiveLayer,
        )
        .unwrap();
        assert!(doc.selection().has_selection());
        assert!(doc.undo().unwrap());
        assert!(!doc.selection().has_selection());
    }
}
