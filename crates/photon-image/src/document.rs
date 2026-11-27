//! Non-destructive editor document, edit operations, bounded CoW history,
//! live brush/eraser/liquify stroke sessions, and mask-aware painting.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{Rgba, RgbaImage};
use photon_protocol::{
    ApplyTarget, BlendMode, BrushSettings, CanvasAnchor, ColorAdjustments, ColorBalanceAdjustment,
    CurvesAdjustment, DrawShapeKind, FlipAxis, GradientColorMode, GradientKind, GradientRepeat,
    GradientStop, HealingMode, HsvAdjustment, LayerTransformSpec, LiquifyMode, LiveStrokeKind,
    ResizeFilter, ResizeMode, SelectionBooleanOp, SelectionModifyOp, SelectionShape, ShapeFillMode,
    UiText, VectorPathNode,
};

use crate::{
    ImageError, Layer, LayerOperation, LayerStack, MAX_DOCUMENT_PIXELS, MAX_HISTORY_STEPS,
    MAX_IMAGE_DIMENSION, MAX_SNAPSHOT_BYTES, SelectionMask, blend_layer_onto, codec,
    composite_stack, decode_rgba, drawing, healing, liquify, pixels, rasterize_shape,
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
        selection: Option<SelectionMask>,
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
        stops: Vec<GradientStop>,
        opacity: f32,
        repeat: GradientRepeat,
        reverse: bool,
        dither: bool,
        blend_mode: BlendMode,
    },
    Healing {
        points: Vec<[f64; 2]>,
        mode: HealingMode,
        radius: f32,
        strength: f32,
        source: Option<[f64; 2]>,
        brush: BrushSettings,
    },
    Denoise {
        strength: f32,
        detail_preserve: f32,
    },
    TransformLayer {
        layer_id: Option<u64>,
        transform: LayerTransformSpec,
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
            Self::Healing { .. } => UiText::new("op_healing"),
            Self::Denoise { .. } => UiText::new("op_denoise"),
            Self::TransformLayer { .. } => UiText::new("op_transform_layer"),
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
    begin_point: [f64; 2],
    last_point: [f64; 2],
    editing_mask: bool,
    quick_mask: bool,
    unclipped_pixels: Option<RgbaImage>,
    unclipped_mask: Option<Vec<u8>>,
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
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("ora") => {
                let stack = crate::ora::decode_ora_stack(&path)?;
                Ok(Self::from_stack(path, stack))
            }
            Some("psd") => {
                let stack = codec::decode_psd_stack(&path)?;
                Ok(Self::from_stack(path, stack))
            }
            _ => {
                let rgba = decode_rgba(&path)?;
                Ok(Self::from_rgba(path, rgba))
            }
        }
    }

    pub fn from_stack(path: PathBuf, stack: LayerStack) -> Self {
        let composite = composite_stack(&stack);
        Self {
            save_path: path.clone(),
            source_path: path,
            original: Arc::clone(&composite),
            original_stack: stack.clone(),
            current: composite,
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

    /// Create a new blank single-layer document filled with `background_rgba`.
    pub fn new_blank(
        width: u32,
        height: u32,
        background_rgba: [u8; 4],
    ) -> Result<Self, ImageError> {
        crate::validate_dimensions(width, height)?;
        let img = RgbaImage::from_pixel(width, height, Rgba(background_rgba));
        let mut doc = Self::from_rgba(PathBuf::from("untitled.png"), img);
        if let Some(layer) = doc.stack.layers.first_mut() {
            layer.name = "Background".to_owned();
        }
        doc.original_stack = doc.stack.clone();
        Ok(doc)
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
        self.apply_with_selection(op, target, true)
    }

    pub fn apply_with_selection(
        &mut self,
        op: EditOp,
        target: ApplyTarget,
        selection_only: bool,
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
        self.execute_op(&op, target, selection_only)?;
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
            LiveStrokeKind::Healing {
                radius,
                hardness,
                strength,
                ..
            } => {
                validate_healing(&[[x, y]], radius, hardness, strength)?;
            }
        }
        let is_paint = matches!(stroke, LiveStrokeKind::Paint { .. });
        let quick_mask = is_paint && self.stack.selection.quick_mask_active;
        let active = self.stack.active_layer();
        if !quick_mask {
            if active.locked {
                return Err(ImageError::coded("layer_locked"));
            }
            if !active.visible {
                return Err(ImageError::coded("layer_hidden"));
            }
        }
        let editing_mask = is_paint && !quick_mask && active.editing_mask && active.mask.is_some();
        let has_sel = !quick_mask && self.stack.selection.has_selection();
        let unclipped_pixels =
            (is_paint && !quick_mask && !editing_mask && has_sel).then(|| (*active.pixels).clone());
        let unclipped_mask = (is_paint && editing_mask && has_sel)
            .then(|| active.mask.as_deref().cloned())
            .flatten();
        let before_stack = self.stack.clone();
        self.live_stroke = Some(LiveStrokeSession {
            before_stack,
            stroke,
            begin_point: [x, y],
            last_point: [x, y],
            editing_mask,
            quick_mask,
            unclipped_pixels,
            unclipped_mask,
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
            if session.quick_mask {
                self.selection_revision += 1;
            }
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
            LiveStrokeKind::Healing { .. } => UiText::new("op_healing"),
        };
        let active = self.stack.active_layer();
        let op = EditOp::LiveStrokeCommit {
            label,
            pixels: Arc::clone(&active.pixels),
            mask: active.mask.as_ref().map(Arc::clone),
            selection: session.quick_mask.then(|| self.stack.selection.clone()),
        };
        self.push_step(op, ApplyTarget::ActiveLayer, session.before_stack);
        self.current = composite_stack(&self.stack);
        self.pixel_revision += 1;
        Ok(true)
    }

    fn apply_live_segment(&mut self, points: &[[f64; 2]]) {
        let Some(session) = self.live_stroke.as_mut() else {
            return;
        };
        let stroke = session.stroke;
        let begin_point = session.begin_point;
        let editing_mask = session.editing_mask;
        let quick_mask = session.quick_mask;
        let before_pixels = Arc::clone(&session.before_stack.active_layer().pixels);
        let before_mask = session.before_stack.active_layer().mask.clone();

        if quick_mask
            && let LiveStrokeKind::Paint {
                color,
                radius,
                opacity,
                erase,
                brush,
            } = stroke
        {
            let (cw, ch) = (self.stack.canvas_width, self.stack.canvas_height);
            let next_mask = paint_mask_stroke(
                &self.stack.selection.alpha,
                cw,
                ch,
                points,
                color,
                radius,
                opacity,
                erase,
                brush,
                None,
            );
            self.stack.selection.alpha = Arc::new(next_mask);
            self.stack.selection.recompute_bounds();
            self.selection_revision += 1;
            return;
        }

        let sel_snapshot = self
            .stack
            .selection
            .has_selection()
            .then(|| self.stack.selection.clone());
        let active = self.stack.active_layer_mut();
        let ox_i = active.offset_x;
        let oy_i = active.offset_y;
        let ox = f64::from(ox_i);
        let oy = f64::from(oy_i);
        let mx_i = active.offset_x + active.mask_offset_x;
        let my_i = active.offset_y + active.mask_offset_y;
        let mx_off = f64::from(mx_i);
        let my_off = f64::from(my_i);
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
                    if let (Some(unclipped), Some(sel), Some(orig_mask)) = (
                        session.unclipped_mask.as_mut(),
                        sel_snapshot.as_ref(),
                        before_mask.as_deref(),
                    ) {
                        *unclipped = paint_mask_stroke(
                            unclipped, lw, lh, &mask_pts, color, radius, opacity, erase, brush,
                            None,
                        );
                        let mut next_mask = (**mask_arc).clone();
                        if let Some((left, top, right, bottom)) =
                            stroke_local_bbox(&mask_pts, radius, brush, lw, lh)
                        {
                            clip_mask_by_selection_rect(
                                orig_mask,
                                unclipped,
                                &mut next_mask,
                                lw,
                                lh,
                                mx_i,
                                my_i,
                                sel,
                                left,
                                top,
                                right,
                                bottom,
                            );
                        }
                        active.mask = Some(Arc::new(next_mask));
                    } else {
                        let next_mask = paint_mask_stroke(
                            mask_arc,
                            lw,
                            lh,
                            &mask_pts,
                            color,
                            radius,
                            opacity,
                            erase,
                            brush,
                            sel_snapshot.as_ref().map(|s| (s, mx_i, my_i)),
                        );
                        active.mask = Some(Arc::new(next_mask));
                    }
                    return;
                }
                let local_pts: Vec<[f64; 2]> =
                    points.iter().map(|[x, y]| [x - ox, y - oy]).collect();
                if let (Some(unclipped), Some(sel)) =
                    (session.unclipped_pixels.as_mut(), sel_snapshot.as_ref())
                {
                    *unclipped = pixels::paint_stroke_with_brush_clipped(
                        unclipped, &local_pts, color, radius, opacity, erase, brush, None,
                    );
                    let mut next = (*active.pixels).clone();
                    let (lw, lh) = next.dimensions();
                    if let Some((left, top, right, bottom)) =
                        stroke_local_bbox(&local_pts, radius, brush, lw, lh)
                    {
                        clip_rgba_by_selection_rect(
                            &before_pixels,
                            unclipped,
                            &mut next,
                            ox_i,
                            oy_i,
                            sel,
                            left,
                            top,
                            right,
                            bottom,
                        );
                    }
                    if active.lock_alpha {
                        preserve_alpha_channel(&before_pixels, &mut next);
                    }
                    active.pixels = Arc::new(next);
                } else {
                    let mut painted = pixels::paint_stroke_with_brush_clipped(
                        &active.pixels,
                        &local_pts,
                        color,
                        radius,
                        opacity,
                        erase,
                        brush,
                        sel_snapshot.as_ref().map(|s| (s, ox_i, oy_i)),
                    );
                    if active.lock_alpha {
                        preserve_alpha_channel(&before_pixels, &mut painted);
                    }
                    active.pixels = Arc::new(painted);
                }
            }
            LiveStrokeKind::Liquify {
                mode,
                radius,
                strength,
            } => {
                let local_pts: Vec<[f64; 2]> =
                    points.iter().map(|[x, y]| [x - ox, y - oy]).collect();
                let mut warped = liquify::liquify_stroke(
                    &active.pixels,
                    active.original.as_deref(),
                    &local_pts,
                    mode,
                    radius,
                    strength,
                );
                if active.lock_alpha {
                    preserve_alpha_channel(&before_pixels, &mut warped);
                }
                active.pixels = Arc::new(warped);
            }
            LiveStrokeKind::Healing {
                mode,
                radius,
                hardness,
                strength,
                source,
            } => {
                let local_pts: Vec<[f64; 2]> =
                    points.iter().map(|[x, y]| [x - ox, y - oy]).collect();
                let seg_source = source.map(|[sx, sy]| {
                    let dx = points[0][0] - begin_point[0];
                    let dy = points[0][1] - begin_point[1];
                    [sx + dx - ox, sy + dy - oy]
                });
                let brush = BrushSettings {
                    hardness,
                    ..BrushSettings::default()
                };
                let sel_ref = sel_snapshot
                    .as_ref()
                    .map(|s| (s, active.offset_x, active.offset_y));
                let mut healed = healing::healing_stroke_with_reference(
                    &active.pixels,
                    &before_pixels,
                    &local_pts,
                    mode,
                    radius,
                    strength,
                    seg_source,
                    brush,
                    sel_ref,
                );
                if active.lock_alpha {
                    preserve_alpha_channel(&before_pixels, &mut healed);
                }
                active.pixels = Arc::new(healed);
            }
        }
    }

    pub fn undo(&mut self) -> Result<bool, ImageError> {
        let Some(step) = self.undo_stack.pop() else {
            return Ok(false);
        };
        let prev_sel = self.stack.selection.clone();
        self.stack = step.before;
        if selection_differs(&prev_sel, &self.stack.selection) {
            self.selection_revision += 1;
        }
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
            EditOp::LiveStrokeCommit {
                pixels,
                mask,
                selection,
                ..
            } => {
                if let Some(sel) = selection {
                    self.stack.selection = sel.clone();
                } else {
                    let active = self.stack.active_layer_mut();
                    active.pixels = Arc::clone(pixels);
                    if let Some(m) = mask {
                        active.mask = Some(Arc::clone(m));
                    }
                }
                Ok(())
            }
            _ => self.execute_op(&op, target, true),
        };
        if let Err(err) = res {
            self.redo_stack.push((op, target));
            return Err(err);
        }
        if selection_differs(&before.selection, &self.stack.selection) {
            self.selection_revision += 1;
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
        let saved = if codec::SaveFormat::from_path(&dest) == Some(codec::SaveFormat::Ora) {
            crate::ora::encode_ora_stack(&self.stack, &dest)?
        } else {
            codec::save_to(&self.current, &dest)?
        };
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
        if codec::SaveFormat::from_path(dest) == Some(codec::SaveFormat::Ora) && max_edge.is_none()
        {
            return crate::ora::encode_ora_stack(&self.stack, dest);
        }
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
            EditOp::Healing {
                points,
                radius,
                strength,
                brush,
                ..
            } => {
                validate_healing(points, *radius, brush.hardness, *strength)?;
                Ok(false)
            }
            EditOp::Denoise {
                strength,
                detail_preserve,
            } => {
                if !strength.is_finite() || !detail_preserve.is_finite() {
                    return Err(ImageError::InvalidEdit(
                        "denoise parameters must be finite".into(),
                    ));
                }
                Ok(*strength <= 1e-4)
            }
            EditOp::TransformLayer { transform, .. } => {
                let vals = [
                    transform.translate_x,
                    transform.translate_y,
                    transform.scale_x,
                    transform.scale_y,
                    transform.rotation_deg,
                    transform.pivot_x,
                    transform.pivot_y,
                ];
                if vals.iter().any(|v| !v.is_finite()) {
                    return Err(ImageError::InvalidEdit(
                        "transform parameters must be finite".into(),
                    ));
                }
                Ok(transform.is_identity())
            }
        }
    }

    fn execute_op(
        &mut self,
        op: &EditOp,
        target: ApplyTarget,
        selection_only: bool,
    ) -> Result<(), ImageError> {
        let filter_sel_snapshot = (selection_only && self.stack.selection.has_selection())
            .then(|| self.stack.selection.clone());
        if target == ApplyTarget::NewLayer {
            let mut next = render_single_op(&self.current, None, op)?;
            if matches!(
                op,
                EditOp::AdjustColor(_)
                    | EditOp::AdjustHsv(_)
                    | EditOp::ColorBalance(_)
                    | EditOp::AdjustCurves(_)
                    | EditOp::Denoise { .. }
            ) && let Some(ref sel) = filter_sel_snapshot
            {
                let blank = RgbaImage::from_pixel(next.width(), next.height(), Rgba([0, 0, 0, 0]));
                clip_rgba_by_selection(&blank, &mut next, 0, 0, sel);
            }
            let name = match op {
                EditOp::AdjustColor(_) => "Color & Light",
                EditOp::AdjustHsv(_) => "HSV Adjustment",
                EditOp::ColorBalance(_) => "Color Balance",
                EditOp::AdjustCurves(_) => "Curves",
                EditOp::Denoise { .. } => "Denoise",
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
                let old_w = self.stack.canvas_width;
                let old_h = self.stack.canvas_height;
                if self.is_simple_single_layer() {
                    let layer = &mut self.stack.layers[0];
                    let next = render_single_op(&layer.pixels, layer.original.as_deref(), op)?;
                    transform_layer_geometry(layer, op)?;
                    self.stack.canvas_width = next.width();
                    self.stack.canvas_height = next.height();
                    layer.pixels = Arc::new(next);
                } else {
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
                self.transform_canvas_selection(old_w, old_h, op);
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
                let old_w = self.stack.canvas_width;
                let old_h = self.stack.canvas_height;
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
                    let old_wf = f64::from(old_w.max(1));
                    let old_hf = f64::from(old_h.max(1));
                    let sx = f64::from(*width) / old_wf;
                    let sy = f64::from(*height) / old_hf;
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
                self.transform_canvas_selection(old_w, old_h, op);
                return Ok(());
            }
            EditOp::CanvasSize {
                width,
                height,
                anchor,
                fill,
                fill_alpha,
            } => {
                let old_w = self.stack.canvas_width;
                let old_h = self.stack.canvas_height;
                if self.is_simple_single_layer() {
                    let layer = &mut self.stack.layers[0];
                    let next = render_single_op(&layer.pixels, layer.original.as_deref(), op)?;
                    transform_layer_geometry(layer, op)?;
                    self.stack.canvas_width = next.width();
                    self.stack.canvas_height = next.height();
                    layer.pixels = Arc::new(next);
                } else {
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
                self.transform_canvas_selection(old_w, old_h, op);
                return Ok(());
            }
            _ => {}
        }

        // Paint always targets Quick Mask (when active), active layer's mask (`editing_mask`),
        // or the active layer's pixels — strictly clipped to any active `SelectionMask`.
        if let EditOp::Paint {
            points,
            color,
            radius,
            opacity,
            erase,
            brush,
        } = op
        {
            if self.stack.selection.quick_mask_active {
                let (cw, ch) = (self.stack.canvas_width, self.stack.canvas_height);
                let next_mask = paint_mask_stroke(
                    &self.stack.selection.alpha,
                    cw,
                    ch,
                    points,
                    *color,
                    *radius,
                    *opacity,
                    *erase,
                    *brush,
                    None,
                );
                self.stack.selection.alpha = Arc::new(next_mask);
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
                let mx_off = f64::from(mx_i);
                let my_off = f64::from(my_i);
                let mask_pts: Vec<[f64; 2]> = points
                    .iter()
                    .map(|[x, y]| [x - mx_off, y - my_off])
                    .collect();
                let (lw, lh) = active.dimensions();
                let next_mask = paint_mask_stroke(
                    mask_arc,
                    lw,
                    lh,
                    &mask_pts,
                    *color,
                    *radius,
                    *opacity,
                    *erase,
                    *brush,
                    sel_snapshot.as_ref().map(|s| (s, mx_i, my_i)),
                );
                self.stack.active_layer_mut().mask = Some(Arc::new(next_mask));
                return Ok(());
            }

            let ox_i = active.offset_x;
            let oy_i = active.offset_y;
            let ox = f64::from(ox_i);
            let oy = f64::from(oy_i);
            let local_pts: Vec<[f64; 2]> = points.iter().map(|[x, y]| [x - ox, y - oy]).collect();
            let lock_alpha = active.lock_alpha;
            let mut next = pixels::paint_stroke_with_brush_clipped(
                &active.pixels,
                &local_pts,
                *color,
                *radius,
                *opacity,
                *erase,
                *brush,
                sel_snapshot.as_ref().map(|s| (s, ox_i, oy_i)),
            );
            if lock_alpha {
                preserve_alpha_channel(&active.pixels, &mut next);
            }
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
            let mut next = liquify::liquify_stroke(
                &active.pixels,
                active.original.as_deref(),
                &local_pts,
                *mode,
                *radius,
                *strength,
            );
            if active.lock_alpha {
                preserve_alpha_channel(&active.pixels, &mut next);
            }
            self.stack.active_layer_mut().pixels = Arc::new(next);
            return Ok(());
        }

        // Healing always targets the active layer and respects SelectionMask.
        if let EditOp::Healing {
            points,
            mode,
            radius,
            strength,
            source,
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
            let sel_snapshot = self
                .stack
                .selection
                .has_selection()
                .then(|| self.stack.selection.clone());
            let ox = f64::from(active.offset_x);
            let oy = f64::from(active.offset_y);
            let local_pts: Vec<[f64; 2]> = points.iter().map(|[x, y]| [x - ox, y - oy]).collect();
            let local_src = source.map(|[sx, sy]| [sx - ox, sy - oy]);
            let sel_ref = sel_snapshot
                .as_ref()
                .map(|s| (s, active.offset_x, active.offset_y));
            let mut next = healing::healing_stroke(
                &active.pixels,
                &local_pts,
                *mode,
                *radius,
                *strength,
                local_src,
                *brush,
                sel_ref,
            );
            if active.lock_alpha {
                preserve_alpha_channel(&active.pixels, &mut next);
            }
            self.stack.active_layer_mut().pixels = Arc::new(next);
            return Ok(());
        }

        // TransformLayer supports full layer affine transform or active selection extract+transform.
        if let EditOp::TransformLayer {
            layer_id,
            transform,
        } = op
        {
            let target_id = layer_id.unwrap_or(self.stack.active_layer_id);
            let Some(idx) = self.stack.index_of(target_id) else {
                return Err(ImageError::coded("layer_not_found"));
            };
            if self.stack.layers[idx].locked {
                return Err(ImageError::coded("layer_locked"));
            }
            if self.stack.selection.has_selection() && target_id == self.stack.active_layer_id {
                self.transform_selected_pixels(idx, transform);
                self.selection_revision += 1;
            } else {
                let layer = &mut self.stack.layers[idx];
                let (lw, lh) = layer.dimensions();
                let (next_px, dx, dy) = pixels::transform_rgba_affine(&layer.pixels, transform);
                layer.pixels = Arc::new(next_px);
                layer.offset_x += dx;
                layer.offset_y += dy;
                if let Some(orig) = &layer.original {
                    let (next_orig, _, _) = pixels::transform_rgba_affine(orig, transform);
                    layer.original = Some(Arc::new(next_orig));
                }
                if let Some(mask) = &layer.mask
                    && mask.len() == (lw as usize) * (lh as usize)
                {
                    if layer.mask_linked {
                        let (next_mask, _, _, _, _) =
                            pixels::transform_mask_affine(mask, lw, lh, transform);
                        layer.mask = Some(Arc::new(next_mask));
                    } else {
                        layer.mask_offset_x -= dx;
                        layer.mask_offset_y -= dy;
                    }
                }
            }
            return Ok(());
        }

        if let EditOp::Denoise {
            strength,
            detail_preserve,
        } = op
        {
            let sel_snapshot = filter_sel_snapshot;
            if target == ApplyTarget::ActiveLayer || self.is_simple_single_layer() {
                let active = self.stack.active_layer();
                if active.locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                if !active.visible {
                    return Err(ImageError::coded("layer_hidden"));
                }
                let sel_ref = sel_snapshot
                    .as_ref()
                    .map(|s| (s, active.offset_x, active.offset_y));
                let next =
                    pixels::denoise_rgba(&active.pixels, *strength, *detail_preserve, sel_ref);
                self.stack.active_layer_mut().pixels = Arc::new(next);
            } else {
                for layer in &mut self.stack.layers {
                    if layer.locked || !layer.visible || layer.effect.is_some() {
                        continue;
                    }
                    let sel_ref = sel_snapshot
                        .as_ref()
                        .map(|s| (s, layer.offset_x, layer.offset_y));
                    let next =
                        pixels::denoise_rgba(&layer.pixels, *strength, *detail_preserve, sel_ref);
                    layer.pixels = Arc::new(next);
                }
            }
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
            if active.lock_alpha {
                preserve_alpha_channel(&active.pixels, &mut next);
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
            let mut next = render_single_op(&active.pixels, active.original.as_deref(), op)?;
            if matches!(
                op,
                EditOp::AdjustColor(_)
                    | EditOp::AdjustHsv(_)
                    | EditOp::ColorBalance(_)
                    | EditOp::AdjustCurves(_)
            ) && let Some(ref sel) = filter_sel_snapshot
            {
                clip_rgba_by_selection(
                    &active.pixels,
                    &mut next,
                    active.offset_x,
                    active.offset_y,
                    sel,
                );
            }
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
            let old_w = self.stack.canvas_width;
            let old_h = self.stack.canvas_height;
            let layer = &mut self.stack.layers[0];
            let mut next = render_single_op(&layer.pixels, layer.original.as_deref(), op)?;
            if matches!(
                op,
                EditOp::AdjustColor(_)
                    | EditOp::AdjustHsv(_)
                    | EditOp::ColorBalance(_)
                    | EditOp::AdjustCurves(_)
            ) && let Some(ref sel) = filter_sel_snapshot
            {
                clip_rgba_by_selection(
                    &layer.pixels,
                    &mut next,
                    layer.offset_x,
                    layer.offset_y,
                    sel,
                );
            }
            transform_layer_geometry(layer, op)?;
            self.stack.canvas_width = next.width();
            self.stack.canvas_height = next.height();
            layer.pixels = Arc::new(next);
            self.transform_canvas_selection(old_w, old_h, op);
            return Ok(());
        }

        let old_w = self.stack.canvas_width;
        let old_h = self.stack.canvas_height;
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
                self.transform_canvas_selection(old_w, old_h, op);
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
                self.transform_canvas_selection(old_w, old_h, op);
            }
            EditOp::AdjustColor(_)
            | EditOp::AdjustHsv(_)
            | EditOp::ColorBalance(_)
            | EditOp::AdjustCurves(_) => {
                for layer in &mut self.stack.layers {
                    let mut next = render_single_op(&layer.pixels, None, op)?;
                    if let Some(ref sel) = filter_sel_snapshot {
                        clip_rgba_by_selection(
                            &layer.pixels,
                            &mut next,
                            layer.offset_x,
                            layer.offset_y,
                            sel,
                        );
                    }
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
                self.transform_canvas_selection(old_w, old_h, op);
            }
        }
        Ok(())
    }

    fn transform_canvas_selection(&mut self, old_w: u32, old_h: u32, op: &EditOp) {
        let had_active =
            self.stack.selection.has_selection() || self.stack.selection.quick_mask_active;
        match op {
            EditOp::Crop {
                x,
                y,
                width,
                height,
                ..
            } => {
                self.stack.selection = self.stack.selection.crop(*x, *y, *width, *height);
            }
            EditOp::Resize {
                width,
                height,
                mode,
                ..
            } => {
                if *mode == ResizeMode::Stretch {
                    self.stack.selection = self.stack.selection.resize(*width, *height);
                } else {
                    self.stack.selection = SelectionMask::empty(*width, *height);
                }
            }
            EditOp::CanvasSize {
                width,
                height,
                anchor,
                ..
            } => {
                let (dx, dy) = pixels::anchor_offset(old_w, old_h, *width, *height, *anchor);
                self.stack.selection = self.stack.selection.crop(-dx, -dy, *width, *height);
            }
            EditOp::Rotate { degrees } => {
                let turns = degrees.rem_euclid(360) / 90;
                self.stack.selection = self.stack.selection.rotate_90(turns);
            }
            EditOp::Flip { axis } => {
                self.stack.selection = self.stack.selection.flip(*axis == FlipAxis::Horizontal);
            }
            _ => {
                let (cw, ch) = (self.stack.canvas_width, self.stack.canvas_height);
                if self.stack.selection.width != cw || self.stack.selection.height != ch {
                    self.stack.selection = SelectionMask::empty(cw, ch);
                }
            }
        }
        if had_active {
            self.selection_revision += 1;
        }
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

    pub fn save_selection(
        &mut self,
        name: String,
        target_id: Option<u64>,
        op: SelectionBooleanOp,
    ) -> u64 {
        let id = self.stack.selection.save_selection(name, target_id, op);
        self.selection_revision += 1;
        id
    }

    pub fn load_selection(&mut self, id: u64, op: SelectionBooleanOp, invert: bool) -> bool {
        let ok = self.stack.selection.load_selection(id, op, invert);
        if ok {
            self.selection_revision += 1;
        }
        ok
    }

    pub fn delete_saved_selection(&mut self, id: u64) -> bool {
        let ok = self.stack.selection.delete_saved_selection(id);
        if ok {
            self.selection_revision += 1;
        }
        ok
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
        if active.lock_alpha {
            return Ok(());
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
        if active.lock_alpha {
            return Ok(());
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
                selection: None,
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

    fn transform_selected_pixels(&mut self, layer_idx: usize, transform: &LayerTransformSpec) {
        let Some([bx, by, bw, bh]) = self.stack.selection.bounds else {
            return;
        };
        if bw == 0 || bh == 0 {
            return;
        }
        let layer = &mut self.stack.layers[layer_idx];
        let (lw, lh) = layer.dimensions();
        let ox = layer.offset_x;
        let oy = layer.offset_y;

        let mut extracted = RgbaImage::from_pixel(bw, bh, Rgba([0, 0, 0, 0]));
        let mut base = (*layer.pixels).clone();

        for sy in 0..bh {
            let cy = by as i32 + sy as i32;
            let ly = cy - oy;
            if ly < 0 || ly >= lh as i32 {
                continue;
            }
            for sx in 0..bw {
                let cx = bx as i32 + sx as i32;
                let lx = cx - ox;
                if lx < 0 || lx >= lw as i32 {
                    continue;
                }
                let sel_a = self.stack.selection.alpha_at(cx, cy);
                if sel_a == 0 {
                    continue;
                }
                let p = base.get_pixel(lx as u32, ly as u32).0;
                let ex_a = ((u16::from(p[3]) * u16::from(sel_a) + 127) / 255) as u8;
                extracted.put_pixel(sx, sy, Rgba([p[0], p[1], p[2], ex_a]));

                let keep = 255 - u16::from(sel_a);
                let rem_a = ((u16::from(p[3]) * keep + 127) / 255) as u8;
                let dst_px = base.get_pixel_mut(lx as u32, ly as u32);
                if rem_a == 0 {
                    dst_px.0 = [0, 0, 0, 0];
                } else {
                    dst_px.0[3] = rem_a;
                }
            }
        }

        let (transformed, dx, dy) = pixels::transform_rgba_affine(&extracted, transform);
        let tx = bx as i32 + dx;
        let ty = by as i32 + dy;
        let (tw, th) = transformed.dimensions();

        // Expand layer bounds if the transformed selection extends outside existing layer bounds.
        let new_ox = ox.min(tx);
        let new_oy = oy.min(ty);
        let new_right = (ox + lw as i32).max(tx + tw as i32);
        let new_bottom = (oy + lh as i32).max(ty + th as i32);
        let new_w = (new_right - new_ox).max(1) as u32;
        let new_h = (new_bottom - new_oy).max(1) as u32;

        let mut merged = RgbaImage::from_pixel(new_w, new_h, Rgba([0, 0, 0, 0]));
        image::imageops::replace(
            &mut merged,
            &base,
            i64::from(ox - new_ox),
            i64::from(oy - new_oy),
        );
        let place_x = tx - new_ox;
        let place_y = ty - new_oy;
        for py in 0..th {
            let my = place_y + py as i32;
            if my < 0 || my >= new_h as i32 {
                continue;
            }
            for px in 0..tw {
                let mx = place_x + px as i32;
                if mx < 0 || mx >= new_w as i32 {
                    continue;
                }
                let src_px = transformed.get_pixel(px, py).0;
                if src_px[3] == 0 {
                    continue;
                }
                let dst_px = merged.get_pixel_mut(mx as u32, my as u32);
                let sa = f32::from(src_px[3]) / 255.0;
                let da = f32::from(dst_px.0[3]) / 255.0;
                let out_a = sa + da * (1.0 - sa);
                if out_a > 1e-5 {
                    let mix = |s: u8, d: u8| -> u8 {
                        ((f32::from(s) * sa + f32::from(d) * da * (1.0 - sa)) / out_a)
                            .round()
                            .clamp(0.0, 255.0) as u8
                    };
                    *dst_px = Rgba([
                        mix(src_px[0], dst_px.0[0]),
                        mix(src_px[1], dst_px.0[1]),
                        mix(src_px[2], dst_px.0[2]),
                        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
                    ]);
                }
            }
        }
        if new_w != lw || new_h != lh || new_ox != ox || new_oy != oy {
            let shift_x = (ox - new_ox) as usize;
            let shift_y = (oy - new_oy) as usize;
            if let Some(old_mask) = &layer.mask
                && old_mask.len() == (lw as usize) * (lh as usize)
            {
                let mut padded_mask = vec![0u8; (new_w as usize) * (new_h as usize)];
                for row in 0..(lh as usize) {
                    let src_start = row * (lw as usize);
                    let dst_start = (shift_y + row) * (new_w as usize) + shift_x;
                    padded_mask[dst_start..dst_start + (lw as usize)]
                        .copy_from_slice(&old_mask[src_start..src_start + (lw as usize)]);
                }
                layer.mask = Some(Arc::new(padded_mask));
            }
            if let Some(old_orig) = &layer.original {
                let mut padded_orig = RgbaImage::from_pixel(new_w, new_h, Rgba([0, 0, 0, 0]));
                image::imageops::replace(
                    &mut padded_orig,
                    old_orig.as_ref(),
                    i64::from(ox - new_ox),
                    i64::from(oy - new_oy),
                );
                layer.original = Some(Arc::new(padded_orig));
            }
        }
        layer.pixels = Arc::new(merged);
        layer.offset_x = new_ox;
        layer.offset_y = new_oy;

        // Transform the selection mask to match the transformed pixels.
        let (cw, ch) = (self.stack.canvas_width, self.stack.canvas_height);
        let mut sel_patch = vec![0u8; (bw as usize) * (bh as usize)];
        for sy in 0..bh {
            for sx in 0..bw {
                sel_patch[(sy * bw + sx) as usize] = self
                    .stack
                    .selection
                    .alpha_at(bx as i32 + sx as i32, by as i32 + sy as i32);
            }
        }
        let (t_mask, mw, mh, mdx, mdy) =
            pixels::transform_mask_affine(&sel_patch, bw, bh, transform);
        let mut next_canvas_sel = vec![0u8; (cw as usize) * (ch as usize)];
        let mtx = bx as i32 + mdx;
        let mty = by as i32 + mdy;
        for py in 0..mh {
            let cy = mty + py as i32;
            if cy < 0 || cy >= ch as i32 {
                continue;
            }
            for px in 0..mw {
                let cx = mtx + px as i32;
                if cx < 0 || cx >= cw as i32 {
                    continue;
                }
                next_canvas_sel[(cy as usize) * (cw as usize) + (cx as usize)] =
                    t_mask[(py * mw + px) as usize];
            }
        }
        self.stack.selection.alpha = Arc::new(next_canvas_sel);
        self.stack.selection.recompute_bounds();
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
            stops,
            opacity,
            repeat,
            reverse,
            dither,
            blend_mode,
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
            let shape = match kind {
                GradientKind::Linear => drawing::GradientShapeSpec::Linear,
                GradientKind::Bilinear | GradientKind::Reflected => {
                    drawing::GradientShapeSpec::Bilinear
                }
                GradientKind::Radial => drawing::GradientShapeSpec::Radial,
                GradientKind::Square => drawing::GradientShapeSpec::Square,
                GradientKind::Conical => drawing::GradientShapeSpec::Conical,
                GradientKind::Spiral => drawing::GradientShapeSpec::Spiral,
            };
            let rep = match repeat {
                GradientRepeat::None => drawing::GradientRepeatSpec::None,
                GradientRepeat::Forwards => drawing::GradientRepeatSpec::Forwards,
                GradientRepeat::Alternating => drawing::GradientRepeatSpec::Alternating,
            };
            let bmode = if is_mask {
                drawing::GradientBlendMode::Normal
            } else {
                match blend_mode {
                    BlendMode::Normal => drawing::GradientBlendMode::Normal,
                    BlendMode::Multiply => drawing::GradientBlendMode::Multiply,
                    BlendMode::Screen => drawing::GradientBlendMode::Screen,
                    BlendMode::Overlay => drawing::GradientBlendMode::Overlay,
                    BlendMode::Darken => drawing::GradientBlendMode::Darken,
                    BlendMode::Lighten => drawing::GradientBlendMode::Lighten,
                    BlendMode::Difference => drawing::GradientBlendMode::Difference,
                }
            };
            let mut spec_stops: Vec<drawing::GradientStopSpec> =
                if *color_mode == GradientColorMode::Custom && stops.len() >= 2 {
                    stops
                        .iter()
                        .map(|s| drawing::GradientStopSpec {
                            pos: s.pos,
                            color: if is_mask { to_gray(s.color) } else { s.color },
                            alpha: s.alpha,
                        })
                        .collect()
                } else {
                    match color_mode {
                        GradientColorMode::FgToTransparent => vec![
                            drawing::GradientStopSpec {
                                pos: 0.0,
                                color: fg,
                                alpha: 1.0,
                            },
                            drawing::GradientStopSpec {
                                pos: 1.0,
                                color: fg,
                                alpha: 0.0,
                            },
                        ],
                        GradientColorMode::FgToBg | GradientColorMode::Custom => vec![
                            drawing::GradientStopSpec {
                                pos: 0.0,
                                color: fg,
                                alpha: 1.0,
                            },
                            drawing::GradientStopSpec {
                                pos: 1.0,
                                color: bg,
                                alpha: 1.0,
                            },
                        ],
                    }
                };
            if spec_stops.len() < 2 {
                spec_stops = vec![
                    drawing::GradientStopSpec {
                        pos: 0.0,
                        color: fg,
                        alpha: 1.0,
                    },
                    drawing::GradientStopSpec {
                        pos: 1.0,
                        color: bg,
                        alpha: 1.0,
                    },
                ];
            }
            drawing::render_gradient(
                src,
                [f64::from(start[0] - ox), f64::from(start[1] - oy)],
                [f64::from(end[0] - ox), f64::from(end[1] - oy)],
                fg,
                1.0,
                bg,
                1.0,
                &spec_stops,
                f64::from(*opacity),
                shape,
                rep,
                *reverse,
                *dither,
                bmode,
                selection.map(|s| (s, offset_x, offset_y)),
            )
        }
        _ => src.clone(),
    }
}

#[inline]
fn preserve_alpha_channel(before: &RgbaImage, next: &mut RgbaImage) {
    if before.dimensions() != next.dimensions() {
        return;
    }
    for (dst, src) in next.pixels_mut().zip(before.pixels()) {
        dst.0[3] = src.0[3];
    }
}

#[inline]
fn selection_differs(a: &SelectionMask, b: &SelectionMask) -> bool {
    !Arc::ptr_eq(&a.alpha, &b.alpha)
        || a.bounds != b.bounds
        || a.quick_mask_active != b.quick_mask_active
        || a.width != b.width
        || a.height != b.height
}

fn stroke_local_bbox(
    local_pts: &[[f64; 2]],
    radius: f32,
    brush: BrushSettings,
    width: u32,
    height: u32,
) -> Option<(u32, u32, u32, u32)> {
    if width == 0 || height == 0 || local_pts.is_empty() {
        return None;
    }
    let r = f64::from(radius);
    let scatter = f64::from(brush.scatter).clamp(0.0, 2.5);
    let pad = r + r * scatter * 1.5 + 3.0;
    let mut min_x = local_pts[0][0];
    let mut min_y = local_pts[0][1];
    let mut max_x = local_pts[0][0];
    let mut max_y = local_pts[0][1];
    for &[x, y] in &local_pts[1..] {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let left = (min_x - pad).floor().max(0.0) as u32;
    let top = (min_y - pad).floor().max(0.0) as u32;
    let right = ((max_x + pad).ceil().max(0.0) as u32).min(width);
    let bottom = ((max_y + pad).ceil().max(0.0) as u32).min(height);
    (left < right && top < bottom).then_some((left, top, right, bottom))
}

#[inline]
fn blend_clipped_rgba_pixel(b: [u8; 4], a: [u8; 4], s: u8) -> [u8; 4] {
    if s == 255 {
        return a;
    }
    if s == 0 {
        return b;
    }
    let t = f32::from(s) / 255.0;
    let inv = 1.0 - t;
    let ba = f32::from(b[3]) / 255.0;
    let aa = f32::from(a[3]) / 255.0;
    let wb = ba * inv;
    let wa = aa * t;
    let out_a = wb + wa;
    if out_a <= 1e-6 {
        return [0, 0, 0, 0];
    }
    let mix_c = |bc: u8, ac: u8| -> u8 {
        ((f32::from(bc) * wb + f32::from(ac) * wa) / out_a)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    [
        mix_c(b[0], a[0]),
        mix_c(b[1], a[1]),
        mix_c(b[2], a[2]),
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

#[allow(clippy::too_many_arguments)]
fn clip_rgba_by_selection_rect(
    before: &RgbaImage,
    unclipped: &RgbaImage,
    out: &mut RgbaImage,
    offset_x: i32,
    offset_y: i32,
    sel: &SelectionMask,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
) {
    let (w, h) = before.dimensions();
    if unclipped.dimensions() != (w, h) || out.dimensions() != (w, h) {
        return;
    }
    for y in top..bottom.min(h) {
        let cy = offset_y + y as i32;
        for x in left..right.min(w) {
            let s = sel.alpha_at(offset_x + x as i32, cy);
            let b = before.get_pixel(x, y).0;
            let a = unclipped.get_pixel(x, y).0;
            out.put_pixel(x, y, Rgba(blend_clipped_rgba_pixel(b, a, s)));
        }
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
            let a = after.get_pixel(x, y).0;
            after.put_pixel(x, y, Rgba(blend_clipped_rgba_pixel(b, a, s)));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn clip_mask_by_selection_rect(
    before: &[u8],
    unclipped: &[u8],
    out: &mut [u8],
    lw: u32,
    lh: u32,
    offset_x: i32,
    offset_y: i32,
    sel: &SelectionMask,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
) {
    let len = (lw as usize).saturating_mul(lh as usize);
    if before.len() < len || unclipped.len() < len || out.len() < len {
        return;
    }
    for y in top..bottom.min(lh) {
        let cy = offset_y + y as i32;
        for x in left..right.min(lw) {
            let s = sel.alpha_at(offset_x + x as i32, cy);
            let idx = (y * lw + x) as usize;
            if s == 255 {
                out[idx] = unclipped[idx];
            } else if s == 0 {
                out[idx] = before[idx];
            } else {
                let t = s as u16;
                let inv = 255 - t;
                out[idx] =
                    ((before[idx] as u16 * inv + unclipped[idx] as u16 * t + 127) / 255) as u8;
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
    selection: Option<(&SelectionMask, i32, i32)>,
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
    let painted = pixels::paint_stroke_with_brush_clipped(
        &mask_img,
        local_pts,
        [gray, gray, gray],
        radius,
        opacity,
        false,
        brush,
        selection,
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

fn validate_healing(
    points: &[[f64; 2]],
    radius: f32,
    hardness: f32,
    strength: f32,
) -> Result<(), ImageError> {
    if points.is_empty() {
        return Err(ImageError::InvalidEdit("healing stroke is empty".into()));
    }
    if points.iter().any(|[x, y]| !x.is_finite() || !y.is_finite()) {
        return Err(ImageError::InvalidEdit(
            "healing stroke contains non-finite coordinates".into(),
        ));
    }
    if !radius.is_finite() || !(0.5..=4096.0).contains(&radius) {
        return Err(ImageError::InvalidEdit(format!(
            "healing radius must be in 0.5..=4096, got {radius}"
        )));
    }
    if !hardness.is_finite() || !(0.0..=1.0).contains(&hardness) {
        return Err(ImageError::InvalidEdit(format!(
            "healing hardness must be in 0..=1, got {hardness}"
        )));
    }
    if !strength.is_finite() || !(0.05..=1.0).contains(&strength) {
        return Err(ImageError::InvalidEdit(format!(
            "healing strength must be in 0.05..=1.0, got {strength}"
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
        EditOp::Healing {
            points,
            mode,
            radius,
            strength,
            source,
            brush,
        } => healing::healing_stroke(
            src, points, *mode, *radius, *strength, *source, *brush, None,
        ),
        EditOp::Denoise {
            strength,
            detail_preserve,
        } => pixels::denoise_rgba(src, *strength, *detail_preserve, None),
        EditOp::TransformLayer { transform, .. } => pixels::transform_rgba_affine(src, transform).0,
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
            clipped_to_below: None,
            lock_alpha: None,
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
            clipped_to_below: None,
            lock_alpha: None,
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

    #[test]
    fn healing_spot_and_sampled_respect_selection_mask() {
        // Uniform background [100, 140, 180] with a bright red blemish at (16, 16).
        let mut img = RgbaImage::from_pixel(40, 40, Rgba([100, 140, 180, 255]));
        for y in 14..=18 {
            for x in 14..=18 {
                img.put_pixel(x, y, Rgba([250, 20, 20, 255]));
            }
        }
        // Source region at (6, 6) has subtle texture.
        img.put_pixel(6, 6, Rgba([40, 50, 60, 255]));
        img.put_pixel(7, 6, Rgba([52, 62, 72, 255]));

        let spot = healing::healing_stroke(
            &img,
            &[[16.0, 16.0]],
            HealingMode::Spot,
            5.0,
            1.0,
            None,
            BrushSettings::default(),
            None,
        );
        let healed_px = spot.get_pixel(16, 16).0;
        assert!(
            healed_px[0] < 130 && healed_px[2] > 150,
            "spot heal failed: {healed_px:?}"
        );

        // Sampled healing with SelectionMask isolating right half (x >= 16).
        let mut sel = SelectionMask::empty(40, 40);
        let mask: Vec<u8> = (0..40 * 40)
            .map(|i| if (i % 40) >= 16 { 255 } else { 0 })
            .collect();
        sel.combine(&mask, SelectionBooleanOp::Replace);

        let sampled = healing::healing_stroke(
            &img,
            &[[16.0, 16.0]],
            HealingMode::Sampled,
            5.0,
            1.0,
            Some([6.0, 6.0]),
            BrushSettings::default(),
            Some((&sel, 0, 0)),
        );
        // Outside selection (14, 16) stays untouched red; inside selection (17, 16) is healed.
        assert_eq!(sampled.get_pixel(14, 16).0, [250, 20, 20, 255]);
        assert!(sampled.get_pixel(17, 16).0[0] < 140);
    }

    #[test]
    fn denoise_selection_border_has_zero_exterior_bleed() {
        // Left half (x < 16) is bright red [255, 0, 0]; right half (x >= 16) is dark [20, 20, 20]
        // with a small noisy pixel at (17, 16).
        let mut img = RgbaImage::from_pixel(32, 32, Rgba([20, 20, 20, 255]));
        for y in 0..32 {
            for x in 0..16 {
                img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        img.put_pixel(17, 16, Rgba([45, 45, 45, 255]));

        let mut sel = SelectionMask::empty(32, 32);
        let mask: Vec<u8> = (0..32 * 32)
            .map(|i| if (i % 32) >= 16 { 255 } else { 0 })
            .collect();
        sel.combine(&mask, SelectionBooleanOp::Replace);

        let denoised = pixels::denoise_rgba(&img, 1.0, 0.2, Some((&sel, 0, 0)));
        // Exterior red stays untouched
        assert_eq!(denoised.get_pixel(15, 16).0, [255, 0, 0, 255]);
        // Border pixel inside selection (16, 16) must have ZERO red bleed from (15, 16)!
        let border_px = denoised.get_pixel(16, 16).0;
        assert!(
            border_px[0] <= 30 && border_px[1] >= 18 && border_px[2] >= 18,
            "red halo bled across selection border: {border_px:?}"
        );
    }

    #[test]
    fn affine_transform_and_document_selection_transform_and_excluding_composite() {
        let mut img = RgbaImage::from_pixel(20, 20, Rgba([10, 10, 10, 255]));
        for y in 4..8 {
            for x in 4..8 {
                img.put_pixel(x, y, Rgba([200, 100, 50, 255]));
            }
        }
        let spec = LayerTransformSpec {
            translate_x: 5.0,
            translate_y: 3.0,
            scale_x: 2.0,
            scale_y: 2.0,
            rotation_deg: 0.0,
            pivot_x: 0.5,
            pivot_y: 0.5,
            flip_h: false,
            flip_v: false,
        };
        let (scaled, dx, dy) = pixels::transform_rgba_affine(&img, &spec);
        assert_eq!(scaled.dimensions(), (40, 40));
        assert_eq!((dx, dy), (-5, -7));

        let mut doc = EditorDocument::from_rgba(PathBuf::from("tr.png"), img);
        doc.apply_layer_op(LayerOperation::AddEmpty {
            name: "Top".to_owned(),
        })
        .unwrap();
        let top_id = doc.stack().active_layer_id;
        doc.apply(EditOp::Paint {
            points: vec![[10.0, 10.0]],
            color: [0, 255, 0],
            radius: 2.0,
            opacity: 1.0,
            erase: false,
            brush: BrushSettings::default(),
        })
        .unwrap();

        // composite_stack_excluding omits the transforming layer cleanly
        let base_without_top = crate::composite_stack_excluding(doc.stack(), top_id);
        assert_eq!(base_without_top.get_pixel(10, 10).0, [10, 10, 10, 255]);

        // Transform active layer and undo/redo
        doc.apply(EditOp::TransformLayer {
            layer_id: Some(top_id),
            transform: LayerTransformSpec {
                translate_x: 4.0,
                translate_y: 0.0,
                ..LayerTransformSpec::default()
            },
        })
        .unwrap();
        assert_eq!(doc.stack().active_layer().offset_x, 4);
        assert!(doc.undo().unwrap());
        assert_eq!(doc.stack().active_layer().offset_x, 0);
    }

    #[test]
    fn ora_multilayer_roundtrip_and_document_save_with_bak() {
        let dir = std::env::temp_dir().join(format!("photon-ora-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ora_path = dir.join("layered.ora");

        let bg = Arc::new(RgbaImage::from_pixel(32, 24, Rgba([40, 60, 80, 255])));
        let mut stack = LayerStack::from_single(bg);
        let top_img = Arc::new(RgbaImage::from_pixel(12, 10, Rgba([220, 40, 30, 255])));
        stack
            .apply_op(&LayerOperation::AddImage {
                name: "Foreground".to_owned(),
                offset_x: 5,
                offset_y: 7,
                pixels: top_img,
            })
            .unwrap();
        let fg_id = stack.active_layer_id;
        stack
            .apply_op(&LayerOperation::SetProps {
                id: fg_id,
                name: None,
                visible: Some(true),
                locked: None,
                clipped_to_below: None,
                lock_alpha: None,
                opacity: Some(0.75),
                blend_mode: Some(BlendMode::Multiply),
                offset_x: None,
                offset_y: None,
            })
            .unwrap();

        crate::ora::encode_ora_stack(&stack, &ora_path).unwrap();
        assert_eq!(crate::probe_dimensions(&ora_path), Some((32, 24)));
        let thumb = crate::generate_thumbnail(&ora_path, 16).unwrap();
        assert!(thumb.width() <= 16 && thumb.height() <= 16);

        let mut doc = EditorDocument::open(ora_path.clone()).unwrap();
        assert_eq!(doc.stack().layers.len(), 2);
        assert_eq!(doc.stack().layers[1].name, "Foreground");
        assert_eq!(doc.stack().layers[1].offset_x, 5);
        assert_eq!(doc.stack().layers[1].offset_y, 7);
        assert_eq!(doc.stack().layers[1].blend_mode, BlendMode::Multiply);
        assert!((doc.stack().layers[1].opacity - 0.75).abs() < 0.01);

        // Save back to existing .ora creates .ora.bak and preserves layers
        doc.save_to(None).unwrap();
        assert!(dir.join("layered.ora.bak").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extra_codecs_svg_svgz_exr_hdr_ico_pnm_ff_avif_probe_and_decode() {
        let dir = std::env::temp_dir().join(format!("photon-codec-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let sample = RgbaImage::from_pixel(16, 12, Rgba([180, 90, 45, 255]));

        // SVG & SVGZ
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="18"><rect width="24" height="18" fill="rgb(200,50,50)"/></svg>"#;
        let svg_path = dir.join("test.svg");
        std::fs::write(&svg_path, svg.as_bytes()).unwrap();
        assert_eq!(crate::probe_dimensions(&svg_path), Some((24, 18)));
        let svg_px = crate::decode_rgba(&svg_path).unwrap();
        assert_eq!(svg_px.dimensions(), (24, 18));
        assert_eq!(svg_px.get_pixel(12, 9).0, [200, 50, 50, 255]);

        let svgz_path = dir.join("test.svgz");
        {
            use flate2::write::GzEncoder;
            use std::io::Write;
            let f = std::fs::File::create(&svgz_path).unwrap();
            let mut gz = GzEncoder::new(f, flate2::Compression::default());
            gz.write_all(svg.as_bytes()).unwrap();
            gz.finish().unwrap();
        }
        assert_eq!(crate::probe_dimensions(&svgz_path), Some((24, 18)));
        assert_eq!(
            crate::decode_rgba(&svgz_path).unwrap().dimensions(),
            (24, 18)
        );

        // EXR, HDR, ICO, PNM/PAM, Farbfeld
        for ext in ["exr", "hdr", "ico", "ppm", "pam", "ff"] {
            let p = dir.join(format!("sample.{ext}"));
            codec::save_to(&sample, &p).unwrap();
            assert_eq!(
                crate::probe_dimensions(&p),
                Some((16, 12)),
                "probe failed for {ext}"
            );
            let dec = crate::decode_rgba(&p).unwrap();
            assert_eq!(dec.dimensions(), (16, 12), "decode failed for {ext}");
        }

        // AVIF encode + native probe & decode test
        let avif_path = dir.join("sample.avif");
        codec::save_to(&sample, &avif_path).unwrap();
        assert!(std::fs::metadata(&avif_path).unwrap().len() > 0);
        assert_eq!(crate::probe_dimensions(&avif_path), Some((16, 12)));
        assert_eq!(
            crate::decode_rgba(&avif_path).unwrap().dimensions(),
            (16, 12)
        );

        // JPEG 2000 (.jp2 / .j2k) probe & decode when opj_compress is available
        let png_src = dir.join("src_for_ext.png");
        codec::save_to(&sample, &png_src).unwrap();
        for ext in ["jp2", "j2k"] {
            let jp2_path = dir.join(format!("sample.{ext}"));
            if let Ok(out) = std::process::Command::new("opj_compress")
                .arg("-i")
                .arg(&png_src)
                .arg("-o")
                .arg(&jp2_path)
                .output()
                && out.status.success()
            {
                assert_eq!(
                    crate::probe_dimensions(&jp2_path),
                    Some((16, 12)),
                    "probe failed for {ext}"
                );
                let dec = crate::decode_rgba(&jp2_path).unwrap();
                assert_eq!(dec.dimensions(), (16, 12), "decode failed for {ext}");
                let thumb = crate::generate_thumbnail(&jp2_path, 8).unwrap();
                assert_eq!(thumb.dimensions(), (8, 6));
            }
        }

        // HEIC (.heic / .hif) probe & decode when heif-enc is available
        let heic_path = dir.join("sample.heic");
        if let Ok(out) = std::process::Command::new("heif-enc")
            .arg(&png_src)
            .arg("-o")
            .arg(&heic_path)
            .output()
            && out.status.success()
        {
            assert_eq!(crate::probe_dimensions(&heic_path), Some((16, 12)));
            let dec = crate::decode_rgba(&heic_path).unwrap();
            assert_eq!(dec.dimensions(), (16, 12));
            let hif_path = dir.join("sample.hif");
            std::fs::copy(&heic_path, &hif_path).unwrap();
            assert_eq!(crate::probe_dimensions(&hif_path), Some((16, 12)));
            assert_eq!(
                crate::decode_rgba(&hif_path).unwrap().dimensions(),
                (16, 12)
            );
        }

        // KRA (.kra) probe, decode & thumbnail
        {
            use image::{ExtendedColorType, ImageEncoder};
            use std::io::Write;
            let mut png_bytes = Vec::new();
            image::codecs::png::PngEncoder::new(&mut png_bytes)
                .write_image(sample.as_raw(), 16, 12, ExtendedColorType::Rgba8)
                .unwrap();
            let kra_path = dir.join("sample.kra");
            let f = std::fs::File::create(&kra_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            zw.start_file("mergedimage.png", opts).unwrap();
            zw.write_all(&png_bytes).unwrap();
            zw.finish().unwrap();
            assert_eq!(crate::probe_dimensions(&kra_path), Some((16, 12)));
            assert_eq!(
                crate::decode_rgba(&kra_path).unwrap().dimensions(),
                (16, 12)
            );
            assert_eq!(
                crate::generate_thumbnail(&kra_path, 8)
                    .unwrap()
                    .dimensions(),
                (8, 6)
            );
        }

        // Extra RAW extensions recognized by codec::is_raw_ext
        assert!(codec::is_raw_ext("pef"));
        assert!(codec::is_raw_ext("srw"));
        assert!(codec::is_raw_ext("3fr"));
        assert!(codec::is_raw_ext("rwl"));
        assert!(!codec::is_raw_ext("png"));

        // EditorDocument::new_blank
        assert!(EditorDocument::new_blank(0, 100, [255, 255, 255, 255]).is_err());
        let blank = EditorDocument::new_blank(32, 24, [10, 20, 30, 255]).unwrap();
        assert_eq!(blank.dimensions(), (32, 24));
        assert_eq!(blank.stack.layers.len(), 1);
        assert_eq!(blank.stack.layers[0].name, "Background");
        assert_eq!(blank.current.get_pixel(0, 0).0, [10, 20, 30, 255]);

        // JXL probe & decode
        let jxl_bytes: &[u8] = &[
            0xff, 0x0a, 0x30, 0x54, 0x10, 0x09, 0x08, 0x06, 0x01, 0x00, 0x78, 0x00, 0x4b, 0x38,
            0x41, 0x3c, 0xb6, 0x3a, 0x51, 0xfe, 0x00, 0x47, 0x1e, 0xa0, 0x85, 0xb8, 0x27, 0x1a,
            0x48, 0x45, 0x84, 0x1b, 0x71, 0x4f, 0xa8, 0x3e, 0x8e, 0x30, 0x03, 0x92, 0x84, 0x01,
        ];
        let jxl_path = dir.join("sample.jxl");
        std::fs::write(&jxl_path, jxl_bytes).unwrap();
        let jxl_dims = crate::probe_dimensions(&jxl_path).expect("jxl probe");
        assert!(jxl_dims.0 > 0 && jxl_dims.1 > 0);
        assert_eq!(
            crate::decode_rgba(&jxl_path).unwrap().dimensions(),
            jxl_dims
        );

        // PSD probe & decode
        let mut psd_bytes: Vec<u8> = Vec::new();
        psd_bytes.extend_from_slice(b"8BPS");
        psd_bytes.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]); // version 1 + reserved
        psd_bytes.extend_from_slice(&[0, 3]); // 3 channels
        psd_bytes.extend_from_slice(&1u32.to_be_bytes()); // height = 1
        psd_bytes.extend_from_slice(&1u32.to_be_bytes()); // width = 1
        psd_bytes.extend_from_slice(&[0, 8, 0, 3]); // depth = 8, RGB mode = 3
        psd_bytes.extend_from_slice(&0u32.to_be_bytes()); // color mode data
        psd_bytes.extend_from_slice(&0u32.to_be_bytes()); // image resources
        psd_bytes.extend_from_slice(&0u32.to_be_bytes()); // layer & mask info
        psd_bytes.extend_from_slice(&[0, 0]); // compression = raw
        psd_bytes.extend_from_slice(&[200, 100, 50]); // R, G, B
        let psd_path = dir.join("sample.psd");
        std::fs::write(&psd_path, &psd_bytes).unwrap();
        assert_eq!(crate::probe_dimensions(&psd_path), Some((1, 1)));
        let psd_px = crate::decode_rgba(&psd_path).unwrap();
        assert_eq!(psd_px.get_pixel(0, 0).0, [200, 100, 50, 255]);
        let psd_doc = EditorDocument::open(psd_path).unwrap();
        assert_eq!(psd_doc.dimensions(), (1, 1));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn brush_and_eraser_strictly_respect_rect_and_inverted_selections() {
        let base = RgbaImage::from_pixel(32, 32, Rgba([10, 20, 30, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("sel_brush.png"), base);

        doc.select_shape(
            &SelectionShape::Rect {
                x: 8.0,
                y: 8.0,
                width: 12.0,
                height: 12.0,
                feather: 0.0,
                anti_alias: false,
            },
            SelectionBooleanOp::Replace,
        );

        // 1. EditOp::Paint (fast round brush) crossing the selection boundary
        doc.apply(EditOp::Paint {
            points: vec![[4.0, 14.0], [24.0, 14.0]],
            color: [240, 40, 40],
            radius: 4.0,
            opacity: 1.0,
            erase: false,
            brush: BrushSettings::default(),
        })
        .unwrap();
        assert_eq!(doc.working_pixels().get_pixel(14, 14).0, [240, 40, 40, 255]);
        assert_eq!(doc.working_pixels().get_pixel(6, 14).0, [10, 20, 30, 255]);
        assert_eq!(doc.working_pixels().get_pixel(22, 14).0, [10, 20, 30, 255]);

        // 2. Live stroke (soft/chalk brush + eraser) crossing the selection boundary
        doc.live_stroke_begin(
            LiveStrokeKind::Paint {
                color: [0, 220, 80],
                radius: 4.0,
                opacity: 1.0,
                erase: false,
                brush: BrushSettings {
                    tip: photon_protocol::BrushTip::Round,
                    hardness: 0.4,
                    ..BrushSettings::default()
                },
            },
            4.0,
            10.0,
        )
        .unwrap();
        doc.live_stroke_step(&[[24.0, 10.0]]).unwrap();
        doc.live_stroke_end(true).unwrap();
        assert!(doc.working_pixels().get_pixel(12, 10).0[1] > 180);
        assert_eq!(doc.working_pixels().get_pixel(5, 10).0, [10, 20, 30, 255]);
        assert_eq!(doc.working_pixels().get_pixel(21, 10).0, [10, 20, 30, 255]);

        // 3. Live eraser stroke crossing the selection boundary
        doc.live_stroke_begin(
            LiveStrokeKind::Paint {
                color: [0, 0, 0],
                radius: 4.0,
                opacity: 1.0,
                erase: true,
                brush: BrushSettings::default(),
            },
            4.0,
            14.0,
        )
        .unwrap();
        doc.live_stroke_step(&[[24.0, 14.0]]).unwrap();
        doc.live_stroke_end(true).unwrap();
        assert_eq!(doc.working_pixels().get_pixel(14, 14).0[3], 0);
        assert_eq!(doc.working_pixels().get_pixel(6, 14).0, [10, 20, 30, 255]);
        assert_eq!(doc.working_pixels().get_pixel(22, 14).0, [10, 20, 30, 255]);

        // 4. Inverted selection: now exterior is editable and interior (8..20) is protected
        doc.invert_selection();
        doc.apply(EditOp::Paint {
            points: vec![[4.0, 14.0], [16.0, 14.0]],
            color: [20, 120, 250],
            radius: 3.0,
            opacity: 1.0,
            erase: false,
            brush: BrushSettings::default(),
        })
        .unwrap();
        assert_eq!(doc.working_pixels().get_pixel(5, 14).0, [20, 120, 250, 255]);
        assert_eq!(doc.working_pixels().get_pixel(14, 14).0[3], 0);
    }

    #[test]
    fn brush_and_eraser_respect_feathered_selection_without_over_saturation() {
        let base = RgbaImage::from_pixel(24, 24, Rgba([0, 0, 0, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("feather.png"), base);

        // Set a half-selected (128) region on x in 8..16 and full (255) on x in 16..20.
        let mut mask = vec![0u8; 24 * 24];
        for y in 4..20 {
            for x in 8..16 {
                mask[y * 24 + x] = 128;
            }
            for x in 16..20 {
                mask[y * 24 + x] = 255;
            }
        }
        doc.stack
            .selection
            .combine(&mask, SelectionBooleanOp::Replace);

        // Repeated live stroke passes over the 50% feathered region in ONE stroke
        // must never exceed ~50% blend (around 128).
        doc.live_stroke_begin(
            LiveStrokeKind::Paint {
                color: [255, 255, 255],
                radius: 4.0,
                opacity: 1.0,
                erase: false,
                brush: BrushSettings::default(),
            },
            10.0,
            12.0,
        )
        .unwrap();
        for _ in 0..5 {
            doc.live_stroke_step(&[[18.0, 12.0]]).unwrap();
            doc.live_stroke_step(&[[10.0, 12.0]]).unwrap();
        }
        doc.live_stroke_end(true).unwrap();

        let outside = doc.working_pixels().get_pixel(6, 12).0;
        let feathered = doc.working_pixels().get_pixel(12, 12).0;
        let full = doc.working_pixels().get_pixel(18, 12).0;
        assert_eq!(outside, [0, 0, 0, 255]);
        assert!(
            (124..=132).contains(&feathered[0]),
            "feathered pixel over-saturated or under-painted: {feathered:?}"
        );
        assert_eq!(full, [255, 255, 255, 255]);

        // Now erase with a feathered selection on an opaque layer: alpha should drop to ~50%
        let base2 = RgbaImage::from_pixel(24, 24, Rgba([200, 100, 50, 255]));
        let mut doc2 = EditorDocument::from_rgba(PathBuf::from("feather_erase.png"), base2);
        doc2.stack
            .selection
            .combine(&mask, SelectionBooleanOp::Replace);
        doc2.apply(EditOp::Paint {
            points: vec![[6.0, 12.0], [19.0, 12.0]],
            color: [0, 0, 0],
            radius: 4.0,
            opacity: 1.0,
            erase: true,
            brush: BrushSettings::default(),
        })
        .unwrap();
        assert_eq!(doc2.working_pixels().get_pixel(6, 12).0[3], 255);
        let feather_erased_a = doc2.working_pixels().get_pixel(12, 12).0[3];
        assert!(
            (124..=132).contains(&feather_erased_a),
            "feathered erase alpha expected ~127, got {feather_erased_a}"
        );
        assert_eq!(doc2.working_pixels().get_pixel(18, 12).0[3], 0);
    }

    #[test]
    fn brush_and_eraser_respect_selection_on_offset_layer_and_layer_mask() {
        let base = RgbaImage::from_pixel(32, 32, Rgba([0, 0, 0, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("offset_sel.png"), base);

        // Add an offset layer at (10, 8) with size 16x16 (covers canvas x=10..26, y=8..24)
        let layer_px = Arc::new(RgbaImage::from_pixel(16, 16, Rgba([50, 50, 50, 255])));
        doc.apply_layer_op(LayerOperation::AddImage {
            name: "Offset".to_owned(),
            offset_x: 10,
            offset_y: 8,
            pixels: layer_px,
        })
        .unwrap();
        let layer_id = doc.stack().active_layer_id;

        // Canvas-space selection at x=14..22, y=10..20
        doc.select_shape(
            &SelectionShape::Rect {
                x: 14.0,
                y: 10.0,
                width: 8.0,
                height: 10.0,
                feather: 0.0,
                anti_alias: false,
            },
            SelectionBooleanOp::Replace,
        );

        // Paint across canvas x=10..25 at y=14 (layer local y=6)
        doc.live_stroke_begin(
            LiveStrokeKind::Paint {
                color: [255, 200, 0],
                radius: 3.0,
                opacity: 1.0,
                erase: false,
                brush: BrushSettings::default(),
            },
            10.0,
            14.0,
        )
        .unwrap();
        doc.live_stroke_step(&[[25.0, 14.0]]).unwrap();
        doc.live_stroke_end(true).unwrap();

        let active_px = &doc.stack().active_layer().pixels;
        // Layer-local x=2 -> canvas x=12 (outside selection) stays [50, 50, 50, 255]
        assert_eq!(active_px.get_pixel(2, 6).0, [50, 50, 50, 255]);
        // Layer-local x=8 -> canvas x=18 (inside selection) becomes [255, 200, 0, 255]
        assert_eq!(active_px.get_pixel(8, 6).0, [255, 200, 0, 255]);
        // Layer-local x=14 -> canvas x=24 (outside selection) stays [50, 50, 50, 255]
        assert_eq!(active_px.get_pixel(14, 6).0, [50, 50, 50, 255]);

        // Add mask and paint/erase on mask with unlinked mask offset (+2, 0)
        doc.apply_layer_op(LayerOperation::AddMask {
            id: layer_id,
            reveal_all: true,
        })
        .unwrap();
        doc.stack.active_layer_mut().mask_offset_x = 2;
        assert!(doc.stack().active_layer().editing_mask);

        // Mask origin in canvas is (10 + 2, 8) = (12, 8).
        // Canvas selection is x=14..22, so mask-local selected x is 2..10.
        doc.apply(EditOp::Paint {
            points: vec![[12.0, 14.0], [24.0, 14.0]],
            color: [0, 0, 0],
            radius: 3.0,
            opacity: 1.0,
            erase: true,
            brush: BrushSettings::default(),
        })
        .unwrap();
        let mask = doc.stack().active_layer().mask.as_ref().unwrap();
        // Mask-local x=0 -> canvas x=12 (outside selection) stays 255
        assert_eq!(mask[6 * 16 + 0], 255);
        // Mask-local x=6 -> canvas x=18 (inside selection) erased to 0
        assert_eq!(mask[6 * 16 + 6], 0);
        // Mask-local x=12 -> canvas x=24 (outside selection) stays 255
        assert_eq!(mask[6 * 16 + 12], 255);
    }

    #[test]
    fn quick_mask_live_brush_and_eraser_edit_selection_with_undo_redo() {
        let base = RgbaImage::from_pixel(24, 24, Rgba([40, 80, 120, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("quick_mask.png"), base);
        assert!(doc.toggle_quick_mask());
        let rev0 = doc.selection_revision;

        // Live paint white adds to quick mask selection without modifying layer pixels
        doc.live_stroke_begin(
            LiveStrokeKind::Paint {
                color: [255, 255, 255],
                radius: 3.0,
                opacity: 1.0,
                erase: false,
                brush: BrushSettings::default(),
            },
            12.0,
            12.0,
        )
        .unwrap();
        assert!(doc.selection_revision > rev0);
        assert!(doc.selection().has_selection());
        assert_eq!(doc.selection().alpha_at(12, 12), 255);
        doc.live_stroke_end(true).unwrap();

        // Layer pixels remain completely untouched
        assert_eq!(doc.working_pixels().get_pixel(12, 12).0, [40, 80, 120, 255]);

        // Undo & redo restore quick mask selection and bump selection_revision
        let rev_after_stroke = doc.selection_revision;
        assert!(doc.undo().unwrap());
        assert!(!doc.selection().has_selection());
        assert!(doc.selection_revision > rev_after_stroke);

        let rev_after_undo = doc.selection_revision;
        assert!(doc.redo().unwrap());
        assert!(doc.selection().has_selection());
        assert_eq!(doc.selection().alpha_at(12, 12), 255);
        assert!(doc.selection_revision > rev_after_undo);

        // Live erase in quick mask removes from selection
        doc.live_stroke_begin(
            LiveStrokeKind::Paint {
                color: [255, 255, 255],
                radius: 2.0,
                opacity: 1.0,
                erase: true,
                brush: BrushSettings::default(),
            },
            12.0,
            12.0,
        )
        .unwrap();
        doc.live_stroke_end(true).unwrap();
        assert_eq!(doc.selection().alpha_at(12, 12), 0);
    }

    #[test]
    fn transform_layer_with_partial_selection_offset_mask_feather_and_undo_redo() {
        // 1. Partial selection on an offset layer + expansion + selection mask update + undo/redo
        let bg = RgbaImage::from_pixel(40, 40, Rgba([20, 20, 20, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("sel_tr.png"), bg);
        let fg = Arc::new(RgbaImage::from_pixel(20, 20, Rgba([200, 40, 30, 255])));
        doc.apply_layer_op(LayerOperation::AddImage {
            name: "FG".to_owned(),
            offset_x: 10,
            offset_y: 10,
            pixels: fg,
        })
        .unwrap();
        let fg_id = doc.stack().active_layer_id;

        // Add a layer mask revealing [4..20, 4..20] in layer-local coords (canvas [14..30, 14..30])
        doc.apply_layer_op(LayerOperation::AddMask {
            id: fg_id,
            reveal_all: false,
        })
        .unwrap();
        {
            let layer = doc.stack.active_layer_mut();
            let mut m = vec![0u8; 20 * 20];
            for y in 4..20 {
                for x in 4..20 {
                    m[y * 20 + x] = 255;
                }
            }
            layer.mask = Some(Arc::new(m));
        }

        // Select partial box [14, 14, 8, 8] (inside both layer and mask)
        doc.select_shape(
            &SelectionShape::Rect {
                x: 14.0,
                y: 14.0,
                width: 8.0,
                height: 8.0,
                feather: 0.0,
                anti_alias: false,
            },
            SelectionBooleanOp::Replace,
        );
        let rev_before = doc.selection_revision;

        // Translate selected pixels to (-10, -10) -> canvas [4..12, 4..12] (expands layer bounds!)
        doc.apply(EditOp::TransformLayer {
            layer_id: Some(fg_id),
            transform: LayerTransformSpec {
                translate_x: -10.0,
                translate_y: -10.0,
                ..LayerTransformSpec::default()
            },
        })
        .unwrap();

        let layer = doc.stack().active_layer();
        assert_eq!((layer.offset_x, layer.offset_y), (4, 4));
        assert_eq!(layer.dimensions(), (26, 26));
        // Layer mask must stay aligned to new dimensions (26x26) instead of being dropped
        let mask_buf = layer.mask.as_ref().expect("mask preserved");
        assert_eq!(mask_buf.len(), 26 * 26);
        // Old canvas (18, 18) -> new layer-local (14, 14) remains revealed in mask
        assert_eq!(mask_buf[14 * 26 + 14], 255);
        // Old hole at canvas (18, 18) in layer pixels is cleared to transparent
        assert_eq!(layer.pixels.get_pixel(14, 14).0[3], 0);
        // Moved pixels at canvas (8, 8) -> layer-local (4, 4) are opaque red
        assert_eq!(layer.pixels.get_pixel(4, 4).0, [200, 40, 30, 255]);
        // Selection mask moved to [4..12, 4..12] and selection_revision bumped
        assert!(doc.selection_revision > rev_before);
        assert_eq!(doc.selection().bounds, Some([4, 4, 8, 8]));
        assert_eq!(doc.selection().alpha_at(8, 8), 255);
        assert_eq!(doc.selection().alpha_at(18, 18), 0);

        // Undo restores layer offset, pixels, mask, and original selection
        let rev_after_tr = doc.selection_revision;
        assert!(doc.undo().unwrap());
        assert!(doc.selection_revision > rev_after_tr);
        assert_eq!(doc.selection().bounds, Some([14, 14, 8, 8]));
        assert_eq!(doc.stack().active_layer().offset_x, 10);
        assert_eq!(doc.stack().active_layer().dimensions(), (20, 20));
        assert_eq!(
            doc.stack().active_layer().pixels.get_pixel(8, 8).0,
            [200, 40, 30, 255]
        );

        // Redo re-applies selection transform cleanly
        assert!(doc.redo().unwrap());
        assert_eq!(doc.selection().bounds, Some([4, 4, 8, 8]));
        assert_eq!(doc.stack().active_layer().dimensions(), (26, 26));

        // 2. Feathered selection + scale/rotate + fractional edge RGB preservation
        let solid = RgbaImage::from_pixel(32, 32, Rgba([100, 150, 200, 255]));
        let mut fdoc = EditorDocument::from_rgba(PathBuf::from("feather.png"), solid);
        let mut feather_mask = vec![0u8; 32 * 32];
        for y in 8..24 {
            for x in 8..24 {
                let edge = x == 8 || x == 23 || y == 8 || y == 23;
                feather_mask[y * 32 + x] = if edge { 128 } else { 255 };
            }
        }
        fdoc.stack
            .selection
            .combine(&feather_mask, SelectionBooleanOp::Replace);
        fdoc.apply(EditOp::TransformLayer {
            layer_id: None,
            transform: LayerTransformSpec {
                translate_x: 4.5,
                translate_y: 2.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation_deg: 0.0,
                ..LayerTransformSpec::default()
            },
        })
        .unwrap();
        // Color channels of moved interior pixel must stay [100, 150, 200] without bicubic overshoot
        let moved_px = fdoc.working_pixels().get_pixel(16, 14).0;
        assert_eq!(moved_px[3], 255);
        assert!((i16::from(moved_px[0]) - 100).abs() <= 1);
        assert!((i16::from(moved_px[1]) - 150).abs() <= 1);
        assert!((i16::from(moved_px[2]) - 200).abs() <= 1);
    }

    #[test]
    fn hsv_adjustment_respects_selection_and_layer_targets() {
        let base = RgbaImage::from_pixel(16, 16, Rgba([220, 40, 40, 255]));
        let mut doc = EditorDocument::from_rgba(PathBuf::from("hsv_sel.png"), base);
        let top_img = RgbaImage::from_pixel(16, 16, Rgba([40, 200, 40, 255]));
        doc.apply_layer_op(LayerOperation::AddImage {
            name: "Top".to_owned(),
            offset_x: 0,
            offset_y: 0,
            pixels: Arc::new(top_img),
        })
        .unwrap();

        let mut sel_rect = vec![0u8; 16 * 16];
        for y in 4..10 {
            for x in 4..10 {
                sel_rect[y * 16 + x] = 255;
            }
        }
        doc.stack
            .selection
            .combine(&sel_rect, SelectionBooleanOp::Replace);

        let hsv = HsvAdjustment {
            hue: 120.0,
            saturation: 0.0,
            value: 0.0,
        };

        // 1. ActiveLayer + selection_only = true: only top layer inside [4..10, 4..10] shifts hue.
        assert!(
            doc.apply_with_selection(EditOp::AdjustHsv(hsv), ApplyTarget::ActiveLayer, true)
                .unwrap()
        );
        assert_eq!(
            doc.stack().layers[0].pixels.get_pixel(6, 6).0,
            [220, 40, 40, 255]
        );
        assert_eq!(
            doc.stack().layers[1].pixels.get_pixel(1, 1).0,
            [40, 200, 40, 255]
        );
        assert_ne!(
            doc.stack().layers[1].pixels.get_pixel(6, 6).0,
            [40, 200, 40, 255]
        );

        // Undo, then ActiveLayer + selection_only = false: outside selection also shifts.
        assert!(doc.undo().unwrap());
        assert!(
            doc.apply_with_selection(EditOp::AdjustHsv(hsv), ApplyTarget::ActiveLayer, false)
                .unwrap()
        );
        assert_ne!(
            doc.stack().layers[1].pixels.get_pixel(1, 1).0,
            [40, 200, 40, 255]
        );
        assert_eq!(
            doc.stack().layers[0].pixels.get_pixel(6, 6).0,
            [220, 40, 40, 255]
        );

        // Undo, then WholeImage + selection_only = true: both layers shift inside selection only.
        assert!(doc.undo().unwrap());
        assert!(
            doc.apply_with_selection(EditOp::AdjustHsv(hsv), ApplyTarget::WholeImage, true)
                .unwrap()
        );
        assert_eq!(
            doc.stack().layers[0].pixels.get_pixel(1, 1).0,
            [220, 40, 40, 255]
        );
        assert_ne!(
            doc.stack().layers[0].pixels.get_pixel(6, 6).0,
            [220, 40, 40, 255]
        );
        assert_eq!(
            doc.stack().layers[1].pixels.get_pixel(1, 1).0,
            [40, 200, 40, 255]
        );
        assert_ne!(
            doc.stack().layers[1].pixels.get_pixel(6, 6).0,
            [40, 200, 40, 255]
        );

        // Undo, then NewLayer + selection_only = true: new layer is transparent outside selection.
        assert!(doc.undo().unwrap());
        assert!(
            doc.apply_with_selection(EditOp::AdjustHsv(hsv), ApplyTarget::NewLayer, true)
                .unwrap()
        );
        assert_eq!(doc.stack().layers.len(), 3);
        let new_layer = &doc.stack().layers[2];
        assert_eq!(new_layer.pixels.get_pixel(1, 1).0[3], 0);
        assert_eq!(new_layer.pixels.get_pixel(6, 6).0[3], 255);
        assert_ne!(new_layer.pixels.get_pixel(6, 6).0, [40, 200, 40, 255]);
    }
}
